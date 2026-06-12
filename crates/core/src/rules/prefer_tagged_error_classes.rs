use oxc_ast::ast::Class;

use crate::diagnostics::{Category, RuleMeta, Severity};
use crate::matchers::ident_name;
use crate::rules::{FileCtx, Rule};

static META: RuleMeta = RuleMeta {
    id: "prefer-tagged-error-classes",
    severity: Severity::Warn,
    category: Category::Idiomatic,
    help: "Plain Error subclasses have no _tag, so Effect.catchTag cannot route them and they serialize poorly. Use Data.TaggedError or Schema.TaggedErrorClass.",
};

pub struct PreferTaggedErrorClasses;

impl Rule for PreferTaggedErrorClasses {
    fn metas(&self) -> &'static [&'static RuleMeta] {
        static METAS: &[&RuleMeta] = &[&META];
        METAS
    }

    fn on_class(&self, class: &Class<'_>, ctx: &mut FileCtx) {
        let Some(superclass) = &class.super_class else {
            return;
        };
        if ident_name(superclass) != Some("Error") {
            return;
        }
        let name = class
            .id
            .as_ref()
            .map(|id| id.name.as_str())
            .unwrap_or("anonymous");
        ctx.report(
            &META,
            class.span,
            format!("class {name} extends Error — use Data.TaggedError / Schema.TaggedErrorClass"),
        );
    }
}
