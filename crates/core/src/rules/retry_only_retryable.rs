use oxc_ast::ast::{Argument, CallExpression, Expression, ObjectPropertyKind, PropertyKey};

use crate::diagnostics::{Category, RuleMeta, Severity};
use crate::matchers::call_module_prop;
use crate::rules::{FileCtx, Rule};

static META: RuleMeta = RuleMeta {
    id: "retry-only-retryable",
    severity: Severity::Info,
    category: Category::Architecture,
    help: "Retrying every failure indiscriminately retries validation errors, 404s, and bugs. Add `while`/`until` (or catchTag routing) so only transient failures are retried.",
};

fn has_retry_filter(call: &CallExpression<'_>) -> bool {
    call.arguments
        .iter()
        .filter_map(Argument::as_expression)
        .any(|expr| {
            let Expression::ObjectExpression(object) = expr else {
                return false;
            };
            object.properties.iter().any(|property| {
                let ObjectPropertyKind::ObjectProperty(entry) = property else {
                    return false;
                };
                let PropertyKey::StaticIdentifier(key) = &entry.key else {
                    return false;
                };
                matches!(key.name.as_str(), "while" | "until")
            })
        })
}

pub struct RetryOnlyRetryable;

impl Rule for RetryOnlyRetryable {
    fn metas(&self) -> &'static [&'static RuleMeta] {
        static METAS: &[&RuleMeta] = &[&META];
        METAS
    }

    fn on_call(&self, call: &CallExpression<'_>, ctx: &mut FileCtx) {
        if call_module_prop(call, &ctx.imports) != Some(("Effect", "retry")) {
            return;
        }
        if call.arguments.is_empty() || has_retry_filter(call) {
            return;
        }
        ctx.report(
            &META,
            call.span,
            "Effect.retry without while/until — every failure will be retried".to_string(),
        );
    }
}
