use oxc_ast::ast::{CallExpression, Expression, ObjectPropertyKind, PropertyKey};

use crate::diagnostics::{Category, RuleMeta, Severity};
use crate::rules::{FileCtx, Rule};

static META: RuleMeta = RuleMeta {
    id: "no-unbounded-concurrency",
    severity: Severity::Info,
    category: Category::Performance,
    help: "`concurrency: \"unbounded\"` over a large collection spawns a fiber per element with no backpressure. Prefer a bounded number sized to the resource.",
};

pub struct NoUnboundedConcurrency;

impl Rule for NoUnboundedConcurrency {
    fn on_call(&self, call: &CallExpression<'_>, ctx: &mut FileCtx) {
        for argument in &call.arguments {
            let Some(Expression::ObjectExpression(object)) = argument.as_expression() else {
                continue;
            };
            for property in &object.properties {
                let ObjectPropertyKind::ObjectProperty(entry) = property else {
                    continue;
                };
                let PropertyKey::StaticIdentifier(key) = &entry.key else {
                    continue;
                };
                if key.name != "concurrency" {
                    continue;
                }
                let Expression::StringLiteral(value) = &entry.value else {
                    continue;
                };
                if value.value != "unbounded" {
                    continue;
                }
                ctx.report(
                    &META,
                    entry.span,
                    "concurrency: \"unbounded\" — prefer a bounded concurrency".to_string(),
                );
            }
        }
    }
}
