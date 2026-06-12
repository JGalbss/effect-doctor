use oxc_ast::ast::{Argument, CallExpression, Expression, YieldExpression};

use crate::diagnostics::{Category, RuleMeta, Severity};
use crate::matchers::call_module_prop;
use crate::rules::{FileCtx, Rule};

static META: RuleMeta = RuleMeta {
    id: "no-unnecessary-fail-of-yieldable",
    severity: Severity::Info,
    category: Category::Idiomatic,
    help: "Tagged errors are themselves yieldable: `return yield* new MyError({...})` — wrapping in Effect.fail is redundant.",
};

pub struct NoUnnecessaryFail;

impl Rule for NoUnnecessaryFail {
    fn on_yield(&self, yield_expr: &YieldExpression<'_>, ctx: &mut FileCtx) {
        if !yield_expr.delegate || !ctx.in_effect_gen() {
            return;
        }
        let Some(Expression::CallExpression(call)) = &yield_expr.argument else {
            return;
        };
        if !is_fail_of_new(call, ctx) {
            return;
        }
        ctx.report(
            &META,
            yield_expr.span,
            "yield* Effect.fail(new TaggedError(...)) — tagged errors are yieldable directly".to_string(),
        );
    }
}

fn is_fail_of_new(call: &CallExpression<'_>, ctx: &FileCtx) -> bool {
    if call_module_prop(call, &ctx.imports) != Some(("Effect", "fail")) {
        return false;
    }
    matches!(
        call.arguments.first().and_then(Argument::as_expression),
        Some(Expression::NewExpression(_))
    )
}
