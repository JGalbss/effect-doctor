//! The orchestrator (toolkit Layer 5): the deterministic runner that wraps a
//! nondeterministic agent. It is the centerpiece — the agent is just step "run
//! the executor"; lease, gate, test, and retry-with-findings happen *around* it,
//! in deterministic code the agent cannot bypass.
//!
//! One task's lifecycle (`run_task`):
//!   estimate footprint → acquire lease → assemble context pack → run executor
//!   → gate the diff → on violation, retry with the exact findings as feedback
//!   → on clean, select impacted tests → release lease.

mod executor;
mod footprint;
mod ledger;

use agent_doctor_policy::{Lease, LeaseSet, Violation};
use agent_doctor_server::{ContextPack, Kernel};
use serde::{Deserialize, Serialize};

pub use executor::CommandExecutor;
pub use footprint::{estimate_footprint, frontier_dedup, Draft, FileChange, FrontierDup};
pub use ledger::{Ledger, Task, TaskStatus};

/// What the executor (the agent) receives.
pub struct TaskSpec {
    pub task: Task,
    /// Minimal context assembled from the kernel (helpers, tests, gate preview).
    pub context: ContextPack,
    /// Structured findings from the previous attempt (empty on first attempt) —
    /// the deterministic retry signal, not "it broke, try again".
    pub feedback: Vec<String>,
}

/// What the executor returns.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskOutcome {
    pub changes: Vec<FileChange>,
    #[serde(default)]
    pub summary: String,
}

/// The agent, abstracted so the loop is testable without an LLM.
pub trait Executor {
    fn run(&mut self, spec: &TaskSpec) -> Result<TaskOutcome, String>;
}

/// Loop tuning.
pub struct RunConfig {
    /// Extra attempts after the first when the gate rejects the diff.
    pub max_retries: u32,
}

impl Default for RunConfig {
    fn default() -> RunConfig {
        RunConfig { max_retries: 2 }
    }
}

/// The deterministic record of running one task.
pub struct TaskReport {
    pub task_id: String,
    pub status: TaskStatus,
    pub attempts: u32,
    pub violations: Vec<Violation>,
    pub impacted_tests: Vec<String>,
    pub changes: Vec<FileChange>,
    pub summary: String,
}

/// Run one task through the deterministic loop.
pub fn run_task(
    kernel: &Kernel,
    leases: &mut LeaseSet,
    actor: &str,
    task: &Task,
    executor: &mut dyn Executor,
    config: &RunConfig,
) -> TaskReport {
    let region = lease_globs(kernel, task);
    let lease = Lease {
        actor: actor.to_string(),
        task_id: task.id.clone(),
        globs: region.clone(),
    };
    if let Err(reason) = leases.acquire(lease) {
        return blocked(task, reason);
    }

    let footprint = estimate_footprint(kernel, &region);
    let mut feedback: Vec<String> = Vec::new();
    let mut attempts = 0;

    loop {
        attempts += 1;
        let spec = TaskSpec {
            task: task.clone(),
            context: kernel.context_pack(&footprint, Some(actor)),
            feedback: feedback.clone(),
        };
        let outcome = match executor.run(&spec) {
            Ok(outcome) => outcome,
            Err(error) => {
                leases.release(&task.id);
                return failed(task, attempts, Vec::new(), error);
            }
        };
        let changed: Vec<String> = outcome.changes.iter().map(|c| c.path.clone()).collect();
        let violations = kernel.gate(&changed, Some(actor));

        if violations.is_empty() {
            let impacted_tests = kernel.impact(&changed, Vec::new()).tests;
            leases.release(&task.id);
            return TaskReport {
                task_id: task.id.clone(),
                status: TaskStatus::Done,
                attempts,
                violations,
                impacted_tests,
                changes: outcome.changes,
                summary: outcome.summary,
            };
        }

        if attempts > config.max_retries {
            leases.release(&task.id);
            return failed(task, attempts, violations, "gate not satisfied".to_string());
        }
        feedback = violations.iter().map(format_violation).collect();
    }
}

