use oxc_ast::ast::{Argument, CallExpression, Expression};

use crate::diagnostics::{Category, RuleMeta, Severity};
use crate::matchers::{call_module_prop, static_member};
use crate::rules::{FileCtx, Rule};

static LONG_CHAINS: RuleMeta = RuleMeta {
    id: "avoid-long-combinator-chains",
    severity: Severity::Info,
    category: Category::Idiomatic,
    help: "Four or more flatMap/andThen steps in one pipe read like callback nesting. Effect.gen turns the same flow into sequential statements.",
};

static MERGEALL_MEGALIST: RuleMeta = RuleMeta {
    id: "no-layer-mergeall-megalist",
    severity: Severity::Info,
    category: Category::Architecture,
    help: "A flat mergeAll of the whole app is hard to navigate and reason about. Compose per-domain modules and merge those.",
};

const CHAIN_THRESHOLD: usize = 4;
const MERGEALL_THRESHOLD: usize = 10;

pub struct CompositionLimits;

impl Rule for CompositionLimits {
    fn metas(&self) -> &'static [&'static RuleMeta] {
        static METAS: &[&RuleMeta] = &[&LONG_CHAINS, &MERGEALL_MEGALIST];
        METAS
    }

    fn on_call(&self, call: &CallExpression<'_>, ctx: &mut FileCtx) {
        if let Some(("Layer", "mergeAll")) = call_module_prop(call, &ctx.imports) {
            if call.arguments.len() > MERGEALL_THRESHOLD {
                ctx.report(
                    &MERGEALL_MEGALIST,
                    call.span,
                    format!(
                        "Layer.mergeAll with {} layers — compose per-domain modules instead",
                        call.arguments.len()
                    ),
                );
            }
            return;
        }

        let Some((_, "pipe")) = static_member(&call.callee) else {
            return;
        };
        let sequencing_steps = call
            .arguments
            .iter()
            .filter_map(Argument::as_expression)
            .filter(|expr| match expr {
                Expression::CallExpression(inner) => matches!(
                    call_module_prop(inner, &ctx.imports),
                    Some(("Effect", "flatMap" | "andThen"))
                ),
                _ => false,
            })
            .count();
        if sequencing_steps < CHAIN_THRESHOLD {
            return;
        }
        ctx.report(
            &LONG_CHAINS,
            call.span,
            format!("{sequencing_steps} flatMap/andThen steps in one pipe — consider Effect.gen"),
        );
    }
}
