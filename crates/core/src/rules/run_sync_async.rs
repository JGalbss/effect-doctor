use oxc_ast::ast::{Argument, CallExpression};

use crate::diagnostics::{Category, RuleMeta, Severity};
use crate::matchers::{call_module_prop, expression_has_call};
use crate::rules::{FileCtx, Rule};

static META: RuleMeta = RuleMeta {
    id: "no-runsync-on-async-effect",
    severity: Severity::Error,
    category: Category::Correctness,
    help: "Effect.runSync throws AsyncFiberException when the effect suspends (promise, sleep, async). Use Effect.runPromise for anything that can be asynchronous.",
};

fn is_async_constructor(prop: &str) -> bool {
    matches!(
        prop,
        "promise" | "tryPromise" | "async" | "sleep" | "delay" | "never"
    )
}

pub struct NoRunSyncOnAsync;

impl Rule for NoRunSyncOnAsync {
    fn metas(&self) -> &'static [&'static RuleMeta] {
        static METAS: &[&RuleMeta] = &[&META];
        METAS
    }

    fn on_call(&self, call: &CallExpression<'_>, ctx: &mut FileCtx) {
        let Some(("Effect", "runSync" | "runSyncExit")) = call_module_prop(call, &ctx.imports)
        else {
            return;
        };
        let Some(argument) = call.arguments.first().and_then(Argument::as_expression) else {
            return;
        };
        let mut async_api = None;
        expression_has_call(argument, |inner| {
            let Some(("Effect", prop)) = call_module_prop(inner, &ctx.imports) else {
                return false;
            };
            if !is_async_constructor(prop) {
                return false;
            }
            async_api = Some(prop.to_string());
            true
        });
        let Some(async_api) = async_api else {
            return;
        };
        ctx.report(
            &META,
            call.span,
            format!("Effect.runSync over Effect.{async_api} — this throws at runtime; use Effect.runPromise"),
        );
    }
}
