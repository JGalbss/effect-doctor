use std::collections::HashMap;

use oxc_ast::ast::{
    ArrowFunctionExpression, CallExpression, Expression, Function, NewExpression, Program,
    ThrowStatement, TryStatement, YieldExpression,
};
use oxc_ast_visit::{walk, Visit};
use oxc_syntax::scope::ScopeFlags;

use crate::effect_imports::EffectImports;
use crate::matchers::{effect_gen_generators, effect_member_prop};
use crate::rules::{FileCtx, FrameKind, Rule, RULES};

/// Single-pass AST walker: maintains the function-frame stack (which functions
/// are Effect generators / Effect callbacks) and dispatches node hooks to all
/// rules. Frames are identified by the function node's span start, marked when
/// the enclosing call expression is visited (calls are visited before their
/// argument functions).
pub struct Runner {
    pub ctx: FileCtx,
    marked: HashMap<u32, FrameKind>,
}

impl Runner {
    pub fn new(imports: EffectImports) -> Self {
        Runner {
            ctx: FileCtx::new(imports),
            marked: HashMap::new(),
        }
    }

    pub fn run(mut self, program: &Program) -> FileCtx {
        self.visit_program(program);
        self.ctx
    }

    fn rules(&self) -> &'static [&'static (dyn Rule + Send + Sync)] {
        RULES
    }

    fn mark_call_arguments(&mut self, call: &CallExpression) {
        for generator in effect_gen_generators(call, &self.ctx.imports) {
            self.marked.insert(generator.span.start, FrameKind::EffectGen);
        }
        if effect_member_prop(call, &self.ctx.imports).is_none() {
            return;
        }
        for argument in &call.arguments {
            let Some(expression) = argument.as_expression() else {
                continue;
            };
            let span_start = match expression {
                Expression::FunctionExpression(function) => function.span.start,
                Expression::ArrowFunctionExpression(arrow) => arrow.span.start,
                _ => continue,
            };
            self.marked
                .entry(span_start)
                .or_insert(FrameKind::EffectCallback);
        }
    }

    fn frame_for(&self, span_start: u32) -> FrameKind {
        self.marked
            .get(&span_start)
            .copied()
            .unwrap_or(FrameKind::Other)
    }
}

impl<'a> Visit<'a> for Runner {
    fn visit_call_expression(&mut self, call: &CallExpression<'a>) {
        for rule in self.rules() {
            rule.on_call(call, &mut self.ctx);
        }
        self.mark_call_arguments(call);
        walk::walk_call_expression(self, call);
    }

    fn visit_new_expression(&mut self, new_expr: &NewExpression<'a>) {
        for rule in self.rules() {
            rule.on_new(new_expr, &mut self.ctx);
        }
        walk::walk_new_expression(self, new_expr);
    }

    fn visit_yield_expression(&mut self, yield_expr: &YieldExpression<'a>) {
        for rule in self.rules() {
            rule.on_yield(yield_expr, &mut self.ctx);
        }
        walk::walk_yield_expression(self, yield_expr);
    }

    fn visit_try_statement(&mut self, try_stmt: &TryStatement<'a>) {
        for rule in self.rules() {
            rule.on_try(try_stmt, &mut self.ctx);
        }
        walk::walk_try_statement(self, try_stmt);
    }

    fn visit_throw_statement(&mut self, throw_stmt: &ThrowStatement<'a>) {
        for rule in self.rules() {
            rule.on_throw(throw_stmt, &mut self.ctx);
        }
        walk::walk_throw_statement(self, throw_stmt);
    }

    fn visit_function(&mut self, function: &Function<'a>, flags: ScopeFlags) {
        let frame = self.frame_for(function.span.start);
        self.ctx.stack.push(frame);
        walk::walk_function(self, function, flags);
        self.ctx.stack.pop();
    }

    fn visit_arrow_function_expression(&mut self, arrow: &ArrowFunctionExpression<'a>) {
        let frame = self.frame_for(arrow.span.start);
        self.ctx.stack.push(frame);
        walk::walk_arrow_function_expression(self, arrow);
        self.ctx.stack.pop();
    }
}
