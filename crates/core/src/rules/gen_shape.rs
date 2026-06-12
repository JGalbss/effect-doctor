//! Generator/pipe shape rules ported from @effect/language-service:
//! nestedEffectGenYield, effectFnIife, unnecessaryPipeChain,
//! returnEffectInGen (constructor-whitelist approximation),
//! redundantSchemaTagIdentifier.

use oxc_ast::ast::{
    Argument, CallExpression, Class, Expression, ReturnStatement, YieldExpression,
};
use oxc_span::GetSpan;

use crate::diagnostics::{Category, RuleMeta, Severity};
use crate::matchers::{
    call_module_prop, direct_effect_gen, effect_member_prop, ident_name, static_member,
    unwrap_parens,
};
use crate::rules::{FileCtx, Rule};

static NESTED_GEN_YIELD: RuleMeta = RuleMeta {
    id: "no-nested-gen-yield",
    severity: Severity::Info,
    category: Category::Idiomatic,
    help: "`yield* Effect.gen(...)` directly inside another generator adds a wrapper for nothing — inline the inner generator's body into the parent.",
};

static EFFECT_FN_IIFE: RuleMeta = RuleMeta {
    id: "no-effect-fn-iife",
    severity: Severity::Warn,
    category: Category::Idiomatic,
    help: "Effect.fn(...)() invoked immediately builds a traced function just to call it once — use Effect.gen directly (keep the span with Effect.withSpan).",
};

static PIPE_CHAIN: RuleMeta = RuleMeta {
    id: "no-unnecessary-pipe-chain",
    severity: Severity::Info,
    category: Category::Idiomatic,
    help: "Chained pipes (`x.pipe(a).pipe(b)` / `pipe(pipe(x, a), b)`) are one pipe — merge the steps.",
};

static RETURN_EFFECT_IN_GEN: RuleMeta = RuleMeta {
    id: "no-return-effect-in-gen",
    severity: Severity::Info,
    category: Category::Idiomatic,
    help: "Returning a bare Effect from Effect.gen makes the success value an Effect (Effect<Effect<A>>) — usually you want `return yield*` so it actually runs.",
};

static REDUNDANT_TAG_IDENTIFIER: RuleMeta = RuleMeta {
    id: "redundant-schema-tag-identifier",
    severity: Severity::Info,
    category: Category::Idiomatic,
    help: "The identifier argument defaults to the tag — repeating the same string is noise; drop it.",
};

fn is_tagged_schema_factory(prop: &str) -> bool {
    matches!(
        prop,
        "TaggedClass" | "TaggedError" | "TaggedErrorClass" | "TaggedRequest"
    )
}

fn string_literal_value<'a, 'b>(expr: &'b Expression<'a>) -> Option<&'b str> {
    match unwrap_parens(expr) {
        Expression::StringLiteral(literal) => Some(literal.value.as_str()),
        _ => None,
    }
}

pub struct GenShape;

impl GenShape {
    fn check_fn_iife(&self, call: &CallExpression<'_>, ctx: &mut FileCtx) {
        // Outer call invoking `Effect.fn(function* () {...})` — the generator
        // lives in the INNER call's arguments. (The curried name form
        // `Effect.fn("name")(function*(){})` has the generator in the outer
        // call and is fine.)
        let Expression::CallExpression(inner) = unwrap_parens(&call.callee) else {
            return;
        };
        let Some(prop) = effect_member_prop(inner, &ctx.imports) else {
            return;
        };
        if prop != "fn" && prop != "fnUntraced" {
            return;
        }
        let inner_has_generator = inner
            .arguments
            .iter()
            .filter_map(Argument::as_expression)
            .any(|expr| matches!(expr, Expression::FunctionExpression(f) if f.generator));
        if !inner_has_generator {
            return;
        }
        ctx.report(
            &EFFECT_FN_IIFE,
            call.span,
            format!("Effect.{prop}(...)() immediately invoked — use Effect.gen"),
        );
    }

