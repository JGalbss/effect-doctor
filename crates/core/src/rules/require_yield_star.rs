use oxc_ast::ast::YieldExpression;

use crate::diagnostics::{Category, RuleMeta, Severity};
use crate::rules::{FileCtx, Rule};

static META: RuleMeta = RuleMeta {
    id: "require-yield-star",
    severity: Severity::Error,
    category: Category::Correctness,
    help: "Inside Effect.gen, effects must be yielded with `yield*`. A plain `yield` hands the raw Effect to the iterator protocol and the result type is wrong.",
};

pub struct RequireYieldStar;

impl Rule for RequireYieldStar {
    fn meta(&self) -> &'static RuleMeta {
        &META
    }

    fn on_yield(&self, yield_expr: &YieldExpression<'_>, ctx: &mut FileCtx) {
        if yield_expr.delegate || !ctx.in_effect_gen() {
            return;
        }
        ctx.report(
            &META,
            yield_expr.span,
            "Plain `yield` inside Effect.gen — use `yield*`".to_string(),
        );
    }
}
