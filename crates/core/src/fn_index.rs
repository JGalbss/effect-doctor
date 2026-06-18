//! Cross-file "this already exists" detector. Agents reimplement helpers that
//! the codebase already has because they lack repo-wide context; this builds a
//! repo-wide index of named/bound functions and flags ones that duplicate
//! another by four signals, strongest first:
//!
//! 1. exact structural body  → `agent-duplicate-cross-file`
//! 2. fuzzy structural body   → `agent-near-duplicate-function`
//! 3. same (non-generic) name → `agent-similar-function-name`
//! 4. same behavioural shape  → `agent-similar-shape`
//!
//! Each function is reported once, under its strongest signal, pointing at the
//! match's location so the agent can reuse it instead. All findings are `info`
//! suggestions — they never affect the score.

use std::collections::HashMap;

use oxc_ast::ast::{BindingPattern, Expression, Function, Program, VariableDeclarator};
use oxc_ast_visit::{walk, Visit};
use oxc_syntax::scope::ScopeFlags;

use crate::diagnostics::{Category, Diagnostic, FileContext, RuleMeta, Severity};
use crate::lint::classify_file;
use crate::matchers::unwrap_parens;
use crate::structural::{self, Shape};

static DUPLICATE_CROSS_FILE: RuleMeta = RuleMeta {
    id: "agent-duplicate-cross-file",
    severity: Severity::Info,
    category: Category::AgentHygiene,
    help: "A function with a structurally identical body exists in another file — agents re-create helpers they can't see. Import and reuse the existing one (or extract a shared module).",
};

static NEAR_DUPLICATE: RuleMeta = RuleMeta {
    id: "agent-near-duplicate-function",
    severity: Severity::Info,
    category: Category::AgentHygiene,
    help: "A function in another file is structurally near-identical (a lightly-edited copy). Confirm it isn't the same logic reimplemented, and consolidate to one helper.",
};

static SIMILAR_NAME: RuleMeta = RuleMeta {
    id: "agent-similar-function-name",
    severity: Severity::Info,
    category: Category::AgentHygiene,
    help: "Another function with the same name is defined elsewhere — likely a duplicate implementation of the same concept. Reuse the existing one or rename to disambiguate.",
};

static SIMILAR_SHAPE: RuleMeta = RuleMeta {
    id: "agent-similar-shape",
    severity: Severity::Info,
    category: Category::AgentHygiene,
    help: "Another function takes the same parameters and calls the same helpers — it may accomplish the same goal by a different route. Check whether one can be derived from or replaced by the other.",
};

/// Metas for the cross-file rules — appended to the global catalog since these
/// fire from the engine pass, not the per-file [`crate::rules::Rule`] dispatch.
pub fn cross_file_metas() -> &'static [&'static RuleMeta] {
    static METAS: &[&RuleMeta] = &[
        &DUPLICATE_CROSS_FILE,
        &NEAR_DUPLICATE,
        &SIMILAR_NAME,
        &SIMILAR_SHAPE,
    ];
    METAS
}

/// Fuzzy-similarity cutoff (cosine over node-kind histograms).
const NEAR_THRESHOLD: f32 = 0.92;
/// Skip the O(n²) fuzzy pass on very large indexes to keep scans fast.
const NEAR_MAX_ENTRIES: usize = 4000;

/// One indexed function: where it lives plus its [`Shape`] for matching.
pub struct FunctionEntry {
    pub file: String,
    pub file_context: FileContext,
    pub name: Option<String>,
    pub line: u32,
    pub column: u32,
    pub snippet: String,
    pub shape: Shape,
}

/// Names too generic for a same-name collision to mean anything.
fn is_generic_name(name: &str) -> bool {
    name.len() < 3
        || matches!(
            name,
            "run" | "handle" | "main" | "init" | "setup" | "teardown" | "callback" | "cb" | "noop"
        )
}

/// Walk a parsed file and index its named / variable-bound functions.
pub fn collect_from_program(
    program: &Program,
    source: &str,
    display_path: &str,
) -> Vec<FunctionEntry> {
    let line_starts: Vec<usize> = std::iter::once(0)
        .chain(
            source
                .bytes()
                .enumerate()
                .filter(|(_, byte)| *byte == b'\n')
                .map(|(offset, _)| offset + 1),
        )
        .collect();
    let mut collector = EntryCollector {
        source,
        line_starts: &line_starts,
        file: display_path,
        file_context: classify_file(display_path),
        entries: Vec::new(),
    };
    collector.visit_program(program);
    collector.entries
}

