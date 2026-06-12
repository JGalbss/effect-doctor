use oxc_ast::ast::{Argument, CallExpression, Expression};

use crate::diagnostics::{Category, RuleMeta, Severity};
use crate::matchers::{
    arrow_body_expression, call_module_prop, is_undefined_expr, member_module_prop, static_member,
};
use crate::rules::{FileCtx, Rule};

static PREFER_EFFECT_VOID: RuleMeta = RuleMeta {
    id: "prefer-effect-void",
    severity: Severity::Info,
    category: Category::Idiomatic,
    help: "`Effect.succeed(undefined)` allocates a new effect for a constant — `Effect.void` is the canonical shared instance.",
};

static PREFER_AS_VOID: RuleMeta = RuleMeta {
    id: "prefer-as-void",
    severity: Severity::Info,
    category: Category::Idiomatic,
    help: "Mapping to a constant has dedicated combinators: `Effect.asVoid` for undefined, `Effect.as(value)` for constants — clearer intent, no closure.",
};

static PREFER_FLATMAP: RuleMeta = RuleMeta {
    id: "prefer-flatmap-over-map-flatten",
    severity: Severity::Info,
    category: Category::Idiomatic,
    help: "`Effect.map` followed by `Effect.flatten` is exactly `Effect.flatMap`.",
};

static NO_UNNECESSARY_PIPE: RuleMeta = RuleMeta {
    id: "no-unnecessary-pipe",
    severity: Severity::Info,
    category: Category::Idiomatic,
    help: "`.pipe()` with no arguments does nothing — remove it.",
};

fn is_constant_literal(expr: &Expression) -> bool {
    matches!(
        expr,
        Expression::StringLiteral(_)
            | Expression::NumericLiteral(_)
            | Expression::BooleanLiteral(_)
            | Expression::NullLiteral(_)
    )
}

pub struct IdiomShortcuts;

impl IdiomShortcuts {
    fn check_succeed_undefined(&self, call: &CallExpression<'_>, ctx: &mut FileCtx) {
        if call_module_prop(call, &ctx.imports) != Some(("Effect", "succeed")) {
            return;
        }
        let Some(Some(argument)) = call.arguments.first().map(Argument::as_expression) else {
            return;
        };
        if !is_undefined_expr(argument) {
            return;
        }
        ctx.report(
            &PREFER_EFFECT_VOID,
            call.span,
            "Effect.succeed(undefined) — use Effect.void".to_string(),
        );
    }

    fn check_map_to_constant(&self, call: &CallExpression<'_>, ctx: &mut FileCtx) {
        if call_module_prop(call, &ctx.imports) != Some(("Effect", "map")) {
            return;
        }
        let Some(Expression::ArrowFunctionExpression(arrow)) = call
            .arguments
            .iter()
            .filter_map(Argument::as_expression)
            .last()
        else {
            return;
        };
        if !arrow.params.items.is_empty() {
            return;
        }
        let Some(body) = arrow_body_expression(arrow) else {
            return;
        };
        if is_undefined_expr(body) {
            ctx.report(
                &PREFER_AS_VOID,
                call.span,
                "Effect.map(() => undefined) — use Effect.asVoid".to_string(),
            );
            return;
        }
        if is_constant_literal(body) {
            ctx.report(
                &PREFER_AS_VOID,
                call.span,
                "Effect.map(() => constant) — use Effect.as(constant)".to_string(),
            );
        }
    }

    fn check_map_flatten(&self, call: &CallExpression<'_>, ctx: &mut FileCtx) {
        // Direct nesting: Effect.flatten(Effect.map(...))
        if call_module_prop(call, &ctx.imports) == Some(("Effect", "flatten")) {
            let inner_is_map = call
                .arguments
                .first()
                .and_then(Argument::as_expression)
                .is_some_and(|expr| match expr {
                    Expression::CallExpression(inner) => {
                        call_module_prop(inner, &ctx.imports) == Some(("Effect", "map"))
                    }
                    _ => false,
                });
            if inner_is_map {
                ctx.report(
                    &PREFER_FLATMAP,
                    call.span,
                    "Effect.flatten(Effect.map(...)) — use Effect.flatMap".to_string(),
                );
            }
            return;
        }
        // Pipe adjacency: x.pipe(Effect.map(f), Effect.flatten)
        let Some((_, "pipe")) = static_member(&call.callee) else {
            return;
        };
        let arguments: Vec<Option<&Expression>> =
            call.arguments.iter().map(Argument::as_expression).collect();
        for pair in arguments.windows(2) {
            let [Some(first), Some(second)] = pair else {
                continue;
            };
            let first_is_map = match first {
                Expression::CallExpression(inner) => {
                    call_module_prop(inner, &ctx.imports) == Some(("Effect", "map"))
                }
                _ => false,
            };
            let second_is_flatten = match second {
                Expression::StaticMemberExpression(member) => {
                    member_module_prop(member, &ctx.imports) == Some(("Effect", "flatten"))
                }
                _ => false,
            };
            if first_is_map && second_is_flatten {
                ctx.report(
                    &PREFER_FLATMAP,
                    call.span,
                    "pipe(Effect.map(f), Effect.flatten) — use Effect.flatMap(f)".to_string(),
                );
            }
        }
    }

    fn check_empty_pipe(&self, call: &CallExpression<'_>, ctx: &mut FileCtx) {
        let Some((_, "pipe")) = static_member(&call.callee) else {
            return;
        };
        if !call.arguments.is_empty() {
            return;
        }
        ctx.report(
            &NO_UNNECESSARY_PIPE,
            call.span,
            "`.pipe()` with no arguments — remove it".to_string(),
        );
    }
}

impl Rule for IdiomShortcuts {
    fn metas(&self) -> &'static [&'static RuleMeta] {
        static METAS: &[&RuleMeta] = &[&PREFER_EFFECT_VOID, &PREFER_AS_VOID, &PREFER_FLATMAP, &NO_UNNECESSARY_PIPE];
        METAS
    }

    fn on_call(&self, call: &CallExpression<'_>, ctx: &mut FileCtx) {
        self.check_succeed_undefined(call, ctx);
        self.check_map_to_constant(call, ctx);
        self.check_map_flatten(call, ctx);
        self.check_empty_pipe(call, ctx);
    }
}
