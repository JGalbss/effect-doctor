//! Structural fingerprinting of function bodies, shared by the intra-file
//! duplicate rule (`agent-duplicate-function`) and the cross-file "this already
//! exists" detector ([`crate::fn_index`]). Identifiers and literal *values* are
//! dropped, so renamed copy-paste still matches; only the shape of the code
//! (statement / expression node kinds + which helpers it calls) is captured.

use std::collections::hash_map::DefaultHasher;
use std::collections::BTreeSet;
use std::hash::{Hash, Hasher};

use oxc_ast::ast::{CallExpression, Expression, FunctionBody, Statement};
use oxc_ast_visit::{walk, Visit};

use crate::matchers::{ident_name, static_member};

/// Minimum structural-signature length — keeps trivial one-liners (getters,
/// single-return wrappers) out of duplicate / similarity detection.
pub const MIN_SIGNATURE: usize = 16;

/// Histogram width: statement tags map to 0..16, expression tags to 16..32.
pub const HIST: usize = 32;

/// The structural summary of one function body.
pub struct Shape {
    /// Exact hash of the node-kind stream — equal hashes ⇒ identical shape.
    pub exact_hash: u64,
    /// Per-tag node-kind counts, for fuzzy (cosine) similarity.
    pub histogram: [u32; HIST],
    /// Signature length (node count) — a coarse complexity / size measure.
    pub len: usize,
    /// Declared parameter count.
    pub param_count: usize,
    /// Names of functions / methods this body calls — the "what it does" set.
    pub callees: BTreeSet<String>,
}

impl Shape {
    /// Hash of the behavioural shape: parameter count + the set of helpers
    /// called. Two different implementations of the same operation tend to
    /// share this even when their control flow differs. `None` when the call
    /// set is too thin to be a meaningful signal.
    pub fn behaviour_hash(&self) -> Option<u64> {
        if self.callees.len() < 2 {
            return None;
        }
        let mut hasher = DefaultHasher::new();
        self.param_count.hash(&mut hasher);
        for callee in &self.callees {
            callee.hash(&mut hasher);
        }
        Some(hasher.finish())
    }
}

/// Compute the [`Shape`] of a function body, or `None` if it is below the
/// complexity floor.
pub fn analyze(body: &FunctionBody, param_count: usize) -> Option<Shape> {
    let mut visitor = SignatureVisitor {
        sig: Vec::new(),
        histogram: [0; HIST],
        callees: BTreeSet::new(),
    };
    for statement in &body.statements {
        visitor.visit_statement(statement);
    }
    if visitor.sig.len() < MIN_SIGNATURE {
        return None;
    }
    let mut hasher = DefaultHasher::new();
    visitor.sig.hash(&mut hasher);
    Some(Shape {
        exact_hash: hasher.finish(),
        histogram: visitor.histogram,
        len: visitor.sig.len(),
        param_count,
        callees: visitor.callees,
    })
}

/// Cosine similarity of two node-kind histograms, in `0.0..=1.0`.
pub fn cosine(a: &[u32; HIST], b: &[u32; HIST]) -> f32 {
    let mut dot = 0.0f64;
    let mut norm_a = 0.0f64;
    let mut norm_b = 0.0f64;
    for index in 0..HIST {
        let x = a[index] as f64;
        let y = b[index] as f64;
        dot += x * y;
        norm_a += x * x;
        norm_b += y * y;
    }
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    (dot / (norm_a.sqrt() * norm_b.sqrt())) as f32
}

struct SignatureVisitor {
    sig: Vec<u8>,
    histogram: [u32; HIST],
    callees: BTreeSet<String>,
}

impl SignatureVisitor {
    fn push(&mut self, tag: u8) {
        self.sig.push(tag);
        self.histogram[tag_index(tag)] += 1;
    }
}

impl<'a> Visit<'a> for SignatureVisitor {
    fn visit_statement(&mut self, statement: &Statement<'a>) {
        self.push(statement_tag(statement));
        walk::walk_statement(self, statement);
    }

    fn visit_expression(&mut self, expression: &Expression<'a>) {
        self.push(expression_tag(expression));
        walk::walk_expression(self, expression);
    }

    fn visit_call_expression(&mut self, call: &CallExpression<'a>) {
        if let Some(name) = callee_name(call) {
            self.callees.insert(name);
        }
        walk::walk_call_expression(self, call);
    }
}

/// The called function's name: a bare identifier, or the property of a
/// `<obj>.<method>(...)` call (so `Effect.map` contributes `map`).
fn callee_name(call: &CallExpression) -> Option<String> {
    if let Some(name) = ident_name(&call.callee) {
        return Some(name.to_string());
    }
    static_member(&call.callee).map(|(_, prop)| prop.to_string())
}

/// Map a statement tag (1..=16) or expression tag (64..=79) to a histogram slot.
fn tag_index(tag: u8) -> usize {
    if tag < 64 {
        return (tag as usize).saturating_sub(1).min(15);
    }
    (16 + (tag as usize - 64)).min(HIST - 1)
}

fn statement_tag(statement: &Statement) -> u8 {
    match statement {
        Statement::BlockStatement(_) => 1,
        Statement::IfStatement(_) => 2,
        Statement::ForStatement(_) => 3,
        Statement::ForOfStatement(_) => 4,
        Statement::ForInStatement(_) => 5,
        Statement::WhileStatement(_) => 6,
        Statement::DoWhileStatement(_) => 7,
        Statement::SwitchStatement(_) => 8,
        Statement::TryStatement(_) => 9,
        Statement::ThrowStatement(_) => 10,
        Statement::ReturnStatement(_) => 11,
        Statement::VariableDeclaration(_) => 12,
        Statement::ExpressionStatement(_) => 13,
        Statement::BreakStatement(_) => 14,
        Statement::ContinueStatement(_) => 15,
        _ => 16,
    }
}

fn expression_tag(expression: &Expression) -> u8 {
    match expression {
        Expression::CallExpression(_) => 64,
        Expression::BinaryExpression(_) => 65,
        Expression::LogicalExpression(_) => 66,
        Expression::ConditionalExpression(_) => 67,
        Expression::StaticMemberExpression(_) => 68,
        Expression::ComputedMemberExpression(_) => 69,
        Expression::AwaitExpression(_) => 70,
        Expression::YieldExpression(_) => 71,
        Expression::NewExpression(_) => 72,
        Expression::ArrowFunctionExpression(_) => 73,
        Expression::ObjectExpression(_) => 74,
        Expression::ArrayExpression(_) => 75,
        Expression::AssignmentExpression(_) => 76,
        Expression::UnaryExpression(_) => 77,
        Expression::TemplateLiteral(_) => 78,
        _ => 79,
    }
}
