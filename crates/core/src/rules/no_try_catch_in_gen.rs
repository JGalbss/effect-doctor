use oxc_ast::ast::TryStatement;

use crate::diagnostics::{Category, RuleMeta, Severity};
use crate::matchers::block_has_own_yield;
use crate::rules::{FileCtx, Rule};

static META: RuleMeta = RuleMeta {
    id: "no-try-catch-in-gen",
    severity: Severity::Error,
    category: Category::Correctness,
    help: "Effect failures do not throw — they flow through the typed error channel, so the catch block is dead code for them. Handle failures with Effect.catchTag / Effect.catch.",
};

static TRY_FINALLY: RuleMeta = RuleMeta {
    id: "no-try-finally-in-gen",
    severity: Severity::Warn,
    category: Category::Correctness,
    help: "try/finally around yields is not interruption-safe — the finalizer can be skipped if the fiber is interrupted. Use Effect.ensuring, Effect.acquireRelease, or Effect.race.",
};

pub struct NoTryCatchInGen;

impl Rule for NoTryCatchInGen {
    fn metas(&self) -> &'static [&'static RuleMeta] {
        static METAS: &[&RuleMeta] = &[&META, &TRY_FINALLY];
        METAS
    }

    fn on_try(&self, try_stmt: &TryStatement<'_>, ctx: &mut FileCtx) {
        if !ctx.in_effect_gen() {
            return;
        }
        let keyword_span = oxc_span::Span::new(try_stmt.span.start, try_stmt.span.start + 3);
        if try_stmt.handler.is_some() {
            ctx.report(
                &META,
                keyword_span,
                "try/catch inside Effect.gen — Effect failures will not be caught here".to_string(),
            );
            return;
        }
        let Some(finalizer) = &try_stmt.finalizer else {
            return;
        };
        if !block_has_own_yield(&try_stmt.block) && !block_has_own_yield(finalizer) {
            return;
        }
        ctx.report(
            &TRY_FINALLY,
            keyword_span,
            "try/finally with yields inside Effect.gen is not interruption-safe — use Effect.ensuring / acquireRelease".to_string(),
        );
    }
}
