use oxc_ast::ast::{ImportDeclaration, ImportDeclarationSpecifier, ModuleExportName};

use crate::diagnostics::{Category, RuleMeta, Severity};
use crate::rules::{FileCtx, Rule};

static FIBERREF_REMOVED: RuleMeta = RuleMeta {
    id: "v4-fiberref-removed",
    severity: Severity::Error,
    category: Category::V4Migration,
    help: "v4 removed FiberRef/FiberRefs/Differ — use Context.Reference (ambient config with defaults) or References.* / Effect.provideService.",
};

static PACKAGE_CONSOLIDATION: RuleMeta = RuleMeta {
    id: "v4-package-consolidation",
    severity: Severity::Error,
    category: Category::V4Migration,
    help: "v4 merged these packages into effect itself: @effect/platform → effect (and effect/unstable/http), @effect/rpc → effect/unstable/rpc, @effect/cluster → effect/unstable/cluster.",
};

static UNSTABLE_IMPORTS: RuleMeta = RuleMeta {
    id: "v4-unstable-import-awareness",
    severity: Severity::Info,
    category: Category::V4Migration,
    help: "effect/unstable/* APIs may break in minor releases — fine to use, worth tracking.",
};

fn is_removed_fiber_module(name: &str) -> bool {
    matches!(name, "FiberRef" | "FiberRefs" | "FiberRefsPatch" | "Differ")
}

fn consolidated_target(source: &str) -> Option<&'static str> {
    match source {
        "@effect/platform" => Some("effect / effect/unstable/http"),
        "@effect/rpc" => Some("effect/unstable/rpc"),
        "@effect/cluster" => Some("effect/unstable/cluster"),
        _ => None,
    }
}

fn imported_name<'b>(specifier: &'b ImportDeclarationSpecifier) -> Option<&'b str> {
    let ImportDeclarationSpecifier::ImportSpecifier(named) = specifier else {
        return None;
    };
    match &named.imported {
        ModuleExportName::IdentifierName(identifier) => Some(identifier.name.as_str()),
        ModuleExportName::IdentifierReference(reference) => Some(reference.name.as_str()),
        ModuleExportName::StringLiteral(literal) => Some(literal.value.as_str()),
    }
}

pub struct V4Imports;

impl Rule for V4Imports {
    fn metas(&self) -> &'static [&'static RuleMeta] {
        static METAS: &[&RuleMeta] = &[&FIBERREF_REMOVED, &PACKAGE_CONSOLIDATION, &UNSTABLE_IMPORTS];
        METAS
    }

    fn on_import(&self, import: &ImportDeclaration<'_>, ctx: &mut FileCtx) {
        if !ctx.v4_active() {
            return;
        }
        let source = import.source.value.as_str();

        if let Some(target) = consolidated_target(source) {
            ctx.report(
                &PACKAGE_CONSOLIDATION,
                import.span,
                format!("\"{source}\" was merged into {target}"),
            );
            return;
        }

        if source.starts_with("effect/unstable/") {
            ctx.report(
                &UNSTABLE_IMPORTS,
                import.span,
                format!("\"{source}\" is an unstable API surface"),
            );
            return;
        }

        if source == "effect" {
            let Some(specifiers) = &import.specifiers else {
                return;
            };
            for specifier in specifiers {
                let Some(name) = imported_name(specifier) else {
                    continue;
                };
                if !is_removed_fiber_module(name) {
                    continue;
                }
                ctx.report(
                    &FIBERREF_REMOVED,
                    import.span,
                    format!("{name} was removed in v4 — use Context.Reference"),
                );
            }
            return;
        }

        if let Some(module) = source.strip_prefix("effect/") {
            if is_removed_fiber_module(module) {
                ctx.report(
                    &FIBERREF_REMOVED,
                    import.span,
                    format!("{module} was removed in v4 — use Context.Reference"),
                );
            }
        }
    }
}
