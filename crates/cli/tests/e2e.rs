//! End-to-end tests that build and run the actual `agent-doctor` binary against
//! real git repositories and stdio — covering the toolkit subcommands that unit
//! tests can't reach (process exit codes, the git merge driver, the serve loop).

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};

/// Path to the built binary, provided by cargo to integration tests.
const BIN: &str = env!("CARGO_BIN_EXE_agent-doctor");

static COUNTER: AtomicU32 = AtomicU32::new(0);

fn temp_dir(tag: &str) -> PathBuf {
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("ad-e2e-{tag}-{}-{}", std::process::id(), unique));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .unwrap()
        .status
        .success();
    assert!(ok, "git {args:?} failed");
}

fn init_repo(dir: &Path) {
    git(dir, &["init", "-q", "-b", "main"]);
    git(dir, &["config", "user.email", "t@t.co"]);
    git(dir, &["config", "user.name", "t"]);
}

fn write(dir: &Path, name: &str, contents: &str) {
    let path = dir.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, contents).unwrap();
}

#[test]
fn gate_denies_protected_and_passes_clean() {
    let dir = temp_dir("gate");
    init_repo(&dir);
    write(&dir, "src/app.ts", "export const a = 1\n");
    git(&dir, &["add", "-A"]);
    git(&dir, &["commit", "-qm", "base"]);
    // change the file in a second commit so it shows in the diff vs HEAD~1.
    write(&dir, "src/app.ts", "export const a = 2\n");
    git(&dir, &["commit", "-qam", "change"]);

    // policy that protects the changed file → deny (exit 1).
    write(&dir, "deny.toml", "[protected]\nglobs = [\"src/app.ts\"]\n");
    let denied = Command::new(BIN)
        .current_dir(&dir)
        .args(["gate", "--base", "HEAD~1", "--policy", "deny.toml"])
        .output()
        .unwrap();
    assert!(!denied.status.success(), "expected non-zero exit on deny");

    // policy that protects something else → pass (exit 0).
    write(&dir, "ok.toml", "[protected]\nglobs = [\"other/**\"]\n");
    let passed = Command::new(BIN)
        .current_dir(&dir)
        .args(["gate", "--base", "HEAD~1", "--policy", "ok.toml"])
        .output()
        .unwrap();
    assert!(passed.status.success(), "expected zero exit when clean");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn impact_emits_valid_json() {
    let dir = temp_dir("impact");
    init_repo(&dir);
    write(&dir, "src/a.ts", "export const x = 1\n");
    write(&dir, "test/a.test.ts", "import { x } from '../src/a'\n");
    git(&dir, &["add", "-A"]);
    git(&dir, &["commit", "-qm", "base"]);
    write(&dir, "src/a.ts", "export const x = 2\n");
    git(&dir, &["commit", "-qam", "change"]);

    let out = Command::new(BIN)
        .current_dir(&dir)
        .args(["impact", "--base", "HEAD~1", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).expect("valid JSON");
    let tests = value["tests"].as_array().unwrap();
    assert!(tests.iter().any(|t| t == "test/a.test.ts"));

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn merge_driver_auto_resolves_additive_conflict() {
    let dir = temp_dir("merge");
    init_repo(&dir);
    write(&dir, "f.ts", "export function a() { return 1 }\n");
    git(&dir, &["add", "-A"]);
    git(&dir, &["commit", "-qm", "base"]);

    git(&dir, &["checkout", "-q", "-b", "feature"]);
    write(
        &dir,
        "f.ts",
        "export function a() { return 1 }\nexport function b() { return 2 }\n",
    );
    git(&dir, &["commit", "-qam", "add b"]);

    git(&dir, &["checkout", "-q", "main"]);
    write(
        &dir,
        "f.ts",
        "export function a() { return 1 }\nexport function c() { return 3 }\n",
    );
    git(&dir, &["commit", "-qam", "add c"]);

    // register the semantic merge driver pointing at the built binary.
    git(&dir, &["config", "merge.ad.name", "agent-doctor"]);
    git(&dir, &["config", "merge.ad.driver", &format!("{BIN} merge %O %A %B")]);
    write(&dir, ".gitattributes", "*.ts merge=ad\n");
    git(&dir, &["add", ".gitattributes"]);
    git(&dir, &["commit", "-qm", "attrs"]);

    // vanilla git would conflict here; the semantic driver should auto-resolve.
    let merged = Command::new("git")
        .arg("-C")
        .arg(&dir)
        .args(["merge", "feature", "-m", "merge"])
        .output()
        .unwrap();
    assert!(merged.status.success(), "merge should succeed cleanly");

    let result = std::fs::read_to_string(dir.join("f.ts")).unwrap();
    assert!(result.contains("function b"), "kept feature's addition");
    assert!(result.contains("function c"), "kept main's addition");
    assert!(!result.contains("<<<<<<<"), "no conflict markers");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn init_scaffolds_and_is_idempotent() {
    let dir = temp_dir("init");
    init_repo(&dir);
    let first = Command::new(BIN).current_dir(&dir).arg("init").output().unwrap();
    assert!(first.status.success());
    assert!(dir.join("agent-doctor.policy.toml").exists());
    assert!(dir.join(".agent-doctor/.gitignore").exists());
    assert!(dir.join(".mcp.json").exists());
    assert!(dir.join(".gitattributes").exists());

    // merge driver registered in git config.
    let driver = Command::new("git")
        .arg("-C")
        .arg(&dir)
        .args(["config", "--get", "merge.agent-doctor.driver"])
        .output()
        .unwrap();
    assert!(driver.status.success() && !driver.stdout.is_empty());

    // second run is idempotent: still succeeds, doesn't duplicate gitattributes.
    let second = Command::new(BIN).current_dir(&dir).arg("init").output().unwrap();
    assert!(second.status.success());
    let attrs = std::fs::read_to_string(dir.join(".gitattributes")).unwrap();
    assert_eq!(attrs.matches("*.ts merge=agent-doctor").count(), 1);

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn serve_answers_a_query_over_stdio() {
    let dir = temp_dir("serve");
    write(&dir, "a.ts", "export function foo() { return 1 }\n");

    let mut child = Command::new(BIN)
        .current_dir(&dir)
        .arg("serve")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"{\"id\":1,\"method\":\"symbol_exists\",\"params\":{\"name\":\"foo\"}}\n")
        .unwrap();
    // dropping stdin closes it → server hits EOF and exits after responding.
    let out = child.wait_with_output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("\"result\""), "got: {stdout}");
    assert!(stdout.contains("foo"), "got: {stdout}");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn serve_mcp_handshake_and_tool_call() {
    let dir = temp_dir("mcp");
    write(&dir, "a.ts", "export function foo() {}\n");
    let mut child = Command::new(BIN)
        .current_dir(&dir)
        .args(["serve", "--mcp"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(
            b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{}}\n\
              {\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/call\",\"params\":{\"name\":\"symbol_exists\",\"arguments\":{\"name\":\"foo\"}}}\n",
        )
        .unwrap();
    let out = child.wait_with_output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("protocolVersion"), "got: {stdout}");
    assert!(stdout.contains("foo"), "got: {stdout}");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn merge_real_conflict_writes_markers_and_exits_nonzero() {
    let dir = temp_dir("conflict");
    write(&dir, "base.ts", "export const x = 1\n");
    write(&dir, "ours.ts", "export const x = 2\n");
    write(&dir, "theirs.ts", "export const x = 3\n");
    let out = Command::new(BIN)
        .current_dir(&dir)
        .args(["merge", "base.ts", "ours.ts", "theirs.ts", "--output", "out.ts"])
        .output()
        .unwrap();
    assert!(!out.status.success(), "same-decl conflict must fail");
    let merged = std::fs::read_to_string(dir.join("out.ts")).unwrap();
    assert!(merged.contains("<<<<<<<"), "expected conflict markers");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn gate_flags_layering_violation() {
    let dir = temp_dir("layer");
    init_repo(&dir);
    write(&dir, "src/ui/button.ts", "export const b = 1\n");
    write(&dir, "src/core/engine.ts", "export const e = 1\n");
    git(&dir, &["add", "-A"]);
    git(&dir, &["commit", "-qm", "base"]);
    // introduce an illegal core→ui import.
    write(&dir, "src/core/engine.ts", "import { b } from '../ui/button'\nexport const e = b\n");
    git(&dir, &["commit", "-qam", "bad import"]);
    write(
        &dir,
        "layer.toml",
        "[[layer]]\nname = \"core\"\npath = \"src/core/**\"\nforbid_imports_from = [\"src/ui/**\"]\n",
    );
    let out = Command::new(BIN)
        .current_dir(&dir)
        .args(["gate", "--base", "HEAD~1", "--policy", "layer.toml", "--json"])
        .output()
        .unwrap();
    assert!(!out.status.success(), "layering violation must fail the gate");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("layering"), "got: {stdout}");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn gate_enforces_leases_per_actor() {
    let dir = temp_dir("lease");
    init_repo(&dir);
    write(&dir, "src/auth/login.ts", "export const a = 1\n");
    git(&dir, &["add", "-A"]);
    git(&dir, &["commit", "-qm", "base"]);
    write(&dir, "src/auth/login.ts", "export const a = 2\n");
    git(&dir, &["commit", "-qam", "change"]);
    // agent-a owns src/auth/**.
    write(
        &dir,
        "leases.json",
        "{\"leases\":[{\"actor\":\"agent-a\",\"task_id\":\"t1\",\"globs\":[\"src/auth/**\"]}]}",
    );

    let intruder = Command::new(BIN)
        .current_dir(&dir)
        .args(["gate", "--base", "HEAD~1", "--actor", "agent-b", "--leases", "leases.json"])
        .output()
        .unwrap();
    assert!(!intruder.status.success(), "agent-b is outside the lease");

    let owner = Command::new(BIN)
        .current_dir(&dir)
        .args(["gate", "--base", "HEAD~1", "--actor", "agent-a", "--leases", "leases.json"])
        .output()
        .unwrap();
    assert!(owner.status.success(), "agent-a owns the region");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn verify_passes_clean_and_blocks_on_policy() {
    let dir = temp_dir("verify");
    init_repo(&dir);
    write(&dir, "src/a.ts", "export const x = 1\n");
    write(&dir, "test/a.test.ts", "import '../src/a'\n");
    git(&dir, &["add", "-A"]);
    git(&dir, &["commit", "-qm", "base"]);
    write(&dir, "src/a.ts", "export const x = 2\n");
    git(&dir, &["commit", "-qam", "change"]);

    // clean: gate ok, lists the impacted test, exit 0.
    let ok = Command::new(BIN)
        .current_dir(&dir)
        .args(["verify", "--base", "HEAD~1"])
        .output()
        .unwrap();
    assert!(ok.status.success());
    assert!(String::from_utf8_lossy(&ok.stdout).contains("test/a.test.ts"));

    // a policy that protects the changed file blocks the submit.
    write(&dir, "deny.toml", "[protected]\nglobs = [\"src/a.ts\"]\n");
    let blocked = Command::new(BIN)
        .current_dir(&dir)
        .args(["verify", "--base", "HEAD~1", "--policy", "deny.toml"])
        .output()
        .unwrap();
    assert!(!blocked.status.success(), "policy violation must block verify");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn orchestrate_runs_a_ledger_through_the_loop() {
    let dir = temp_dir("orch");
    write(&dir, "src/app.ts", "export const a = 1\n");
    // an executor: ignore stdin, emit an outcome editing src/app.ts.
    let script = dir.join("exec.sh");
    std::fs::write(
        &script,
        "#!/bin/sh\ncat >/dev/null\nprintf '%s' '{\"changes\":[{\"path\":\"src/app.ts\",\"new_source\":\"export const a = 2\"}],\"summary\":\"done\"}'\n",
    )
    .unwrap();
    set_executable(&script);
    write(
        &dir,
        "ledger.json",
        "{\"tasks\":[{\"id\":\"t1\",\"intent\":\"edit app\",\"targets\":[\"src/app.ts\"],\"status\":\"pending\"}]}",
    );

    let out = Command::new(BIN)
        .current_dir(&dir)
        .args([
            "orchestrate",
            "--ledger",
            "ledger.json",
            "--executor",
            script.to_str().unwrap(),
            "--policy",
            "nonexistent.toml",
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    // the ledger is written back with the task marked done.
    let ledger = std::fs::read_to_string(dir.join("ledger.json")).unwrap();
    assert!(ledger.contains("\"done\""), "ledger: {ledger}");
    std::fs::remove_dir_all(&dir).ok();
}

#[cfg(unix)]
fn set_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms).unwrap();
}

#[cfg(not(unix))]
fn set_executable(_path: &Path) {}
