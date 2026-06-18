//! Experimental `--agent` mode ("agent doctor") rules: the non-Effect slop
//! patterns LLM agents emit. Every rule is opt-in — silent without `--agent`.

mod common;

use common::{assert_fires_agent, count, lint, lint_agent, lint_agent_strict};
use effect_doctor_core::Severity;

const PRELUDE: &str = "import { Effect, Match } from \"effect\"\n";

fn src(body: &str) -> String {
    format!("{PRELUDE}{body}")
}

#[test]
fn flags_if_else_chain_once_per_chain() {
    let source = src(
        "export const label = (level: string) => {\n  let out\n  if (level === \"error\") {\n    out = \"!\"\n  } else if (level === \"warn\") {\n    out = \"?\"\n  } else {\n    out = \".\"\n  }\n  return out\n}\n",
    );
    // One chain → exactly one if/else finding (not one per else-if link).
    assert_fires_agent(&source, "agent-no-if-else-chain", 1);
}

#[test]
fn bare_if_without_else_is_fine() {
    let source = src(
        "export const f = (x: number) => {\n  if (x > 0) {\n    return x\n  }\n  return 0\n}\n",
    );
    assert_fires_agent(&source, "agent-no-if-else-chain", 0);
}

#[test]
fn flags_ternary() {
    let source =
        src("export const tone = (p: number) => (p >= 100 ? \"critical\" : \"neutral\")\n");
    assert_fires_agent(&source, "agent-no-ternary", 1);
}

#[test]
fn flags_string_equality_guard() {
    let source = src("export const f = (kind: string) => {\n  if (kind === \"user\") {\n    return 1\n  }\n  return 0\n}\n");
    assert_fires_agent(&source, "agent-no-string-equality-guard", 1);
}

#[test]
fn defers_tag_comparison_to_equality_idioms() {
    // `_tag === "Some"` is owned by no-tag-string-comparison, not the agent rule.
    let source = src("export const f = (o: { _tag: string }) => o._tag === \"Some\"\n");
    assert_fires_agent(&source, "agent-no-string-equality-guard", 0);
}

#[test]
fn flags_raw_loops() {
    let source = src("export const f = (rows: number[]) => {\n  for (const r of rows) {\n    sink(r)\n  }\n  while (more()) {\n    step()\n  }\n}\n");
    assert_fires_agent(&source, "agent-no-raw-loop", 2);
}

#[test]
fn flags_let_and_var_not_const() {
    let source = src("export const f = () => {\n  let a = 1\n  var b = 2\n  const c = 3\n  return a + b + c\n}\n");
    assert_fires_agent(&source, "agent-no-let", 2);
}

#[test]
fn flags_reassignment_and_in_place_mutation() {
    let source = src(
        "export const build = (rows: number[]) => {\n  let total = 0\n  const payload = { total: 0 }\n  total = rows.length\n  payload.total = total\n  return payload\n}\n",
    );
    // `total = rows.length` (reassignment) + `payload.total = total` (in-place).
    assert_fires_agent(&source, "agent-no-mutation", 2);
}

#[test]
fn const_binding_without_reassignment_is_not_mutation() {
    let source =
        src("export const f = (n: number) => {\n  const doubled = n * 2\n  return doubled\n}\n");
    assert_fires_agent(&source, "agent-no-mutation", 0);
}

#[test]
fn flags_duplicate_function_bodies() {
    let source = src(
        "export const barFill = (p: number) => {\n  const list = collect(p)\n  for (const x of list) {\n    push(x)\n  }\n  return list.length > 0 ? list[0] : null\n}\n\nexport const textFill = (q: number) => {\n  const list = collect(q)\n  for (const x of list) {\n    push(x)\n  }\n  return list.length > 0 ? list[0] : null\n}\n",
    );
    // Both copies are reported (info severity).
    assert_fires_agent(&source, "agent-duplicate-function", 2);
}

#[test]
fn distinct_functions_are_not_duplicates() {
    let source = src(
        "export const a = (p: number) => {\n  const list = collect(p)\n  for (const x of list) {\n    push(x)\n  }\n  return list.length\n}\n\nexport const b = (q: number) => {\n  return q + 1\n}\n",
    );
    assert_fires_agent(&source, "agent-duplicate-function", 0);
}

#[test]
fn silent_without_agent_flag() {
    let source = src(
        "export const f = (kind: string) => {\n  let out\n  if (kind === \"user\") {\n    out = 1\n  } else {\n    out = 0\n  }\n  return out > 0 ? out : -1\n}\n",
    );
    let diagnostics = lint(&source);
    assert_eq!(count(&diagnostics, "agent-no-if-else-chain"), 0);
    assert_eq!(count(&diagnostics, "agent-no-ternary"), 0);
    assert_eq!(count(&diagnostics, "agent-no-string-equality-guard"), 0);
    assert_eq!(count(&diagnostics, "agent-no-let"), 0);
}

#[test]
fn warn_by_default_error_under_strict() {
    let source = src("export const tone = (p: number) => (p >= 100 ? \"a\" : \"b\")\n");

    let warn = lint_agent(&source);
    let ternary_warn = warn
        .iter()
        .find(|d| d.rule == "agent-no-ternary")
        .expect("ternary finding");
    assert_eq!(ternary_warn.severity, Severity::Warn);

    let strict = lint_agent_strict(&source);
    let ternary_strict = strict
        .iter()
        .find(|d| d.rule == "agent-no-ternary")
        .expect("ternary finding");
    assert_eq!(ternary_strict.severity, Severity::Error);
}

#[test]
fn duplicate_suggestion_stays_info_even_under_strict() {
    let source = src(
        "export const a = (p: number) => {\n  const list = collect(p)\n  for (const x of list) {\n    push(x)\n  }\n  return list.length > 0 ? list[0] : null\n}\n\nexport const b = (q: number) => {\n  const list = collect(q)\n  for (const x of list) {\n    push(x)\n  }\n  return list.length > 0 ? list[0] : null\n}\n",
    );
    let strict = lint_agent_strict(&source);
    let dup = strict
        .iter()
        .find(|d| d.rule == "agent-duplicate-function")
        .expect("duplicate finding");
    // `--agent-strict` escalates warns, but the duplicate suggestion stays info.
    assert_eq!(dup.severity, Severity::Info);
}
