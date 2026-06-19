//! Impact-based test selection (toolkit Layer 2).
//!
//! Given the kernel [`SymbolGraph`] and a set of changed files, compute the
//! transitively-affected files and, among them, the tests worth running. This is
//! pure leverage: a fact (which tests reach the change) the model cannot read
//! off its context window, computed deterministically from the import graph.
//!
//! Granularity is file-level: a test is selected when it (transitively) imports
//! a changed file. Static graphs under-approximate dynamic dispatch, so any
//! `import()`/`require()` in the affected set raises an explicit caveat rather
//! than being silently dropped — callers pair selection with an always-run set.

use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};

use agent_doctor_core::{is_test_file, SymbolGraph};
use serde::Serialize;

/// Knobs for selection.
#[derive(Debug, Clone, Default)]
pub struct ImpactConfig {
    /// Tests that always run regardless of the diff (smoke / integration that
    /// the static graph can't connect to the change).
    pub always_run: Vec<String>,
}

/// The result of a selection.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ImpactResult {
    /// Test files to run, sorted and de-duplicated.
    pub tests: Vec<String>,
    /// Every file transitively affected by the change (includes the changed
    /// files and non-test dependents), sorted.
    pub affected: Vec<String>,
    /// Non-fatal warnings about possible under-selection.
    pub caveats: Vec<String>,
}

/// A precomputed reverse-dependency graph: the expensive import-resolution pass
/// is done once at [`DepGraph::build`], so each [`DepGraph::select`] is just a
/// graph walk. A warm server builds this once per index revision and reuses it
/// across thousands of selections.
pub struct DepGraph {
    /// file → files that import it (its direct dependents).
    dependents: HashMap<String, Vec<String>>,
    /// Files that use dynamic `import()`/`require()`.
    dynamic: HashSet<String>,
}

impl DepGraph {
    /// Build the reverse-dependency graph from a symbol graph (one-time cost).
    pub fn build(graph: &SymbolGraph) -> DepGraph {
        let mut dependents: HashMap<String, Vec<String>> = HashMap::new();
        for edge in graph.import_edges() {
            dependents.entry(edge.to).or_default().push(edge.from);
        }
        let dynamic = graph
            .files()
            .filter(|file| file.dynamic_imports)
            .map(|file| file.path.clone())
            .collect();
        DepGraph {
            dependents,
            dynamic,
        }
    }

    /// Select the tests reaching `changed`, plus the always-run set.
    pub fn select(&self, changed: &[String], config: &ImpactConfig) -> ImpactResult {
        let affected = self.affected_set(changed);
        let mut tests: BTreeSet<String> = affected
            .iter()
            .filter(|file| is_test_file(file))
            .cloned()
            .collect();
        tests.extend(config.always_run.iter().cloned());
        let caveats = self.caveats(&affected);
        ImpactResult {
            tests: tests.into_iter().collect(),
            affected: affected.into_iter().collect(),
            caveats,
        }
    }

    /// Transitive closure of dependents: every file that directly or indirectly
    /// imports any changed file, plus the changed files themselves.
    fn affected_set(&self, changed: &[String]) -> BTreeSet<String> {
        let mut seen: BTreeSet<String> = BTreeSet::new();
        let mut queue: VecDeque<&str> = changed.iter().map(String::as_str).collect();
        while let Some(file) = queue.pop_front() {
            if !seen.insert(file.to_string()) {
                continue;
            }
            if let Some(importers) = self.dependents.get(file) {
                for importer in importers {
                    if !seen.contains(importer.as_str()) {
                        queue.push_back(importer);
                    }
                }
            }
        }
        seen
    }

    /// Warn when the affected set contains dynamic imports the static graph
    /// can't follow — the one place file-level selection under-approximates.
    fn caveats(&self, affected: &BTreeSet<String>) -> Vec<String> {
        let dynamic: Vec<&str> = affected
            .iter()
            .filter(|file| self.dynamic.contains(file.as_str()))
            .map(String::as_str)
            .collect();
        if dynamic.is_empty() {
            return Vec::new();
        }
        let sample = dynamic.iter().take(3).copied().collect::<Vec<_>>().join(", ");
        vec![format!(
            "{} affected file(s) use dynamic import()/require() ({sample}{}) — \
             dependencies may be hidden and selection can under-approximate; \
             the always-run set is the safety net",
            dynamic.len(),
            if dynamic.len() > 3 { ", …" } else { "" }
        )]
    }
}

/// One-shot selection (builds a [`DepGraph`] and discards it). Use [`DepGraph`]
/// directly when selecting repeatedly against the same index.
pub fn select(graph: &SymbolGraph, changed: &[String], config: &ImpactConfig) -> ImpactResult {
    DepGraph::build(graph).select(changed, config)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// math ← calc, math ← math.test, calc ← calc.test
    fn fixture() -> SymbolGraph {
        let mut graph = SymbolGraph::new();
        graph.add_file("src/math.ts", "export function add(a, b) { return a + b }");
        graph.add_file("src/calc.ts", "import { add } from './math'\nexport const c = add");
        graph.add_file("test/math.test.ts", "import { add } from '../src/math'");
        graph.add_file("test/calc.test.ts", "import { c } from '../src/calc'");
        graph
    }

    #[test]
    fn selects_direct_and_transitive_tests() {
        let graph = fixture();
        let result = select(&graph, &["src/math.ts".to_string()], &ImpactConfig::default());
        assert_eq!(
            result.tests,
            vec!["test/calc.test.ts".to_string(), "test/math.test.ts".to_string()]
        );
    }

    #[test]
    fn narrow_change_selects_only_reaching_tests() {
        let graph = fixture();
        let result = select(&graph, &["src/calc.ts".to_string()], &ImpactConfig::default());
        assert_eq!(result.tests, vec!["test/calc.test.ts".to_string()]);
    }

    #[test]
    fn always_run_is_unioned_in() {
        let graph = fixture();
        let config = ImpactConfig {
            always_run: vec!["test/smoke.test.ts".to_string()],
        };
        let result = select(&graph, &["src/calc.ts".to_string()], &config);
        assert!(result.tests.contains(&"test/smoke.test.ts".to_string()));
        assert!(result.tests.contains(&"test/calc.test.ts".to_string()));
    }

    #[test]
    fn dynamic_import_raises_caveat() {
        let mut graph = SymbolGraph::new();
        graph.add_file("src/loader.ts", "export const load = () => import('./plugin')");
        graph.add_file("test/loader.test.ts", "import { load } from '../src/loader'");
        let result = select(&graph, &["src/loader.ts".to_string()], &ImpactConfig::default());
        assert_eq!(result.tests, vec!["test/loader.test.ts".to_string()]);
        assert_eq!(result.caveats.len(), 1);
        assert!(result.caveats[0].contains("dynamic"));
    }

    #[test]
    fn no_caveat_when_static() {
        let graph = fixture();
        let result = select(&graph, &["src/math.ts".to_string()], &ImpactConfig::default());
        assert!(result.caveats.is_empty());
    }
}
