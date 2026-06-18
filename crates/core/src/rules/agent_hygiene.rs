//! Experimental `--agent` mode ("agent doctor"): the non-Effect, non-functional
//! "slop" patterns LLM agents reach for by default — `if/else` chains, ternaries,
//! string-equality guards, raw loops, `let` mutation — plus copy-pasted function
//! bodies. Every finding has a cleaner Effect / `Match` / combinator form.
//!
//! The whole family is opt-in (`--agent`) and defaults to `warn`; `--agent-strict`
//! escalates each to `error` so the scan can gate CI. `agent-duplicate-function`
//! is the exception — it stays an `info` suggestion regardless of `--agent-strict`.

use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use oxc_ast::ast::{
    ArrowFunctionExpression, BinaryExpression, ConditionalExpression, Expression, Function,
    FunctionBody, IfStatement, Statement, VariableDeclaration, VariableDeclarationKind,
};
use oxc_ast_visit::{walk, Visit};
use oxc_span::Span;
use oxc_syntax::operator::BinaryOperator;

use crate::diagnostics::{Category, RuleMeta, Severity};
use crate::matchers::unwrap_parens;
use crate::rules::{FileCtx, Rule};

static IF_ELSE_CHAIN: RuleMeta = RuleMeta {
    id: "agent-no-if-else-chain",
    severity: Severity::Warn,
    category: Category::AgentHygiene,
    help: "if/else (and else-if) chains hide branching. Prefer early returns, a lookup map, or Match.value(...).pipe(Match.when(...), Match.exhaustive) for declarative, exhaustive dispatch.",
};

static TERNARY: RuleMeta = RuleMeta {
    id: "agent-no-ternary",
    severity: Severity::Warn,
    category: Category::AgentHygiene,
    help: "Ternaries hide branching. Extract a named helper, or use Match.value(...).pipe(Match.when(...), Match.orElse(...)) so each branch is named and exhaustive.",
};

static STRING_EQUALITY_GUARD: RuleMeta = RuleMeta {
    id: "agent-no-string-equality-guard",
    severity: Severity::Warn,
    category: Category::AgentHygiene,
    help: "Comparing against a string literal (`x === \"user\"`) is a stringly-typed guard. Use a type guard / predicate (isUser(x)) or Match.when on a tagged value for exhaustiveness.",
};

static RAW_LOOP: RuleMeta = RuleMeta {
    id: "agent-no-raw-loop",
    severity: Severity::Warn,
    category: Category::AgentHygiene,
    help: "Raw for/while loops mutate and obscure intent. Use Array combinators (map/filter/reduce) for pure data, or Effect.forEach / Effect.reduce for effectful iteration (concurrency + interruption included).",
};

static LET_BINDING: RuleMeta = RuleMeta {
    id: "agent-no-let",
    severity: Severity::Warn,
    category: Category::AgentHygiene,
    help: "`let`/`var` signal mutation. Bind with `const` and build values functionally — Array.reduce, Match for conditional values, or an Effect pipeline instead of reassigning.",
};

static DUPLICATE_FUNCTION: RuleMeta = RuleMeta {
    id: "agent-duplicate-function",
    severity: Severity::Info,
    category: Category::AgentHygiene,
    help: "Two functions in this file share a structurally identical body — agents copy-paste instead of extracting. Hoist the shared logic into one reusable helper.",
};

/// Minimum structural-signature length for duplicate detection — keeps trivial
/// one-liners (getters, single-return wrappers) from being flagged as clones.
const MIN_DUP_SIGNATURE: usize = 16;

fn is_equality(operator: BinaryOperator) -> bool {
    matches!(
        operator,
        BinaryOperator::Equality
            | BinaryOperator::Inequality
            | BinaryOperator::StrictEquality
            | BinaryOperator::StrictInequality
    )
}

fn is_string_literal(expr: &Expression) -> bool {
    matches!(unwrap_parens(expr), Expression::StringLiteral(_))
}

/// `<x>._tag` — owned by the equality-idioms tag rules; the agent guard rule
/// defers to those so tag comparisons aren't double-reported.
fn is_tag_member(expr: &Expression) -> bool {
    matches!(
        unwrap_parens(expr),
        Expression::StaticMemberExpression(member) if member.property.name == "_tag"
    )
}

fn keyword_span(span: Span, len: u32) -> Span {
    Span::new(span.start, span.start + len)
}

/// Structural fingerprint of a function body: a byte stream of node kinds with
/// identifiers and literal *values* dropped, so renamed copy-paste still
/// matches. Returns `None` for bodies below the complexity floor.
fn fingerprint(body: &FunctionBody) -> Option<u64> {
    let mut visitor = SignatureVisitor { sig: Vec::new() };
    for statement in &body.statements {
        visitor.visit_statement(statement);
    }
    if visitor.sig.len() < MIN_DUP_SIGNATURE {
        return None;
    }
    let mut hasher = DefaultHasher::new();
    visitor.sig.hash(&mut hasher);
    Some(hasher.finish())
}

struct SignatureVisitor {
    sig: Vec<u8>,
}

impl<'a> Visit<'a> for SignatureVisitor {
    fn visit_statement(&mut self, statement: &Statement<'a>) {
        // A coarse, stable discriminant per statement kind.
        self.sig.push(statement_tag(statement));
        walk::walk_statement(self, statement);
    }

