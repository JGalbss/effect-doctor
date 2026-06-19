//! `Repo` — the agent-native VCS facade.
//!
//! It is "our own model on a git-compatible backend": git provides object
//! storage and worktrees ([`GitVcs`]), while the agent-native model lives on
//! top — an agent/task-attributed operation log ([`OpLog`]) with deterministic
//! undo, first-class region leases ([`LeaseSet`]), and semantic (AST) merge.
//! State persists under `.agent-doctor/` so the model survives across runs.

use std::path::{Path, PathBuf};

use agent_doctor_merge::MergeResult;
use agent_doctor_policy::{Lease, LeaseSet};

use crate::oplog::{OpKind, OpLog, Operation};
use crate::{GitVcs, Vcs};

/// The agent-native repository.
pub struct Repo {
    root: PathBuf,
    vcs: GitVcs,
    oplog: OpLog,
    leases: LeaseSet,
}

impl Repo {
    /// Open a repo rooted at `root`, loading any persisted op-log and leases.
    pub fn open(root: impl AsRef<Path>) -> Result<Repo, String> {
        let root = root.as_ref().to_path_buf();
        let oplog = OpLog::load(&oplog_path(&root)).map_err(|error| error.to_string())?;
        let leases = LeaseSet::load(&leases_path(&root)).map_err(|error| error.to_string())?;
        Ok(Repo {
            vcs: GitVcs::new(&root),
            root,
            oplog,
            leases,
        })
    }

    pub fn oplog(&self) -> &OpLog {
        &self.oplog
    }

    pub fn leases(&self) -> &LeaseSet {
        &self.leases
    }

    /// Acquire a region lease for an agent's task (rejected if it overlaps
    /// another agent's lease), recording it in the op-log.
    pub fn acquire_lease(
        &mut self,
        agent: &str,
        task_id: &str,
        globs: Vec<String>,
    ) -> Result<(), String> {
        self.leases.acquire(Lease {
            actor: agent.to_string(),
            task_id: task_id.to_string(),
            globs: globs.clone(),
        })?;
        self.oplog.record(
            OpKind::Lease,
            agent,
            task_id,
            &format!("lease {}", globs.join(", ")),
            Vec::new(),
        );
        self.persist()
    }

    /// Release a task's lease, recording it.
    pub fn release_lease(&mut self, agent: &str, task_id: &str) -> Result<(), String> {
        self.leases.release(task_id);
        self.oplog
            .record(OpKind::Release, agent, task_id, "release", Vec::new());
        self.persist()
    }

    /// Record a snapshot operation for an agent's task over a set of files.
    pub fn snapshot(
        &mut self,
        agent: &str,
        task_id: &str,
        summary: &str,
        files: Vec<String>,
    ) -> Result<String, String> {
        let id = self
            .oplog
            .record(OpKind::Snapshot, agent, task_id, summary, files);
        self.persist()?;
        Ok(id)
    }

    /// Undo the last operation (deterministic).
    pub fn undo(&mut self, agent: &str) -> Result<Option<String>, String> {
        let restored = self.oplog.undo(agent);
        self.persist()?;
        Ok(restored)
    }

    /// Revert an entire agent/task session.
    pub fn revert_task(&mut self, agent: &str, task_id: &str) -> Result<Option<String>, String> {
        let restored = self.oplog.revert_task(agent, task_id);
        self.persist()?;
        Ok(restored)
    }

    /// The current head operation.
    pub fn head(&self) -> Option<&Operation> {
        self.oplog.head()
    }

    /// Create an isolated workspace for an agent (delegates to the backend).
    pub fn create_workspace(&self, name: &str, base: &str) -> Result<PathBuf, String> {
        self.vcs.create_workspace(name, base)
    }

    pub fn remove_workspace(&self, name: &str) -> Result<(), String> {
        self.vcs.remove_workspace(name)
    }

    /// Semantic 3-way merge of a file across revisions (delegates to the
    /// backend's AST merge).
    pub fn merge_file(
        &self,
        path: &str,
        base_ref: &str,
        ours_ref: &str,
        theirs_ref: &str,
    ) -> Result<MergeResult, String> {
        self.vcs.merge_file(path, base_ref, ours_ref, theirs_ref)
    }

    fn persist(&self) -> Result<(), String> {
        self.oplog
            .save(&oplog_path(&self.root))
            .map_err(|error| error.to_string())?;
        self.leases
            .save(&leases_path(&self.root))
            .map_err(|error| error.to_string())
    }
}

fn oplog_path(root: &Path) -> PathBuf {
    root.join(".agent-doctor").join("oplog.json")
}

fn leases_path(root: &Path) -> PathBuf {
    root.join(".agent-doctor").join("leases.json")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    fn temp_root() -> PathBuf {
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("ad-repo-{}-{}", std::process::id(), unique));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn snapshot_persists_and_reloads() {
        let root = temp_root();
        {
            let mut repo = Repo::open(&root).unwrap();
            repo.snapshot("agent-a", "t1", "edit", vec!["f.ts".into()])
                .unwrap();
        }
        // reopening loads the persisted op-log.
        let repo = Repo::open(&root).unwrap();
        assert_eq!(repo.oplog().history().len(), 1);
        assert_eq!(repo.head().unwrap().agent, "agent-a");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn lease_conflict_is_rejected_and_records_on_success() {
        let root = temp_root();
        let mut repo = Repo::open(&root).unwrap();
        repo.acquire_lease("agent-a", "t1", vec!["src/auth/**".into()])
            .unwrap();
        let conflict = repo.acquire_lease("agent-b", "t2", vec!["src/auth/login.ts".into()]);
        assert!(conflict.is_err());
        // the successful lease is recorded in the op-log and persisted.
        assert!(repo
            .oplog()
            .history()
            .iter()
            .any(|op| op.kind == OpKind::Lease));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn undo_and_revert_task_work_through_repo() {
        let root = temp_root();
        let mut repo = Repo::open(&root).unwrap();
        let base = repo.snapshot("human", "setup", "base", vec![]).unwrap();
        repo.snapshot("agent-a", "feature", "a1", vec![]).unwrap();
        repo.snapshot("agent-a", "feature", "a2", vec![]).unwrap();
        let restored = repo.revert_task("orchestrator", "feature").unwrap();
        assert_eq!(restored.as_deref(), Some(base.as_str()));
        assert_eq!(repo.head().unwrap().id, base);
        std::fs::remove_dir_all(&root).ok();
    }
}
