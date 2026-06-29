//! Type-safety + maintainability rules. These are always-on (no flag), so the
//! fixtures still need an `effect` import to pass the file gate, but the rules
//! fire under plain `lint`. `--agent-strict` escalation is checked separately.

mod common;

use agent_doctor_core::Severity;
use common::{count, lint, lint_agent, lint_agent_strict};

const PRELUDE: &str = "import { Effect } from \"effect\"\n";

fn src(body: &str) -> String {
    format!("{PRELUDE}{body}")
}

#[test]
fn flags_explicit_any() {
    let source = src("export function parse(input: any): number { return 1 }\n");
    assert_eq!(count(&lint(&source), "no-explicit-any"), 1);
}

#[test]
fn flags_non_null_assertion() {
    let source = src("export const f = (xs: number[]) => xs.find((x) => x > 0)!\n");
    assert_eq!(count(&lint(&source), "no-non-null-assertion"), 1);
}

#[test]
fn flags_double_cast_with_or_without_parens() {
    let bare = src("export const a = raw as unknown as number\n");
    assert_eq!(count(&lint(&bare), "no-unsafe-double-cast"), 1);
    let parens = src("export const b = (raw as unknown) as number\n");
    assert_eq!(count(&lint(&parens), "no-unsafe-double-cast"), 1);
}

#[test]
fn single_cast_is_not_flagged() {
    let source = src("export const a = raw as number\n");
    assert_eq!(count(&lint(&source), "no-unsafe-double-cast"), 0);
}

#[test]
fn flags_empty_catch() {
    let source = src("export const f = () => {\n  try { risky() } catch {}\n}\n");
    assert_eq!(count(&lint(&source), "no-empty-catch"), 1);
}

#[test]
fn non_empty_catch_is_fine() {
    let source = src("export const f = () => {\n  try { risky() } catch (e) { log(e) }\n}\n");
    assert_eq!(count(&lint(&source), "no-empty-catch"), 0);
}

#[test]
fn flags_ts_ignore_and_expect_error() {
    let source = src("// @ts-ignore\nexport const a = bad.thing\n// @ts-expect-error\nexport const b = also.bad\n");
    assert_eq!(count(&lint(&source), "no-ts-ignore"), 2);
}

#[test]
fn flags_too_many_parameters() {
    let source =
        src("export function f(a: number, b: number, c: number, d: number, e: number) { return a }\n");
    assert_eq!(count(&lint(&source), "max-function-parameters"), 1);
}

#[test]
fn four_parameters_is_fine() {
    let source = src("export function f(a: number, b: number, c: number, d: number) { return a }\n");
    assert_eq!(count(&lint(&source), "max-function-parameters"), 0);
}

#[test]
fn flags_deep_nesting() {
    let source = src(
        "export function f(xs: number[][][][][]) {\n  for (const a of xs) {\n    for (const b of a) {\n      for (const c of b) {\n        for (const d of c) {\n          for (const e of d) { sink(e) }\n        }\n      }\n    }\n  }\n}\n",
    );
    assert_eq!(count(&lint(&source), "max-nesting-depth"), 1);
}

#[test]
fn flags_high_cognitive_complexity() {
    // a tangle of nested branches + logical operators well past the limit of 15.
    let source = src(
        "export function score(o: any) {\n  let n = 0\n  if (o.a) { if (o.b && o.c) { n++ } else if (o.d) { n++ } }\n  if (o.e || o.f) { for (const x of o.g) { if (x > 0 && x < 9) { n++ } } }\n  while (o.h) { if (o.i) { if (o.j || o.k) { n++ } } }\n  for (const y of o.l) { if (y) { if (y.z) { if (y.z.w) { n++ } } } }\n  return n\n}\n",
    );
    assert!(count(&lint(&source), "high-cognitive-complexity") >= 1);
}

#[test]
fn simple_function_has_no_complexity_finding() {
    let source = src("export const add = (a: number, b: number) => a + b\n");
    assert_eq!(count(&lint(&source), "high-cognitive-complexity"), 0);
    assert_eq!(count(&lint(&source), "max-nesting-depth"), 0);
}

#[test]
fn flags_default_export_under_agent_only() {
    let source = src("const Button = () => null\nexport default Button\n");
    assert_eq!(count(&lint(&source), "agent-no-default-export"), 0);
    assert_eq!(count(&lint_agent(&source), "agent-no-default-export"), 1);
}

#[test]
fn agent_strict_escalates_type_safety_to_error() {
    let source = src("export function parse(input: any): number { return 1 }\n");
    let escalated = lint_agent_strict(&source)
        .into_iter()
        .any(|d| d.rule == "no-explicit-any" && d.severity == Severity::Error);
    assert!(escalated, "--agent-strict should escalate type-safety findings to error");
}
