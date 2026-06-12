use oxc_ast::ast::{
    Argument, ArrowFunctionExpression, BlockStatement, CallExpression, Expression, Function,
    NewExpression, Statement, StaticMemberExpression,
};
use oxc_ast_visit::{walk, Visit};
use oxc_syntax::scope::ScopeFlags;

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

/// Is the function-ish expression declared `async`?
pub fn is_async_function(expr: &Expression) -> bool {
    match expr {
        Expression::ArrowFunctionExpression(arrow) => arrow.r#async,
        Expression::FunctionExpression(function) => function.r#async,
        _ => false,
    }
}

struct CallSearch<'p, 'a> {
    found: bool,
    pred: &'p mut dyn FnMut(&CallExpression<'a>) -> bool,
}

impl<'a> Visit<'a> for CallSearch<'_, 'a> {
    fn visit_call_expression(&mut self, call: &CallExpression<'a>) {
        if self.found {
            return;
        }
        if (self.pred)(call) {
            self.found = true;
            return;
        }
        walk::walk_call_expression(self, call);
    }
}

/// Does any call expression in the subtree satisfy the predicate?
pub fn expression_has_call<'a>(
    expr: &Expression<'a>,
    mut pred: impl FnMut(&CallExpression<'a>) -> bool,
) -> bool {
    let mut search = CallSearch {
        found: false,
        pred: &mut pred,
    };
    search.visit_expression(expr);
    search.found
}

struct NewSearch<'p, 'a> {
    found: bool,
    pred: &'p mut dyn FnMut(&NewExpression<'a>) -> bool,
}

impl<'a> Visit<'a> for NewSearch<'_, 'a> {
    fn visit_new_expression(&mut self, new_expr: &NewExpression<'a>) {
        if self.found {
            return;
        }
        if (self.pred)(new_expr) {
            self.found = true;
            return;
        }
        walk::walk_new_expression(self, new_expr);
    }
}

/// Does any `new` expression in the subtree satisfy the predicate?
pub fn expression_has_new<'a>(
    expr: &Expression<'a>,
    mut pred: impl FnMut(&NewExpression<'a>) -> bool,
) -> bool {
    let mut search = NewSearch {
        found: false,
        pred: &mut pred,
    };
    search.visit_expression(expr);
    search.found
}

struct OwnYieldSearch {
    found: bool,
}

impl<'a> Visit<'a> for OwnYieldSearch {
    fn visit_yield_expression(&mut self, _yield_expr: &oxc_ast::ast::YieldExpression<'a>) {
        self.found = true;
    }
    // Yields inside nested functions belong to those generators, not this one.
    fn visit_function(&mut self, _function: &Function<'a>, _flags: ScopeFlags) {}
    fn visit_arrow_function_expression(&mut self, _arrow: &ArrowFunctionExpression<'a>) {}
}

/// Does the block contain a `yield` belonging to the enclosing generator
/// (not one inside a nested function)?
pub fn block_has_own_yield(block: &BlockStatement) -> bool {
    let mut search = OwnYieldSearch { found: false };
    search.visit_block_statement(block);
    search.found
}

/// Does the statement contain a `yield` belonging to the enclosing generator?
pub fn statement_has_own_yield(statement: &Statement) -> bool {
    let mut search = OwnYieldSearch { found: false };
    search.visit_statement(statement);
    search.found
}

struct OwnAwaitSearch {
    found: bool,
}

impl<'a> Visit<'a> for OwnAwaitSearch {
    fn visit_await_expression(&mut self, _await_expr: &oxc_ast::ast::AwaitExpression<'a>) {
        self.found = true;
    }
    // Awaits inside nested functions belong to those functions.
    fn visit_function(&mut self, _function: &Function<'a>, _flags: ScopeFlags) {}
    fn visit_arrow_function_expression(&mut self, _arrow: &ArrowFunctionExpression<'a>) {}
}

/// Does the statement contain an `await` belonging to the enclosing function?
pub fn statement_has_own_await(statement: &Statement) -> bool {
    let mut search = OwnAwaitSearch { found: false };
    search.visit_statement(statement);
    search.found
}

/// The body expression of a handler: concise arrow body, or the argument of a
/// lone `return` in a block body.
pub fn function_result_expression<'a, 'b>(
    handler: &'b Expression<'a>,
) -> Option<&'b Expression<'a>> {
    match handler {
        Expression::ArrowFunctionExpression(arrow) => {
            if let Some(body) = arrow_body_expression(arrow) {
                return Some(body);
            }
            single_return_expression(&arrow.body.statements)
        }
        Expression::FunctionExpression(function) => {
            single_return_expression(&function.body.as_ref()?.statements)
        }
        _ => None,
    }
}

fn single_return_expression<'a, 'b>(
    statements: &'b [Statement<'a>],
) -> Option<&'b Expression<'a>> {
    if statements.len() != 1 {
        return None;
    }
    let Statement::ReturnStatement(return_stmt) = &statements[0] else {
        return None;
    };
    return_stmt.argument.as_ref().map(unwrap_parens)
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