/// Drive an entire ledger: repeatedly run all ready tasks (deps satisfied),
/// updating each task's status, until none remain runnable. Dependents unlock as
/// their prerequisites complete. Returns a report per task run.
pub fn run_ledger(
    kernel: &Kernel,
    leases: &mut LeaseSet,
    actor: &str,
    ledger: &mut Ledger,
    executor: &mut dyn Executor,
    config: &RunConfig,
) -> Vec<TaskReport> {
    let mut reports = Vec::new();
    loop {
        let ready: Vec<String> = ledger.ready().iter().map(|task| task.id.clone()).collect();
        if ready.is_empty() {
            break;
        }
        for id in ready {
            let Some(task) = ledger.get(&id).cloned() else {
                continue;
            };
            let report = run_task(kernel, leases, actor, &task, executor, config);
            ledger.set_status(&id, report.status);
            reports.push(report);
        }
    }
    reports
}

/// The lease region for a task: its declared targets, or — if it declared none —
/// nothing reserved (an unleased task competes only via the gate).
fn lease_globs(_kernel: &Kernel, task: &Task) -> Vec<String> {
    task.targets.clone()
}

fn format_violation(violation: &Violation) -> String {
    format!(
        "{:?} {}: {}",
        violation.kind, violation.file, violation.reason
    )
}

fn blocked(task: &Task, reason: String) -> TaskReport {
    TaskReport {
        task_id: task.id.clone(),
        status: TaskStatus::Blocked,
        attempts: 0,
        violations: Vec::new(),
        impacted_tests: Vec::new(),
        changes: Vec::new(),
        summary: reason,
    }
}

