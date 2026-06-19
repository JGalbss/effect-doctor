//! The context server's engine: a warm [`Kernel`] holding the index,
//! dependency graph, policy, and leases, answering the queries an orchestrator
//! or agent asks (toolkit Layer 4 — the context server). Every answer is a fact
//! derived from the index, not a judgement; the agent stays smart, the kernel
//! supplies ground truth.

mod dispatch;

use std::path::Path;

use agent_doctor_core::{Index, SymbolKind};
use agent_doctor_impact::{DepGraph, ImpactConfig, ImpactResult};
use agent_doctor_policy::{evaluate, GateInput, LeaseSet, Policy, Violation};
use serde::Serialize;

pub use dispatch::{handle, serve};

/// One definition the kernel knows about.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SymbolHit {
    pub name: String,
    pub file: String,
    pub kind: &'static str,
    pub exported: bool,
    pub line: u32,
}

/// The minimal context for a task: what to test, what would be denied, and the
/// reusable symbols already present in the affected code (so agents don't
/// reinvent helpers).
#[derive(Debug, Clone, Serialize)]
pub struct ContextPack {
    pub impacted_tests: Vec<String>,
    pub gate_preview: Vec<Violation>,
    pub related_symbols: Vec<SymbolHit>,
    pub caveats: Vec<String>,
}

/// Cap on symbols returned in a context pack — keeps the agent's window small.
const CONTEXT_SYMBOL_CAP: usize = 50;

/// The warm kernel.
pub struct Kernel {
    index: Index,
    deps: DepGraph,
    policy: Policy,
    leases: LeaseSet,
}

impl Kernel {
    /// Build the kernel: index the repo, build the dependency graph, load policy
    /// and leases (missing files are treated as empty).
    pub fn build(root: &Path, policy_path: &Path, leases_path: &Path) -> Result<Kernel, String> {
        let index = Index::build(root);
        let deps = DepGraph::build(index.graph());
        let policy = Policy::load(policy_path)?;
        let leases = LeaseSet::load(leases_path).map_err(|error| error.to_string())?;
        Ok(Kernel {
            index,
            deps,
            policy,
            leases,
        })
    }

    /// Build with default (empty) policy and leases — handy for tests.
    pub fn build_bare(root: &Path) -> Kernel {
        let index = Index::build(root);
        let deps = DepGraph::build(index.graph());
        Kernel {
            index,
            deps,
            policy: Policy::default(),
            leases: LeaseSet::default(),
        }
    }

    /// Push-based incremental refresh: re-read one file and rebuild the
    /// dependency graph. (The graph rebuild is O(edges); incremental dep updates
    /// are a future optimisation.)
    pub fn update_file(&mut self, path: impl AsRef<Path>) {
        self.index.update_file(path);
        self.deps = DepGraph::build(self.index.graph());
    }

    /// Definitions matching `name` across the repo (the "does this exist?" query).
    pub fn symbol_exists(&self, name: &str) -> Vec<SymbolHit> {
        self.index
            .graph()
            .definitions_named(name)
            .into_iter()
            .map(|(file, def)| SymbolHit {
                name: def.name.clone(),
                file: file.to_string(),
                kind: kind_str(def.kind),
                exported: def.exported,
                line: def.line,
            })
            .collect()
    }

    /// Tests reaching a change.
    pub fn impact(&self, changed: &[String], always_run: Vec<String>) -> ImpactResult {
        self.deps.select(changed, &ImpactConfig { always_run })
    }

    /// Gate a change against policy + leases.
    pub fn gate(&self, changed: &[String], actor: Option<&str>) -> Vec<Violation> {
        evaluate(&GateInput {
            policy: &self.policy,
            graph: self.index.graph(),
            changed,
            actor,
            leases: Some(&self.leases),
        })
    }

    /// Assemble the minimal context pack for a task touching `changed`.
    pub fn context_pack(&self, changed: &[String], actor: Option<&str>) -> ContextPack {
        let impact = self.impact(changed, Vec::new());
        let gate_preview = self.gate(changed, actor);
        let related_symbols = self.related_symbols(&impact.affected);
        ContextPack {
            impacted_tests: impact.tests,
            gate_preview,
            related_symbols,
            caveats: impact.caveats,
        }
    }

    /// Exported definitions in the affected non-test files — reusable helpers
    /// the agent should know about before writing new ones.
    fn related_symbols(&self, affected: &[String]) -> Vec<SymbolHit> {
        let mut hits = Vec::new();
        for file in affected {
            if agent_doctor_core::is_test_file(file) {
                continue;
            }
            let Some(symbols) = self.index.graph().file(file) else {
                continue;
            };
            for def in symbols.defs.iter().filter(|def| def.exported) {
                hits.push(SymbolHit {
                    name: def.name.clone(),
                    file: file.clone(),
                    kind: kind_str(def.kind),
                    exported: def.exported,
                    line: def.line,
                });
                if hits.len() >= CONTEXT_SYMBOL_CAP {
                    return hits;
                }
            }
        }
        hits
    }
}

fn kind_str(kind: SymbolKind) -> &'static str {
    match kind {
        SymbolKind::Function => "function",
        SymbolKind::Class => "class",
        SymbolKind::Const => "const",
        SymbolKind::Variable => "variable",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    fn temp_project(files: &[(&str, &str)]) -> PathBuf {
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("ad-server-{}-{}", std::process::id(), unique));
        for (name, source) in files {
            let path = dir.join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, source).unwrap();
        }
        dir
    }

    #[test]
    fn symbol_exists_finds_definitions() {
        let dir = temp_project(&[("src/util.ts", "export function isValidEmail(s) { return true }")]);
        let kernel = Kernel::build_bare(&dir);
        let hits = kernel.symbol_exists("isValidEmail");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].file, "src/util.ts");
        assert_eq!(hits[0].kind, "function");
        assert!(hits[0].exported);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn context_pack_surfaces_tests_and_helpers() {
        let dir = temp_project(&[
            ("src/math.ts", "export function add(a, b) { return a + b }"),
            ("test/math.test.ts", "import { add } from '../src/math'"),
        ]);
        let kernel = Kernel::build_bare(&dir);
        let pack = kernel.context_pack(&["src/math.ts".to_string()], None);
        assert_eq!(pack.impacted_tests, vec!["test/math.test.ts".to_string()]);
        assert!(pack.related_symbols.iter().any(|hit| hit.name == "add"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn update_file_is_reflected() {
        let dir = temp_project(&[("src/a.ts", "export const x = 1")]);
        let mut kernel = Kernel::build_bare(&dir);
        assert!(kernel.symbol_exists("y").is_empty());
        std::fs::write(dir.join("src/a.ts"), "export const x = 1\nexport const y = 2").unwrap();
        kernel.update_file("src/a.ts");
        assert_eq!(kernel.symbol_exists("y").len(), 1);
        std::fs::remove_dir_all(&dir).ok();
    }
}
