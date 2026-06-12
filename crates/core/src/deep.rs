//! `--deep` tier: type-aware diagnostics harvested from
//! `@effect/language-service` (its `diagnostics` CLI runs all ~78 type-aware
//! checks headlessly with JSON output). We never reimplement type analysis —
//! we merge.

use std::path::Path;
use std::process::Command;

use serde::Deserialize;

use crate::diagnostics::{Category, Diagnostic, Severity};

#[derive(Deserialize)]
struct LsReport {
    diagnostics: Vec<LsDiagnostic>,
}

#[derive(Deserialize)]
struct LsDiagnostic {
    file: String,
    line: u32,
    column: u32,
    severity: String,
    name: String,
    message: String,
}

fn map_severity(severity: &str) -> Severity {
    match severity {
        "error" => Severity::Error,
        "warning" => Severity::Warn,
        _ => Severity::Info,
    }
}

/// `Box::leak` for the rule id: deep rule names arrive at runtime but the
/// Diagnostic schema uses &'static str ids. The set is tiny (≤78 names) and
/// lives for the process — leaking is the right call.
fn static_rule_id(name: &str) -> &'static str {
    Box::leak(format!("ls/{name}").into_boxed_str())
}

pub fn run_language_service(root: &Path) -> Result<Vec<Diagnostic>, String> {
    let output = Command::new("npx")
        .current_dir(root)
        .args([
            "--no-install",
            "effect-language-service",
            "diagnostics",
            "--format",
            "json",
        ])
        .output()
        .map_err(|error| format!("failed to run npx: {error}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    // The CLI exits non-zero when it finds errors — only treat empty/unparsable
    // output as failure.
    let report: LsReport = serde_json::from_str(stdout.trim()).map_err(|_| {
        let stderr = String::from_utf8_lossy(&output.stderr);
        format!(
            "could not run @effect/language-service — install it (`npm i -D @effect/language-service`) for --deep. {}",
            stderr.trim()
        )
    })?;

    let root_display = root.to_string_lossy().into_owned();
    Ok(report
        .diagnostics
        .into_iter()
        .map(|diagnostic| {
            let file = diagnostic
                .file
                .strip_prefix(&root_display)
                .map(|stripped| stripped.trim_start_matches('/').to_string())
                .unwrap_or(diagnostic.file);
            let file_context = crate::engine::classify_file(&file);
            Diagnostic {
                rule: static_rule_id(&diagnostic.name),
                severity: map_severity(&diagnostic.severity),
                category: Category::TypeAware,
                message: diagnostic.message,
                help: "Type-aware diagnostic from @effect/language-service.",
                file,
                file_context,
                line: diagnostic.line,
                column: diagnostic.column,
                snippet: String::new(),
            }
        })
        .collect())
}
