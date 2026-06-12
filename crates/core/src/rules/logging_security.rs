use oxc_ast::ast::{Argument, CallExpression, Expression};

use crate::diagnostics::{Category, RuleMeta, Severity};
use crate::matchers::{call_module_prop, expression_has_call, static_member, unwrap_parens};
use crate::rules::{FileCtx, Rule};

static CONFIG_REDACTED: RuleMeta = RuleMeta {
    id: "prefer-config-redacted",
    severity: Severity::Warn,
    category: Category::Correctness,
    help: "Secrets loaded as plain Config.string leak into logs, errors, and serialized output. Config.redacted wraps them so they can't be printed accidentally.",
};

static STRUCTURED_LOGGING: RuleMeta = RuleMeta {
    id: "prefer-structured-logging-args",
    severity: Severity::Info,
    category: Category::Idiomatic,
    help: "Interpolating JSON.stringify into a log message defeats structured logging. Log a stable message and attach data with Effect.annotateLogs.",
};

static JSON_RESPONSE: RuleMeta = RuleMeta {
    id: "prefer-json-response-helper",
    severity: Severity::Warn,
    category: Category::Idiomatic,
    help: "Manual JSON.stringify into a text response skips the content-type header clients rely on. HttpServerResponse.json(value) sets it correctly.",
};

fn looks_like_secret(name: &str) -> bool {
    let normalized: String = name
        .to_ascii_lowercase()
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .collect();
    ["secret", "password", "passwd", "token", "apikey", "privatekey", "credential"]
        .iter()
        .any(|needle| normalized.contains(needle))
}

fn is_log_method(prop: &str) -> bool {
    matches!(
        prop,
        "log" | "logInfo" | "logDebug" | "logWarning" | "logError" | "logFatal" | "logTrace"
    )
}

fn is_json_stringify(call: &CallExpression<'_>) -> bool {
    let Some((object, "stringify")) = static_member(&call.callee) else {
        return false;
    };
    matches!(unwrap_parens(object), Expression::Identifier(identifier) if identifier.name == "JSON")
}

pub struct LoggingSecurity;

impl LoggingSecurity {
    fn check_config_secret(&self, call: &CallExpression<'_>, ctx: &mut FileCtx) {
        if call_module_prop(call, &ctx.imports) != Some(("Config", "string")) {
            return;
        }
        let Some(Expression::StringLiteral(name)) =
            call.arguments.first().and_then(Argument::as_expression)
        else {
            return;
        };
        if !looks_like_secret(&name.value) {
            return;
        }
        ctx.report(
            &CONFIG_REDACTED,
            call.span,
            format!("Config.string(\"{}\") looks like a secret — use Config.redacted", name.value),
        );
    }

    fn check_structured_logging(&self, call: &CallExpression<'_>, ctx: &mut FileCtx) {
        let Some(("Effect", prop)) = call_module_prop(call, &ctx.imports) else {
            return;
        };
        if !is_log_method(prop) {
            return;
        }
        let Some(Expression::TemplateLiteral(template)) =
            call.arguments.first().and_then(Argument::as_expression)
        else {
            return;
        };
        let stringifies = template
            .expressions
            .iter()
            .any(|expr| expression_has_call(expr, is_json_stringify));
        if !stringifies {
            return;
        }
        ctx.report(
            &STRUCTURED_LOGGING,
            call.span,
            format!("JSON.stringify inside Effect.{prop} message — use Effect.annotateLogs"),
        );
    }

    fn check_json_response(&self, call: &CallExpression<'_>, ctx: &mut FileCtx) {
        let Some(("HttpServerResponse", "text")) = call_module_prop(call, &ctx.imports) else {
            return;
        };
        let stringifies = matches!(
            call.arguments.first().and_then(Argument::as_expression).map(unwrap_parens),
            Some(Expression::CallExpression(inner)) if is_json_stringify(inner)
        );
        if !stringifies {
            return;
        }
        ctx.report(
            &JSON_RESPONSE,
            call.span,
            "HttpServerResponse.text(JSON.stringify(x)) — use HttpServerResponse.json(x)".to_string(),
        );
    }
}

impl Rule for LoggingSecurity {
    fn metas(&self) -> &'static [&'static RuleMeta] {
        static METAS: &[&RuleMeta] = &[&CONFIG_REDACTED, &STRUCTURED_LOGGING, &JSON_RESPONSE];
        METAS
    }

    fn on_call(&self, call: &CallExpression<'_>, ctx: &mut FileCtx) {
        self.check_config_secret(call, ctx);
        self.check_structured_logging(call, ctx);
        self.check_json_response(call, ctx);
    }
}
