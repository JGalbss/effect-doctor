use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_span::SourceType;
use rayon::prelude::*;
use serde::Serialize;

use crate::diagnostics::{Diagnostic, FileContext, RawDiagnostic};
use crate::effect_imports::EffectImports;
use crate::git_scope::{collect_diff, resolve_base, DiffInfo, ScanScope};
use crate::runner::Runner;
use crate::score::{compute_score, ScoreReport};
use crate::walk::collect_files;

pub struct ScanOptions {
    pub root: PathBuf,
    /// Run v4-migration rules even when the codebase targets effect v3.
    pub migrate: bool,
    pub scope: ScanScope,
    /// Diff base ref for changed/lines scopes (default: merge-base with main).
    pub base: Option<String>,
    /// Merge type-aware diagnostics from @effect/language-service.
    pub deep: bool,
}

#[derive(Debug, Serialize)]
pub struct ScanResult {
    pub files_scanned: usize,
    pub effect_files: usize,
    pub effect_major: Option<u32>,
    pub v4_rules_active: bool,
    pub scope: &'static str,
    pub changed_files: Option<usize>,
    pub diagnostics: Vec<Diagnostic>,
    pub duration_ms: u64,
    pub score: ScoreReport,
}

fn scope_label(scope: ScanScope) -> &'static str {
    match scope {
        ScanScope::Full => "full",
        ScanScope::ChangedFiles => "changed",
        ScanScope::ChangedLines => "lines",
    }
}

struct ScopeFilter {
    toplevel: PathBuf,
    diff: DiffInfo,
    lines_only: bool,
}

impl ScopeFilter {
    fn relative_path(&self, path: &Path) -> Option<String> {
        let canonical = path.canonicalize().ok()?;
        let relative = canonical.strip_prefix(&self.toplevel).ok()?;
        Some(relative.to_string_lossy().into_owned())
    }

    fn includes_file(&self, path: &Path) -> bool {
        self.relative_path(path)
            .is_some_and(|relative| self.diff.contains_file(&relative))
    }

    fn retain_diagnostics(&self, path: &Path, diagnostics: &mut Vec<Diagnostic>) {
        if !self.lines_only {
            return;
        }
        let Some(relative) = self.relative_path(path) else {
            diagnostics.clear();
            return;
        };
        diagnostics.retain(|diagnostic| self.diff.line_is_changed(&relative, diagnostic.line));
    }
}

fn build_scope_filter(options: &ScanOptions) -> Result<Option<ScopeFilter>, String> {
    if options.scope == ScanScope::Full {
        return Ok(None);
    }
    let toplevel_raw = std::process::Command::new("git")
        .arg("-C")
        .arg(&options.root)
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .map_err(|error| format!("failed to run git: {error}"))?;
    if !toplevel_raw.status.success() {
        return Err("not a git repository — --scope changed/lines needs git".to_string());
    }
    let toplevel = PathBuf::from(String::from_utf8_lossy(&toplevel_raw.stdout).trim());
    let base = resolve_base(&options.root, options.base.as_deref())?;
    let lines_only = options.scope == ScanScope::ChangedLines;
    let diff = collect_diff(&options.root, &base, lines_only)?;
    Ok(Some(ScopeFilter {
        toplevel,
        diff,
        lines_only,
    }))
}

/// Effect major version from the nearest package.json ("effect" in
/// dependencies/devDependencies/peerDependencies), checking the scan root
/// first and then one level of common workspace directories.
fn detect_effect_major(root: &Path) -> Option<u32> {
    if let Some(major) = package_effect_major(&root.join("package.json")) {
        return Some(major);
    }
    for workspace_dir in ["packages", "apps", "libs", "services"] {
        let Ok(entries) = fs::read_dir(root.join(workspace_dir)) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            if let Some(major) = package_effect_major(&entry.path().join("package.json")) {
                return Some(major);
            }
        }
    }
    None
}

fn package_effect_major(path: &Path) -> Option<u32> {
    let text = fs::read_to_string(path).ok()?;
    let manifest: serde_json::Value = serde_json::from_str(&text).ok()?;
    ["dependencies", "devDependencies", "peerDependencies"]
        .iter()
        .find_map(|table| {
            let version = manifest.get(*table)?.get("effect")?.as_str()?;
            parse_major(version)
        })
}

fn parse_major(version: &str) -> Option<u32> {
    let digits: String = version
        .chars()
        .skip_while(|character| !character.is_ascii_digit())
        .take_while(|character| character.is_ascii_digit())
        .collect();
    digits.parse().ok()
}

