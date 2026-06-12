use oxc_ast::ast::{CallExpression, Expression, Statement};

use crate::diagnostics::{Category, RuleMeta, Severity};
use crate::matchers::{arrow_body_expression, call_module_prop, first_function_arg};
use crate::rules::{FileCtx, Rule};

static PREFER_CATCH_TAG: RuleMeta = RuleMeta {
    id: "prefer-catch-tag",
    severity: Severity::Warn,
    category: Category::Idiomatic,
    help: "Branching on `error._tag` inside a catch-all handler re-implements what `Effect.catchTag` / `Effect.catchTags` do with full type narrowing and exhaustiveness.",
};

static CATCH_TO_MAP_ERROR: RuleMeta = RuleMeta {
    id: "catch-to-map-error",
    severity: Severity::Info,
    category: Category::Idiomatic,
    help: "A catch handler that always fails again is just transforming the error — `Effect.mapError` says that directly.",
};

fn is_catch_all(prop: &str) -> bool {
    matches!(prop, "catch" | "catchAll")
}

/// Does the expression read `<something>._tag`? Shallow walk through the
/// expressions a tag-dispatch handler typically uses.
fn references_tag(expr: &Expression) -> bool {
    match expr {
        Expression::StaticMemberExpression(member) => {
            if member.property.name == "_tag" {
                return true;
            }
            references_tag(&member.object)
        }
        Expression::BinaryExpression(binary) => {
            references_tag(&binary.left) || references_tag(&binary.right)
        }
        Expression::LogicalExpression(logical) => {
            references_tag(&logical.left) || references_tag(&logical.right)
        }
        Expression::ConditionalExpression(conditional) => references_tag(&conditional.test),
        _ => false,
    }
}

fn handler_dispatches_on_tag(handler: &Expression) -> bool {
    let Expression::ArrowFunctionExpression(arrow) = handler else {
        return false;
    };
    if let Some(body) = arrow_body_expression(arrow) {
        return match body {
            Expression::ConditionalExpression(conditional) => references_tag(&conditional.test),
            _ => false,
        };
    }
    arrow.body.statements.iter().any(|statement| match statement {
        Statement::IfStatement(if_stmt) => references_tag(&if_stmt.test),
        Statement::SwitchStatement(switch_stmt) => references_tag(&switch_stmt.discriminant),
        _ => false,
    })
}

fn handler_always_fails(handler: &Expression, ctx: &FileCtx) -> bool {
    let Expression::ArrowFunctionExpression(arrow) = handler else {
        return false;
    };
    let Some(Expression::CallExpression(body_call)) = arrow_body_expression(arrow) else {
        return false;
    };
    call_module_prop(body_call, &ctx.imports) == Some(("Effect", "fail"))
}

pub struct CatchIdioms;

impl Rule for CatchIdioms {
    fn on_call(&self, call: &CallExpression<'_>, ctx: &mut FileCtx) {
        let Some(("Effect", prop)) = call_module_prop(call, &ctx.imports) else {
            return;
        };
        if !is_catch_all(prop) {
            return;
        }
        let Some(handler) = first_function_arg(call) else {
            return;
        };
        if handler_dispatches_on_tag(handler) {
            ctx.report(
                &PREFER_CATCH_TAG,
                call.span,
                format!("Effect.{prop} handler branches on `_tag` — use Effect.catchTag/catchTags"),
            );
            return;
        }
        if handler_always_fails(handler, ctx) {
            ctx.report(
                &CATCH_TO_MAP_ERROR,
                call.span,
                format!("Effect.{prop} handler always re-fails — use Effect.mapError"),
            );
        }
    }
}
