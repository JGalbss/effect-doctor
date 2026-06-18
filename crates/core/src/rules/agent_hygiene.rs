//! Experimental `--agent` mode ("agent doctor"): the non-Effect, non-functional
//! "slop" patterns LLM agents reach for by default — `if/else` chains, ternaries,
//! string-equality guards, raw loops, `let` mutation — plus copy-pasted function
//! bodies. Every finding has a cleaner Effect / `Match` / combinator form.
//!
//! The whole family is opt-in (`--agent`) and defaults to `warn`; `--agent-strict`
//! escalates each to `error` so the scan can gate CI. `agent-duplicate-function`
//! is the exception — it stays an `info` suggestion regardless of `--agent-strict`.

use std::collections::HashMap;

use oxc_ast::ast::{
    ArrowFunctionExpression, AssignmentExpression, AssignmentTarget, BinaryExpression,
    ConditionalExpression, Expression, Function, FunctionBody, IfStatement, Statement,
    VariableDeclaration, VariableDeclarationKind,
};
use oxc_span::Span;
use oxc_syntax::operator::BinaryOperator;

use crate::diagnostics::{Category, RuleMeta, Severity};
use crate::matchers::unwrap_parens;
use crate::rules::{FileCtx, Rule};
use crate::structural;

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

static MUTATION: RuleMeta = RuleMeta {
    id: "agent-no-mutation",
    severity: Severity::Warn,
    category: Category::AgentHygiene,
    help: "Reassigning a binding or mutating a payload in place creates intermediate states that are hard to follow. Derive the final value in one expression (const + Match/reduce/pipe) instead of building it up by mutation.",
};

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

/// `=` (or a compound `+=`…) onto a plain identifier is a reassignment; onto a
/// member/index target it mutates an object in place. Both are intermediate
/// state — describe which for the message.
fn mutation_kind(target: &AssignmentTarget) -> &'static str {
    match target {
        AssignmentTarget::AssignmentTargetIdentifier(_) => "reassignment",
        _ => "in-place mutation",
    }
}

pub struct AgentHygiene;

impl AgentHygiene {
    fn record_body(
        &self,
        body: Option<&FunctionBody>,
        param_count: usize,
        span: Span,
        ctx: &mut FileCtx,
    ) {
        if !ctx.agent_active() {
            return;
        }
        let Some(body) = body else { return };
        if let Some(shape) = structural::analyze(body, param_count) {
            ctx.scratch.fn_fingerprints.push((shape.exact_hash, span));
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
            &MUTATION,
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

    fn on_assignment(&self, assignment: &AssignmentExpression<'_>, ctx: &mut FileCtx) {
        if !ctx.agent_active() {
            return;
        }
        let kind = mutation_kind(&assignment.left);
        ctx.report_agent(
            &MUTATION,
            assignment.span,
            format!("{kind} — derive the final value once (const + Match/reduce/pipe) instead of building it up"),
        );
    }

    fn on_function(&self, function: &Function<'_>, ctx: &mut FileCtx) {
        self.record_body(
            function.body.as_deref(),
            function.params.items.len(),
            function.span,
            ctx,
        );
    }

    fn on_arrow(&self, arrow: &ArrowFunctionExpression<'_>, ctx: &mut FileCtx) {
        self.record_body(Some(&arrow.body), arrow.params.items.len(), arrow.span, ctx);
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
