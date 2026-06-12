use oxc_ast::ast::TryStatement;

use crate::diagnostics::{Category, RuleMeta, Severity};
use crate::rules::{FileCtx, Rule};

static META: RuleMeta = RuleMeta {
    id: "no-try-catch-in-gen",
    severity: Severity::Error,
    category: Category::Correctness,
    help: "Effect failures do not throw — they flow through the typed error channel, so the catch block is dead code for them. Handle failures with Effect.catchTag / Effect.catch.",
};

pub struct NoTryCatchInGen;

impl Rule for NoTryCatchInGen {
    fn on_try(&self, try_stmt: &TryStatement<'_>, ctx: &mut FileCtx) {
        if !ctx.in_effect_gen() {
            return;
        }
        ctx.report(
            &META,
            oxc_span::Span::new(try_stmt.span.start, try_stmt.span.start + 3),
            "try/catch inside Effect.gen — Effect failures will not be caught here".to_string(),
        );
    }
}
