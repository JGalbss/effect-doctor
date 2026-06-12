use oxc_ast::ast::{ArrowFunctionExpression, Expression, Function, Statement};

use crate::diagnostics::{Category, RuleMeta, Severity};
use crate::matchers::{arrow_body_expression, direct_effect_gen};
use crate::rules::{FileCtx, Rule};

static META: RuleMeta = RuleMeta {
    id: "prefer-effect-fn",
    severity: Severity::Warn,
    category: Category::Idiomatic,
    help: "A function whose body is one Effect.gen is exactly what Effect.fn is for — it adds a named span and proper stack frames for free: `Effect.fn(\"name\")(function* (args) {...})`.",
};

pub struct PreferEffectFn;

impl PreferEffectFn {
    fn report(&self, span: oxc_span::Span, ctx: &mut FileCtx) {
        ctx.report(
            &META,
            span,
            "function returning a single Effect.gen — use Effect.fn".to_string(),
        );
    }
}

impl Rule for PreferEffectFn {
    fn on_arrow(&self, arrow: &ArrowFunctionExpression<'_>, ctx: &mut FileCtx) {
        // Zero-param arrows are usually thunks (Effect.suspend, Layer factories).
        if arrow.params.items.is_empty() {
            return;
        }
        let Some(Expression::CallExpression(call)) = arrow_body_expression(arrow) else {
            return;
        };
        if direct_effect_gen(call, &ctx.imports).is_none() {
            return;
        }
        self.report(arrow.span, ctx);
    }

    fn on_function(&self, function: &Function<'_>, ctx: &mut FileCtx) {
        if function.generator || function.params.items.is_empty() {
            return;
        }
        let Some(body) = &function.body else {
            return;
        };
        if body.statements.len() != 1 {
            return;
        }
        let Statement::ReturnStatement(return_stmt) = &body.statements[0] else {
            return;
        };
        let Some(Expression::CallExpression(call)) = &return_stmt.argument else {
            return;
        };
        if direct_effect_gen(call, &ctx.imports).is_none() {
            return;
        }
        self.report(function.span, ctx);
    }
}
