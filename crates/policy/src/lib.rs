//! Policy / ACL / lease evaluation (toolkit Layer 1): the deterministic gate
//! that decides whether a diff is allowed to land.
//!
//! Four rule kinds, all evaluated over the changed-file set:
//!   - **protected** — files no agent may touch,
//!   - **acl**        — path → which actors may write it,
//!   - **layering**   — forbidden import edges between architectural layers,
//!   - **lease**      — an actor may only write within its active lease.
//!
//! The result is a list of facts (`Violation`s), never an opinion — consistent
//! with "leverage, not judgment". A smarter model doesn't make this redundant;
//! ground-truth ownership and architecture boundaries aren't in its context.

pub mod glob;
mod lease;

use std::path::Path;

use agent_doctor_core::SymbolGraph;
use serde::{Deserialize, Serialize};

pub use lease::{Lease, LeaseSet};

/// An architectural layer and the import edges forbidden out of it.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct Layer {
    pub name: String,
    /// Glob of files belonging to this layer.
    pub path: String,
    /// Globs that files in this layer may not import from.
    #[serde(default)]
    pub forbid_imports_from: Vec<String>,
}

/// An access-control entry: only `allow`ed actors may write matching files.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct Acl {
    pub glob: String,
    #[serde(default)]
    pub allow: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct Protected {
    #[serde(default)]
    globs: Vec<String>,
}

/// The parsed policy.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct Policy {
    #[serde(default, rename = "layer")]
    pub layers: Vec<Layer>,
    #[serde(default, rename = "acl")]
    pub acls: Vec<Acl>,
    #[serde(default)]
    protected: Protected,
}

impl Policy {
    /// Parse a policy from TOML text.
    pub fn parse(text: &str) -> Result<Policy, String> {
        toml::from_str(text).map_err(|error| error.to_string())
    }

    /// Load a policy from a file. A missing file yields an empty policy.
    pub fn load(path: &Path) -> Result<Policy, String> {
        match std::fs::read_to_string(path) {
            Ok(text) => Policy::parse(&text),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Policy::default()),
            Err(error) => Err(error.to_string()),
        }
    }

    /// Protected globs (read-only accessor).
    pub fn protected_globs(&self) -> &[String] {
        &self.protected.globs
    }
}

/// What kind of rule a violation breaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ViolationKind {
    Protected,
    Acl,
    Layering,
    Lease,
}

/// A single policy breach found in the diff.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Violation {
    pub kind: ViolationKind,
    pub file: String,
    pub reason: String,
}

/// Everything the gate needs to evaluate a diff.
pub struct GateInput<'a> {
    pub policy: &'a Policy,
    /// Symbol graph, for layering (resolved import edges).
    pub graph: &'a SymbolGraph,
    /// Changed files (repo-relative, forward slashes).
    pub changed: &'a [String],
    /// The acting agent's id, for ACL + lease checks. `None` skips them.
    pub actor: Option<&'a str>,
    /// Active leases, for lease enforcement. `None` skips lease checks.
    pub leases: Option<&'a LeaseSet>,
}

/// Evaluate the policy against a diff, returning every violation (sorted).
pub fn evaluate(input: &GateInput) -> Vec<Violation> {
    let mut violations = Vec::new();
    let changed: std::collections::BTreeSet<&str> =
        input.changed.iter().map(String::as_str).collect();

    for file in &changed {
        check_protected(input.policy, file, &mut violations);
        check_acl(input.policy, input.actor, file, &mut violations);
        check_lease(input.leases, input.actor, file, &mut violations);
    }
    check_layering(input.policy, input.graph, &changed, &mut violations);

    violations.sort_by(|a, b| {
        (a.kind as u8, a.file.as_str(), a.reason.as_str()).cmp(&(
            b.kind as u8,
            b.file.as_str(),
            b.reason.as_str(),
        ))
    });
    violations
}

fn check_protected(policy: &Policy, file: &str, out: &mut Vec<Violation>) {
    if glob::matches_any(policy.protected_globs(), file) {
        out.push(Violation {
            kind: ViolationKind::Protected,
            file: file.to_string(),
            reason: "protected path — no agent may modify it".to_string(),
        });
    }
}

fn check_acl(policy: &Policy, actor: Option<&str>, file: &str, out: &mut Vec<Violation>) {
    for acl in &policy.acls {
        if !glob::matches(&acl.glob, file) {
            continue;
        }
        let permitted = actor.is_some_and(|actor| acl.allow.iter().any(|a| a == actor));
        if !permitted {
            out.push(Violation {
                kind: ViolationKind::Acl,
                file: file.to_string(),
                reason: format!(
                    "{} is not permitted to write '{}' (allowed: {})",
                    actor.unwrap_or("<no actor>"),
                    acl.glob,
                    acl.allow.join(", ")
                ),
            });
        }
    }
}