struct EntryCollector<'a> {
    source: &'a str,
    line_starts: &'a [usize],
    file: &'a str,
    file_context: FileContext,
    entries: Vec<FunctionEntry>,
}

impl EntryCollector<'_> {
    fn push(&mut self, name: Option<String>, shape: Shape, span: oxc_span::Span) {
        let offset = span.start as usize;
        let line_index = match self.line_starts.binary_search(&offset) {
            Ok(index) => index,
            Err(index) => index - 1,
        };
        let line_start = self.line_starts[line_index];
        let line_end = self.source[line_start..]
            .find('\n')
            .map(|relative| line_start + relative)
            .unwrap_or(self.source.len());
        self.entries.push(FunctionEntry {
            file: self.file.to_string(),
            file_context: self.file_context,
            name,
            line: (line_index + 1) as u32,
            column: (offset - line_start + 1) as u32,
            snippet: self.source[line_start..line_end].trim_end().to_string(),
            shape,
        });
    }
}

impl<'a> Visit<'a> for EntryCollector<'a> {
    fn visit_function(&mut self, func: &Function<'a>, flags: ScopeFlags) {
        // Named functions (declarations + named expressions). Anonymous
        // function expressions bound to a variable are handled by the
        // declarator below, so they aren't missed.
        if let (Some(id), Some(body)) = (func.id.as_ref(), func.body.as_ref()) {
            if let Some(shape) = structural::analyze(body, func.params.items.len()) {
                self.push(Some(id.name.to_string()), shape, func.span);
            }
        }
        walk::walk_function(self, func, flags);
    }

    fn visit_variable_declarator(&mut self, declarator: &VariableDeclarator<'a>) {
        if let Some(name) = binding_name(declarator) {
            if let Some((shape, span)) = declarator_function_shape(declarator) {
                self.push(Some(name), shape, span);
            }
        }
        walk::walk_variable_declarator(self, declarator);
    }
}

fn binding_name(declarator: &VariableDeclarator) -> Option<String> {
    match &declarator.id {
        BindingPattern::BindingIdentifier(id) => Some(id.name.to_string()),
        _ => None,
    }
}

/// Shape of `const x = () => {…}` / `const x = function () {…}` (anonymous only —
/// named function expressions are indexed under their own name).
fn declarator_function_shape(declarator: &VariableDeclarator) -> Option<(Shape, oxc_span::Span)> {
    let init = declarator.init.as_ref()?;
    match unwrap_parens(init) {
        Expression::ArrowFunctionExpression(arrow) => {
            structural::analyze(&arrow.body, arrow.params.items.len())
                .map(|shape| (shape, arrow.span))
        }
        Expression::FunctionExpression(func) if func.id.is_none() => {
            let body = func.body.as_ref()?;
            structural::analyze(body, func.params.items.len()).map(|shape| (shape, func.span))
        }
        _ => None,
    }
}

/// Cross-file analysis over the whole repo index → `info` suggestions.
pub fn cross_file_findings(entries: &[FunctionEntry]) -> Vec<Diagnostic> {
    let by_exact = group_by(entries, |entry| Some(entry.shape.exact_hash));
    let by_name = group_by(entries, |entry| {
        entry
            .name
            .as_deref()
            .filter(|name| !is_generic_name(name))
            .map(str::to_string)
    });
    let by_shape = group_by(entries, |entry| entry.shape.behaviour_hash());
    let allow_fuzzy = entries.len() <= NEAR_MAX_ENTRIES;

    let mut findings = Vec::new();
    for (index, entry) in entries.iter().enumerate() {
        let finding = exact_partner(entries, index, &by_exact)
            .map(|partner| {
                (
                    &DUPLICATE_CROSS_FILE,
                    describe(entry, partner, "is a structural duplicate of"),
                )
            })
            .or_else(|| {
                allow_fuzzy
                    .then(|| near_partner(entries, index))
                    .flatten()
                    .map(|(partner, pct)| {
                        (
                            &NEAR_DUPLICATE,
                            describe(
                                entry,
                                partner,
                                &format!("is ~{pct}% structurally similar to"),
                            ),
                        )
                    })
            })
            .or_else(|| {
                name_partner(entries, index, &by_name).map(|partner| {
                    (
                        &SIMILAR_NAME,
                        describe(entry, partner, "shares a name with"),
                    )
                })
            })
            .or_else(|| {
                shape_partner(entries, index, &by_shape).map(|partner| {
                    (
                        &SIMILAR_SHAPE,
                        describe(entry, partner, "has the same params + call set as"),
                    )
                })
            });
        if let Some((meta, message)) = finding {
            findings.push(to_diagnostic(meta, entry, message));
        }
    }
    findings
        .sort_by(|a, b| (a.rule, a.file.as_str(), a.line).cmp(&(b.rule, b.file.as_str(), b.line)));
    findings
}

