use oxc_ast::ast::{Argument, CallExpression, Expression};

use crate::diagnostics::{Category, RuleMeta, Severity};
use crate::matchers::{arrow_body_expression, call_module_prop, ident_name};
use crate::rules::{FileCtx, Rule};

static META: RuleMeta = RuleMeta {
    id: "prefer-it-effect",
    severity: Severity::Warn,
    category: Category::Idiomatic,
    help: "@effect/vitest's it.effect runs the test inside a managed runtime (TestClock, proper failure rendering) — no manual runPromise or async glue.",
};

fn is_test_function(name: &str) -> bool {
    matches!(name, "it" | "test")
}

fn test_body<'a, 'b>(call: &'b CallExpression<'a>) -> Option<&'b Expression<'a>> {
    call.arguments.iter().filter_map(Argument::as_expression).nth(1)
}

fn body_runs_effect(body: &Expression, ctx: &FileCtx) -> bool {
    let Expression::ArrowFunctionExpression(arrow) = body else {
        return false;
    };
    let Some(Expression::CallExpression(call)) = arrow_body_expression(arrow) else {
        return false;
    };
    matches!(
        call_module_prop(call, &ctx.imports),
        Some(("Effect", "runPromise" | "runPromiseExit"))
    )
}

pub struct PreferItEffect;

impl Rule for PreferItEffect {
    fn on_call(&self, call: &CallExpression<'_>, ctx: &mut FileCtx) {
        let Some(callee_name) = ident_name(&call.callee) else {
            return;
        };
        if !is_test_function(callee_name) {
            return;
        }
        let Some(body) = test_body(call) else {
            return;
        };
        if !body_runs_effect(body, ctx) {
            return;
        }
        ctx.report(
            &META,
            call.span,
            format!("{callee_name}() running Effect.runPromise — use it.effect from @effect/vitest"),
        );
    }
}
