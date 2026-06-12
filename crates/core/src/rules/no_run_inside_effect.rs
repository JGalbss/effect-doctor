use oxc_ast::ast::CallExpression;

use crate::diagnostics::{Category, RuleMeta, Severity};
use crate::matchers::effect_member_prop;
use crate::rules::{FileCtx, Rule};

static META: RuleMeta = RuleMeta {
    id: "no-run-inside-effect",
    severity: Severity::Error,
    category: Category::Correctness,
    help: "Running an Effect inside another Effect creates a detached runtime: context, interruption, and tracing are lost. Compose with `yield*` instead, and run only at the program edge.",
};

fn is_run_method(prop: &str) -> bool {
    matches!(
        prop,
        "runPromise" | "runSync" | "runFork" | "runPromiseExit" | "runSyncExit" | "runCallback"
    )
}

pub struct NoRunInsideEffect;

impl Rule for NoRunInsideEffect {
    fn meta(&self) -> &'static RuleMeta {
        &META
    }

    fn on_call(&self, call: &CallExpression<'_>, ctx: &mut FileCtx) {
        if !ctx.in_effect_code() {
            return;
        }
        let Some(prop) = effect_member_prop(call, &ctx.imports) else {
            return;
        };
        if !is_run_method(prop) {
            return;
        }
        ctx.report(
            &META,
            call.span,
            format!("Effect.{prop} called inside Effect code — compose with `yield*` instead"),
        );
    }
}
