use oxc_ast::ast::{
    Argument, CallExpression, Expression, ObjectPropertyKind, PropertyKey, Statement,
};

use crate::diagnostics::{Category, RuleMeta, Severity};
use crate::matchers::{call_module_prop, direct_effect_gen, static_member, unwrap_parens};
use crate::rules::{FileCtx, Rule};

static MISSING_CONCURRENCY: RuleMeta = RuleMeta {
    id: "effect-all-missing-concurrency",
    severity: Severity::Info,
    category: Category::Performance,
    help: "Effect.all / Effect.forEach run sequentially unless you pass { concurrency } — usually a surprise for code that looks parallel. Make the choice explicit.",
};

static RACE_SLEEP: RuleMeta = RuleMeta {
    id: "prefer-timeout-over-race-sleep",
    severity: Severity::Warn,
    category: Category::Idiomatic,
    help: "Racing against Effect.sleep hand-rolls a deadline with worse semantics. Effect.timeout / timeoutTo / timeoutFail say it directly.",
};

static FORK_JOIN: RuleMeta = RuleMeta {
    id: "no-fork-then-immediate-join",
    severity: Severity::Warn,
    category: Category::Idiomatic,
    help: "Forking and immediately joining is just running the effect, with extra fiber overhead. Yield the effect directly; fork only when work continues in the background.",
};

fn options_have_concurrency(call: &CallExpression<'_>) -> bool {
    call.arguments
        .iter()
        .filter_map(Argument::as_expression)
        .any(|expr| {
            let Expression::ObjectExpression(object) = expr else {
                return false;
            };
            object.properties.iter().any(|property| {
                let ObjectPropertyKind::ObjectProperty(entry) = property else {
                    return false;
                };
                matches!(&entry.key, PropertyKey::StaticIdentifier(key) if key.name == "concurrency")
            })
        })
}

fn is_sleep_expression(expr: &Expression, ctx: &FileCtx) -> bool {
    match unwrap_parens(expr) {
        Expression::CallExpression(call) => {
            if call_module_prop(call, &ctx.imports) == Some(("Effect", "sleep")) {
                return true;
            }
            // Effect.sleep(...).pipe(...)
            let Some((object, "pipe")) = static_member(&call.callee) else {
                return false;
            };
            matches!(
                unwrap_parens(object),
                Expression::CallExpression(root)
                    if call_module_prop(root, &ctx.imports) == Some(("Effect", "sleep"))
            )
        }
        _ => false,
    }
}

/// `const fiber = yield* Effect.fork(x)` — returns the binding name.
fn forked_binding<'a, 'b>(statement: &'b Statement<'a>, ctx: &FileCtx) -> Option<&'b str> {
    let Statement::VariableDeclaration(declaration) = statement else {
        return None;
    };
    let declarator = declaration.declarations.first()?;
    let name = declarator.id.get_identifier_name()?;
    let Some(Expression::YieldExpression(yield_expr)) =
        declarator.init.as_ref().map(unwrap_parens)
    else {
        return None;
    };
    if !yield_expr.delegate {
        return None;
    }
    let Some(Expression::CallExpression(call)) = yield_expr.argument.as_ref().map(unwrap_parens)
    else {
        return None;
    };
    let Some(("Effect", prop)) = call_module_prop(call, &ctx.imports) else {
        return None;
    };
    if !matches!(prop, "fork" | "forkChild" | "forkDaemon" | "forkDetach" | "forkScoped") {
        return None;
    }
    Some(name.as_str())
}

