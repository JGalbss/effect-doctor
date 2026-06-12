//! Experimental `--adopt` mode: vanilla TS patterns in an Effect codebase
//! that have a cleaner Effect equivalent. Opinionated by design — every
//! finding is a migration recommendation, so the whole family is opt-in,
//! except `prefer-foreach-over-yield-loop` which is Effect-code advice and
//! always on (info).

use oxc_ast::ast::{
    ArrowFunctionExpression, CallExpression, Expression, Function, NewExpression, Statement,
};
use oxc_span::Span;

use crate::diagnostics::{Category, RuleMeta, Severity};
use crate::matchers::{
    ident_name, static_member, statement_has_own_await, statement_has_own_yield, unwrap_parens,
};
use crate::rules::{FileCtx, Rule};

static ASYNC_FUNCTION: RuleMeta = RuleMeta {
    id: "adopt-async-function",
    severity: Severity::Info,
    category: Category::Adoption,
    help: "async/await runs outside Effect: untyped errors, no interruption, no tracing. Migrate to Effect.fn (or Effect.gen) with Effect.tryPromise at the Promise boundaries.",
};

static PROMISE_CHAIN: RuleMeta = RuleMeta {
    id: "adopt-promise-chain",
    severity: Severity::Info,
    category: Category::Adoption,
    help: ".then() chains are Effect pipelines without the safety. Wrap the source with Effect.tryPromise and compose with Effect.map / Effect.flatMap.",
};

static NEW_PROMISE: RuleMeta = RuleMeta {
    id: "adopt-new-promise",
    severity: Severity::Info,
    category: Category::Adoption,
    help: "Hand-rolled Promise constructors (resolve/reject plumbing) map directly onto Effect.async — with interruption support included.",
};

static PROMISE_ALL: RuleMeta = RuleMeta {
    id: "adopt-promise-all",
    severity: Severity::Info,
    category: Category::Adoption,
    help: "Promise.all has no concurrency limit and no interruption. Effect.all / Effect.forEach with { concurrency } is the structured equivalent.",
};

static AWAIT_IN_LOOP: RuleMeta = RuleMeta {
    id: "adopt-await-in-loop",
    severity: Severity::Warn,
    category: Category::Adoption,
    help: "Sequential awaits in a loop are the slowest possible shape. Effect.forEach(items, fn, { concurrency: N }) runs them structured and concurrent — or keep it sequential but interruptible.",
};

static YIELD_LOOP: RuleMeta = RuleMeta {
    id: "prefer-foreach-over-yield-loop",
    severity: Severity::Info,
    category: Category::Idiomatic,
    help: "A loop of yields inside Effect.gen works, but Effect.forEach / Effect.all make concurrency explicit and tunable, and keep interruption semantics obvious.",
};

fn is_promise_combinator(prop: &str) -> bool {
    matches!(prop, "all" | "allSettled" | "race" | "any")
}

pub struct Adopt;

impl Rule for Adopt {
    fn metas(&self) -> &'static [&'static RuleMeta] {
        static METAS: &[&RuleMeta] = &[
            &ASYNC_FUNCTION,
            &PROMISE_CHAIN,
            &NEW_PROMISE,
            &PROMISE_ALL,
            &AWAIT_IN_LOOP,
            &YIELD_LOOP,
        ];
        METAS
    }

    fn on_function(&self, function: &Function<'_>, ctx: &mut FileCtx) {
        if !ctx.adopt_active() || !function.r#async {
            return;
        }
        let name = function
            .id
            .as_ref()
            .map(|id| id.name.as_str())
            .unwrap_or("function");
        ctx.report(
            &ASYNC_FUNCTION,
            Span::new(function.span.start, function.span.start + 5),
            format!("async {name} — migrate to Effect.fn + Effect.tryPromise"),
        );
    }

    fn on_arrow(&self, arrow: &ArrowFunctionExpression<'_>, ctx: &mut FileCtx) {
        if !ctx.adopt_active() || !arrow.r#async {
            return;
        }
        ctx.report(
            &ASYNC_FUNCTION,
            Span::new(arrow.span.start, arrow.span.start + 5),
            "async arrow — migrate to Effect.fn + Effect.tryPromise".to_string(),
        );
    }

    fn on_call(&self, call: &CallExpression<'_>, ctx: &mut FileCtx) {
        if !ctx.adopt_active() {
            return;
        }
        let Some((object, prop)) = static_member(&call.callee) else {
            return;
        };

        // .then(fn) — report once per chain, at the innermost .then.
        if prop == "then" && !call.arguments.is_empty() {
            let inner_is_then = matches!(
                unwrap_parens(object),
                Expression::CallExpression(inner)
                    if matches!(static_member(&inner.callee), Some((_, "then")))
            );
            if !inner_is_then {
                ctx.report(
                    &PROMISE_CHAIN,
                    call.span,
                    ".then() chain — wrap with Effect.tryPromise and compose with Effect.flatMap".to_string(),
                );
            }
            return;
        }

        // Promise.all/allSettled/race/any outside Effect wrappers (inside
        // wrappers, no-promise-all-in-effect already covers it).
        if ident_name(object) == Some("Promise")
            && is_promise_combinator(prop)
            && !ctx.in_effect_code()
        {
            ctx.report(
                &PROMISE_ALL,
                call.span,
                format!("Promise.{prop} — use Effect.all / Effect.forEach with {{ concurrency }}"),
            );
        }
    }

    fn on_new(&self, new_expr: &NewExpression<'_>, ctx: &mut FileCtx) {
        if !ctx.adopt_active() || ctx.in_effect_code() {
            return;
        }
        if ident_name(&new_expr.callee) != Some("Promise") {
            return;
        }
        ctx.report(
            &NEW_PROMISE,
            new_expr.span,
            "new Promise(...) — Effect.async is the structured equivalent".to_string(),
        );
    }

    fn on_loop(&self, loop_span: Span, body: &Statement<'_>, ctx: &mut FileCtx) {
        let keyword_span = Span::new(loop_span.start, loop_span.start + 3);
        if ctx.in_effect_gen() && statement_has_own_yield(body) {
            ctx.report(
                &YIELD_LOOP,
                keyword_span,
                "loop of yields inside Effect.gen — consider Effect.forEach / Effect.all".to_string(),
            );
            return;
        }
        if !ctx.adopt_active() {
            return;
        }
        if statement_has_own_await(body) {
            ctx.report(
                &AWAIT_IN_LOOP,
                keyword_span,
                "await inside a loop runs strictly sequentially — use Effect.forEach with { concurrency }".to_string(),
            );
        }
    }
}
