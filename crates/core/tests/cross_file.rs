//! Cross-file "this already exists" detection (engine `--agent` pass): agents
//! reimplement helpers they can't see, so the scan indexes every file's
//! functions and flags duplicates / near-duplicates / name + shape collisions.

use std::fs;
use std::path::PathBuf;

use agent_doctor_core::{scan, Diagnostic, ScanOptions, ScanScope};

/// Write `files` into a fresh temp dir and scan it with `--agent`.
fn scan_agent(test: &str, files: &[(&str, &str)]) -> Vec<Diagnostic> {
    scan_with(test, files, true)
}

fn scan_with(test: &str, files: &[(&str, &str)], agent: bool) -> Vec<Diagnostic> {
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("xfile-{test}"));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("temp dir");
    for (name, contents) in files {
        fs::write(root.join(name), contents).expect("write fixture");
    }
    let result = scan(&ScanOptions {
        root,
        migrate: false,
        scope: ScanScope::Full,
        base: None,
        deep: false,
        adopt: false,
        agent,
        agent_strict: false,
    })
    .expect("scan");
    result.diagnostics
}

fn count(diagnostics: &[Diagnostic], rule: &str) -> usize {
    diagnostics.iter().filter(|d| d.rule == rule).count()
}

const IDENTICAL_BODY: &str = "(input: number[]) => {\n  const list = collect(input)\n  for (const item of list) {\n    push(item)\n  }\n  return list.length > 0 ? list[0] : null\n}\n";

#[test]
fn exact_structural_duplicate_across_files() {
    let a =
        format!("import {{ Effect }} from \"effect\"\nexport const usageTone = {IDENTICAL_BODY}");
    let b = format!("import {{ Effect }} from \"effect\"\nexport const barFill = {IDENTICAL_BODY}");
    let diagnostics = scan_agent("exact", &[("a.ts", &a), ("b.ts", &b)]);
    // Both copies flagged, once each.
    assert_eq!(count(&diagnostics, "agent-duplicate-cross-file"), 2);
}

#[test]
fn near_duplicate_across_files() {
    let a = "import { Effect } from \"effect\"\nexport const toTone = (p: number) => {\n  const picked = pick(p)\n  const clamped = clamp(picked)\n  const label = String(clamped)\n  return label.trim()\n}\n";
    let b = "import { Effect } from \"effect\"\nexport const barTone = (q: number) => {\n  const chosen = pick(q)\n  const bounded = clamp(chosen)\n  const text = String(bounded)\n  return text.padStart(2)\n}\n";
    let diagnostics = scan_agent("near", &[("a.ts", a), ("b.ts", b)]);
    assert!(
        count(&diagnostics, "agent-near-duplicate-function") >= 1,
        "expected a near-duplicate finding, got: {diagnostics:#?}"
    );
}

#[test]
fn same_name_divergent_bodies() {
    // Same name, deliberately different shape + call sets so the stronger
    // exact/near/shape signals don't fire — only the name collision remains.
    let a = "import { Effect } from \"effect\"\nexport const parseConfig = (raw: string) => {\n  if (raw.length === 0) {\n    return alpha(raw)\n  }\n  if (raw.length === 1) {\n    return beta(raw)\n  }\n  return gamma(raw)\n}\n";
    let b = "import { Effect } from \"effect\"\nexport const parseConfig = (data: string) => {\n  for (const ch of data) {\n    delta(ch)\n  }\n  while (epsilon()) {\n    zeta()\n  }\n  return data\n}\n";
    let diagnostics = scan_agent("name", &[("a.ts", a), ("b.ts", b)]);
    assert!(
        count(&diagnostics, "agent-similar-function-name") >= 1,
        "expected a same-name finding, got: {diagnostics:#?}"
    );
}

#[test]
fn silent_without_agent_flag() {
    let a =
        format!("import {{ Effect }} from \"effect\"\nexport const usageTone = {IDENTICAL_BODY}");
    let b = format!("import {{ Effect }} from \"effect\"\nexport const barFill = {IDENTICAL_BODY}");
    let diagnostics = scan_with("off", &[("a.ts", &a), ("b.ts", &b)], false);
    assert_eq!(count(&diagnostics, "agent-duplicate-cross-file"), 0);
    assert_eq!(count(&diagnostics, "agent-near-duplicate-function"), 0);
}

#[test]
fn same_file_duplicates_are_not_cross_file() {
    // Two identical bodies in ONE file → intra-file rule owns it, not cross-file.
    let single = format!(
        "import {{ Effect }} from \"effect\"\nexport const usageTone = {IDENTICAL_BODY}\nexport const barFill = {IDENTICAL_BODY}"
    );
    let diagnostics = scan_agent("samefile", &[("only.ts", &single)]);
    assert_eq!(count(&diagnostics, "agent-duplicate-cross-file"), 0);
    assert!(count(&diagnostics, "agent-duplicate-function") >= 1);
}

#[test]
fn flags_exported_helper_imported_by_one_module() {
    // formatOne: exported, imported by exactly one module (b.ts) → single-use.
    // shared: imported by two modules → not flagged.
    let a = "export const formatOne = (n: number) => `${n} cr`\n";
    let b = "import { formatOne } from \"./a\"\nexport const show = (n: number) => formatOne(n)\n";
    let shared = "export const shared = (n: number) => n + 1\n";
    let c = "import { shared } from \"./shared\"\nexport const c = shared(1)\n";
    let d = "import { shared } from \"./shared\"\nexport const d = shared(2)\n";
    let diagnostics = scan_agent(
        "singleuse",
        &[
            ("a.ts", a),
            ("b.ts", b),
            ("shared.ts", shared),
            ("c.ts", c),
            ("d.ts", d),
        ],
    );
    assert_eq!(count(&diagnostics, "agent-no-single-use-helper"), 1);
    let hit = diagnostics
        .iter()
        .find(|x| x.rule == "agent-no-single-use-helper")
        .expect("single-use finding");
    assert!(hit.message.contains("formatOne"), "got: {}", hit.message);
}

#[test]
fn flags_circular_imports() {
    // a -> b -> a is a 2-file cycle; standalone.ts is not.
    let a = "import { b } from \"./b\"\nexport const a = () => b()\n";
    let b = "import { a } from \"./a\"\nexport const b = () => a()\n";
    let standalone = "export const c = () => 1\n";
    let diagnostics = scan_agent(
        "cycles",
        &[("a.ts", a), ("b.ts", b), ("standalone.ts", standalone)],
    );
    // Both files in the cycle are flagged; standalone is not.
    assert_eq!(count(&diagnostics, "agent-circular-import"), 2);
    assert!(diagnostics
        .iter()
        .any(|d| d.rule == "agent-circular-import" && d.message.contains("import cycle")));
}