pub fn scan(options: &ScanOptions) -> Result<ScanResult, String> {
    let started = Instant::now();
    let effect_major = detect_effect_major(&options.root);
    let v4_active = effect_major == Some(4) || options.migrate;
    let scope_filter = build_scope_filter(options)?;
    let mut files = collect_files(&options.root);
    if let Some(filter) = &scope_filter {
        files.retain(|path| filter.includes_file(path));
    }
    let outcomes: Vec<FileOutcome> = files
        .par_iter()
        .filter_map(|path| {
            let mut outcome = process_file(&options.root, path, v4_active)?;
            if let Some(filter) = &scope_filter {
                filter.retain_diagnostics(path, &mut outcome.diagnostics);
            }
            Some(outcome)
        })
        .collect();

    let effect_files = outcomes.iter().filter(|outcome| outcome.has_effect).count();
    let mut diagnostics: Vec<Diagnostic> = outcomes
        .into_iter()
        .flat_map(|outcome| outcome.diagnostics)
        .collect();
    if options.deep {
        let mut deep_diagnostics = crate::deep::run_language_service(&options.root)?;
        if let Some(filter) = &scope_filter {
            deep_diagnostics.retain(|diagnostic| {
                let path = options.root.join(&diagnostic.file);
                filter.includes_file(&path)
                    && (!filter.lines_only || {
                        filter
                            .relative_path(&path)
                            .is_some_and(|relative| {
                                filter.diff.line_is_changed(&relative, diagnostic.line)
                            })
                    })
            });
        }
        diagnostics.extend(deep_diagnostics);
    }
    diagnostics.sort_by(|a, b| {
        (a.severity, a.rule, a.file.as_str(), a.line)
            .cmp(&(b.severity, b.rule, b.file.as_str(), b.line))
    });

    let score = compute_score(&diagnostics);
    Ok(ScanResult {
        files_scanned: files.len(),
        effect_files,
        effect_major,
        v4_rules_active: v4_active,
        scope: scope_label(options.scope),
        changed_files: scope_filter
            .as_ref()
            .map(|filter| filter.diff.files.len()),
        diagnostics,
        duration_ms: started.elapsed().as_millis() as u64,
        score,
    })
}

struct FileOutcome {
    has_effect: bool,
    diagnostics: Vec<Diagnostic>,
}

fn process_file(root: &Path, path: &Path, v4_active: bool) -> Option<FileOutcome> {
    let source = fs::read_to_string(path).ok()?;
    // Fast pre-filter: every rule today requires an effect import; skip the
    // parse entirely for files that cannot mention one.
    if !source.contains("effect") {
        return Some(FileOutcome {
            has_effect: false,
            diagnostics: Vec::new(),
        });
    }
    let display_path = path
        .strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned();
    let diagnostics = lint_source(&display_path, &source, v4_active);
    Some(FileOutcome {
        has_effect: true,
        diagnostics,
    })
}

/// Lint a single source text. Public so tests (and future editor/LSP hosts)
/// can lint snippets without touching the filesystem.
pub fn lint_source(display_path: &str, source: &str, v4_active: bool) -> Vec<Diagnostic> {
    let allocator = Allocator::default();
    let source_type =
        SourceType::from_path(Path::new(display_path)).unwrap_or_else(|_| SourceType::ts());
    let parsed = Parser::new(&allocator, source, source_type).parse();
    if parsed.panicked {
        return Vec::new();
    }
    let imports = EffectImports::from_program(&parsed.program);
    if !imports.has_any() {
        return Vec::new();
    }
    let ctx = Runner::new(imports, v4_active).run(&parsed.program);
    finalize(source, display_path, classify_file(display_path), ctx.raw)
}

/// Test files keep their diagnostics in the report but (mostly) out of the
/// score — deliberate rule-breaking is normal in tests.
pub(crate) fn classify_file(path: &str) -> FileContext {
    let lowered = path.to_ascii_lowercase();
    let is_test_path = lowered
        .split(['/', '\\'])
        .any(|segment| matches!(segment, "test" | "tests" | "__tests__" | "e2e"));
    if is_test_path
        || lowered.contains(".test.")
        || lowered.contains(".spec.")
        || lowered.contains("-test.")
    {
        return FileContext::Test;
    }
    FileContext::Production
}

/// Convert span-based raw diagnostics to line/column + source-line snippets.
fn finalize(
    source: &str,
    display_path: &str,
    file_context: FileContext,
    raw: Vec<RawDiagnostic>,
) -> Vec<Diagnostic> {
    if raw.is_empty() {
        return Vec::new();
    }
    let line_starts: Vec<usize> = std::iter::once(0)
        .chain(
            source
                .bytes()
                .enumerate()
                .filter(|(_, byte)| *byte == b'\n')
                .map(|(offset, _)| offset + 1),
        )
        .collect();

    raw.into_iter()
        .map(|diagnostic| {
            let offset = diagnostic.span.start as usize;
            let line_index = match line_starts.binary_search(&offset) {
                Ok(index) => index,
                Err(index) => index - 1,
            };
            let line_start = line_starts[line_index];
            let line_end = source[line_start..]
                .find('\n')
                .map(|relative| line_start + relative)
                .unwrap_or(source.len());
            Diagnostic {
                rule: diagnostic.meta.id,
                severity: diagnostic.meta.severity,
                category: diagnostic.meta.category,
                message: diagnostic.message,
                help: diagnostic.meta.help,
                file: display_path.to_string(),
                file_context,
                line: (line_index + 1) as u32,
                column: (offset - line_start + 1) as u32,
                snippet: source[line_start..line_end].trim_end().to_string(),
            }
        })
        .collect()
}
