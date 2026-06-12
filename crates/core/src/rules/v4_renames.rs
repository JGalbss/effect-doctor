use oxc_ast::ast::{Argument, CallExpression, Expression, StaticMemberExpression};

use crate::diagnostics::{Category, RuleMeta, Severity};
use crate::matchers::{call_module_prop, member_module_prop};
use crate::rules::{FileCtx, Rule};

macro_rules! v4_meta {
    ($name:ident, $id:literal, $help:literal) => {
        static $name: RuleMeta = RuleMeta {
            id: $id,
            severity: Severity::Error,
            category: Category::V4Migration,
            help: $help,
        };
    };
}

v4_meta!(CATCH_RENAMES, "v4-catch-renames", "v4 renamed the catch family: catchAll→catch, catchAllCause→catchCause, catchAllDefect→catchDefect, catchSome→catchFilter, catchSomeCause→catchCauseFilter; catchSomeDefect was removed.");
v4_meta!(FORK_RENAMES, "v4-fork-renames", "v4 renamed forking: fork→forkChild, forkDaemon→forkDetach; forkAll and forkWithErrorHandler were removed.");
v4_meta!(CONTEXT_SERVICE, "v4-context-service", "v4 consolidates service definition: Context.Tag / Context.GenericTag / Effect.Tag / Effect.Service all become `class X extends Context.Service<X, Shape>()(\"X\") {}`.");
v4_meta!(CAUSE_FLATTENED, "v4-cause-flattened", "v4 flattens Cause to a reasons array: isFailType→isFailReason, isFailure→hasFails, isDie→hasDies, isInterrupted→hasInterrupts, failureOption→findErrorOption, dieOption→findDefect, sequential/parallel→combine, *Exception→*Error.");
v4_meta!(RUNTIME_REMOVED, "v4-runtime-removed", "v4 removed Runtime<R> and Effect.runtime — use Effect.context<R>() plus Effect.runForkWith(services).");
v4_meta!(SCOPE_PROVIDE, "v4-scope-provide", "v4 renamed Scope.extend to Scope.provide.");
v4_meta!(LAYER_SCOPED, "v4-layer-scoped-to-effect", "v4's Layer.effect handles scoping/finalization itself — Layer.scoped is gone; use Layer.effect.");
v4_meta!(GEN_SELF, "v4-gen-self-options", "v4 changed the self-binding form: Effect.gen(this, function*(){}) becomes Effect.gen({ self: this }, function*(){}).");
v4_meta!(OPTION_RENAMES, "v4-option-renames", "v4 renamed Option.fromNullable to Option.fromNullishOr.");
v4_meta!(SCHEMA_RENAMES, "v4-schema-renames", "v4 reworked Schema's API surface — see the effect-smol MIGRATION.md Schema table for the full mapping.");

struct Rename {
    module: &'static str,
    prop: &'static str,
    meta: &'static RuleMeta,
    hint: &'static str,
}

