//! Single-source linting — the platform-independent heart of the engine,
//! shared by the CLI scan pipeline, the LSP, tests, and the WASM playground.

use std::path::Path;

use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_span::SourceType;

use crate::diagnostics::{Diagnostic, FileContext, RawDiagnostic};
use crate::effect_imports::EffectImports;
use crate::fn_index::{self, FunctionEntry};
use crate::runner::Runner;
use crate::text::LineIndex;

/// One file's full analysis: its diagnostics plus, under `--agent`, the indexed
/// functions for the engine's cross-file "this already exists" pass.
pub(crate) struct FileAnalysis {
    pub diagnostics: Vec<Diagnostic>,
    pub functions: Vec<FunctionEntry>,
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

/// Toggles for the optional rule tiers layered on top of the always-on
/// syntactic rules. `Default` is the plain production scan.
#[derive(Debug, Clone, Copy, Default)]
pub struct LintOptions {
    /// Run v4-migration rules even on an effect v3 codebase.
    pub v4_active: bool,
    /// Experimental: vanilla-TS → Effect adoption recommendations.
    pub adopt: bool,
    /// Experimental: agent-hygiene rules (if/else, ternary, raw loops, …).
    pub agent: bool,
    /// Escalate agent-hygiene findings from `warn` to `error` (implies `agent`).
    pub agent_strict: bool,
}

/// Lint a single source text. Public so tests and the LSP host can lint
/// snippets without touching the filesystem.
pub fn lint_source(display_path: &str, source: &str, v4_active: bool) -> Vec<Diagnostic> {
    lint_source_opts(
        display_path,
        source,
        LintOptions {
            v4_active,
            ..LintOptions::default()
        },
    )
}

/// `lint_source` with the experimental adoption rules toggled.
pub fn lint_source_with(
    display_path: &str,
    source: &str,
    v4_active: bool,
    adopt: bool,
) -> Vec<Diagnostic> {
    lint_source_opts(
        display_path,
        source,
        LintOptions {
            v4_active,
            adopt,
            ..LintOptions::default()
        },
    )
}

/// Lint with the full set of optional tiers selected via [`LintOptions`].
pub fn lint_source_opts(display_path: &str, source: &str, options: LintOptions) -> Vec<Diagnostic> {
    analyze_source(display_path, source, options).diagnostics
}

/// Parse a file once and produce both its diagnostics and (under `--agent`) its
/// function index. The engine uses the index for cross-file duplicate findings;
/// every other caller takes `.diagnostics` via [`lint_source_opts`].
pub(crate) fn analyze_source(
    display_path: &str,
    source: &str,
    options: LintOptions,
) -> FileAnalysis {
    let allocator = Allocator::default();
    let source_type =
        SourceType::from_path(Path::new(display_path)).unwrap_or_else(|_| SourceType::ts());
    let parsed = Parser::new(&allocator, source, source_type).parse();
    if parsed.panicked {
        return FileAnalysis {
            diagnostics: Vec::new(),
            functions: Vec::new(),
        };
    }
    let imports = EffectImports::from_program(&parsed.program);
    if !imports.has_any() {
        return FileAnalysis {
            diagnostics: Vec::new(),
            functions: Vec::new(),
        };
    }
    let agent_active = options.agent || options.agent_strict;
    let functions = match agent_active {
        true => fn_index::collect_from_program(&parsed.program, source, display_path),
        false => Vec::new(),
    };
    let mut ctx = Runner::new(
        imports,
        options.v4_active,
        options.adopt,
        agent_active,
        options.agent_strict,
    )
    .run(&parsed.program);
    // `@ts-ignore` / `@ts-expect-error` live in comments, not the AST — scan them
    // from the parsed comment list and fold the findings into the file's set.
    ctx.raw
        .extend(crate::rules::ts_safety::ts_ignore_findings(&parsed.program, source));
    let diagnostics = finalize(source, display_path, classify_file(display_path), ctx.raw);
    FileAnalysis {
        diagnostics,
        functions,
    }
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
    let lines = LineIndex::new(source);

    raw.into_iter()
        .map(|diagnostic| {
            let offset = diagnostic.span.start as usize;
            let (line, column) = lines.line_col(offset);
            Diagnostic {
                rule: diagnostic.meta.id,
                severity: diagnostic.severity.unwrap_or(diagnostic.meta.severity),
                category: diagnostic.meta.category,
                message: diagnostic.message,
                help: diagnostic.meta.help,
                file: display_path.to_string(),
                file_context,
                line,
                column,
                snippet: lines.line_text(source, offset).to_string(),
            }
        })
        .collect()
}
