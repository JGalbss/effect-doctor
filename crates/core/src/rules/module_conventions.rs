//! Module conventions. Opinionated (some ecosystems require default exports —
//! React components, Next.js pages), so gated under `--agent`.

use oxc_ast::ast::ExportDefaultDeclaration;

use crate::diagnostics::{Category, RuleMeta, Severity};
use crate::rules::{FileCtx, Rule};

static NO_DEFAULT_EXPORT: RuleMeta = RuleMeta {
    id: "agent-no-default-export",
    severity: Severity::Warn,
    category: Category::AgentHygiene,
    help: "Default exports rename freely at each import site (so the same thing gets many names) and resist auto-import/refactor. Use a named export so the identity is stable across the codebase.",
};

pub struct NoDefaultExport;

impl Rule for NoDefaultExport {
    fn metas(&self) -> &'static [&'static RuleMeta] {
        static METAS: &[&RuleMeta] = &[&NO_DEFAULT_EXPORT];
        METAS
    }

    fn on_export_default(&self, export: &ExportDefaultDeclaration<'_>, ctx: &mut FileCtx) {
        if !ctx.agent_active() {
            return;
        }
        ctx.report_agent(
            &NO_DEFAULT_EXPORT,
            export.span,
            "default export — prefer a named export for stable identity across imports".to_string(),
        );
    }
}