static RENAMES: &[Rename] = &[
    // Effect.catch*
    Rename { module: "Effect", prop: "catchAll", meta: &CATCH_RENAMES, hint: "Effect.catch" },
    Rename { module: "Effect", prop: "catchAllCause", meta: &CATCH_RENAMES, hint: "Effect.catchCause" },
    Rename { module: "Effect", prop: "catchAllDefect", meta: &CATCH_RENAMES, hint: "Effect.catchDefect" },
    Rename { module: "Effect", prop: "catchSome", meta: &CATCH_RENAMES, hint: "Effect.catchFilter" },
    Rename { module: "Effect", prop: "catchSomeCause", meta: &CATCH_RENAMES, hint: "Effect.catchCauseFilter" },
    Rename { module: "Effect", prop: "catchSomeDefect", meta: &CATCH_RENAMES, hint: "removed in v4" },
    // Effect.fork*
    Rename { module: "Effect", prop: "fork", meta: &FORK_RENAMES, hint: "Effect.forkChild" },
    Rename { module: "Effect", prop: "forkDaemon", meta: &FORK_RENAMES, hint: "Effect.forkDetach" },
    Rename { module: "Effect", prop: "forkAll", meta: &FORK_RENAMES, hint: "removed in v4" },
    Rename { module: "Effect", prop: "forkWithErrorHandler", meta: &FORK_RENAMES, hint: "removed in v4" },
    // services
    Rename { module: "Effect", prop: "Tag", meta: &CONTEXT_SERVICE, hint: "Context.Service" },
    Rename { module: "Effect", prop: "Service", meta: &CONTEXT_SERVICE, hint: "Context.Service" },
    Rename { module: "Context", prop: "Tag", meta: &CONTEXT_SERVICE, hint: "Context.Service" },
    Rename { module: "Context", prop: "GenericTag", meta: &CONTEXT_SERVICE, hint: "Context.Service" },
    // runtime / scope / layer
    Rename { module: "Effect", prop: "runtime", meta: &RUNTIME_REMOVED, hint: "Effect.context + Effect.runForkWith" },
    Rename { module: "Scope", prop: "extend", meta: &SCOPE_PROVIDE, hint: "Scope.provide" },
    Rename { module: "Layer", prop: "scoped", meta: &LAYER_SCOPED, hint: "Layer.effect" },
    // Option
    Rename { module: "Option", prop: "fromNullable", meta: &OPTION_RENAMES, hint: "Option.fromNullishOr" },
    // Cause
    Rename { module: "Cause", prop: "isFailType", meta: &CAUSE_FLATTENED, hint: "Cause.isFailReason" },
    Rename { module: "Cause", prop: "isFailure", meta: &CAUSE_FLATTENED, hint: "Cause.hasFails" },
    Rename { module: "Cause", prop: "isDie", meta: &CAUSE_FLATTENED, hint: "Cause.hasDies" },
    Rename { module: "Cause", prop: "isInterrupted", meta: &CAUSE_FLATTENED, hint: "Cause.hasInterrupts" },
    Rename { module: "Cause", prop: "failureOption", meta: &CAUSE_FLATTENED, hint: "Cause.findErrorOption" },
    Rename { module: "Cause", prop: "dieOption", meta: &CAUSE_FLATTENED, hint: "Cause.findDefect" },
    Rename { module: "Cause", prop: "sequential", meta: &CAUSE_FLATTENED, hint: "Cause.combine" },
    Rename { module: "Cause", prop: "parallel", meta: &CAUSE_FLATTENED, hint: "Cause.combine" },
    Rename { module: "Cause", prop: "NoSuchElementException", meta: &CAUSE_FLATTENED, hint: "Cause.NoSuchElementError" },
    Rename { module: "Cause", prop: "TimeoutException", meta: &CAUSE_FLATTENED, hint: "Cause.TimeoutError" },
    Rename { module: "Cause", prop: "RuntimeException", meta: &CAUSE_FLATTENED, hint: "Cause.RuntimeError" },
    Rename { module: "Cause", prop: "IllegalArgumentException", meta: &CAUSE_FLATTENED, hint: "Cause.IllegalArgumentError" },
    Rename { module: "Cause", prop: "UnknownException", meta: &CAUSE_FLATTENED, hint: "Cause.UnknownError" },
    // Schema simple renames
    Rename { module: "Schema", prop: "TaggedError", meta: &SCHEMA_RENAMES, hint: "Schema.TaggedErrorClass" },
    Rename { module: "Schema", prop: "decodeUnknown", meta: &SCHEMA_RENAMES, hint: "Schema.decodeUnknownEffect" },
    Rename { module: "Schema", prop: "decode", meta: &SCHEMA_RENAMES, hint: "Schema.decodeEffect" },
    Rename { module: "Schema", prop: "decodeUnknownEither", meta: &SCHEMA_RENAMES, hint: "Schema.decodeUnknownExit" },
    Rename { module: "Schema", prop: "encodeEither", meta: &SCHEMA_RENAMES, hint: "Schema.encodeExit" },
    Rename { module: "Schema", prop: "parseJson", meta: &SCHEMA_RENAMES, hint: "Schema.fromJsonString / UnknownFromJsonString" },
    Rename { module: "Schema", prop: "asSchema", meta: &SCHEMA_RENAMES, hint: "Schema.revealCodec" },
    Rename { module: "Schema", prop: "encodedSchema", meta: &SCHEMA_RENAMES, hint: "Schema.toEncoded" },
    Rename { module: "Schema", prop: "typeSchema", meta: &SCHEMA_RENAMES, hint: "Schema.toType" },
    Rename { module: "Schema", prop: "compose", meta: &SCHEMA_RENAMES, hint: "Schema.decodeTo" },
    Rename { module: "Schema", prop: "pick", meta: &SCHEMA_RENAMES, hint: "mapFields(Struct.pick([...]))" },
    Rename { module: "Schema", prop: "omit", meta: &SCHEMA_RENAMES, hint: "mapFields(Struct.omit([...]))" },
    Rename { module: "Schema", prop: "partial", meta: &SCHEMA_RENAMES, hint: "mapFields(Struct.map(Schema.optional))" },
    Rename { module: "Schema", prop: "extend", meta: &SCHEMA_RENAMES, hint: "Schema.fieldsAssign" },
    Rename { module: "Schema", prop: "filter", meta: &SCHEMA_RENAMES, hint: "Schema.check / Schema.refine" },
    Rename { module: "Schema", prop: "transform", meta: &SCHEMA_RENAMES, hint: "Schema.decodeTo + SchemaTransformation" },
    Rename { module: "Schema", prop: "transformOrFail", meta: &SCHEMA_RENAMES, hint: "Schema.decodeTo + SchemaGetter" },
    Rename { module: "Schema", prop: "attachPropertySignature", meta: &SCHEMA_RENAMES, hint: "Schema.tagDefaultOmit" },
    Rename { module: "Schema", prop: "rename", meta: &SCHEMA_RENAMES, hint: "Schema.encodeKeys" },
    Rename { module: "Schema", prop: "keyof", meta: &SCHEMA_RENAMES, hint: "removed in v4" },
    Rename { module: "Schema", prop: "validate", meta: &SCHEMA_RENAMES, hint: "removed — use decode* + toType" },
    Rename { module: "Schema", prop: "validateSync", meta: &SCHEMA_RENAMES, hint: "removed — use decode* + toType" },
    Rename { module: "Schema", prop: "withDefaults", meta: &SCHEMA_RENAMES, hint: "removed in v4" },
    Rename { module: "Schema", prop: "Data", meta: &SCHEMA_RENAMES, hint: "removed — structural equality is the v4 default" },
    Rename { module: "Schema", prop: "BigIntFromSelf", meta: &SCHEMA_RENAMES, hint: "Schema.BigInt" },
    Rename { module: "Schema", prop: "DateFromSelf", meta: &SCHEMA_RENAMES, hint: "Schema.Date" },
    Rename { module: "Schema", prop: "OptionFromSelf", meta: &SCHEMA_RENAMES, hint: "Schema.Option" },
    Rename { module: "Schema", prop: "EitherFromSelf", meta: &SCHEMA_RENAMES, hint: "Schema.Result" },
    Rename { module: "Schema", prop: "Redacted", meta: &SCHEMA_RENAMES, hint: "Schema.RedactedFromValue" },
];

