use oxc_ast::ast::ImportDeclaration;

use crate::diagnostics::{Category, RuleMeta, Severity};
use crate::rules::{FileCtx, Rule};

static META: RuleMeta = RuleMeta {
    id: "prefer-node-effect-counterparts",
    severity: Severity::Info,
    category: Category::Idiomatic,
    help: "Effect's platform services (FileSystem, Path, Command) wrap these with typed errors, interruption, and test layers.",
};

fn platform_counterpart(source: &str) -> Option<&'static str> {
    match source {
        "node:fs" | "fs" | "node:fs/promises" | "fs/promises" => Some("FileSystem"),
        "node:path" | "path" => Some("Path"),
        "node:child_process" | "child_process" => Some("Command"),
        _ => None,
    }
}

pub struct PreferNodeCounterparts;

impl Rule for PreferNodeCounterparts {
    fn on_import(&self, import: &ImportDeclaration<'_>, ctx: &mut FileCtx) {
        let Some(service) = platform_counterpart(import.source.value.as_str()) else {
            return;
        };
        ctx.report(
            &META,
            import.span,
            format!(
                "import from \"{}\" in an Effect file — consider the {service} platform service",
                import.source.value
            ),
        );
    }
}
