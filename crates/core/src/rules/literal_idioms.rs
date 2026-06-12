use oxc_ast::ast::{Argument, CallExpression, Expression};

use crate::diagnostics::{Category, RuleMeta, Severity};
use crate::matchers::{arrow_body_expression, call_module_prop, unwrap_parens};
use crate::rules::{FileCtx, Rule};

static RAW_MILLIS: RuleMeta = RuleMeta {
    id: "prefer-duration-over-raw-millis",
    severity: Severity::Info,
    category: Category::Idiomatic,
    help: "A bare number for time is ambiguous (ms? s?). Duration strings (\"2 seconds\") or Duration.seconds(2) eliminate the guesswork.",
};

static SYNC_LITERAL: RuleMeta = RuleMeta {
    id: "prefer-succeed-over-sync-literal",
    severity: Severity::Info,
    category: Category::Idiomatic,
    help: "Effect.sync(() => literal) adds a pointless thunk — Effect.succeed(literal) is direct.",
};

fn takes_duration(module: &str, prop: &str) -> bool {
    match module {
        "Effect" => matches!(
            prop,
            "sleep" | "delay" | "timeout" | "timeoutTo" | "timeoutFail" | "interruptAfter"
                | "cachedWithTTL"
        ),
        "Schedule" => matches!(prop, "spaced" | "fixed" | "exponential" | "upTo"),
        "Stream" => matches!(prop, "timeout" | "debounce" | "interruptAfter" | "groupedWithin"),
        _ => false,
    }
}

fn is_primitive_literal(expr: &Expression) -> bool {
    match unwrap_parens(expr) {
        Expression::StringLiteral(_)
        | Expression::NumericLiteral(_)
        | Expression::BooleanLiteral(_)
        | Expression::NullLiteral(_) => true,
        Expression::TemplateLiteral(template) => template.expressions.is_empty(),
        _ => false,
    }
}

pub struct LiteralIdioms;

impl LiteralIdioms {
    fn check_duration(&self, call: &CallExpression<'_>, ctx: &mut FileCtx) {
        let Some((module, prop)) = call_module_prop(call, &ctx.imports) else {
            return;
        };
        if !takes_duration(module, prop) {
            return;
        }
        let arguments: Vec<&Expression> = call
            .arguments
            .iter()
            .filter_map(Argument::as_expression)
            .take(2)
            .collect();
        // A duration string in the first slot means later numerics are
        // factors/options (e.g. Schedule.exponential("100 millis", 2)).
        if matches!(
            arguments.first().map(|expr| unwrap_parens(expr)),
            Some(Expression::StringLiteral(_))
        ) {
            return;
        }
        let Some(numeric) = arguments.iter().find_map(|expr| match unwrap_parens(expr) {
            Expression::NumericLiteral(literal) => Some(literal),
            _ => None,
        }) else {
            return;
        };
        ctx.report(
            &RAW_MILLIS,
            numeric.span,
            format!(
                "{module}.{prop}({}) — is that ms or s? Use a Duration (\"{} millis\")",
                numeric.raw.as_ref().map(|raw| raw.as_str()).unwrap_or("n"),
                numeric.raw.as_ref().map(|raw| raw.as_str()).unwrap_or("n"),
            ),
        );
    }

    fn check_sync_literal(&self, call: &CallExpression<'_>, ctx: &mut FileCtx) {
        if call_module_prop(call, &ctx.imports) != Some(("Effect", "sync")) {
            return;
        }
        let Some(Expression::ArrowFunctionExpression(arrow)) =
            call.arguments.first().and_then(Argument::as_expression)
        else {
            return;
        };
        let Some(body) = arrow_body_expression(arrow) else {
            return;
        };
        if !is_primitive_literal(body) {
            return;
        }
        ctx.report(
            &SYNC_LITERAL,
            call.span,
            "Effect.sync(() => literal) — use Effect.succeed(literal)".to_string(),
        );
    }
}

impl Rule for LiteralIdioms {
    fn metas(&self) -> &'static [&'static RuleMeta] {
        static METAS: &[&RuleMeta] = &[&RAW_MILLIS, &SYNC_LITERAL];
        METAS
    }

    fn on_call(&self, call: &CallExpression<'_>, ctx: &mut FileCtx) {
        self.check_duration(call, ctx);
        self.check_sync_literal(call, ctx);
    }
}
