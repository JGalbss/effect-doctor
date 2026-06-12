use oxc_ast::ast::StaticMemberExpression;

use crate::diagnostics::{Category, RuleMeta, Severity};
use crate::matchers::member_module_prop;
use crate::rules::{FileCtx, Rule};

static META: RuleMeta = RuleMeta {
    id: "no-orDie-to-silence-errors",
    severity: Severity::Info,
    category: Category::Correctness,
    help: "orDie converts every failure into an unrecoverable defect. Fine when failure truly is impossible — but if the error is expected (config, validation, IO), handle it with catchTag instead.",
};

fn is_or_die_module(module: &str) -> bool {
    matches!(module, "Effect" | "Layer")
}

pub struct NoOrDie;

impl Rule for NoOrDie {
    fn metas(&self) -> &'static [&'static RuleMeta] {
        static METAS: &[&RuleMeta] = &[&META];
        METAS
    }

    fn on_member(&self, member: &StaticMemberExpression<'_>, ctx: &mut FileCtx) {
        let Some((module, "orDie")) = member_module_prop(member, &ctx.imports) else {
            return;
        };
        if !is_or_die_module(module) {
            return;
        }
        ctx.report(
            &META,
            member.span,
            format!("{module}.orDie — make sure this failure is truly unrecoverable"),
        );
    }
}
