use oxc_ast::ast::StaticMemberExpression;

use crate::diagnostics::{Category, RuleMeta, Severity};
use crate::matchers::member_module_prop;
use crate::rules::{FileCtx, Rule};

static META: RuleMeta = RuleMeta {
    id: "no-effect-do-notation",
    severity: Severity::Info,
    category: Category::Idiomatic,
    help: "Do-notation pipelines (Effect.Do / bind / bindTo / let) read worse than the equivalent Effect.gen and lose stack quality.",
};

pub struct NoEffectDo;

impl Rule for NoEffectDo {
    fn metas(&self) -> &'static [&'static RuleMeta] {
        static METAS: &[&RuleMeta] = &[&META];
        METAS
    }

    fn on_member(&self, member: &StaticMemberExpression<'_>, ctx: &mut FileCtx) {
        let Some(("Effect", "Do")) = member_module_prop(member, &ctx.imports) else {
            return;
        };
        ctx.report(
            &META,
            member.span,
            "Effect.Do pipeline — prefer Effect.gen".to_string(),
        );
    }
}
