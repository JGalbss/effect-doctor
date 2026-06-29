//! Maintainability metrics, computed per function: parameter count, maximum
//! nesting depth, and cognitive complexity (a SonarJS-style score that rewards
//! flat, linear control flow). Always-on `warn`, escalating to `error` under
//! `--agent-strict`. Agents tend to emit long, deeply-nested procedural blobs;
//! these turn the health score into a real maintainability grade.

use oxc_ast::ast::{
    ArrowFunctionExpression, CatchClause, ConditionalExpression, DoWhileStatement, ForInStatement,
    ForOfStatement, ForStatement, Function, FunctionBody, IfStatement, LogicalExpression, Statement,
    SwitchStatement, WhileStatement,
};
use oxc_ast_visit::{walk, Visit};
use oxc_span::Span;
use oxc_syntax::operator::LogicalOperator;
use oxc_syntax::scope::ScopeFlags;

use crate::diagnostics::{Category, RuleMeta, Severity};
use crate::rules::{FileCtx, Rule};

/// Functions with more parameters than this are flagged (group them into an
/// options object / a `Data` class).
const MAX_PARAMETERS: usize = 4;
/// Maximum allowed control-flow nesting depth.
const MAX_NESTING: u32 = 4;
/// Cognitive-complexity ceiling (SonarJS's default).
const MAX_COGNITIVE: u32 = 15;

static MAX_PARAMS: RuleMeta = RuleMeta {
    id: "max-function-parameters",
    severity: Severity::Warn,
    category: Category::Maintainability,
    help: "A long parameter list is hard to call correctly and usually signals the function does too much. Group related arguments into a single options object (or a Data/Schema struct).",
};

static MAX_NESTING_DEPTH: RuleMeta = RuleMeta {
    id: "max-nesting-depth",
    severity: Severity::Warn,
    category: Category::Maintainability,
    help: "Deeply nested control flow is hard to follow and test. Flatten with early returns/guard clauses, extract inner blocks into named helpers, or use Match / Effect combinators.",
};

static HIGH_COGNITIVE: RuleMeta = RuleMeta {
    id: "high-cognitive-complexity",
    severity: Severity::Warn,
    category: Category::Maintainability,
    help: "High cognitive complexity (many branches and nested conditions) means the function is hard to reason about. Split it into smaller named functions, or replace branching with Match / lookup maps / Effect combinators.",
};

/// Walks a single function body computing cognitive complexity and max nesting,
/// stopping at nested function boundaries (those are measured on their own).
struct MetricsVisitor {
    complexity: u32,
    nesting: u32,
    max_nesting: u32,
}

impl MetricsVisitor {
    fn new() -> Self {
        MetricsVisitor {
            complexity: 0,
            nesting: 0,
            max_nesting: 0,
        }
    }

    fn enter(&mut self) {
        self.nesting += 1;
        if self.nesting > self.max_nesting {
            self.max_nesting = self.nesting;
        }
    }

    fn leave(&mut self) {
        self.nesting -= 1;
    }
}

