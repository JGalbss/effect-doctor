mod diagnostics;
mod effect_imports;
mod engine;
mod matchers;
mod rules;
mod runner;
mod score;
mod walk;

pub use diagnostics::{Category, Diagnostic, FileContext, Severity};
pub use engine::{lint_source, scan, ScanOptions, ScanResult};
pub use rules::RULES;
pub use score::{compute_score, ScoreReport, SCORE_GOOD_THRESHOLD, SCORE_OK_THRESHOLD};
