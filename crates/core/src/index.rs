//! The kernel index: a warm, incrementally-updatable view of a TypeScript repo.
//!
//! Today it owns the [`SymbolGraph`]; as the toolkit grows this is where the
//! function index, policy state, and impact graph will be co-located so every
//! consumer (impact, policy, merge, orchestrator) reads from one source of
//! truth. Building parses files in parallel; updating re-parses a single file.

use std::path::{Path, PathBuf};

use rayon::prelude::*;

use crate::content_addr::FileId;
use crate::symbol_graph::{FileSymbols, SymbolGraph};
use crate::walk::collect_files;

/// A repo index rooted at a directory.
pub struct Index {
    root: PathBuf,
    graph: SymbolGraph,
}

impl Index {
    /// Walk `root` for scannable TypeScript files and build the index in
    /// parallel. Unreadable files are skipped.
    pub fn build(root: impl AsRef<Path>) -> Index {
        let root = root.as_ref().to_path_buf();
        let parsed: Vec<FileSymbols> = collect_files(&root)
            .par_iter()
            .filter_map(|path| {
                let source = std::fs::read_to_string(path).ok()?;
                let relative = relative_path(&root, path);
                Some(SymbolGraph::analyze(&relative, &source))
            })
            .collect();
        let mut graph = SymbolGraph::new();
        for file in parsed {
            graph.insert(file);
        }
        Index { root, graph }
    }

    /// The symbol graph backing this index.
    pub fn graph(&self) -> &SymbolGraph {
        &self.graph
    }

    /// The root directory.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Re-read one file from disk and patch the index. Removes the file from the
    /// index if it no longer exists. `path` may be absolute or repo-relative.
    pub fn update_file(&mut self, path: impl AsRef<Path>) {
        let absolute = self.absolute(path.as_ref());
        let relative = relative_path(&self.root, &absolute);
        match std::fs::read_to_string(&absolute) {
            Ok(source) => self.graph.add_file(&relative, &source),
            Err(_) => self.graph.remove_file(&relative),
        }
    }

    fn absolute(&self, path: &Path) -> PathBuf {
        if path.is_absolute() {
            return path.to_path_buf();
        }
        self.root.join(path)
    }
}

/// Repo-relative, forward-slash path used as a file's identity in the index.
fn relative_path(root: &Path, path: &Path) -> String {
    let relative = path.strip_prefix(root).unwrap_or(path);
    FileId::new(&relative.to_string_lossy()).0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    /// A unique, empty temp directory for one test (no external dep).
    fn temp_dir() -> PathBuf {
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "agent-doctor-index-{}-{}",
            std::process::id(),
            unique
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(dir: &Path, name: &str, source: &str) {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, source).unwrap();
    }

    #[test]
    fn builds_graph_and_resolves_edges() {
        let dir = temp_dir();
        write(&dir, "a.ts", "export function foo() { return 1 }");
        write(&dir, "b.ts", "import { foo } from './a'\nfoo()");
        let index = Index::build(&dir);
        assert_eq!(index.graph().len(), 2);
        let edges = index.graph().import_edges();
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].from, "b.ts");
        assert_eq!(edges[0].to, "a.ts");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn update_file_reparses_one_and_preserves_others() {
        let dir = temp_dir();
        write(&dir, "a.ts", "export const x = 1");
        write(&dir, "b.ts", "export const y = 1");
        let mut index = Index::build(&dir);
        let a_hash = index.graph().file("a.ts").unwrap().content_hash;

        write(&dir, "b.ts", "export const y = 1\nexport const z = 2");
        index.update_file("b.ts");

        // a.ts untouched (same content hash); b.ts reflects the new def.
        assert_eq!(index.graph().file("a.ts").unwrap().content_hash, a_hash);
        assert_eq!(index.graph().file("b.ts").unwrap().defs.len(), 2);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn update_file_removes_deleted_file() {
        let dir = temp_dir();
        write(&dir, "a.ts", "export const x = 1");
        let mut index = Index::build(&dir);
        assert!(index.graph().file("a.ts").is_some());
        std::fs::remove_file(dir.join("a.ts")).unwrap();
        index.update_file("a.ts");
        assert!(index.graph().file("a.ts").is_none());
        std::fs::remove_dir_all(&dir).ok();
    }
}
