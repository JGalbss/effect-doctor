//! Experimental `--agent` mode ("agent doctor") rules: the non-Effect slop
//! patterns LLM agents emit. Every rule is opt-in — silent without `--agent`.

mod common;

use agent_doctor_core::Severity;
use common::{assert_fires_agent, count, lint, lint_agent, lint_agent_strict};

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
fn flags_inline_dynamic_import_and_require() {
    let source = src(
        "export const load = async (path: string) => {\n  const mod = await import(\"./parser\")\n  const fs = require(\"node:fs\")\n  return mod.parse(fs.readFileSync(path))\n}\n",
    );
    // dynamic import() + require() → two inline-import findings.
    assert_fires_agent(&source, "agent-no-inline-import", 2);
}

#[test]
fn top_level_static_import_is_fine() {
    let source = src("export const f = (n: number) => n + 1\n");
    assert_fires_agent(&source, "agent-no-inline-import", 0);
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

// ─── opencode / Rogo style-guide mined rules ───

#[test]
fn flags_any_type() {
    let source = src("export const parse = (input: any): any => JSON.parse(input)\n");
    // `: any` (param) + `: any` (return) → two findings.
    assert_fires_agent(&source, "agent-no-any", 2);
}

#[test]
fn flags_as_any_too() {
    let source = src("export const f = (x: unknown) => (x as any).foo\n");
    assert_fires_agent(&source, "agent-no-any", 1);
}

#[test]
fn flags_import_alias_but_not_effect() {
    let source = src("import { UsersService as Users } from \"./users\"\nexport const u = Users\n");
    assert_fires_agent(&source, "agent-no-import-alias", 1);
    // The PRELUDE's `import { Effect, Match } from "effect"` is not aliased and exempt anyway.
    assert_fires_agent(&source, "agent-no-namespace-import", 0);
}

#[test]
fn flags_namespace_import_but_exempts_effect() {
    let source = format!(
        "{PRELUDE}import * as utils from \"./utils\"\nimport * as NodeFs from \"effect/platform/FileSystem\"\nexport const x = utils.a\n"
    );
    // Only the non-effect star import fires.
    assert_fires_agent(&source, "agent-no-namespace-import", 1);
}

#[test]
fn flags_try_catch_outside_gen() {
    let source = src("export const f = () => {\n  try {\n    return risky()\n  } catch (e) {\n    return null\n  }\n}\n");
    assert_fires_agent(&source, "agent-no-try-catch", 1);
}

#[test]
fn defers_try_catch_inside_effect_gen() {
    let source = src("export const f = Effect.gen(function* () {\n  try {\n    return yield* risky()\n  } catch (e) {\n    return null\n  }\n})\n");
    // no-try-catch-in-gen owns this; the agent rule defers.
    assert_fires_agent(&source, "agent-no-try-catch", 0);
}

#[test]
fn flags_default_export() {
    let source = src("const value = 1\nexport default value\n");
    assert_fires_agent(&source, "agent-no-default-export", 1);
}

#[test]
fn flags_as_cast_but_not_as_const() {
    let cast = src("export const u = rows[0] as User\n");
    assert_fires_agent(&cast, "agent-no-as-cast", 1);
    let as_const = src("export const dirs = [\"up\", \"down\"] as const\n");
    assert_fires_agent(&as_const, "agent-no-as-cast", 0);
}

#[test]
fn flags_unbounded_promise_all_over_map() {
    let fanned = src("export const run = (items: number[]) => Promise.all(items.map(process))\n");
    assert_fires_agent(&fanned, "agent-no-unbounded-promise-all", 1);
    // A fixed tuple is fine.
    let tuple = src("export const run = () => Promise.all([fetchUser(), fetchTeam()])\n");
    assert_fires_agent(&tuple, "agent-no-unbounded-promise-all", 0);
}

#[test]
fn flags_ts_ignore_directives() {
    let source = src("// @ts-ignore\nexport const total = sum(values)\n");
    assert_fires_agent(&source, "agent-no-ts-ignore", 1);
}

#[test]
fn ts_ignore_escalates_under_strict() {
    let source = src("// @ts-expect-error legacy\nexport const total = sum(values)\n");
    let strict = lint_agent_strict(&source);
    let hit = strict
        .iter()
        .find(|d| d.rule == "agent-no-ts-ignore")
        .expect("ts-ignore finding");
    assert_eq!(hit.severity, Severity::Error);
}

#[test]
fn flags_loose_equality_but_exempts_null() {
    let loose = src("export const f = (s: string) => s == \"done\"\n");
    assert_fires_agent(&loose, "agent-no-loose-equality", 1);
    let nullish = src("export const f = (x: unknown) => x == null\n");
    assert_fires_agent(&nullish, "agent-no-loose-equality", 0);
    let strict = src("export const f = (s: string) => s === \"done\"\n");
    assert_fires_agent(&strict, "agent-no-loose-equality", 0);
}

#[test]
fn flags_non_null_assertion() {
    let source = src("export const name = (u?: { name: string }) => u!.name\n");
    assert_fires_agent(&source, "agent-no-non-null-assertion", 1);
}

#[test]
fn flags_ts_enum() {
    let source = src("export enum Status {\n  Active,\n  Done,\n}\n");
    assert_fires_agent(&source, "agent-no-enum", 1);
}

#[test]
fn flags_schema_parse_but_not_json_parse() {
    let schema = src("export const u = UserSchema.parse(raw)\n");
    assert_fires_agent(&schema, "agent-prefer-safe-parse", 1);
    let json = src("export const u = JSON.parse(raw)\n");
    assert_fires_agent(&json, "agent-prefer-safe-parse", 0);
}

#[test]
fn flags_inline_type_import() {
    let source = src("export const save = (u: import(\"./users\").User) => store(u)\n");
    assert_fires_agent(&source, "agent-no-inline-type-import", 1);
}

#[test]
fn flags_ts_namespace() {
    let source = src("export namespace Utils {\n  export const f = (n: number) => n + 1\n}\n");
    assert_fires_agent(&source, "agent-no-ts-namespace", 1);
}

#[test]
fn flags_throw_outside_effect_but_not_inside_gen() {
    let outside = src("export const f = (raw: string) => {\n  if (raw.length === 0) throw new Error(\"empty\")\n  return raw\n}\n");
    assert_fires_agent(&outside, "agent-no-throw", 1);
    let inside = src("export const f = Effect.gen(function* () {\n  throw new Error(\"x\")\n})\n");
    // no-throw-in-effect owns throws inside Effect code; the agent rule defers.
    assert_fires_agent(&inside, "agent-no-throw", 0);
}

#[test]
fn flags_delete_operator() {
    let source = src("export const scrub = (payload: { secret?: string }) => {\n  delete payload.secret\n  return payload\n}\n");
    assert_fires_agent(&source, "agent-no-delete", 1);
}

#[test]
fn flags_deep_nesting() {
    let source = src("export const f = (xs: number[][][]) => {\n  for (const a of xs) {\n    for (const b of a) {\n      for (const c of b) {\n        if (c > 0) {\n          if (c > 10) {\n            sink(c)\n          }\n        }\n      }\n    }\n  }\n}\n");
    assert_fires_agent(&source, "agent-deep-nesting", 1);
}

#[test]
fn flags_too_many_params() {
    let many = src("export const f = (a: 1, b: 2, c: 3, d: 4, e: 5, g: 6) => a\n");
    assert_fires_agent(&many, "agent-too-many-params", 1);
    let few = src("export const f = (a: 1, b: 2) => a\n");
    assert_fires_agent(&few, "agent-too-many-params", 0);
}

#[test]
fn flags_deep_relative_import() {
    let deep = format!("{PRELUDE}import {{ x }} from \"../../../shared/x\"\nexport const y = x\n");
    assert_fires_agent(&deep, "agent-deep-relative-import", 1);
    let shallow = format!("{PRELUDE}import {{ x }} from \"../shared/x\"\nexport const y = x\n");
    assert_fires_agent(&shallow, "agent-deep-relative-import", 0);
}

#[test]
fn flags_high_complexity() {
    // ~18 decision points via a chain of independent ifs.
    let mut body = String::from("export const f = (n: number) => {\n  let r = 0\n");
    for i in 0..18 {
        body.push_str(&format!("  if (n === {i}) r = {i}\n"));
    }
    body.push_str("  return r\n}\n");
    assert_fires_agent(&src(&body), "agent-high-complexity", 1);
}
