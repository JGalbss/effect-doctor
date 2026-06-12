use oxc_ast::ast::{CallExpression, Expression};

use crate::diagnostics::{Category, RuleMeta, Severity};
use crate::matchers::{call_module_prop, first_function_arg, function_result_expression};
use crate::rules::{FileCtx, Rule};

static META: RuleMeta = RuleMeta {
    id: "no-map-returning-effect",
    severity: Severity::Warn,
    category: Category::Correctness,
    help: "map wraps the callback result as a plain value, so returning an Effect produces Effect<Effect<A>> — the inner effect silently never runs. Use flatMap (or tap for side effects).",
};

fn is_value_predicate(prop: &str) -> bool {
    prop.starts_with("is")
}

pub struct NoMapReturningEffect;

impl Rule for NoMapReturningEffect {
    fn metas(&self) -> &'static [&'static RuleMeta] {
        static METAS: &[&RuleMeta] = &[&META];
        METAS
    }

    fn on_call(&self, call: &CallExpression<'_>, ctx: &mut FileCtx) {
        let Some((module, "map")) = call_module_prop(call, &ctx.imports) else {
            return;
        };
        if module != "Effect" && module != "Stream" {
            return;
        }
        let Some(handler) = first_function_arg(call) else {
            return;
        };
        let Some(result) = function_result_expression(handler) else {
            return;
        };
        let Expression::CallExpression(inner) = result else {
            return;
        };
        let Some((inner_module, inner_prop)) = call_module_prop(inner, &ctx.imports) else {
            return;
        };
        if inner_module != "Effect" && inner_module != "Stream" {
            return;
        }
        if is_value_predicate(inner_prop) {
            return;
        }
        ctx.report(
            &META,
            call.span,
            format!(
                "{module}.map callback returns {inner_module}.{inner_prop}(...) — the inner effect never runs; use {module}.flatMap or {module}.tap"
            ),
        );
    }
}
