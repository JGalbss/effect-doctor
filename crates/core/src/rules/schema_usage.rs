use oxc_ast::ast::{CallExpression, Expression};

use crate::diagnostics::{Category, RuleMeta, Severity};
use crate::matchers::call_module_prop;
use crate::rules::{FileCtx, Rule};

static PREFER_DECODE_EFFECT: RuleMeta = RuleMeta {
    id: "prefer-decode-effect",
    severity: Severity::Warn,
    category: Category::Idiomatic,
    help: "Sync decoding throws inside Effect code, bypassing the typed error channel. Use Schema.decodeUnknownEffect so failures land in the error channel as SchemaError.",
};

static HOIST_SCHEMA_CODECS: RuleMeta = RuleMeta {
    id: "hoist-schema-codecs",
    severity: Severity::Info,
    category: Category::Performance,
    help: "Schema.decode*/encode* compiles a codec — building it on every call is wasted work. Hoist `const decode = Schema.decodeUnknownEffect(MySchema)` to module scope.",
};

fn is_sync_decode(prop: &str) -> bool {
    matches!(
        prop,
        "decodeUnknownSync" | "decodeSync" | "encodeSync" | "encodeUnknownSync"
    )
}

fn is_codec_factory(prop: &str) -> bool {
    prop.starts_with("decode") || prop.starts_with("encode")
}

pub struct SchemaUsage;

impl Rule for SchemaUsage {
    fn on_call(&self, call: &CallExpression<'_>, ctx: &mut FileCtx) {
        if let Some(("Schema", prop)) = call_module_prop(call, &ctx.imports) {
            if is_sync_decode(prop) && ctx.in_effect_code() {
                ctx.report(
                    &PREFER_DECODE_EFFECT,
                    call.span,
                    format!("Schema.{prop} inside Effect code — use Schema.decodeUnknownEffect"),
                );
            }
            return;
        }
        // Immediate double application: Schema.decodeUnknownEffect(S)(input)
        // builds the codec per call instead of reusing a hoisted one.
        let Expression::CallExpression(inner) = &call.callee else {
            return;
        };
        let Some(("Schema", prop)) = call_module_prop(inner, &ctx.imports) else {
            return;
        };
        if !is_codec_factory(prop) {
            return;
        }
        ctx.report(
            &HOIST_SCHEMA_CODECS,
            call.span,
            format!("Schema.{prop}(schema)(input) builds the codec on every call — hoist it to module scope"),
        );
    }
}
