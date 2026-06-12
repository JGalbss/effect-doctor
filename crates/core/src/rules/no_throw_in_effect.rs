use oxc_ast::ast::ThrowStatement;

use crate::diagnostics::{Category, RuleMeta, Severity};
use crate::rules::{FileCtx, Rule};

static META: RuleMeta = RuleMeta {
    id: "no-throw-in-effect",
    severity: Severity::Error,
    category: Category::Correctness,
    help: "A throw inside Effect code becomes an untyped defect, invisible to the error channel. Fail with a tagged error instead: `yield* Effect.fail(new MyError({...}))`.",
};

pub struct NoThrowInEffect;

impl Rule for NoThrowInEffect {
    fn meta(&self) -> &'static RuleMeta {
        &META
    }

    fn on_throw(&self, throw_stmt: &ThrowStatement<'_>, ctx: &mut FileCtx) {
        if !ctx.in_effect_gen() {
            return;
        }
        ctx.report(
            &META,
            throw_stmt.span,
            "`throw` inside Effect.gen becomes an untyped defect — use Effect.fail with a tagged error".to_string(),
        );
    }
}
