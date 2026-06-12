mod deep;
mod diagnostics;
mod effect_imports;
mod engine;
mod examples;
mod git_scope;
mod matchers;
mod rules;
mod runner;
mod score;
mod walk;

pub use diagnostics::{Category, Diagnostic, FileContext, RuleMeta, Severity};
pub use engine::{detect_effect_major, lint_source, scan, ScanOptions, ScanResult};
pub use examples::{example_for, RuleExample};
pub use git_scope::ScanScope;
pub use rules::{all_metas, RULES};
pub use score::{compute_score, ScoreReport, SCORE_GOOD_THRESHOLD, SCORE_OK_THRESHOLD};
