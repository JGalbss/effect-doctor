use oxc_ast::ast::{Argument, CallExpression, Expression};

use crate::diagnostics::{Category, RuleMeta, Severity};
use crate::matchers::{call_module_prop, static_member};
use crate::rules::{FileCtx, Rule};

static META: RuleMeta = RuleMeta {
    id: "no-chained-provides",
    severity: Severity::Warn,
    category: Category::Architecture,
    help: "Multiple Effect.provide calls in one pipe build layers independently (v3 even double-builds shared ones). Compose layers first (Layer.provide / Layer.mergeAll), then provide once.",
};

pub struct NoChainedProvides;

impl Rule for NoChainedProvides {
    fn metas(&self) -> &'static [&'static RuleMeta] {
        static METAS: &[&RuleMeta] = &[&META];
        METAS
    }

    fn on_call(&self, call: &CallExpression<'_>, ctx: &mut FileCtx) {
        let Some((_, "pipe")) = static_member(&call.callee) else {
            return;
        };
        let provide_count = call
            .arguments
            .iter()
            .filter_map(Argument::as_expression)
            .filter(|expr| match expr {
                Expression::CallExpression(inner) => {
                    call_module_prop(inner, &ctx.imports) == Some(("Effect", "provide"))
                }
                _ => false,
            })
            .count();
        if provide_count < 2 {
            return;
        }
        ctx.report(
            &META,
            call.span,
            format!("{provide_count} Effect.provide calls in one pipe — compose the layers and provide once"),
        );
    }
}
