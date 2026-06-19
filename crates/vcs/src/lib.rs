//! Version-control abstraction for the toolkit (toolkit Layer 6, foundation).
//!
//! The toolkit needs only a handful of VCS operations: list a diff, read a file
//! at a revision, and spin/tear-down an isolated workspace per agent. Those sit
//! behind the [`Vcs`] trait so the agent-native VCS (the deferred jj-inspired
//! engine) can swap in later without touching the orchestrator. The semantic
//! 3-way merge is VCS-agnostic and provided as a default method over the trait.
//!
//! [`GitVcs`] is the shell-to-git implementation used today. [`Repo`] composes
//! it with the agent-native model (op-log + leases + semantic merge).

mod oplog;
mod repo;

use std::path::{Path, PathBuf};
use std::process::Command;

use agent_doctor_merge::{merge, MergeResult};

pub use oplog::{OpKind, OpLog, Operation};
pub use repo::Repo;

/// The operations the toolkit needs from a version-control backend.
pub trait Vcs {
    /// Files changed between `base` and the current state (repo-relative).
    fn changed_files(&self, base: &str) -> Result<Vec<String>, String>;

    /// Contents of `path` at revision `reference`, or `None` if absent there.
    fn file_at(&self, reference: &str, path: &str) -> Result<Option<String>, String>;

    /// Create an isolated workspace named `name` checked out at `base`.
    fn create_workspace(&self, name: &str, base: &str) -> Result<PathBuf, String>;

    /// Tear down a workspace created by [`Vcs::create_workspace`].
    fn remove_workspace(&self, name: &str) -> Result<(), String>;

    /// Semantic 3-way merge of `path` across three revisions. Default impl reads
    /// the three versions via [`Vcs::file_at`] and runs the AST merge — backend
    /// independent, so every `Vcs` gets conflict-aware merging for free.
    fn merge_file(
        &self,
        path: &str,
        base_ref: &str,
        ours_ref: &str,
        theirs_ref: &str,
    ) -> Result<MergeResult, String> {
        let base = self.file_at(base_ref, path)?.unwrap_or_default();
        let ours = self.file_at(ours_ref, path)?.unwrap_or_default();
        let theirs = self.file_at(theirs_ref, path)?.unwrap_or_default();
        Ok(merge(&base, &ours, &theirs))
    }
}

/// A git-backed [`Vcs`] that shells out to `git`.
pub struct GitVcs {
    root: PathBuf,
}

impl GitVcs {
    pub fn new(root: impl AsRef<Path>) -> GitVcs {
        GitVcs {
            root: root.as_ref().to_path_buf(),
        }
    }

    fn git(&self, args: &[&str]) -> Result<String, String> {
        let output = Command::new("git")
            .arg("-C")
            .arg(&self.root)
            .args(args)
            .output()
            .map_err(|error| format!("failed to run git: {error}"))?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    fn workspace_path(&self, name: &str) -> PathBuf {
        self.root.join(".agent-doctor").join("wt").join(name)
    }
}

impl Vcs for GitVcs {
    fn changed_files(&self, base: &str) -> Result<Vec<String>, String> {
        let stdout = self.git(&["diff", "--name-only", "--diff-filter=ACMR", base])?;
        Ok(stdout.lines().map(str::to_string).collect())
    }

    fn file_at(&self, reference: &str, path: &str) -> Result<Option<String>, String> {
        // `git show <ref>:<path>` errors when the path is absent there — that's
        // a legitimate "not present" answer, not a hard failure.
        match self.git(&["show", &format!("{reference}:{path}")]) {
            Ok(contents) => Ok(Some(contents)),
            Err(_) => Ok(None),
        }
    }

    fn create_workspace(&self, name: &str, base: &str) -> Result<PathBuf, String> {
        let path = self.workspace_path(name);
        let path_str = path.to_string_lossy().into_owned();
        // Detached: ephemeral agent workspaces don't own a branch, and detaching
        // avoids "branch already checked out" when basing on a live branch.
        self.git(&["worktree", "add", "--quiet", "--detach", &path_str, base])?;
        Ok(path)
    }

    fn remove_workspace(&self, name: &str) -> Result<(), String> {
        let path = self.workspace_path(name);
        let path_str = path.to_string_lossy().into_owned();
        self.git(&["worktree", "remove", "--force", &path_str])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    fn run(dir: &Path, args: &[&str]) {
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

    /// A temp git repo with one commit adding `f.ts` on `main`.
    fn temp_repo() -> PathBuf {
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("ad-vcs-{}-{}", std::process::id(), unique));
        std::fs::create_dir_all(&dir).unwrap();
        run(&dir, &["init", "-q", "-b", "main"]);
        run(&dir, &["config", "user.email", "t@t.co"]);
        run(&dir, &["config", "user.name", "t"]);
        std::fs::write(dir.join("f.ts"), "export const a = 1\n").unwrap();
        run(&dir, &["add", "-A"]);
        run(&dir, &["commit", "-qm", "base"]);
        dir
    }

    #[test]
    fn changed_files_reports_working_diff() {
        let dir = temp_repo();
        std::fs::write(dir.join("f.ts"), "export const a = 2\n").unwrap();
        run(&dir, &["add", "-A"]);
        run(&dir, &["commit", "-qm", "change"]);
        let vcs = GitVcs::new(&dir);
        let changed = vcs.changed_files("HEAD~1").unwrap();
        assert_eq!(changed, vec!["f.ts".to_string()]);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn file_at_reads_revision_and_missing() {
        let dir = temp_repo();
        let vcs = GitVcs::new(&dir);
        assert_eq!(
            vcs.file_at("HEAD", "f.ts").unwrap().as_deref(),
            Some("export const a = 1\n")
        );
        assert_eq!(vcs.file_at("HEAD", "nope.ts").unwrap(), None);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn workspace_create_and_remove() {
        let dir = temp_repo();
        let vcs = GitVcs::new(&dir);
        let path = vcs.create_workspace("agent-a", "main").unwrap();
        assert!(path.join("f.ts").exists());
        vcs.remove_workspace("agent-a").unwrap();
        assert!(!path.exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn merge_file_resolves_additive_change_cleanly() {
        let dir = temp_repo();
        // feature adds b; main adds c — different functions, same file.
        run(&dir, &["checkout", "-q", "-b", "feature"]);
        std::fs::write(dir.join("f.ts"), "export const a = 1\nexport const b = 2\n").unwrap();
        run(&dir, &["commit", "-qam", "b"]);
        run(&dir, &["checkout", "-q", "main"]);
        std::fs::write(dir.join("f.ts"), "export const a = 1\nexport const c = 3\n").unwrap();
        run(&dir, &["commit", "-qam", "c"]);

        let vcs = GitVcs::new(&dir);
        // base is the merge-base (the original commit, HEAD~1 of main).
        let result = vcs.merge_file("f.ts", "HEAD~1", "main", "feature").unwrap();
        assert!(result.is_clean(), "conflicts: {:?}", result.conflicts);
        assert!(result.merged.contains("b = 2"));
        assert!(result.merged.contains("c = 3"));
        std::fs::remove_dir_all(&dir).ok();
    }
}
