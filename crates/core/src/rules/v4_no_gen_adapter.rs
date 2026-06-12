use oxc_ast::ast::CallExpression;

use crate::diagnostics::{Category, RuleMeta, Severity};
use crate::matchers::direct_effect_gen;
use crate::rules::{FileCtx, Rule};

static META: RuleMeta = RuleMeta {
    id: "v4-no-gen-adapter",
    severity: Severity::Warn,
    category: Category::V4Migration,
    help: "The `_` adapter parameter (`Effect.gen(function*(_) { yield* _(op) })`) is deprecated and removed in v4 — yield effects directly with `yield* op`.",
};

pub struct V4NoGenAdapter;

impl Rule for V4NoGenAdapter {
    fn meta(&self) -> &'static RuleMeta {
        &META
    }

    fn on_call(&self, call: &CallExpression<'_>, ctx: &mut FileCtx) {
        let Some(generator) = direct_effect_gen(call, &ctx.imports) else {
            return;
        };
        if generator.params.items.is_empty() {
            return;
        }
        ctx.report(
            &META,
            generator.params.span,
            "Effect.gen adapter parameter is deprecated — yield effects directly".to_string(),
        );
    }
}
