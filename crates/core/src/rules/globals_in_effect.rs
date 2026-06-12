use oxc_ast::ast::{CallExpression, StaticMemberExpression};

use crate::diagnostics::{Category, RuleMeta, Severity};
use crate::matchers::{ident_name, static_member};
use crate::rules::{FileCtx, Rule};

static PREFER_EFFECT_TIMERS: RuleMeta = RuleMeta {
    id: "prefer-effect-timers",
    severity: Severity::Warn,
    category: Category::Idiomatic,
    help: "Raw timers escape Effect's interruption and TestClock. Use Effect.sleep, Schedule, or Effect.repeat.",
};

static PREFER_PLATFORM_FETCH: RuleMeta = RuleMeta {
    id: "prefer-platform-fetch",
    severity: Severity::Warn,
    category: Category::Idiomatic,
    help: "Global fetch has untyped errors and no tracing/interruption integration. Use HttpClient from effect (platform).",
};

static PREFER_CONFIG_MODULE: RuleMeta = RuleMeta {
    id: "prefer-config-module",
    severity: Severity::Warn,
    category: Category::Idiomatic,
    help: "Direct process.env reads are untyped and unvalidated. The Config module gives typed, validated configuration with redaction support.",
};

static PREFER_SCHEMA_OVER_JSON: RuleMeta = RuleMeta {
    id: "prefer-schema-over-json",
    severity: Severity::Info,
    category: Category::Idiomatic,
    help: "Raw JSON.parse returns `any` and throws. At data boundaries use Schema.fromJsonString / UnknownFromJsonString for typed, validated decoding.",
};

fn is_timer(name: &str) -> bool {
    matches!(name, "setTimeout" | "setInterval")
}

pub struct GlobalsInEffect;

impl Rule for GlobalsInEffect {
    fn on_call(&self, call: &CallExpression<'_>, ctx: &mut FileCtx) {
        if !ctx.in_effect_code() {
            return;
        }
        if let Some(name) = ident_name(&call.callee) {
            if is_timer(name) {
                ctx.report(
                    &PREFER_EFFECT_TIMERS,
                    call.span,
                    format!("{name} inside Effect code — use Effect.sleep / Schedule"),
                );
                return;
            }
            if name == "fetch" {
                ctx.report(
                    &PREFER_PLATFORM_FETCH,
                    call.span,
                    "global fetch inside Effect code — use HttpClient".to_string(),
                );
                return;
            }
        }
        if let Some((object, prop)) = static_member(&call.callee) {
            if ident_name(object) == Some("JSON") && (prop == "parse" || prop == "stringify") {
                ctx.report(
                    &PREFER_SCHEMA_OVER_JSON,
                    call.span,
                    format!("JSON.{prop} inside Effect code — use Schema.fromJsonString"),
                );
            }
        }
    }

    fn on_member(&self, member: &StaticMemberExpression<'_>, ctx: &mut FileCtx) {
        if !ctx.in_effect_code() {
            return;
        }
        if ident_name(&member.object) != Some("process") || member.property.name != "env" {
            return;
        }
        ctx.report(
            &PREFER_CONFIG_MODULE,
            member.span,
            "process.env inside Effect code — use the Config module".to_string(),
        );
    }
}
