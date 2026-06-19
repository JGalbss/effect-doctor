//! Region leases: who (which agent) owns which paths right now. A lease grants
//! an actor exclusive write access to a set of path globs for the life of a
//! task, so a fleet of agents can edit disjoint regions in parallel without
//! collisions. Persisted as JSON (default `.agent-doctor/leases.json`).

use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::glob;

/// One actor's claim on a set of path globs.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Lease {
    pub actor: String,
    pub task_id: String,
    pub globs: Vec<String>,
}

/// The set of active leases.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LeaseSet {
    #[serde(default)]
    pub leases: Vec<Lease>,
}

impl LeaseSet {
    /// Load from disk; a missing file is an empty set (not an error).
    pub fn load(path: &Path) -> io::Result<LeaseSet> {
        match std::fs::read_to_string(path) {
            Ok(text) => serde_json::from_str(&text)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(LeaseSet::default()),
            Err(error) => Err(error),
        }
    }

    /// Persist to disk, creating the parent directory if needed.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = serde_json::to_string_pretty(self)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        std::fs::write(path, text)
    }

    /// Acquire a lease, rejecting it if any glob overlaps a *different* actor's
    /// active lease. Re-acquiring for the same task replaces it.
    pub fn acquire(&mut self, lease: Lease) -> Result<(), String> {
        for existing in &self.leases {
            if existing.actor == lease.actor && existing.task_id == lease.task_id {
                continue;
            }
            if existing.actor != lease.actor && leases_overlap(&existing.globs, &lease.globs) {
                return Err(format!(
                    "region already leased by '{}' (task {})",
                    existing.actor, existing.task_id
                ));
            }
        }
        self.leases
            .retain(|existing| !(existing.actor == lease.actor && existing.task_id == lease.task_id));
        self.leases.push(lease);
        Ok(())
    }

    /// Release a task's lease.
    pub fn release(&mut self, task_id: &str) {
        self.leases.retain(|lease| lease.task_id != task_id);
    }

    /// Does `actor` hold a lease covering `path`?
    pub fn covers(&self, actor: &str, path: &str) -> bool {
        self.leases
            .iter()
            .filter(|lease| lease.actor == actor)
            .any(|lease| glob::matches_any(&lease.globs, path))
    }

    /// The actor whose lease covers `path`, if any.
    pub fn owner_of(&self, path: &str) -> Option<&str> {
        self.leases
            .iter()
            .find(|lease| glob::matches_any(&lease.globs, path))
            .map(|lease| lease.actor.as_str())
    }

    /// Is any lease (by any actor) covering `path`?
    pub fn is_leased(&self, path: &str) -> bool {
        self.owner_of(path).is_some()
    }
}

/// Do two glob sets overlap (claim a common path region)?
fn leases_overlap(a: &[String], b: &[String]) -> bool {
    a.iter()
        .any(|left| b.iter().any(|right| globs_overlap(left, right)))
}

/// Conservative glob overlap: compare the literal prefix up to the first
/// wildcard segment. An empty prefix (`**`/`*` lead) is treated as matching
/// everything, so a broad lease conflicts with any other.
fn globs_overlap(a: &str, b: &str) -> bool {
    let pa = base_prefix(a);
    let pb = base_prefix(b);
    pa.is_empty() || pb.is_empty() || is_path_prefix(&pa, &pb) || is_path_prefix(&pb, &pa)
}

/// Path segments before the first one containing a wildcard.
fn base_prefix(pattern: &str) -> String {
    pattern
        .split('/')
        .take_while(|segment| !segment.contains('*'))
        .collect::<Vec<_>>()
        .join("/")
}

/// Is `prefix` equal to `path` or an ancestor directory of it?
fn is_path_prefix(prefix: &str, path: &str) -> bool {
    path == prefix || path.starts_with(&format!("{prefix}/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lease(actor: &str, task: &str, globs: &[&str]) -> Lease {
        Lease {
            actor: actor.to_string(),
            task_id: task.to_string(),
            globs: globs.iter().map(|g| g.to_string()).collect(),
        }
    }

    #[test]
    fn disjoint_regions_both_acquire() {
        let mut set = LeaseSet::default();
        assert!(set.acquire(lease("a", "t1", &["src/auth/**"])).is_ok());
        assert!(set.acquire(lease("b", "t2", &["src/billing/**"])).is_ok());
        assert_eq!(set.leases.len(), 2);
    }

    #[test]
    fn overlapping_regions_conflict() {
        let mut set = LeaseSet::default();
        set.acquire(lease("a", "t1", &["src/auth/**"])).unwrap();
        let err = set.acquire(lease("b", "t2", &["src/auth/login.ts"]));
        assert!(err.is_err());
    }

    #[test]
    fn same_task_reacquire_replaces() {
        let mut set = LeaseSet::default();
        set.acquire(lease("a", "t1", &["src/auth/**"])).unwrap();
        set.acquire(lease("a", "t1", &["src/auth/**", "src/extra/**"]))
            .unwrap();
        assert_eq!(set.leases.len(), 1);
        assert_eq!(set.leases[0].globs.len(), 2);
    }

    #[test]
    fn covers_and_release() {
        let mut set = LeaseSet::default();
        set.acquire(lease("a", "t1", &["src/auth/**"])).unwrap();
        assert!(set.covers("a", "src/auth/login.ts"));
        assert!(!set.covers("b", "src/auth/login.ts"));
        assert_eq!(set.owner_of("src/auth/login.ts"), Some("a"));
        set.release("t1");
        assert!(!set.is_leased("src/auth/login.ts"));
    }

    #[test]
    fn round_trips_through_disk() {
        let dir = std::env::temp_dir().join(format!("ad-lease-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("leases.json");
        let mut set = LeaseSet::default();
        set.acquire(lease("a", "t1", &["src/auth/**"])).unwrap();
        set.save(&path).unwrap();
        let loaded = LeaseSet::load(&path).unwrap();
        assert_eq!(loaded.leases, set.leases);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_missing_is_empty() {
        let set = LeaseSet::load(Path::new("/nonexistent/leases.json")).unwrap();
        assert!(set.leases.is_empty());
    }
}