fn failed(task: &Task, attempts: u32, violations: Vec<Violation>, summary: String) -> TaskReport {
    TaskReport {
        task_id: task.id.clone(),
        status: TaskStatus::Failed,
        attempts,
        violations,
        impacted_tests: Vec::new(),
        changes: Vec::new(),
        summary,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU32, Ordering};

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    fn temp_project(files: &[(&str, &str)]) -> PathBuf {
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("ad-orch-{}-{}", std::process::id(), unique));
        for (name, source) in files {
            let path = dir.join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, source).unwrap();
        }
        dir
    }

    fn kernel(dir: &Path) -> Kernel {
        Kernel::build(
            dir,
            &dir.join("agent-doctor.policy.toml"),
            &dir.join(".agent-doctor/leases.json"),
        )
        .unwrap()
    }

    /// Executor that writes `first` initially, then `second` once it has feedback.
    struct RetryStub {
        first: Vec<FileChange>,
        second: Vec<FileChange>,
    }
    impl Executor for RetryStub {
        fn run(&mut self, spec: &TaskSpec) -> Result<TaskOutcome, String> {
            let changes = if spec.feedback.is_empty() {
                self.first.clone()
            } else {
                self.second.clone()
            };
            Ok(TaskOutcome {
                changes,
                summary: "stub".to_string(),
            })
        }
    }

    fn change(path: &str) -> FileChange {
        FileChange {
            path: path.to_string(),
            new_source: "export const v = 1".to_string(),
        }
    }

    #[test]
    fn clean_task_completes_first_try() {
        let dir = temp_project(&[("src/app.ts", "export const a = 1")]);
        let kernel = kernel(&dir);
        let mut leases = LeaseSet::default();
        let task = Task::new("t1", "edit app").with_targets(&["src/app.ts"]);
        let mut exec = RetryStub {
            first: vec![change("src/app.ts")],
            second: vec![],
        };
        let report = run_task(
            &kernel,
            &mut leases,
            "agent-a",
            &task,
            &mut exec,
            &RunConfig::default(),
        );
        assert_eq!(report.status, TaskStatus::Done);
        assert_eq!(report.attempts, 1);
        assert!(leases.leases.is_empty(), "lease released on completion");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn policy_violation_is_retried_with_findings() {
        let dir = temp_project(&[("src/app.ts", "export const a = 1")]);
        std::fs::write(
            dir.join("agent-doctor.policy.toml"),
            "[protected]\nglobs = [\"src/locked.ts\"]\n",
        )
        .unwrap();
        let kernel = kernel(&dir);
        let mut leases = LeaseSet::default();
        let task = Task::new("t1", "edit").with_targets(&["src/**"]);
        // attempt 1 touches the protected file → denied; attempt 2 is clean.
        let mut exec = RetryStub {
            first: vec![change("src/locked.ts")],
            second: vec![change("src/app.ts")],
        };
        let report = run_task(
            &kernel,
            &mut leases,
            "agent-a",
            &task,
            &mut exec,
            &RunConfig::default(),
        );
        assert_eq!(report.status, TaskStatus::Done);
        assert_eq!(report.attempts, 2, "retried once after the gate denial");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn persistent_violation_fails_after_retries() {
        let dir = temp_project(&[("src/app.ts", "export const a = 1")]);
        std::fs::write(
            dir.join("agent-doctor.policy.toml"),
            "[protected]\nglobs = [\"src/locked.ts\"]\n",
        )
        .unwrap();
        let kernel = kernel(&dir);
        let mut leases = LeaseSet::default();
        let task = Task::new("t1", "edit").with_targets(&["src/**"]);
        let mut exec = RetryStub {
            first: vec![change("src/locked.ts")],
            second: vec![change("src/locked.ts")], // never fixes it
        };
        let report = run_task(
            &kernel,
            &mut leases,
            "agent-a",
            &task,
            &mut exec,
            &RunConfig { max_retries: 1 },
        );
        assert_eq!(report.status, TaskStatus::Failed);
        assert!(!report.violations.is_empty());
        assert!(leases.leases.is_empty(), "lease released on failure");
        std::fs::remove_dir_all(&dir).ok();
    }

    struct AlwaysOk;
    impl Executor for AlwaysOk {
        fn run(&mut self, _spec: &TaskSpec) -> Result<TaskOutcome, String> {
            Ok(TaskOutcome {
                changes: vec![change("src/app.ts")],
                summary: "ok".to_string(),
            })
        }
    }

    #[test]
    fn run_ledger_drives_dag_to_completion() {
        let dir = temp_project(&[("src/app.ts", "export const a = 1")]);
        let kernel = kernel(&dir);
        let mut leases = LeaseSet::default();
        let mut ledger = Ledger::new();
        ledger.add(Task::new("a", "first").with_targets(&["src/app.ts"]));
        ledger.add(
            Task::new("b", "second")
                .with_deps(&["a"])
                .with_targets(&["src/app.ts"]),
        );
        let mut exec = AlwaysOk;
        let reports = run_ledger(
            &kernel,
            &mut leases,
            "agent",
            &mut ledger,
            &mut exec,
            &RunConfig::default(),
        );
        assert_eq!(reports.len(), 2);
        assert!(reports.iter().all(|r| r.status == TaskStatus::Done));
        assert_eq!(ledger.get("b").unwrap().status, TaskStatus::Done);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn lease_conflict_blocks_task() {
        let dir = temp_project(&[("src/app.ts", "export const a = 1")]);
        let kernel = kernel(&dir);
        let mut leases = LeaseSet::default();
        // another agent already owns the region.
        leases
            .acquire(Lease {
                actor: "agent-b".to_string(),
                task_id: "other".to_string(),
                globs: vec!["src/**".to_string()],
            })
            .unwrap();
        let task = Task::new("t1", "edit").with_targets(&["src/app.ts"]);
        let mut exec = RetryStub {
            first: vec![change("src/app.ts")],
            second: vec![],
        };
        let report = run_task(
            &kernel,
            &mut leases,
            "agent-a",
            &task,
            &mut exec,
            &RunConfig::default(),
        );
        assert_eq!(report.status, TaskStatus::Blocked);
        std::fs::remove_dir_all(&dir).ok();
    }
}