    fn check_pipe_chain(&self, call: &CallExpression<'_>, ctx: &mut FileCtx) {
        // x.pipe(a).pipe(b)
        if let Some((object, "pipe")) = static_member(&call.callee) {
            if let Expression::CallExpression(inner) = unwrap_parens(object) {
                if matches!(static_member(&inner.callee), Some((_, "pipe"))) {
                    ctx.report(
                        &PIPE_CHAIN,
                        call.span,
                        "chained .pipe().pipe() — merge into one pipe".to_string(),
                    );
                }
            }
            return;
        }
        // pipe(pipe(x, ...), ...)
        if ident_name(&call.callee) != Some("pipe") {
            return;
        }
        let first_is_pipe = matches!(
            call.arguments.first().and_then(Argument::as_expression).map(unwrap_parens),
            Some(Expression::CallExpression(inner)) if ident_name(&inner.callee) == Some("pipe")
        );
        if !first_is_pipe {
            return;
        }
        ctx.report(
            &PIPE_CHAIN,
            call.span,
            "nested pipe(pipe(...)) — merge into one pipe".to_string(),
        );
    }
}

impl Rule for GenShape {
    fn metas(&self) -> &'static [&'static RuleMeta] {
        static METAS: &[&RuleMeta] = &[
            &NESTED_GEN_YIELD,
            &EFFECT_FN_IIFE,
            &PIPE_CHAIN,
            &RETURN_EFFECT_IN_GEN,
            &REDUNDANT_TAG_IDENTIFIER,
        ];
        METAS
    }

    fn on_yield(&self, yield_expr: &YieldExpression<'_>, ctx: &mut FileCtx) {
        if !yield_expr.delegate || !ctx.in_effect_gen() {
            return;
        }
        let Some(Expression::CallExpression(call)) =
            yield_expr.argument.as_ref().map(unwrap_parens)
        else {
            return;
        };
        if direct_effect_gen(call, &ctx.imports).is_none() {
            return;
        }
        ctx.report(
            &NESTED_GEN_YIELD,
            yield_expr.span,
            "yield* Effect.gen(...) inside a generator — inline the inner body".to_string(),
        );
    }

    fn on_call(&self, call: &CallExpression<'_>, ctx: &mut FileCtx) {
        self.check_fn_iife(call, ctx);
        self.check_pipe_chain(call, ctx);
    }

    fn on_return(&self, return_stmt: &ReturnStatement<'_>, ctx: &mut FileCtx) {
        if !ctx.in_effect_gen() {
            return;
        }
        let Some(Expression::CallExpression(call)) =
            return_stmt.argument.as_ref().map(unwrap_parens)
        else {
            return;
        };
        let Some(("Effect", prop)) = call_module_prop(call, &ctx.imports) else {
            return;
        };
        // Predicates and run-methods do not produce Effects to be yielded.
        if prop.starts_with("is") || prop.starts_with("run") {
            return;
        }
        ctx.report(
            &RETURN_EFFECT_IN_GEN,
            return_stmt.span,
            format!("returning Effect.{prop}(...) from Effect.gen — did you mean `return yield*`?"),
        );
    }

    fn on_class(&self, class: &Class<'_>, ctx: &mut FileCtx) {
        // class X extends Schema.TaggedError<X>("Identifier")("Tag", fields):
        // identifier (inner call arg) equal to tag (outer call arg) is noise.
        let Some(superclass) = &class.super_class else {
            return;
        };
        let Expression::CallExpression(outer) = unwrap_parens(superclass) else {
            return;
        };
        let Expression::CallExpression(inner) = unwrap_parens(&outer.callee) else {
            return;
        };
        let Some(("Schema", prop)) = call_module_prop(inner, &ctx.imports) else {
            return;
        };
        if !is_tagged_schema_factory(prop) {
            return;
        }
        let identifier = inner
            .arguments
            .first()
            .and_then(Argument::as_expression)
            .and_then(string_literal_value);
        let tag = outer
            .arguments
            .first()
            .and_then(Argument::as_expression)
            .and_then(string_literal_value);
        let (Some(identifier), Some(tag)) = (identifier, tag) else {
            return;
        };
        if identifier != tag {
            return;
        }
        ctx.report(
            &REDUNDANT_TAG_IDENTIFIER,
            inner.arguments.first().map(|argument| argument.span()).unwrap_or(inner.span),
            format!("identifier \"{identifier}\" repeats the tag — drop it"),
        );
    }
}
