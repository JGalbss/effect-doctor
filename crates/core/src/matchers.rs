use oxc_ast::ast::{Argument, CallExpression, Expression, Function};

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

/// If the call is `<EffectBinding>.<prop>(...)`, return `prop`.
pub fn effect_member_prop<'a, 'b>(
    call: &'b CallExpression<'a>,
    imports: &EffectImports,
) -> Option<&'b str> {
    let (object, prop) = static_member(&call.callee)?;
    let object_name = ident_name(object)?;
    if !imports.is_module(object_name, "Effect") {
        return None;
    }
    Some(prop)
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
