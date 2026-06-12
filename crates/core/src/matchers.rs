use oxc_ast::ast::{
    Argument, ArrowFunctionExpression, CallExpression, Expression, Function, Statement,
    StaticMemberExpression,
};

use crate::effect_imports::EffectImports;

/// `<object>.<name>` with a static (non-computed) property.
pub fn static_member<'a, 'b>(expr: &'b Expression<'a>) -> Option<(&'b Expression<'a>, &'b str)> {
    match expr {
        Expression::StaticMemberExpression(member) => {
            Some((&member.object, member.property.name.as_str()))
        }
        _ => None,
    }
}

pub fn ident_name<'a, 'b>(expr: &'b Expression<'a>) -> Option<&'b str> {
    match expr {
        Expression::Identifier(identifier) => Some(identifier.name.as_str()),
        _ => None,
    }
}

/// `<binding>.<prop>` where the binding is an effect import — returns the
/// canonical module name ("Effect", "Schema", "Layer", ...) and the property.
pub fn member_module_prop<'a, 'b>(
    member: &'b StaticMemberExpression<'a>,
    imports: &'b EffectImports,
) -> Option<(&'b str, &'b str)> {
    let object_name = ident_name(&member.object)?;
    let module = imports.module_of(object_name)?;
    Some((module, member.property.name.as_str()))
}

/// If the call is `<binding>.<prop>(...)` with the binding imported from
/// effect, return the canonical module name and property.
pub fn call_module_prop<'a, 'b>(
    call: &'b CallExpression<'a>,
    imports: &'b EffectImports,
) -> Option<(&'b str, &'b str)> {
    let (object, prop) = static_member(&call.callee)?;
    let object_name = ident_name(object)?;
    let module = imports.module_of(object_name)?;
    Some((module, prop))
}

/// If the call is `<EffectBinding>.<prop>(...)`, return `prop`.
pub fn effect_member_prop<'a, 'b>(
    call: &'b CallExpression<'a>,
    imports: &'b EffectImports,
) -> Option<&'b str> {
    let (module, prop) = call_module_prop(call, imports)?;
    if module != "Effect" {
        return None;
    }
    Some(prop)
}

/// `undefined` or `void 0`.
pub fn is_undefined_expr(expr: &Expression) -> bool {
    match expr {
        Expression::Identifier(identifier) => identifier.name == "undefined",
        Expression::UnaryExpression(unary) => unary.operator.as_str() == "void",
        _ => false,
    }
}

/// Strip parenthesized-expression wrappers (oxc preserves parens as nodes).
pub fn unwrap_parens<'a, 'b>(expr: &'b Expression<'a>) -> &'b Expression<'a> {
    let mut current = expr;
    while let Expression::ParenthesizedExpression(parenthesized) = current {
        current = &parenthesized.expression;
    }
    current
}

/// The expression body of `() => expr` (concise arrows only), parens stripped.
pub fn arrow_body_expression<'a, 'b>(
    arrow: &'b ArrowFunctionExpression<'a>,
) -> Option<&'b Expression<'a>> {
    if !arrow.expression {
        return None;
    }
    match arrow.body.statements.first()? {
        Statement::ExpressionStatement(statement) => Some(unwrap_parens(&statement.expression)),
        _ => None,
    }
}

/// First function-ish argument of a call (arrow or function expression).
pub fn first_function_arg<'a, 'b>(
    call: &'b CallExpression<'a>,
) -> Option<&'b Expression<'a>> {
    call.arguments
        .iter()
        .filter_map(Argument::as_expression)
        .find(|expr| {
            matches!(
                expr,
                Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_)
            )
        })
}

fn is_gen_like(prop: &str) -> bool {
    matches!(prop, "gen" | "fn" | "fnUntraced")
}

fn generator_args<'a, 'b>(call: &'b CallExpression<'a>) -> Vec<&'b Function<'a>> {
    call.arguments
        .iter()
        .filter_map(Argument::as_expression)
        .filter_map(|expr| match expr {
            Expression::FunctionExpression(function) if function.generator => {
                Some(function.as_ref())
            }
            _ => None,
        })
        .collect()
}

/// Generator functions that run as Effect generators:
/// `Effect.gen(function*(){})`, `Effect.fn(function*(){})`,
/// and the curried `Effect.fn("name")(function*(){})`.
pub fn effect_gen_generators<'a, 'b>(
    call: &'b CallExpression<'a>,
    imports: &EffectImports,
) -> Vec<&'b Function<'a>> {
    if effect_member_prop(call, imports).is_some_and(is_gen_like) {
        return generator_args(call);
    }
    if let Expression::CallExpression(inner) = &call.callee {
        if effect_member_prop(inner, imports).is_some_and(is_gen_like) {
            return generator_args(call);
        }
    }
    Vec::new()
}

/// The generator argument of a direct `Effect.gen(...)` call only.
pub fn direct_effect_gen<'a, 'b>(
    call: &'b CallExpression<'a>,
    imports: &EffectImports,
) -> Option<&'b Function<'a>> {
    if effect_member_prop(call, imports) != Some("gen") {
        return None;
    }
    generator_args(call).into_iter().next()
}
