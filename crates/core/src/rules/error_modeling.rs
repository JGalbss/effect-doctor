use oxc_ast::ast::{Argument, CallExpression, Expression, ObjectPropertyKind, PropertyKey};

use crate::diagnostics::{Category, RuleMeta, Severity};
use crate::matchers::{
    call_module_prop, first_function_arg, function_result_expression, is_undefined_expr,
    member_module_prop, unwrap_parens,
};
use crate::rules::{FileCtx, Rule};

static NO_STRING_ERRORS: RuleMeta = RuleMeta {
    id: "no-string-errors",
    severity: Severity::Warn,
    category: Category::Idiomatic,
    help: "Failing with a string loses all type information — catchTag can't route it and call sites get `string` in the error channel. Use Data.TaggedError / Schema.TaggedErrorClass.",
};

static NO_CATCHALL_TO_NULL: RuleMeta = RuleMeta {
    id: "no-catchall-to-null",
    severity: Severity::Warn,
    category: Category::Correctness,
    help: "catchAll(() => succeed(null)) swallows every failure — including bugs — and turns them into a nullable success. Catch the specific tag, or use Effect.option if absence is the model.",
};

fn is_string_like(expr: &Expression) -> bool {
    matches!(
        unwrap_parens(expr),
        Expression::StringLiteral(_) | Expression::TemplateLiteral(_)
    )
}

fn is_null_like(expr: &Expression) -> bool {
    matches!(unwrap_parens(expr), Expression::NullLiteral(_)) || is_undefined_expr(expr)
}

fn try_catch_property<'a, 'b>(call: &'b CallExpression<'a>) -> Option<&'b Expression<'a>> {
    let Some(Expression::ObjectExpression(object)) =
        call.arguments.first().and_then(Argument::as_expression)
    else {
        return None;
    };
    object.properties.iter().find_map(|property| {
        let ObjectPropertyKind::ObjectProperty(entry) = property else {
            return None;
        };
        let PropertyKey::StaticIdentifier(key) = &entry.key else {
            return None;
        };
        if key.name != "catch" {
            return None;
        }
        Some(&entry.value)
    })
}

pub struct ErrorModeling;

impl Rule for ErrorModeling {
    fn metas(&self) -> &'static [&'static RuleMeta] {
        static METAS: &[&RuleMeta] = &[&NO_STRING_ERRORS, &NO_CATCHALL_TO_NULL];
        METAS
    }

    fn on_call(&self, call: &CallExpression<'_>, ctx: &mut FileCtx) {
        let Some(("Effect", prop)) = call_module_prop(call, &ctx.imports) else {
            return;
        };

        if prop == "fail" {
            let fails_with_string = call
                .arguments
                .first()
                .and_then(Argument::as_expression)
                .is_some_and(is_string_like);
            if fails_with_string {
                ctx.report(
                    &NO_STRING_ERRORS,
                    call.span,
                    "Effect.fail with a string — use a tagged error class".to_string(),
                );
            }
            return;
        }

        if prop == "try" || prop == "tryPromise" {
            let Some(handler) = try_catch_property(call) else {
                return;
            };
            let returns_string = function_result_expression(handler).is_some_and(is_string_like);
            if returns_string {
                ctx.report(
                    &NO_STRING_ERRORS,
                    call.span,
                    format!("Effect.{prop} catch returns a string — map to a tagged error instead"),
                );
            }
            return;
        }

        if prop == "catchAll" || prop == "catch" {
            let Some(handler) = first_function_arg(call) else {
                return;
            };
            let swallows = match function_result_expression(handler) {
                Some(Expression::CallExpression(inner)) => {
                    call_module_prop(inner, &ctx.imports) == Some(("Effect", "succeed"))
                        && inner
                            .arguments
                            .first()
                            .and_then(Argument::as_expression)
                            .is_some_and(is_null_like)
                }
                Some(Expression::StaticMemberExpression(member)) => {
                    member_module_prop(member, &ctx.imports) == Some(("Effect", "void"))
                }
                _ => false,
            };
            if swallows {
                ctx.report(
                    &NO_CATCHALL_TO_NULL,
                    call.span,
                    format!("Effect.{prop} converting every failure to null — catch the specific tag or use Effect.option"),
                );
            }
        }
    }
}
