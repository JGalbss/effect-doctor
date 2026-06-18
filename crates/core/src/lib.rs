#[cfg(feature = "native")]
mod deep;
mod diagnostics;
mod effect_imports;
#[cfg(feature = "native")]
mod engine;
mod examples;
#[cfg(feature = "native")]
mod git_scope;
mod lint;
mod matchers;
mod rules;
mod runner;
mod score;
#[cfg(feature = "native")]
mod walk;

pub use diagnostics::{Category, Diagnostic, FileContext, RuleMeta, Severity};
#[cfg(feature = "native")]
pub use engine::{detect_effect_major, scan, ScanOptions, ScanResult};
pub use examples::{example_for, RuleExample};
#[cfg(feature = "native")]
pub use git_scope::ScanScope;
pub use lint::{lint_source, lint_source_opts, lint_source_with, LintOptions};
pub use rules::{all_metas, RULES};
pub use score::{compute_score, ScoreReport, SCORE_GOOD_THRESHOLD, SCORE_OK_THRESHOLD};
