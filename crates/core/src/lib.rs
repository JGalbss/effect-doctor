mod content_addr;
mod cycles;
#[cfg(feature = "native")]
mod deep;
mod diagnostics;
mod effect_imports;
#[cfg(feature = "native")]
mod engine;
mod examples;
mod fn_index;
#[cfg(feature = "native")]
mod git_scope;
#[cfg(feature = "native")]
mod index;
mod lint;
mod matchers;
mod rules;
mod runner;
mod score;
mod single_use;
mod structural;
mod symbol_graph;
mod text;
mod ts_directives;
#[cfg(feature = "native")]
mod walk;

pub use content_addr::{fnv1a, ContentHash, FileId};
pub use diagnostics::{Category, Diagnostic, FileContext, RuleMeta, Severity};
#[cfg(feature = "native")]
pub use engine::{detect_effect_major, scan, ScanOptions, ScanResult};
pub use examples::{example_for, RuleExample};
#[cfg(feature = "native")]
pub use git_scope::{collect_diff, resolve_base, DiffInfo, ScanScope};
#[cfg(feature = "native")]
pub use index::Index;
pub use lint::is_test_file;
pub use lint::{lint_source, lint_source_opts, lint_source_with, LintOptions};
pub use rules::{all_metas, RULES};
pub use score::{compute_score, ScoreReport, SCORE_GOOD_THRESHOLD, SCORE_OK_THRESHOLD};
pub use symbol_graph::{FileSymbols, ImportEdge, ResolvedEdge, SymbolDef, SymbolGraph, SymbolKind};