fn check_lease(leases: Option<&LeaseSet>, actor: Option<&str>, file: &str, out: &mut Vec<Violation>) {
    let (Some(leases), Some(actor)) = (leases, actor) else {
        return;
    };
    // Only enforce on files that someone has leased; unleased files are free.
    if leases.is_leased(file) && !leases.covers(actor, file) {
        let owner = leases.owner_of(file).unwrap_or("another agent");
        out.push(Violation {
            kind: ViolationKind::Lease,
            file: file.to_string(),
            reason: format!("outside {actor}'s lease — region leased by '{owner}'"),
        });
    }
}

fn check_layering(
    policy: &Policy,
    graph: &SymbolGraph,
    changed: &std::collections::BTreeSet<&str>,
    out: &mut Vec<Violation>,
) {
    if policy.layers.is_empty() {
        return;
    }
    // Resolve imports only for the *changed* files (O(changed) — not the whole
    // repo's edges), since a gate only judges the diff.
    for &from in changed {
        let Some(file) = graph.file(from) else {
            continue;
        };
        for import in &file.imports {
            let Some(to) = graph.resolve_import(from, &import.specifier) else {
                continue;
            };
            for layer in &policy.layers {
                if glob::matches(&layer.path, from)
                    && glob::matches_any(&layer.forbid_imports_from, to)
                {
                    out.push(Violation {
                        kind: ViolationKind::Layering,
                        file: from.to_string(),
                        reason: format!(
                            "layer '{}' may not import from '{to}' ({from} → {to})",
                            layer.name
                        ),
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
[[layer]]
name = "core"
path = "src/core/**"
forbid_imports_from = ["src/ui/**"]

[[acl]]
glob = "src/payments/**"
allow = ["agent-payments", "human"]

[protected]
globs = ["**/*.gen.ts"]
"#;

    fn gate(graph: &SymbolGraph, changed: &[&str], actor: Option<&str>) -> Vec<Violation> {
        let policy = Policy::parse(SAMPLE).unwrap();
        let changed: Vec<String> = changed.iter().map(|s| s.to_string()).collect();
        evaluate(&GateInput {
            policy: &policy,
            graph,
            changed: &changed,
            actor,
            leases: None,
        })
    }

    #[test]
    fn parses_all_sections() {
        let policy = Policy::parse(SAMPLE).unwrap();
        assert_eq!(policy.layers.len(), 1);
        assert_eq!(policy.acls.len(), 1);
        assert_eq!(policy.protected_globs(), &["**/*.gen.ts".to_string()]);
    }

    #[test]
    fn protected_file_is_denied() {
        let graph = SymbolGraph::new();
        let violations = gate(&graph, &["src/schema.gen.ts"], Some("human"));
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].kind, ViolationKind::Protected);
    }

    #[test]
    fn acl_denies_wrong_actor_allows_right_one() {
        let graph = SymbolGraph::new();
        let denied = gate(&graph, &["src/payments/charge.ts"], Some("agent-x"));
        assert_eq!(denied.len(), 1);
        assert_eq!(denied[0].kind, ViolationKind::Acl);
        let allowed = gate(&graph, &["src/payments/charge.ts"], Some("agent-payments"));
        assert!(allowed.is_empty());
    }

    #[test]
    fn unrestricted_file_is_allowed() {
        let graph = SymbolGraph::new();
        assert!(gate(&graph, &["src/app/main.ts"], Some("agent-x")).is_empty());
    }

    #[test]
    fn layering_violation_detected() {
        let mut graph = SymbolGraph::new();
        graph.add_file("src/ui/button.ts", "export const b = 1");
        graph.add_file("src/core/engine.ts", "import { b } from '../ui/button'");
        let violations = gate(&graph, &["src/core/engine.ts"], Some("human"));
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].kind, ViolationKind::Layering);
    }

    #[test]
    fn legal_import_passes_layering() {
        let mut graph = SymbolGraph::new();
        graph.add_file("src/core/util.ts", "export const u = 1");
        graph.add_file("src/core/engine.ts", "import { u } from './util'");
        assert!(gate(&graph, &["src/core/engine.ts"], Some("human")).is_empty());
    }

    #[test]
    fn lease_violation_when_outside_actors_region() {
        let graph = SymbolGraph::new();
        let policy = Policy::default();
        let mut leases = LeaseSet::default();
        leases
            .acquire(Lease {
                actor: "agent-a".to_string(),
                task_id: "t1".to_string(),
                globs: vec!["src/auth/**".to_string()],
            })
            .unwrap();
        let changed = vec!["src/auth/login.ts".to_string()];
        // agent-b touching agent-a's leased region → violation.
        let violations = evaluate(&GateInput {
            policy: &policy,
            graph: &graph,
            changed: &changed,
            actor: Some("agent-b"),
            leases: Some(&leases),
        });
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].kind, ViolationKind::Lease);
        // agent-a (the owner) is fine.
        let owner_ok = evaluate(&GateInput {
            policy: &policy,
            graph: &graph,
            changed: &changed,
            actor: Some("agent-a"),
            leases: Some(&leases),
        });
        assert!(owner_ok.is_empty());
    }
}