fn group_by<K, F>(entries: &[FunctionEntry], key: F) -> HashMap<K, Vec<usize>>
where
    K: std::hash::Hash + Eq,
    F: Fn(&FunctionEntry) -> Option<K>,
{
    let mut map: HashMap<K, Vec<usize>> = HashMap::new();
    for (index, entry) in entries.iter().enumerate() {
        if let Some(key) = key(entry) {
            map.entry(key).or_default().push(index);
        }
    }
    map
}

/// First index in `group` that lives in a different file from `index`.
fn other_file<'a>(
    entries: &'a [FunctionEntry],
    index: usize,
    group: &[usize],
) -> Option<&'a FunctionEntry> {
    let file = &entries[index].file;
    group
        .iter()
        .copied()
        .find(|&candidate| candidate != index && &entries[candidate].file != file)
        .map(|candidate| &entries[candidate])
}

fn exact_partner<'a>(
    entries: &'a [FunctionEntry],
    index: usize,
    by_exact: &HashMap<u64, Vec<usize>>,
) -> Option<&'a FunctionEntry> {
    let group = by_exact.get(&entries[index].shape.exact_hash)?;
    other_file(entries, index, group)
}

fn name_partner<'a>(
    entries: &'a [FunctionEntry],
    index: usize,
    by_name: &HashMap<String, Vec<usize>>,
) -> Option<&'a FunctionEntry> {
    let name = entries[index].name.as_deref()?;
    let group = by_name.get(name)?;
    other_file(entries, index, group)
}

fn shape_partner<'a>(
    entries: &'a [FunctionEntry],
    index: usize,
    by_shape: &HashMap<u64, Vec<usize>>,
) -> Option<&'a FunctionEntry> {
    let group = by_shape.get(&entries[index].shape.behaviour_hash()?)?;
    other_file(entries, index, group)
}

/// Best fuzzy match in another file above [`NEAR_THRESHOLD`], with its percent.
fn near_partner(entries: &[FunctionEntry], index: usize) -> Option<(&FunctionEntry, u32)> {
    let me = &entries[index];
    let mut best: Option<(&FunctionEntry, f32)> = None;
    for (other_index, other) in entries.iter().enumerate() {
        if other_index == index || other.file == me.file {
            continue;
        }
        // Exact duplicates are reported by the stronger rule; lengths far apart
        // can't be near-duplicates.
        if other.shape.exact_hash == me.shape.exact_hash || !lengths_comparable(me, other) {
            continue;
        }
        let score = structural::cosine(&me.shape.histogram, &other.shape.histogram);
        if score >= NEAR_THRESHOLD && best.is_none_or(|(_, current)| score > current) {
            best = Some((other, score));
        }
    }
    best.map(|(partner, score)| (partner, (score * 100.0).round() as u32))
}

fn lengths_comparable(a: &FunctionEntry, b: &FunctionEntry) -> bool {
    let (small, large) = (a.shape.len.min(b.shape.len), a.shape.len.max(b.shape.len));
    large as f32 <= small as f32 * 1.34
}

fn describe(entry: &FunctionEntry, partner: &FunctionEntry, relation: &str) -> String {
    let subject = entry
        .name
        .as_deref()
        .map(|name| format!("`{name}`"))
        .unwrap_or_else(|| "this function".to_string());
    let target = partner
        .name
        .as_deref()
        .map(|name| format!("`{name}`"))
        .unwrap_or_else(|| "a function".to_string());
    format!(
        "{subject} {relation} {target} at {}:{} — reuse one instead of maintaining both",
        partner.file, partner.line
    )
}

fn to_diagnostic(meta: &'static RuleMeta, entry: &FunctionEntry, message: String) -> Diagnostic {
    Diagnostic {
        rule: meta.id,
        severity: meta.severity,
        category: meta.category,
        message,
        help: meta.help,
        file: entry.file.clone(),
        file_context: entry.file_context,
        line: entry.line,
        column: entry.column,
        snippet: entry.snippet.clone(),
    }
}