impl<'a> Visit<'a> for MetricsVisitor {
    // Do not descend into nested functions — they get their own measurement.
    fn visit_function(&mut self, _function: &Function<'a>, _flags: ScopeFlags) {}
    fn visit_arrow_function_expression(&mut self, _arrow: &ArrowFunctionExpression<'a>) {}

    fn visit_if_statement(&mut self, node: &IfStatement<'a>) {
        self.visit_expression(&node.test);
        self.complexity += 1 + self.nesting;
        self.enter();
        self.visit_statement(&node.consequent);
        self.leave();
        let Some(alternate) = &node.alternate else {
            return;
        };
        match alternate {
            // `else if` is a continuation of the same chain: it adds a branch
            // (its own `visit_if_statement` adds +1) but not an extra nesting level.
            Statement::IfStatement(_) => self.visit_statement(alternate),
            _ => {
                self.complexity += 1;
                self.enter();
                self.visit_statement(alternate);
                self.leave();
            }
        }
    }

    fn visit_for_statement(&mut self, node: &ForStatement<'a>) {
        self.complexity += 1 + self.nesting;
        self.enter();
        walk::walk_for_statement(self, node);
        self.leave();
    }

    fn visit_for_in_statement(&mut self, node: &ForInStatement<'a>) {
        self.complexity += 1 + self.nesting;
        self.enter();
        walk::walk_for_in_statement(self, node);
        self.leave();
    }

    fn visit_for_of_statement(&mut self, node: &ForOfStatement<'a>) {
        self.complexity += 1 + self.nesting;
        self.enter();
        walk::walk_for_of_statement(self, node);
        self.leave();
    }

    fn visit_while_statement(&mut self, node: &WhileStatement<'a>) {
        self.complexity += 1 + self.nesting;
        self.enter();
        walk::walk_while_statement(self, node);
        self.leave();
    }

    fn visit_do_while_statement(&mut self, node: &DoWhileStatement<'a>) {
        self.complexity += 1 + self.nesting;
        self.enter();
        walk::walk_do_while_statement(self, node);
        self.leave();
    }

    fn visit_switch_statement(&mut self, node: &SwitchStatement<'a>) {
        self.complexity += 1 + self.nesting;
        self.enter();
        walk::walk_switch_statement(self, node);
        self.leave();
    }

    fn visit_catch_clause(&mut self, node: &CatchClause<'a>) {
        self.complexity += 1 + self.nesting;
        self.enter();
        walk::walk_catch_clause(self, node);
        self.leave();
    }

    fn visit_conditional_expression(&mut self, node: &ConditionalExpression<'a>) {
        self.complexity += 1 + self.nesting;
        self.enter();
        walk::walk_conditional_expression(self, node);
        self.leave();
    }

    fn visit_logical_expression(&mut self, node: &LogicalExpression<'a>) {
        if matches!(node.operator, LogicalOperator::And | LogicalOperator::Or) {
            self.complexity += 1;
        }
        walk::walk_logical_expression(self, node);
    }
}

pub struct FunctionMetrics;

impl FunctionMetrics {
    fn check_params(&self, count: usize, span: Span, ctx: &mut FileCtx) {
        if count > MAX_PARAMETERS {
            ctx.report_agent(
                &MAX_PARAMS,
                span,
                format!("{count} parameters (limit {MAX_PARAMETERS}) — group them into an options object"),
            );
        }
    }

    fn check_body(&self, body: &FunctionBody<'_>, span: Span, ctx: &mut FileCtx) {
        let mut metrics = MetricsVisitor::new();
        metrics.visit_function_body(body);
        if metrics.complexity > MAX_COGNITIVE {
            ctx.report_agent(
                &HIGH_COGNITIVE,
                span,
                format!(
                    "cognitive complexity {} (limit {MAX_COGNITIVE}) — split it up or flatten the branching",
                    metrics.complexity
                ),
            );
        }
        if metrics.max_nesting > MAX_NESTING {
            ctx.report_agent(
                &MAX_NESTING_DEPTH,
                span,
                format!(
                    "nesting depth {} (limit {MAX_NESTING}) — flatten with early returns or extract helpers",
                    metrics.max_nesting
                ),
            );
        }
    }
}

impl Rule for FunctionMetrics {
    fn metas(&self) -> &'static [&'static RuleMeta] {
        static METAS: &[&RuleMeta] = &[&MAX_PARAMS, &MAX_NESTING_DEPTH, &HIGH_COGNITIVE];
        METAS
    }

    fn on_function(&self, function: &Function<'_>, ctx: &mut FileCtx) {
        self.check_params(function.params.items.len(), function.span, ctx);
        if let Some(body) = &function.body {
            self.check_body(body, function.span, ctx);
        }
    }

    fn on_arrow(&self, arrow: &ArrowFunctionExpression<'_>, ctx: &mut FileCtx) {
        self.check_params(arrow.params.items.len(), arrow.span, ctx);
        self.check_body(&arrow.body, arrow.span, ctx);
    }
}
