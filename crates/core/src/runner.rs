use std::collections::HashMap;

use oxc_ast::ast::{
    ArrowFunctionExpression, BinaryExpression, CallExpression, Class, Expression, Function,
    ImportDeclaration, NewExpression, Program, ReturnStatement, StaticMemberExpression,
    SwitchStatement, TaggedTemplateExpression, ThrowStatement, TryStatement, YieldExpression,
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
    pub fn new(imports: EffectImports, v4_active: bool) -> Self {
        Runner {
            ctx: FileCtx::new(imports, v4_active),
            marked: HashMap::new(),
        }
    }

    pub fn run(mut self, program: &Program) -> FileCtx {
        self.visit_program(program);
        for rule in self.rules() {
            rule.on_file_end(&mut self.ctx);
        }
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

    fn visit_return_statement(&mut self, return_stmt: &ReturnStatement<'a>) {
        for rule in self.rules() {
            rule.on_return(return_stmt, &mut self.ctx);
        }
        walk::walk_return_statement(self, return_stmt);
    }

    fn visit_binary_expression(&mut self, binary: &BinaryExpression<'a>) {
        for rule in self.rules() {
            rule.on_binary(binary, &mut self.ctx);
        }
        walk::walk_binary_expression(self, binary);
    }

    fn visit_switch_statement(&mut self, switch_stmt: &SwitchStatement<'a>) {
        for rule in self.rules() {
            rule.on_switch(switch_stmt, &mut self.ctx);
        }
        walk::walk_switch_statement(self, switch_stmt);
    }

    fn visit_static_member_expression(&mut self, member: &StaticMemberExpression<'a>) {
        for rule in self.rules() {
            rule.on_member(member, &mut self.ctx);
        }
        walk::walk_static_member_expression(self, member);
    }

    fn visit_class(&mut self, class: &Class<'a>) {
        for rule in self.rules() {
            rule.on_class(class, &mut self.ctx);
        }
        walk::walk_class(self, class);
    }

    fn visit_import_declaration(&mut self, import: &ImportDeclaration<'a>) {
        for rule in self.rules() {
            rule.on_import(import, &mut self.ctx);
        }
        walk::walk_import_declaration(self, import);
    }

    fn visit_tagged_template_expression(&mut self, template: &TaggedTemplateExpression<'a>) {
        for rule in self.rules() {
            rule.on_tagged_template(template, &mut self.ctx);
        }
        walk::walk_tagged_template_expression(self, template);
    }

    fn visit_function(&mut self, function: &Function<'a>, flags: ScopeFlags) {
        for rule in self.rules() {
            rule.on_function(function, &mut self.ctx);
        }
        let frame = self.frame_for(function.span.start);
        self.ctx.stack.push(frame);
        walk::walk_function(self, function, flags);
        self.ctx.stack.pop();
    }

    fn visit_arrow_function_expression(&mut self, arrow: &ArrowFunctionExpression<'a>) {
        for rule in self.rules() {
            rule.on_arrow(arrow, &mut self.ctx);
        }
        let frame = self.frame_for(arrow.span.start);
        self.ctx.stack.push(frame);
        walk::walk_arrow_function_expression(self, arrow);
        self.ctx.stack.pop();
    }
}
