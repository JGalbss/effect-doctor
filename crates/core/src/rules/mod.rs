use oxc_ast::ast::{
    CallExpression, NewExpression, ThrowStatement, TryStatement, YieldExpression,
};
use oxc_span::Span;

use crate::diagnostics::{RawDiagnostic, RuleMeta};
use crate::effect_imports::EffectImports;

mod no_run_inside_effect;
mod no_throw_in_effect;
mod no_try_catch_in_gen;
mod no_unnecessary_gen;
mod prefer_clock_service;
mod prefer_effect_logging;
mod prefer_random_service;
mod require_yield_star;
mod v4_no_gen_adapter;

/// What kind of function frame the runner is currently inside.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameKind {
    /// Generator passed to `Effect.gen` / `Effect.fn`.
    EffectGen,
    /// Function/arrow passed as an argument to any `Effect.*` call.
    EffectCallback,
    /// Any other function.
    Other,
}

/// Per-file context handed to rules: import provenance, the function-frame
/// stack maintained by the runner, and the diagnostics sink.
pub struct FileCtx {
    pub imports: EffectImports,
    pub stack: Vec<FrameKind>,
    pub raw: Vec<RawDiagnostic>,
}

impl FileCtx {
    pub fn new(imports: EffectImports) -> Self {
        FileCtx {
            imports,
            stack: Vec::new(),
            raw: Vec::new(),
        }
    }

    /// Innermost function frame is an Effect generator.
    pub fn in_effect_gen(&self) -> bool {
        self.stack.last() == Some(&FrameKind::EffectGen)
    }

    /// Anywhere inside Effect-managed code (gen body or Effect.* callback).
    pub fn in_effect_code(&self) -> bool {
        self.stack
            .iter()
            .any(|frame| matches!(frame, FrameKind::EffectGen | FrameKind::EffectCallback))
    }

    pub fn report(&mut self, meta: &'static RuleMeta, span: Span, message: String) {
        self.raw.push(RawDiagnostic {
            meta,
            span,
            message,
        });
    }
}

/// A lint rule. The runner makes one pass over the AST and dispatches each
/// node kind to every rule; rules must be cheap per-node.
pub trait Rule: Sync {
    fn meta(&self) -> &'static RuleMeta;
    fn on_call(&self, _call: &CallExpression<'_>, _ctx: &mut FileCtx) {}
    fn on_new(&self, _new: &NewExpression<'_>, _ctx: &mut FileCtx) {}
    fn on_yield(&self, _yield_expr: &YieldExpression<'_>, _ctx: &mut FileCtx) {}
    fn on_try(&self, _try_stmt: &TryStatement<'_>, _ctx: &mut FileCtx) {}
    fn on_throw(&self, _throw_stmt: &ThrowStatement<'_>, _ctx: &mut FileCtx) {}
}

pub static RULES: &[&(dyn Rule + Send + Sync)] = &[
    &require_yield_star::RequireYieldStar,
    &no_try_catch_in_gen::NoTryCatchInGen,
    &no_throw_in_effect::NoThrowInEffect,
    &no_run_inside_effect::NoRunInsideEffect,
    &no_unnecessary_gen::NoUnnecessaryGen,
    &prefer_clock_service::PreferClockService,
    &prefer_random_service::PreferRandomService,
    &prefer_effect_logging::PreferEffectLogging,
    &v4_no_gen_adapter::V4NoGenAdapter,
];
