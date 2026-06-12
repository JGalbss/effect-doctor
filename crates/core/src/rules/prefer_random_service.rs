use oxc_ast::ast::CallExpression;

use crate::diagnostics::{Category, RuleMeta, Severity};
use crate::matchers::{ident_name, static_member};
use crate::rules::{FileCtx, Rule};

static META: RuleMeta = RuleMeta {
    id: "prefer-random-service",
    severity: Severity::Warn,
    category: Category::Idiomatic,
    help: "Direct randomness makes Effect code non-deterministic in tests. Use the `Random` service (`Random.next`, `Random.nextIntBetween`, ...) so tests can seed it.",
};

pub struct PreferRandomService;

impl Rule for PreferRandomService {
    fn meta(&self) -> &'static RuleMeta {
        &META
    }

    fn on_call(&self, call: &CallExpression<'_>, ctx: &mut FileCtx) {
        if !ctx.in_effect_code() {
            return;
        }
        let Some((object, prop)) = static_member(&call.callee) else {
            return;
        };
        if ident_name(object) != Some("Math") || prop != "random" {
            return;
        }
        ctx.report(
            &META,
            call.span,
            "Math.random() inside Effect code — use the Random service".to_string(),
        );
    }
}
