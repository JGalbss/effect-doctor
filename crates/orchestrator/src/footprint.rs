//! Footprint estimation and live cross-draft dedup.
//!
//! - **Footprint**: from a task's target globs, the files it's likely to touch —
//!   the matching files plus everything that imports them (their dependents),
//!   computed from the warm import graph. This drives lease size.
//! - **Frontier dedup**: across the in-flight drafts of *concurrent* tasks,
//!   flag the same helper being written twice (or a helper that already exists
//!   in the base) before either lands — the orchestration-only superpower.

use std::collections::BTreeMap;

use agent_doctor_core::SymbolGraph;
use agent_doctor_policy::glob;
use agent_doctor_server::Kernel;
use serde::{Deserialize, Serialize};

/// One file an in-flight task proposes to write.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChange {
    pub path: String,
    pub new_source: String,
}

/// A concurrent task's uncommitted draft.
#[derive(Debug, Clone)]
pub struct Draft {
    pub task_id: String,
    pub changes: Vec<FileChange>,
}

/// A duplicate definition found across the live frontier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrontierDup {
    pub symbol: String,
    /// In-flight tasks that define this symbol (sorted). One entry + `existing_in_base`
    /// means the task reinvents something that already exists.
    pub task_ids: Vec<String>,
    /// Whether a symbol of this name already exists in the committed base.
    pub existing_in_base: bool,
}

/// Estimate the files a task will touch: glob-matched files plus their
/// dependents. Falls back to the raw target globs when nothing matches yet.
pub fn estimate_footprint(kernel: &Kernel, targets: &[String]) -> Vec<String> {
    let matched: Vec<String> = kernel
        .graph()
        .files()
        .map(|file| file.path.clone())
        .filter(|path| glob::matches_any(targets, path))
        .collect();
    if matched.is_empty() {
        return targets.to_vec();
    }
    // `impact.affected` is exactly matched-files ∪ their transitive dependents.
    kernel.impact(&matched, Vec::new()).affected
}

/// Flag symbols defined by more than one in-flight draft, or that already exist
/// in the committed base — so the orchestrator can dedup before merge.
pub fn frontier_dedup(base: &SymbolGraph, drafts: &[Draft]) -> Vec<FrontierDup> {
    // symbol name → set of task ids defining it across drafts.
    let mut by_symbol: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for draft in drafts {
        let mut seen_here: Vec<String> = Vec::new();
        for change in &draft.changes {
            for def in SymbolGraph::analyze(&change.path, &change.new_source).defs {
                if !seen_here.contains(&def.name) {
                    seen_here.push(def.name);
                }
            }
        }
        for symbol in seen_here {
            let entry = by_symbol.entry(symbol).or_default();
            if !entry.contains(&draft.task_id) {
                entry.push(draft.task_id.clone());
            }
        }
    }

    by_symbol
        .into_iter()
        .filter_map(|(symbol, mut task_ids)| {
            let existing_in_base = !base.definitions_named(&symbol).is_empty();
            let duplicated = task_ids.len() > 1 || existing_in_base;
            if !duplicated {
                return None;
            }
            task_ids.sort();
            Some(FrontierDup {
                symbol,
                task_ids,
                existing_in_base,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    fn kernel_with(files: &[(&str, &str)]) -> (Kernel, std::path::PathBuf) {
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("ad-fp-{}-{}", std::process::id(), unique));
        for (name, source) in files {
            let path = dir.join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, source).unwrap();
        }
        (Kernel::build_bare(&dir), dir)
    }

    #[test]
    fn footprint_includes_module_and_dependents() {
        let (kernel, dir) = kernel_with(&[
            ("src/auth/login.ts", "export const login = 1"),
            ("src/app/main.ts", "import { login } from '../auth/login'"),
        ]);
        let footprint = estimate_footprint(&kernel, &["src/auth/**".to_string()]);
        assert!(footprint.contains(&"src/auth/login.ts".to_string()));
        assert!(footprint.contains(&"src/app/main.ts".to_string())); // dependent
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn frontier_flags_two_tasks_same_helper() {
        let base = SymbolGraph::new();
        let drafts = vec![
            Draft {
                task_id: "t1".to_string(),
                changes: vec![FileChange {
                    path: "src/a.ts".to_string(),
                    new_source: "export function slugify(s) { return s }".to_string(),
                }],
            },
            Draft {
                task_id: "t2".to_string(),
                changes: vec![FileChange {
                    path: "src/b.ts".to_string(),
                    new_source: "export function slugify(s) { return s }".to_string(),
                }],
            },
        ];
        let dups = frontier_dedup(&base, &drafts);
        assert_eq!(dups.len(), 1);
        assert_eq!(dups[0].symbol, "slugify");
        assert_eq!(dups[0].task_ids, vec!["t1", "t2"]);
        assert!(!dups[0].existing_in_base);
    }

    #[test]
    fn frontier_flags_reinvention_of_existing() {
        let mut base = SymbolGraph::new();
        base.add_file("src/util.ts", "export function slugify(s) { return s }");
        let drafts = vec![Draft {
            task_id: "t1".to_string(),
            changes: vec![FileChange {
                path: "src/new.ts".to_string(),
                new_source: "export function slugify(s) { return s }".to_string(),
            }],
        }];
        let dups = frontier_dedup(&base, &drafts);
        assert_eq!(dups.len(), 1);
        assert!(dups[0].existing_in_base);
    }

    #[test]
    fn no_dup_when_distinct() {
        let base = SymbolGraph::new();
        let drafts = vec![
            Draft {
                task_id: "t1".to_string(),
                changes: vec![FileChange {
                    path: "src/a.ts".to_string(),
                    new_source: "export const a = 1".to_string(),
                }],
            },
            Draft {
                task_id: "t2".to_string(),
                changes: vec![FileChange {
                    path: "src/b.ts".to_string(),
                    new_source: "export const b = 2".to_string(),
                }],
            },
        ];
        assert!(frontier_dedup(&base, &drafts).is_empty());
    }
}
