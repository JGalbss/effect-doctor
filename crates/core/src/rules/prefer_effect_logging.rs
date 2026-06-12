use oxc_ast::ast::CallExpression;

use crate::diagnostics::{Category, RuleMeta, Severity};
use crate::matchers::{ident_name, static_member};
use crate::rules::{FileCtx, Rule};

static META: RuleMeta = RuleMeta {
    id: "prefer-effect-logging",
    severity: Severity::Warn,
    category: Category::Idiomatic,
    help: "console.* bypasses Effect's structured, context-aware logging (levels, spans, annotations). Use Effect.log / Effect.logInfo / Effect.logError.",
};

fn is_console_method(prop: &str) -> bool {
    matches!(prop, "log" | "warn" | "error" | "info" | "debug" | "trace")
}

pub struct PreferEffectLogging;

impl Rule for PreferEffectLogging {
    fn on_call(&self, call: &CallExpression<'_>, ctx: &mut FileCtx) {
        if !ctx.in_effect_code() {
            return;
        }
        let Some((object, prop)) = static_member(&call.callee) else {
            return;
        };
        if ident_name(object) != Some("console") || !is_console_method(prop) {
            return;
        }
        ctx.report(
            &META,
            call.span,
            format!("console.{prop} inside Effect code — use Effect.log*"),
        );
    }
}
