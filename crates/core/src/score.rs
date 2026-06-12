use std::collections::HashSet;

use serde::Serialize;

use crate::diagnostics::{Diagnostic, FileContext, Severity};

pub const SCORE_GOOD_THRESHOLD: u32 = 75;
pub const SCORE_OK_THRESHOLD: u32 = 50;

#[derive(Debug, Clone, Serialize)]
pub struct ScoreReport {
    pub score: u32,
    pub label: &'static str,
    pub error_rules: usize,
    pub warning_rules: usize,
}

fn label_for(score: u32) -> &'static str {
    if score >= SCORE_GOOD_THRESHOLD {
        return "Great";
    }
    if score >= SCORE_OK_THRESHOLD {
        return "Needs work";
    }
    "Critical"
}

/// Rules that are *about* test code — these count toward the score even when
/// they fire in test files.
fn is_test_rule(rule: &str) -> bool {
    matches!(
        rule,
        "prefer-it-effect" | "no-provide-in-test-bodies" | "prefer-assert-in-effect-tests"
    )
}

fn is_scored(diagnostic: &Diagnostic) -> bool {
    diagnostic.file_context == FileContext::Production || is_test_rule(diagnostic.rule)
}

/// react-doctor's scoring model: penalty per *distinct rule* fired, not per
/// occurrence. Info-severity rules never affect the score, and test-file
/// findings only count for test-specific rules.
pub fn compute_score(diagnostics: &[Diagnostic]) -> ScoreReport {
    let mut error_rules: HashSet<&str> = HashSet::new();
    let mut warning_rules: HashSet<&str> = HashSet::new();
    for diagnostic in diagnostics.iter().filter(|diagnostic| is_scored(diagnostic)) {
        match diagnostic.severity {
            Severity::Error => {
                error_rules.insert(diagnostic.rule);
            }
            Severity::Warn => {
                warning_rules.insert(diagnostic.rule);
            }
            Severity::Info => {}
        }
    }
    let penalty = 1.5 * error_rules.len() as f64 + 0.75 * warning_rules.len() as f64;
    let score = (100.0 - penalty).round().max(0.0) as u32;
    ScoreReport {
        score,
        label: label_for(score),
        error_rules: error_rules.len(),
        warning_rules: warning_rules.len(),
    }
}
