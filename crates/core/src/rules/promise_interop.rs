use oxc_ast::ast::{Argument, CallExpression, Expression, ObjectPropertyKind, PropertyKey};

use crate::diagnostics::{Category, RuleMeta, Severity};
use crate::matchers::{
    call_module_prop, expression_has_call, expression_has_new, first_function_arg,
    is_async_function, static_member,
};
use crate::rules::{FileCtx, Rule};

static NO_ASYNC_CALLBACK: RuleMeta = RuleMeta {
    id: "no-async-callback-in-effect-combinators",
    severity: Severity::Error,
    category: Category::Correctness,
    help: "An async callback returns a Promise the Effect runtime never awaits — the work dangles or the value becomes Effect<Promise<T>>. Use Effect.tryPromise / Effect.promise for async work.",
};

static NO_THEN_IN_SYNC: RuleMeta = RuleMeta {
    id: "no-then-in-sync",
    severity: Severity::Warn,
    category: Category::Correctness,
    help: "Promise chains inside Effect.sync escape the runtime — no error channel, no interruption. Use Effect.tryPromise and compose with Effect.map/flatMap.",
};

static NO_PROMISE_ALL: RuleMeta = RuleMeta {
    id: "no-promise-all-in-effect",
    severity: Severity::Warn,
    category: Category::Correctness,
    help: "Promise.all inside an Effect wrapper bypasses concurrency limits, interruption, and structured concurrency. Use Effect.forEach(items, fn, { concurrency: N }) or Effect.all.",
};

static REQUIRE_TYPED_CATCH: RuleMeta = RuleMeta {
    id: "require-typed-catch-in-try",
    severity: Severity::Info,
    category: Category::Idiomatic,
    help: "Bare Effect.try/tryPromise puts UnknownException in the error channel. The { try, catch } form maps the failure to a typed domain error.",
};

fn is_sync_callback_combinator(prop: &str) -> bool {
    matches!(
        prop,
        "map" | "flatMap" | "andThen" | "tap" | "sync" | "try" | "filter" | "forEach" | "reduce"
            | "mapEffect"
    )
}

fn is_promise_combinator(prop: &str) -> bool {
    matches!(prop, "all" | "allSettled" | "race" | "any")
}

fn is_promise_wrapper(prop: &str) -> bool {
    matches!(prop, "promise" | "tryPromise")
}

fn has_async_argument(call: &CallExpression<'_>) -> Option<oxc_span::Span> {
    call.arguments
        .iter()
        .filter_map(Argument::as_expression)
        .find(|expr| is_async_function(expr))
        .map(|expr| match expr {
            Expression::ArrowFunctionExpression(arrow) => arrow.span,
            Expression::FunctionExpression(function) => function.span,
            _ => unreachable!("is_async_function only matches functions"),
        })
}

fn contains_promise_chain(handler: &Expression<'_>) -> bool {
    let chained = expression_has_call(handler, |inner| {
        static_member(&inner.callee)
            .is_some_and(|(_, prop)| matches!(prop, "then" | "catch" | "finally"))
    });
    if chained {
        return true;
    }
    expression_has_new(handler, |new_expr| {
        matches!(&new_expr.callee, Expression::Identifier(identifier) if identifier.name == "Promise")
    })
}

fn contains_promise_combinator(handler: &Expression<'_>) -> Option<&'static str> {
    let mut found = None;
    expression_has_call(handler, |inner| {
        let Some((object, prop)) = static_member(&inner.callee) else {
            return false;
        };
        let is_promise_object = matches!(object, Expression::Identifier(identifier) if identifier.name == "Promise");
        if is_promise_object && is_promise_combinator(prop) {
            found = Some(match prop {
                "all" => "Promise.all",
                "allSettled" => "Promise.allSettled",
                "race" => "Promise.race",
                _ => "Promise.any",
            });
            return true;
        }
        false
    });
    found
}

fn options_try_handler<'a, 'b>(call: &'b CallExpression<'a>) -> Option<&'b Expression<'a>> {
    let Some(Expression::ObjectExpression(object)) =
        call.arguments.first().and_then(Argument::as_expression)
    else {
        return None;
    };
    object.properties.iter().find_map(|property| {
        let ObjectPropertyKind::ObjectProperty(entry) = property else {
            return None;
        };
        let PropertyKey::StaticIdentifier(key) = &entry.key else {
            return None;
        };
        if key.name != "try" {
            return None;
        }
        Some(&entry.value)
    })
}

pub struct PromiseInterop;

impl Rule for PromiseInterop {
    fn metas(&self) -> &'static [&'static RuleMeta] {
        static METAS: &[&RuleMeta] = &[&NO_ASYNC_CALLBACK, &NO_THEN_IN_SYNC, &NO_PROMISE_ALL, &REQUIRE_TYPED_CATCH];
        METAS
    }

    fn on_call(&self, call: &CallExpression<'_>, ctx: &mut FileCtx) {
        let Some((module, prop)) = call_module_prop(call, &ctx.imports) else {
            return;
        };
        if !matches!(module, "Effect" | "Stream") {
            return;
        }
        let (module, prop) = (module.to_string(), prop.to_string());
        let (module, prop) = (module.as_str(), prop.as_str());

        if is_sync_callback_combinator(prop) && !is_promise_wrapper(prop) {
            if let Some(span) = has_async_argument(call) {
                let fix = match prop {
                    "sync" => "Effect.promise",
                    "try" => "Effect.tryPromise",
                    _ => "Effect.tryPromise inside flatMap",
                };
                ctx.report(
                    &NO_ASYNC_CALLBACK,
                    span,
                    format!("async callback passed to {module}.{prop} — the Promise is never awaited; use {fix}"),
                );
            }
        }

        if module == "Effect" && prop == "sync" {
            if let Some(handler) = first_function_arg(call) {
                if contains_promise_chain(handler) {
                    ctx.report(
                        &NO_THEN_IN_SYNC,
                        call.span,
                        "Promise chain inside Effect.sync — use Effect.tryPromise".to_string(),
                    );
                }
            }
        }

        if module == "Effect" && is_promise_wrapper(prop) {
            let handler = first_function_arg(call).or_else(|| options_try_handler(call));
            if let Some(handler) = handler {
                if let Some(combinator) = contains_promise_combinator(handler) {
                    ctx.report(
                        &NO_PROMISE_ALL,
                        call.span,
                        format!("{combinator} inside Effect.{prop} — use Effect.forEach with a concurrency option"),
                    );
                }
            }
            if prop == "tryPromise" && first_function_arg(call).is_some() {
                ctx.report(
                    &REQUIRE_TYPED_CATCH,
                    call.span,
                    "bare Effect.tryPromise — add { try, catch } to map the failure to a typed error".to_string(),
                );
            }
        }
    }
}
