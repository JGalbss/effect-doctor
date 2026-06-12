use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_span::SourceType;
use rayon::prelude::*;
use serde::Serialize;

use crate::diagnostics::{Diagnostic, RawDiagnostic};
use crate::effect_imports::EffectImports;
use crate::runner::Runner;
use crate::score::{compute_score, ScoreReport};
use crate::walk::collect_files;

pub struct ScanOptions {
    pub root: PathBuf,
}

#[derive(Debug, Serialize)]
pub struct ScanResult {
    pub files_scanned: usize,
    pub effect_files: usize,
    pub diagnostics: Vec<Diagnostic>,
    pub duration_ms: u64,
    pub score: ScoreReport,
}

pub fn scan(options: &ScanOptions) -> ScanResult {
    let started = Instant::now();
    let files = collect_files(&options.root);
    let outcomes: Vec<FileOutcome> = files
        .par_iter()
        .filter_map(|path| process_file(&options.root, path))
        .collect();

    let effect_files = outcomes.iter().filter(|outcome| outcome.has_effect).count();
    let mut diagnostics: Vec<Diagnostic> = outcomes
        .into_iter()
        .flat_map(|outcome| outcome.diagnostics)
        .collect();
    diagnostics.sort_by(|a, b| {
        (a.severity, a.rule, a.file.as_str(), a.line)
            .cmp(&(b.severity, b.rule, b.file.as_str(), b.line))
    });

    let score = compute_score(&diagnostics);
    ScanResult {
        files_scanned: files.len(),
        effect_files,
        diagnostics,
        duration_ms: started.elapsed().as_millis() as u64,
        score,
    }
}

struct FileOutcome {
    has_effect: bool,
    diagnostics: Vec<Diagnostic>,
}

fn process_file(root: &Path, path: &Path) -> Option<FileOutcome> {
    let source = fs::read_to_string(path).ok()?;
    // Fast pre-filter: every rule today requires an effect import; skip the
    // parse entirely for files that cannot mention one.
    if !source.contains("effect") {
        return Some(FileOutcome {
            has_effect: false,
            diagnostics: Vec::new(),
        });
    }

    let allocator = Allocator::default();
    let source_type = SourceType::from_path(path).unwrap_or_default();
    let parsed = Parser::new(&allocator, &source, source_type).parse();
    if parsed.panicked {
        return Some(FileOutcome {
            has_effect: false,
            diagnostics: Vec::new(),
        });
    }

    let imports = EffectImports::from_program(&parsed.program);
    if !imports.has_any() {
        return Some(FileOutcome {
            has_effect: false,
            diagnostics: Vec::new(),
        });
    }

    let ctx = Runner::new(imports).run(&parsed.program);
    let display_path = path
        .strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned();
    let diagnostics = finalize(&source, &display_path, ctx.raw);
    Some(FileOutcome {
        has_effect: true,
        diagnostics,
    })
}

/// Convert span-based raw diagnostics to line/column + source-line snippets.
fn finalize(source: &str, display_path: &str, raw: Vec<RawDiagnostic>) -> Vec<Diagnostic> {
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
                line: (line_index + 1) as u32,
                column: (offset - line_start + 1) as u32,
                snippet: source[line_start..line_end].trim_end().to_string(),
            }
        })
        .collect()
}
