//! Operation log — the agent-native heart of the VCS.
//!
//! Every state-changing operation is recorded as a content-addressed entry
//! attributed to an **agent** and a **task**, chained to its parent. This makes
//! two things first-class that git lacks for a fleet of agents:
//!   - **deterministic undo** of a single op *or an entire agent/task session*
//!     (`undo`, `revert_task`), and
//!   - **attribution / audit** (`by_agent`, `by_task`) — who did what.
//!
//! Ordering uses a monotonic sequence (not wall-clock), and ids are content
//! hashes, so the log is fully deterministic and reproducible.

use agent_doctor_core::fnv1a;
use serde::{Deserialize, Serialize};

/// What an operation did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OpKind {
    Snapshot,
    Lease,
    Release,
    Merge,
    Undo,
}

/// One recorded operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Operation {
    /// Content-addressed id (hex).
    pub id: String,
    /// Monotonic sequence number (insertion order).
    pub seq: u64,
    /// The op this one followed (the head at record time).
    pub parent: Option<String>,
    pub agent: String,
    pub task_id: String,
    pub kind: OpKind,
    pub summary: String,
    /// Files this op touched (for snapshots).
    #[serde(default)]
    pub files: Vec<String>,
}

/// An append-only, content-addressed operation log with a movable head.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OpLog {
    ops: Vec<Operation>,
    /// Id of the operation representing the current effective state.
    head: Option<String>,
}

impl OpLog {
    pub fn new() -> OpLog {
        OpLog::default()
    }

    /// Record an operation, advancing head to it. Returns its id.
    pub fn record(
        &mut self,
        kind: OpKind,
        agent: &str,
        task_id: &str,
        summary: &str,
        files: Vec<String>,
    ) -> String {
        let seq = self.ops.len() as u64;
        let parent = self.head.clone();
        let id = hash_operation(seq, parent.as_deref(), agent, task_id, kind, summary, &files);
        self.ops.push(Operation {
            id: id.clone(),
            seq,
            parent: parent.clone(),
            agent: agent.to_string(),
            task_id: task_id.to_string(),
            kind,
            summary: summary.to_string(),
            files,
        });
        self.head = Some(id.clone());
        id
    }

    /// All operations ever recorded (the audit history, including undos).
    pub fn history(&self) -> &[Operation] {
        &self.ops
    }

    /// The operation representing the current effective state.
    pub fn head(&self) -> Option<&Operation> {
        self.head.as_deref().and_then(|id| self.find(id))
    }

    /// Undo the last effective operation: records an `Undo` (for audit) and
    /// moves head back to the undone op's parent. Returns the restored head id.
    pub fn undo(&mut self, agent: &str) -> Option<String> {
        let current = self.head()?.clone();
        let restore_to = current.parent.clone();
        self.record(
            OpKind::Undo,
            agent,
            &current.task_id,
            &format!("undo {}", short(&current.id)),
            Vec::new(),
        );
        self.head = restore_to.clone();
        restore_to
    }

    /// Revert an entire agent/task session: move head to the state *before* the
    /// task's first operation. Records an `Undo` for audit. Returns the restored
    /// head id (or `None` if reverted to the empty state).
    pub fn revert_task(&mut self, agent: &str, task_id: &str) -> Option<String> {
        let restore_to = self
            .ops
            .iter()
            .filter(|op| op.task_id == task_id && op.kind != OpKind::Undo)
            .min_by_key(|op| op.seq)
            .and_then(|first| first.parent.clone());
        self.record(
            OpKind::Undo,
            agent,
            task_id,
            &format!("revert task {task_id}"),
            Vec::new(),
        );
        self.head = restore_to.clone();
        restore_to
    }

    /// Operations attributed to an agent.
    pub fn by_agent(&self, agent: &str) -> Vec<&Operation> {
        self.ops.iter().filter(|op| op.agent == agent).collect()
    }

    /// Operations belonging to a task.
    pub fn by_task(&self, task_id: &str) -> Vec<&Operation> {
        self.ops.iter().filter(|op| op.task_id == task_id).collect()
    }

    pub fn load(path: &std::path::Path) -> std::io::Result<OpLog> {
        match std::fs::read_to_string(path) {
            Ok(text) => serde_json::from_str(&text)
                .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(OpLog::default()),
            Err(error) => Err(error),
        }
    }

