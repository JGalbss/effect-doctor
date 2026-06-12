use oxc_ast::ast::{Argument, CallExpression, Expression};

use crate::diagnostics::{Category, RuleMeta, Severity};
use crate::matchers::call_module_prop;
use crate::rules::{FileCtx, Rule};

static META: RuleMeta = RuleMeta {
    id: "meaningful-span-names",
    severity: Severity::Info,
    category: Category::Idiomatic,
    help: "Span names end up in traces and dashboards — name the business operation (\"UserRepo.create\"), not the mechanism.",
};

fn is_generic_name(name: &str) -> bool {
    let lowered = name.to_ascii_lowercase();
    matches!(
        lowered.as_str(),
        "run" | "helper" | "process" | "main" | "handler" | "fn" | "function" | "do" | "execute"
            | "step" | "task" | "work" | "op"
    )
}

fn is_span_factory(prop: &str) -> bool {
    matches!(prop, "fn" | "fnUntraced" | "withSpan")
}

pub struct MeaningfulSpanNames;

impl Rule for MeaningfulSpanNames {
    fn metas(&self) -> &'static [&'static RuleMeta] {
        static METAS: &[&RuleMeta] = &[&META];
        METAS
    }

    fn on_call(&self, call: &CallExpression<'_>, ctx: &mut FileCtx) {
        let Some(("Effect", prop)) = call_module_prop(call, &ctx.imports) else {
            return;
        };
        if !is_span_factory(prop) {
            return;
        }
        let Some(name_literal) = call
            .arguments
            .iter()
            .filter_map(Argument::as_expression)
            .find_map(|expr| match expr {
                Expression::StringLiteral(literal) => Some(literal),
                _ => None,
            })
        else {
            return;
        };
        if !is_generic_name(&name_literal.value) {
            return;
        }
        ctx.report(
            &META,
            name_literal.span,
            format!("span name \"{}\" is generic — name the business operation", name_literal.value),
        );
    }
}
