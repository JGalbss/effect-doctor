//! Circular-import detection (engine `--agent` pass). Import cycles couple
//! modules, break tree-shaking, and cause initialization-order bugs. Using the
//! repo-wide [`SymbolGraph`], find strongly-connected components of the resolved
//! import graph (Tarjan) and flag every file that participates in one.

use std::collections::HashMap;

use crate::diagnostics::{Category, Diagnostic, FileContext, RuleMeta, Severity};
use crate::lint::is_test_file;
use crate::symbol_graph::SymbolGraph;

static CIRCULAR_IMPORT: RuleMeta = RuleMeta {
    id: "agent-circular-import",
    severity: Severity::Warn,
    category: Category::AgentHygiene,
    help: "This file is part of an import cycle. Cycles couple modules, defeat tree-shaking, and cause initialization-order bugs. Break it: extract the shared types/values into a leaf module, or invert one of the dependencies.",
};

/// Catalog metadata for the cross-file circular-import rule.
pub fn metas() -> &'static [&'static RuleMeta] {
    static METAS: &[&RuleMeta] = &[&CIRCULAR_IMPORT];
    METAS
}

/// A file that participates in an import cycle, with one other cycle member.
pub struct CycleHit {
    pub file: String,
    pub partner: String,
    pub size: usize,
}

/// Find files in import cycles (non-trivial strongly-connected components).
pub fn analyze(graph: &SymbolGraph) -> Vec<CycleHit> {
    let files: Vec<&str> = graph.files().map(|file| file.path.as_str()).collect();
    let index_of: HashMap<&str, usize> = files
        .iter()
        .enumerate()
        .map(|(i, path)| (*path, i))
        .collect();
    let mut adjacency: Vec<Vec<usize>> = vec![Vec::new(); files.len()];
    for edge in graph.import_edges() {
        if let (Some(&from), Some(&to)) = (
            index_of.get(edge.from.as_str()),
            index_of.get(edge.to.as_str()),
        ) {
            adjacency[from].push(to);
        }
    }

    let mut hits = Vec::new();
    for component in tarjan_sccs(&adjacency) {
        if component.len() < 2 {
            continue;
        }
        for &node in &component {
            // Point at the next member so the cycle is easy to trace.
            let partner = component
                .iter()
                .copied()
                .find(|&other| other != node)
                .unwrap_or(node);
            hits.push(CycleHit {
                file: files[node].to_string(),
                partner: files[partner].to_string(),
                size: component.len(),
            });
        }
    }
    hits.sort_by(|a, b| a.file.cmp(&b.file));
    hits
}

/// Build a `Diagnostic` for a cycle member; `snippet` is the file's first line.
pub fn to_diagnostic(hit: &CycleHit, snippet: String) -> Diagnostic {
    let file_context = match is_test_file(&hit.file) {
        true => FileContext::Test,
        false => FileContext::Production,
    };
    let message = format!(
        "import cycle ({} files) — `{}` and `{}` depend on each other; break the cycle",
        hit.size, hit.file, hit.partner
    );
    Diagnostic::from_meta(
        &CIRCULAR_IMPORT,
        message,
        hit.file.clone(),
        file_context,
        1,
        1,
        snippet,
    )
}

/// Tarjan's strongly-connected-components, iterative to avoid deep recursion on
/// large graphs.
fn tarjan_sccs(adjacency: &[Vec<usize>]) -> Vec<Vec<usize>> {
    let n = adjacency.len();
    let mut index_of = vec![usize::MAX; n];
    let mut low = vec![0usize; n];
    let mut on_stack = vec![false; n];
    let mut stack: Vec<usize> = Vec::new();
    let mut next_index = 0usize;
    let mut sccs = Vec::new();

    // Explicit work stack of (node, next-neighbor-cursor).
    for start in 0..n {
        if index_of[start] != usize::MAX {
            continue;
        }
        let mut work: Vec<(usize, usize)> = vec![(start, 0)];
        while let Some((node, cursor)) = work.pop() {
            if cursor == 0 {
                index_of[node] = next_index;
                low[node] = next_index;
                next_index += 1;
                stack.push(node);
                on_stack[node] = true;
            }
            let mut recursed = false;
            for edge_index in cursor..adjacency[node].len() {
                let next = adjacency[node][edge_index];
                if index_of[next] == usize::MAX {
                    // Resume `node` after `edge_index`, then descend into `next`.
                    work.push((node, edge_index + 1));
                    work.push((next, 0));
                    recursed = true;
                    break;
                }
                if on_stack[next] {
                    low[node] = low[node].min(index_of[next]);
                }
            }
            if recursed {
                continue;
            }
            // Finished `node`: on return, relax the parent's low-link.
            if low[node] == index_of[node] {
                let mut component = Vec::new();
                while let Some(member) = stack.pop() {
                    on_stack[member] = false;
                    component.push(member);
                    if member == node {
                        break;
                    }
                }
                sccs.push(component);
            }
            if let Some(&(parent, _)) = work.last() {
                low[parent] = low[parent].min(low[node]);
            }
        }
    }
    sccs
}