    pub fn save(&self, path: &std::path::Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = serde_json::to_string_pretty(self)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
        std::fs::write(path, text)
    }

    fn find(&self, id: &str) -> Option<&Operation> {
        self.ops.iter().find(|op| op.id == id)
    }
}

/// Content hash of an operation's fields → its id.
fn hash_operation(
    seq: u64,
    parent: Option<&str>,
    agent: &str,
    task_id: &str,
    kind: OpKind,
    summary: &str,
    files: &[String],
) -> String {
    let payload = format!(
        "{seq}\0{}\0{agent}\0{task_id}\0{kind:?}\0{summary}\0{}",
        parent.unwrap_or(""),
        files.join(",")
    );
    format!("{:016x}", fnv1a(payload.as_bytes()))
}

fn short(id: &str) -> &str {
    &id[..id.len().min(8)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_chain_with_advancing_head() {
        let mut log = OpLog::new();
        let a = log.record(OpKind::Snapshot, "agent-a", "t1", "first", vec!["f.ts".into()]);
        let b = log.record(OpKind::Snapshot, "agent-a", "t1", "second", vec!["g.ts".into()]);
        assert_eq!(log.head().unwrap().id, b);
        assert_eq!(log.head().unwrap().parent.as_deref(), Some(a.as_str()));
        assert_eq!(log.history().len(), 2);
    }

    #[test]
    fn ids_are_deterministic_but_unique_per_position() {
        let mut log = OpLog::new();
        let a = log.record(OpKind::Snapshot, "x", "t", "s", vec![]);
        let b = log.record(OpKind::Snapshot, "x", "t", "s", vec![]);
        // same fields, different seq/parent ⇒ different ids.
        assert_ne!(a, b);
        // re-hashing the first op's exact inputs reproduces its id.
        let reproduced = hash_operation(0, None, "x", "t", OpKind::Snapshot, "s", &[]);
        assert_eq!(reproduced, a);
    }

    #[test]
    fn undo_moves_head_back_and_audits() {
        let mut log = OpLog::new();
        let a = log.record(OpKind::Snapshot, "agent-a", "t1", "first", vec![]);
        log.record(OpKind::Snapshot, "agent-a", "t1", "second", vec![]);
        let restored = log.undo("agent-a");
        assert_eq!(restored.as_deref(), Some(a.as_str()));
        assert_eq!(log.head().unwrap().id, a);
        // the undo itself is retained in history for audit.
        assert!(log.history().iter().any(|op| op.kind == OpKind::Undo));
    }

    #[test]
    fn revert_task_restores_pre_task_state() {
        let mut log = OpLog::new();
        let base = log.record(OpKind::Snapshot, "human", "setup", "base", vec![]);
        log.record(OpKind::Snapshot, "agent-a", "feature", "a1", vec![]);
        log.record(OpKind::Snapshot, "agent-a", "feature", "a2", vec![]);
        let restored = log.revert_task("orchestrator", "feature");
        // back to the state before the feature task began.
        assert_eq!(restored.as_deref(), Some(base.as_str()));
        assert_eq!(log.head().unwrap().id, base);
    }

    #[test]
    fn attribution_queries() {
        let mut log = OpLog::new();
        log.record(OpKind::Snapshot, "agent-a", "t1", "x", vec![]);
        log.record(OpKind::Snapshot, "agent-b", "t2", "y", vec![]);
        log.record(OpKind::Snapshot, "agent-a", "t1", "z", vec![]);
        assert_eq!(log.by_agent("agent-a").len(), 2);
        assert_eq!(log.by_task("t2").len(), 1);
    }

    #[test]
    fn round_trips_through_disk() {
        let dir = std::env::temp_dir().join(format!("ad-oplog-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("oplog.json");
        let mut log = OpLog::new();
        log.record(OpKind::Snapshot, "a", "t", "s", vec!["f.ts".into()]);
        log.save(&path).unwrap();
        let loaded = OpLog::load(&path).unwrap();
        assert_eq!(loaded.head().unwrap().id, log.head().unwrap().id);
        assert_eq!(loaded.history().len(), 1);
        std::fs::remove_dir_all(&dir).ok();
    }
}