    fn visit_expression(&mut self, expression: &Expression<'a>) {
        self.sig.push(expression_tag(expression));
        walk::walk_expression(self, expression);
    }
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

pub struct AgentHygiene;

impl AgentHygiene {
    fn record_body(&self, body: Option<&FunctionBody>, span: Span, ctx: &mut FileCtx) {
        if !ctx.agent_active() {
            return;
        }
        let Some(body) = body else { return };
        if let Some(hash) = fingerprint(body) {
            ctx.scratch.fn_fingerprints.push((hash, span));
        }
    }
}

impl Rule for AgentHygiene {
    fn metas(&self) -> &'static [&'static RuleMeta] {
        static METAS: &[&RuleMeta] = &[
            &IF_ELSE_CHAIN,
            &TERNARY,
            &STRING_EQUALITY_GUARD,
            &RAW_LOOP,
            &LET_BINDING,
            &DUPLICATE_FUNCTION,
        ];
        METAS
    }

    fn on_if(&self, if_stmt: &IfStatement<'_>, ctx: &mut FileCtx) {
        if !ctx.agent_active() {
            return;
        }
        // Each `else if` is itself an IfStatement nested in a parent's
        // `alternate`; the runner visits the chain head first, where we mark the
        // descendant links so the chain reports exactly once.
        if let Some(position) = ctx
            .scratch
            .if_chain_skip
            .iter()
            .position(|start| *start == if_stmt.span.start)
        {
            ctx.scratch.if_chain_skip.swap_remove(position);
            return;
        }
        let Some(alternate) = if_stmt.alternate.as_ref() else {
            return;
        };
        let mut link = Some(alternate);
        while let Some(Statement::IfStatement(inner)) = link {
            ctx.scratch.if_chain_skip.push(inner.span.start);
            link = inner.alternate.as_ref();
        }
        ctx.report_agent(
            &IF_ELSE_CHAIN,
            keyword_span(if_stmt.span, 2),
            "if/else chain — use early returns, a lookup map, or Match.exhaustive".to_string(),
        );
    }

    fn on_conditional(&self, conditional: &ConditionalExpression<'_>, ctx: &mut FileCtx) {
        if !ctx.agent_active() {
            return;
        }
        ctx.report_agent(
            &TERNARY,
            conditional.span,
            "ternary — extract a named helper or use Match.when/orElse".to_string(),
        );
    }

    fn on_binary(&self, binary: &BinaryExpression<'_>, ctx: &mut FileCtx) {
        if !ctx.agent_active() || !is_equality(binary.operator) {
            return;
        }
        // `_tag` comparisons belong to the equality-idioms tag rules.
        if is_tag_member(&binary.left) || is_tag_member(&binary.right) {
            return;
        }
        if !is_string_literal(&binary.left) && !is_string_literal(&binary.right) {
            return;
        }
        ctx.report_agent(
            &STRING_EQUALITY_GUARD,
            binary.span,
            format!(
                "`{}` against a string literal — use a type guard / predicate or Match.when",
                binary.operator.as_str()
            ),
        );
    }

    fn on_loop(&self, loop_span: Span, _body: &Statement<'_>, ctx: &mut FileCtx) {
        if !ctx.agent_active() {
            return;
        }
        ctx.report_agent(
            &RAW_LOOP,
            keyword_span(loop_span, 3),
            "raw loop — use Array.map/filter/reduce or Effect.forEach / Effect.reduce".to_string(),
        );
    }

    fn on_var_decl(&self, decl: &VariableDeclaration<'_>, ctx: &mut FileCtx) {
        if !ctx.agent_active() {
            return;
        }
        if !matches!(
            decl.kind,
            VariableDeclarationKind::Let | VariableDeclarationKind::Var
        ) {
            return;
        }
        ctx.report_agent(
            &LET_BINDING,
            keyword_span(decl.span, decl.kind.as_str().len() as u32),
            format!(
                "`{}` signals mutation — bind with const and build values functionally",
                decl.kind.as_str()
            ),
        );
    }

    fn on_function(&self, function: &Function<'_>, ctx: &mut FileCtx) {
        self.record_body(function.body.as_deref(), function.span, ctx);
    }

    fn on_arrow(&self, arrow: &ArrowFunctionExpression<'_>, ctx: &mut FileCtx) {
        self.record_body(Some(&arrow.body), arrow.span, ctx);
    }

    fn on_file_end(&self, ctx: &mut FileCtx) {
        if !ctx.agent_active() {
            return;
        }
        let mut by_hash: HashMap<u64, Vec<Span>> = HashMap::new();
        for (hash, span) in std::mem::take(&mut ctx.scratch.fn_fingerprints) {
            by_hash.entry(hash).or_default().push(span);
        }
        let mut duplicate_spans: Vec<Span> = by_hash
            .into_values()
            .filter(|spans| spans.len() > 1)
            .flatten()
            .collect();
        duplicate_spans.sort_by_key(|span| span.start);
        for span in duplicate_spans {
            ctx.report(
                &DUPLICATE_FUNCTION,
                keyword_span(span, 1),
                "function body is structurally identical to another in this file — extract a shared helper".to_string(),
            );
        }
    }
}
