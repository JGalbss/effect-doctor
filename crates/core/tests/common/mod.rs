// Each integration-test binary compiles this module separately and uses a
// different subset of helpers.
#![allow(dead_code)]

use effect_doctor_core::{lint_source, lint_source_with, Diagnostic};

pub fn lint(source: &str) -> Vec<Diagnostic> {
    lint_source("src/example.ts", source, false)
}

pub fn lint_v4(source: &str) -> Vec<Diagnostic> {
    lint_source("src/example.ts", source, true)
}

pub fn lint_adopt(source: &str) -> Vec<Diagnostic> {
    lint_source_with("src/example.ts", source, false, true)
}

#[track_caller]
pub fn assert_fires_adopt(source: &str, rule: &str, times: usize) {
    let diagnostics = lint_adopt(source);
    let found = count(&diagnostics, rule);
    assert_eq!(
        found, times,
        "expected `{rule}` to fire {times}x under --adopt, fired {found}x.\nall: {:#?}\nsource:\n{source}",
        diagnostics
    );
}

pub fn count(diagnostics: &[Diagnostic], rule: &str) -> usize {
    diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.rule == rule)
        .count()
}

#[track_caller]
pub fn assert_fires(source: &str, rule: &str, times: usize) {
    let diagnostics = lint(source);
    let found = count(&diagnostics, rule);
    assert_eq!(
        found, times,
        "expected `{rule}` to fire {times}x, fired {found}x.\nall: {:#?}\nsource:\n{source}",
        diagnostics
    );
}

#[track_caller]
pub fn assert_silent(source: &str, rule: &str) {
    assert_fires(source, rule, 0);
}

#[track_caller]
pub fn assert_fires_v4(source: &str, rule: &str, times: usize) {
    let diagnostics = lint_v4(source);
    let found = count(&diagnostics, rule);
    assert_eq!(
        found, times,
        "expected `{rule}` to fire {times}x under v4 profile, fired {found}x.\nall: {:#?}\nsource:\n{source}",
        diagnostics
    );
}
