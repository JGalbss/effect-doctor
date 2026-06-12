use oxc_ast::ast::{CallExpression, Expression, Statement};

use crate::diagnostics::{Category, RuleMeta, Severity};
use crate::matchers::direct_effect_gen;
use crate::rules::{FileCtx, Rule};

static META: RuleMeta = RuleMeta {
    id: "no-unnecessary-gen",
    severity: Severity::Info,
    category: Category::Idiomatic,
    help: "An Effect.gen whose body is just `return yield* effect` adds a generator allocation for nothing — use the effect expression directly.",
};

pub struct NoUnnecessaryGen;

impl Rule for NoUnnecessaryGen {
    fn metas(&self) -> &'static [&'static RuleMeta] {
        static METAS: &[&RuleMeta] = &[&META];
        METAS
    }

    fn on_call(&self, call: &CallExpression<'_>, ctx: &mut FileCtx) {
        let Some(generator) = direct_effect_gen(call, &ctx.imports) else {
            return;
        };
        let Some(body) = &generator.body else {
            return;
        };
        if body.statements.len() != 1 {
            return;
        }
        let Statement::ReturnStatement(return_stmt) = &body.statements[0] else {
            return;
        };
        let Some(Expression::YieldExpression(yield_expr)) = &return_stmt.argument else {
            return;
        };
        if !yield_expr.delegate {
            return;
        }
        ctx.report(
            &META,
            call.span,
            "Effect.gen wrapping a single `return yield*` — use the inner effect directly"
                .to_string(),
        );
    }
}