/// Does the statement contain `yield* Fiber.join(<name>)`?
fn joins_fiber(statement: &Statement<'_>, name: &str, ctx: &FileCtx) -> Option<oxc_span::Span> {
    let yielded = match statement {
        Statement::VariableDeclaration(declaration) => declaration
            .declarations
            .first()?
            .init
            .as_ref()
            .map(unwrap_parens)?,
        Statement::ReturnStatement(return_stmt) => unwrap_parens(return_stmt.argument.as_ref()?),
        Statement::ExpressionStatement(expression_stmt) => {
            unwrap_parens(&expression_stmt.expression)
        }
        _ => return None,
    };
    let Expression::YieldExpression(yield_expr) = yielded else {
        return None;
    };
    let Some(Expression::CallExpression(call)) = yield_expr.argument.as_ref().map(unwrap_parens)
    else {
        return None;
    };
    if call_module_prop(call, &ctx.imports) != Some(("Fiber", "join")) {
        return None;
    }
    let joins_same = matches!(
        call.arguments.first().and_then(Argument::as_expression),
        Some(Expression::Identifier(identifier)) if identifier.name == name
    );
    if !joins_same {
        return None;
    }
    Some(call.span)
}

pub struct ConcurrencyIdioms;

impl ConcurrencyIdioms {
    fn check_missing_concurrency(&self, call: &CallExpression<'_>, ctx: &mut FileCtx) {
        let Some(("Effect", prop)) = call_module_prop(call, &ctx.imports) else {
            return;
        };
        if prop == "all" {
            let first_is_multi_array = matches!(
                call.arguments.first().and_then(Argument::as_expression),
                Some(Expression::ArrayExpression(array)) if array.elements.len() >= 2
            );
            if first_is_multi_array && !options_have_concurrency(call) {
                ctx.report(
                    &MISSING_CONCURRENCY,
                    call.span,
                    "Effect.all without { concurrency } runs sequentially — make it explicit".to_string(),
                );
            }
            return;
        }
        if prop == "forEach" && call.arguments.len() >= 2 && !options_have_concurrency(call) {
            ctx.report(
                &MISSING_CONCURRENCY,
                call.span,
                "Effect.forEach without { concurrency } runs sequentially — make it explicit".to_string(),
            );
        }
    }

    fn check_race_sleep(&self, call: &CallExpression<'_>, ctx: &mut FileCtx) {
        let Some(("Effect", prop)) = call_module_prop(call, &ctx.imports) else {
            return;
        };
        if !matches!(prop, "race" | "raceAll" | "raceFirst") {
            return;
        }
        let races_sleep = call
            .arguments
            .iter()
            .filter_map(Argument::as_expression)
            .any(|expr| match unwrap_parens(expr) {
                Expression::ArrayExpression(array) => array
                    .elements
                    .iter()
                    .filter_map(|element| element.as_expression())
                    .any(|element| is_sleep_expression(element, ctx)),
                other => is_sleep_expression(other, ctx),
            });
        if !races_sleep {
            return;
        }
        ctx.report(
            &RACE_SLEEP,
            call.span,
            format!("Effect.{prop} against Effect.sleep — use Effect.timeout"),
        );
    }

    fn check_fork_join(&self, call: &CallExpression<'_>, ctx: &mut FileCtx) {
        let Some(generator) = direct_effect_gen(call, &ctx.imports) else {
            return;
        };
        let Some(body) = &generator.body else {
            return;
        };
        let statements = &body.statements;
        let mut reports = Vec::new();
        for index in 0..statements.len().saturating_sub(1) {
            let Some(name) = forked_binding(&statements[index], ctx) else {
                continue;
            };
            if let Some(span) = joins_fiber(&statements[index + 1], name, ctx) {
                reports.push(span);
            }
        }
        for span in reports {
            ctx.report(
                &FORK_JOIN,
                span,
                "fork immediately followed by Fiber.join — just yield the effect directly".to_string(),
            );
        }
    }
}

impl Rule for ConcurrencyIdioms {
    fn metas(&self) -> &'static [&'static RuleMeta] {
        static METAS: &[&RuleMeta] = &[&MISSING_CONCURRENCY, &RACE_SLEEP, &FORK_JOIN];
        METAS
    }

    fn on_call(&self, call: &CallExpression<'_>, ctx: &mut FileCtx) {
        self.check_missing_concurrency(call, ctx);
        self.check_race_sleep(call, ctx);
        self.check_fork_join(call, ctx);
    }
}
