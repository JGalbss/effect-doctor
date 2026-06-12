use oxc_ast::ast::{CallExpression, NewExpression};

use crate::diagnostics::{Category, RuleMeta, Severity};
use crate::matchers::{ident_name, static_member};
use crate::rules::{FileCtx, Rule};

static META: RuleMeta = RuleMeta {
    id: "prefer-clock-service",
    severity: Severity::Warn,
    category: Category::Idiomatic,
    help: "Reading the wall clock directly makes Effect code untestable. Use `Clock.currentTimeMillis` / `DateTime.now` so TestClock can control time in tests.",
};

pub struct PreferClockService;

impl Rule for PreferClockService {
    fn on_call(&self, call: &CallExpression<'_>, ctx: &mut FileCtx) {
        if !ctx.in_effect_code() {
            return;
        }
        let Some((object, prop)) = static_member(&call.callee) else {
            return;
        };
        if ident_name(object) != Some("Date") || prop != "now" {
            return;
        }
        ctx.report(
            &META,
            call.span,
            "Date.now() inside Effect code — use Clock.currentTimeMillis".to_string(),
        );
    }

    fn on_new(&self, new_expr: &NewExpression<'_>, ctx: &mut FileCtx) {
        if !ctx.in_effect_code() {
            return;
        }
        if ident_name(&new_expr.callee) != Some("Date") || !new_expr.arguments.is_empty() {
            return;
        }
        ctx.report(
            &META,
            new_expr.span,
            "new Date() inside Effect code — use DateTime.now".to_string(),
        );
    }
}
