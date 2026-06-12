//! Interruption-quality rules: patterns from real Effect codebases where
//! cancellation silently degrades.

use oxc_ast::ast::{Argument, CallExpression, Expression, ObjectPropertyKind, PropertyKey};

use crate::diagnostics::{Category, RuleMeta, Severity};
use crate::matchers::{call_module_prop, expression_has_call, ident_name};
use crate::rules::{FileCtx, Rule};

static ABORT_SIGNAL: RuleMeta = RuleMeta {
    id: "prefer-abort-signal-passthrough",
    severity: Severity::Info,
    category: Category::Idiomatic,
    help: "Promises can't be interrupted — but Effect hands tryPromise an AbortSignal. Pass it to signal-aware APIs (fetch, SDKs) so interruption actually cancels the work.",
};

static NESTED_FLATMAP: RuleMeta = RuleMeta {
    id: "prefer-gen-over-nested-flatmap",
    severity: Severity::Info,
    category: Category::Idiomatic,
    help: "flatMap inside flatMap nests like callbacks. Effect.gen turns the same sequencing into flat, readable statements: const a = yield* ...; const b = yield* ...",
};

fn function_param_count(expr: &Expression) -> Option<usize> {
    match expr {
        Expression::ArrowFunctionExpression(arrow) => Some(arrow.params.items.len()),
        Expression::FunctionExpression(function) => Some(function.params.items.len()),
        _ => None,
    }
}

fn try_property<'a, 'b>(call: &'b CallExpression<'a>) -> Option<&'b Expression<'a>> {
    match call.arguments.first().and_then(Argument::as_expression)? {
        Expression::ObjectExpression(object) => object.properties.iter().find_map(|property| {
            let ObjectPropertyKind::ObjectProperty(entry) = property else {
                return None;
            };
            let PropertyKey::StaticIdentifier(key) = &entry.key else {
                return None;
            };
            if key.name != "try" {
                return None;
            }
            Some(&entry.value)
        }),
        function @ (Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_)) => {
            Some(function)
        }
        _ => None,
    }
}

fn contains_signal_aware_call(handler: &Expression) -> bool {
    expression_has_call(handler, |inner| ident_name(&inner.callee) == Some("fetch"))
}

pub struct Interruption;

impl Rule for Interruption {
    fn metas(&self) -> &'static [&'static RuleMeta] {
        static METAS: &[&RuleMeta] = &[&ABORT_SIGNAL, &NESTED_FLATMAP];
        METAS
    }

    fn on_call(&self, call: &CallExpression<'_>, ctx: &mut FileCtx) {
        let Some(("Effect", prop)) = call_module_prop(call, &ctx.imports) else {
            return;
        };

        if prop == "tryPromise" || prop == "promise" {
            let Some(handler) = try_property(call) else {
                return;
            };
            // Callback ignores the AbortSignal parameter AND calls a
            // signal-aware API — interruption won't reach the request.
            if function_param_count(handler) == Some(0) && contains_signal_aware_call(handler) {
                ctx.report(
                    &ABORT_SIGNAL,
                    call.span,
                    format!("Effect.{prop} ignores the AbortSignal — use `({{ signal }}) form: try: (signal) => fetch(url, {{ signal }})`"),
                );
            }
            return;
        }

        if prop == "flatMap" || prop == "andThen" {
            let Some(handler) = call
                .arguments
                .iter()
                .filter_map(Argument::as_expression)
                .find(|expr| {
                    matches!(
                        expr,
                        Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_)
                    )
                })
            else {
                return;
            };
            let nests_sequencing = expression_has_call(handler, |inner| {
                matches!(
                    call_module_prop(inner, &ctx.imports),
                    Some(("Effect", "flatMap" | "andThen"))
                )
            });
            if !nests_sequencing {
                return;
            }
            ctx.report(
                &NESTED_FLATMAP,
                call.span,
                format!("Effect.{prop} nested inside Effect.{prop} — Effect.gen reads flat"),
            );
        }
    }
}
