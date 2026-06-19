use std::collections::HashSet;

use agent_doctor_core::{all_metas, example_for};

#[test]
fn every_rule_has_a_rewrite_example() {
    let missing: Vec<&str> = all_metas()
        .into_iter()
        .map(|meta| meta.id)
        .filter(|id| example_for(id).is_none())
        .collect();
    assert!(
        missing.is_empty(),
        "rules without rewrite examples: {missing:?}"
    );
}

#[test]
fn rule_ids_are_unique() {
    let metas = all_metas();
    let mut seen = HashSet::new();
    let duplicates: Vec<&str> = metas
        .iter()
        .map(|meta| meta.id)
        .filter(|id| !seen.insert(*id))
        .collect();
    assert!(duplicates.is_empty(), "duplicate rule ids: {duplicates:?}");
}

#[test]
fn catalog_size_matches_expectation() {
    // Bump deliberately when adding rules — catches metas() lists that were
    // not updated when a new RuleMeta was added to a file.
    assert_eq!(all_metas().len(), 118);
}
