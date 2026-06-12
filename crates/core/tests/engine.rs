mod common;

use effect_doctor_core::{compute_score, lint_source, FileContext, Severity};

#[test]
fn files_without_effect_imports_are_skipped() {
    let diagnostics = lint_source(
        "src/example.ts",
        r#"
const Effect = { gen: (f: unknown) => f }
const program = Effect.gen(function* () {
  const value = yield somePromise
  return value
})
declare const somePromise: Promise<number>
"#,
        false,
    );
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
}

#[test]
fn test_files_are_classified() {
    let source = r#"
import { Effect } from "effect"
const program = Effect.gen(function* () {
  Effect.runPromise(Effect.succeed(1))
})
"#;
    let in_test = lint_source("test/runtime.test.ts", source, false);
    assert!(!in_test.is_empty());
    assert!(in_test
        .iter()
        .all(|diagnostic| diagnostic.file_context == FileContext::Test));

    let in_production = lint_source("src/runtime.ts", source, false);
    assert!(in_production
        .iter()
        .all(|diagnostic| diagnostic.file_context == FileContext::Production));
}

#[test]
fn test_file_findings_do_not_affect_score() {
    let source = r#"
import { Effect } from "effect"
const program = Effect.gen(function* () {
  const value = yield Effect.succeed(1)
  return value
})
"#;
    let production = lint_source("src/app.ts", source, false);
    let test = lint_source("src/app.test.ts", source, false);
    assert!(compute_score(&production).score < 100);
    assert_eq!(compute_score(&test).score, 100);
}

#[test]
fn score_counts_distinct_rules_not_occurrences() {
    let many_hits_one_rule = lint_source(
        "src/app.ts",
        r#"
import { Effect } from "effect"
const program = Effect.gen(function* () {
  const a = yield Effect.succeed(1)
  const b = yield Effect.succeed(2)
  const c = yield Effect.succeed(3)
  return a + b + c
})
"#,
        false,
    );
    assert_eq!(common::count(&many_hits_one_rule, "require-yield-star"), 3);
    let report = compute_score(&many_hits_one_rule);
    assert_eq!(report.error_rules, 1);
    assert_eq!(report.score, 99); // 100 - 1.5 rounded
}

#[test]
fn info_rules_never_affect_score() {
    let diagnostics = lint_source(
        "src/app.ts",
        r#"
import { Effect } from "effect"
export const done = Effect.succeed(undefined)
"#,
        false,
    );
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == Severity::Info));
    assert_eq!(compute_score(&diagnostics).score, 100);
}

#[test]
fn diagnostics_carry_position_and_snippet() {
    let diagnostics = lint_source(
        "src/app.ts",
        "import { Effect } from \"effect\"\nconst p = Effect.gen(function* () {\n  const a = yield Effect.succeed(1)\n  return a\n})\n",
        false,
    );
    let yield_diagnostic = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.rule == "require-yield-star")
        .expect("require-yield-star fires");
    assert_eq!(yield_diagnostic.line, 3);
    assert!(yield_diagnostic.snippet.contains("yield Effect.succeed(1)"));
}