pub struct V4Renames;

impl V4Renames {
    fn check_variadic_shapes(&self, call: &CallExpression<'_>, ctx: &mut FileCtx) {
        let Some((module, prop)) = call_module_prop(call, &ctx.imports) else {
            return;
        };
        if module == "Schema" {
            let first_is_array = matches!(
                call.arguments.first().and_then(Argument::as_expression),
                Some(Expression::ArrayExpression(_))
            );
            if prop == "Literal" && call.arguments.len() > 1 {
                ctx.report(&SCHEMA_RENAMES, call.span, "Schema.Literal(a, b, ...) — v4 uses Schema.Literals([a, b, ...])".to_string());
                return;
            }
            if (prop == "Union" || prop == "Tuple") && call.arguments.len() >= 2 && !first_is_array {
                ctx.report(&SCHEMA_RENAMES, call.span, format!("Schema.{prop}(A, B) — v4 takes an array: Schema.{prop}([A, B])"));
                return;
            }
            if prop == "Record" && call.arguments.len() == 1 {
                let is_object = matches!(
                    call.arguments.first().and_then(Argument::as_expression),
                    Some(Expression::ObjectExpression(_))
                );
                if is_object {
                    ctx.report(&SCHEMA_RENAMES, call.span, "Schema.Record({key, value}) — v4 takes Schema.Record(key, value)".to_string());
                }
            }
            return;
        }
        if module == "Effect" && prop == "gen" {
            let first_is_this = matches!(
                call.arguments.first().and_then(Argument::as_expression),
                Some(Expression::ThisExpression(_))
            );
            if first_is_this {
                ctx.report(&GEN_SELF, call.span, "Effect.gen(this, ...) — v4 uses Effect.gen({ self: this }, ...)".to_string());
            }
        }
    }
}

impl Rule for V4Renames {
    fn on_member(&self, member: &StaticMemberExpression<'_>, ctx: &mut FileCtx) {
        if !ctx.v4_active() {
            return;
        }
        let Some((module, prop)) = member_module_prop(member, &ctx.imports) else {
            return;
        };
        let Some(rename) = RENAMES
            .iter()
            .find(|entry| entry.module == module && entry.prop == prop)
        else {
            return;
        };
        ctx.report(
            rename.meta,
            member.span,
            format!("{module}.{prop} — {}", rename.hint),
        );
    }

    fn on_call(&self, call: &CallExpression<'_>, ctx: &mut FileCtx) {
        if !ctx.v4_active() {
            return;
        }
        self.check_variadic_shapes(call, ctx);
    }
}
