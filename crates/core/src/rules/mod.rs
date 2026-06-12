use oxc_ast::ast::{
    ArrowFunctionExpression, BinaryExpression, CallExpression, Class, Function, ImportDeclaration,
    NewExpression, StaticMemberExpression, SwitchStatement, TaggedTemplateExpression,
    ThrowStatement, TryStatement, YieldExpression,
};
use oxc_span::Span;

use crate::diagnostics::{RawDiagnostic, RuleMeta};
use crate::effect_imports::EffectImports;

mod catch_idioms;
mod composition_limits;
mod concurrency_idioms;
mod equality_idioms;
mod error_modeling;
mod globals_in_effect;
mod idiom_shortcuts;
mod literal_idioms;
mod logging_security;
mod map_misuse;
mod promise_interop;
mod run_sync_async;
mod stream_hygiene;
mod meaningful_span_names;
mod no_chained_provides;
mod no_effect_do;
mod no_manual_sql_transactions;
mod no_or_die;
mod no_run_inside_effect;
mod no_throw_in_effect;
mod no_try_catch_in_gen;
mod no_unbounded_concurrency;
mod no_unnecessary_fail;
mod no_unnecessary_gen;
mod prefer_clock_service;
mod prefer_effect_fn;
mod prefer_effect_logging;
mod prefer_it_effect;
mod prefer_node_counterparts;
mod prefer_random_service;
mod prefer_tagged_error_classes;
mod require_yield_star;
mod retry_only_retryable;
mod schedule_hygiene;
mod schema_class_hygiene;
mod schema_usage;
mod v4_imports;
mod v4_no_gen_adapter;
mod v4_renames;

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

/// File-level facts accumulated during the walk, consumed by `on_file_end`.
#[derive(Default)]
pub struct Scratch {
    pub exponential_spans: Vec<Span>,
    pub has_schedule_jitter: bool,
    pub has_schedule_cap: bool,
}

/// Per-file context handed to rules: import provenance, the function-frame
/// stack maintained by the runner, profile flags, and the diagnostics sink.
pub struct FileCtx {
    pub imports: EffectImports,
    pub stack: Vec<FrameKind>,
    pub scratch: Scratch,
    pub raw: Vec<RawDiagnostic>,
    v4_active: bool,
}

impl FileCtx {
    pub fn new(imports: EffectImports, v4_active: bool) -> Self {
        FileCtx {
            imports,
            stack: Vec::new(),
            scratch: Scratch::default(),
            raw: Vec::new(),
            v4_active,
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

    /// v4-migration rules fire only when the codebase targets effect v4
    /// (detected from package.json) or the scan runs with --migrate.
    pub fn v4_active(&self) -> bool {
        self.v4_active
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
/// node kind to every rule; rules must be cheap per-node. A rule may own
/// several rule ids (several `RuleMeta`s) when the checks share matching.
pub trait Rule: Sync {
    /// Every rule id this implementation owns — powers `rules`/`explain`
    /// listings and the docs-site export.
    fn metas(&self) -> &'static [&'static RuleMeta];

    fn on_call(&self, _call: &CallExpression<'_>, _ctx: &mut FileCtx) {}
    fn on_new(&self, _new: &NewExpression<'_>, _ctx: &mut FileCtx) {}
    fn on_member(&self, _member: &StaticMemberExpression<'_>, _ctx: &mut FileCtx) {}
    fn on_binary(&self, _binary: &BinaryExpression<'_>, _ctx: &mut FileCtx) {}
    fn on_switch(&self, _switch_stmt: &SwitchStatement<'_>, _ctx: &mut FileCtx) {}
    fn on_yield(&self, _yield_expr: &YieldExpression<'_>, _ctx: &mut FileCtx) {}
    fn on_try(&self, _try_stmt: &TryStatement<'_>, _ctx: &mut FileCtx) {}
    fn on_throw(&self, _throw_stmt: &ThrowStatement<'_>, _ctx: &mut FileCtx) {}
    fn on_class(&self, _class: &Class<'_>, _ctx: &mut FileCtx) {}
    fn on_import(&self, _import: &ImportDeclaration<'_>, _ctx: &mut FileCtx) {}
    fn on_tagged_template(&self, _template: &TaggedTemplateExpression<'_>, _ctx: &mut FileCtx) {}
    fn on_function(&self, _function: &Function<'_>, _ctx: &mut FileCtx) {}
    fn on_arrow(&self, _arrow: &ArrowFunctionExpression<'_>, _ctx: &mut FileCtx) {}
    fn on_file_end(&self, _ctx: &mut FileCtx) {}
}

/// All rule metadata across the registry, for listings and export.
pub fn all_metas() -> Vec<&'static RuleMeta> {
    RULES.iter().flat_map(|rule| rule.metas().iter().copied()).collect()
}

pub static RULES: &[&(dyn Rule + Send + Sync)] = &[
    // correctness
    &require_yield_star::RequireYieldStar,
    &no_try_catch_in_gen::NoTryCatchInGen,
    &no_throw_in_effect::NoThrowInEffect,
    &no_run_inside_effect::NoRunInsideEffect,
    &schema_class_hygiene::SchemaClassHygiene,
    &promise_interop::PromiseInterop,
    &run_sync_async::NoRunSyncOnAsync,
    &map_misuse::NoMapReturningEffect,
    &stream_hygiene::StreamHygiene,
    &equality_idioms::EqualityIdioms,
    &error_modeling::ErrorModeling,
    &concurrency_idioms::ConcurrencyIdioms,
    &literal_idioms::LiteralIdioms,
    &logging_security::LoggingSecurity,
    &composition_limits::CompositionLimits,
    // idiomatic
    &no_unnecessary_gen::NoUnnecessaryGen,
    &no_unnecessary_fail::NoUnnecessaryFail,
    &no_effect_do::NoEffectDo,
    &no_or_die::NoOrDie,
    &idiom_shortcuts::IdiomShortcuts,
    &catch_idioms::CatchIdioms,
    &prefer_clock_service::PreferClockService,
    &prefer_random_service::PreferRandomService,
    &prefer_effect_logging::PreferEffectLogging,
    &globals_in_effect::GlobalsInEffect,
    &prefer_effect_fn::PreferEffectFn,
    &prefer_tagged_error_classes::PreferTaggedErrorClasses,
    &schema_usage::SchemaUsage,
    &meaningful_span_names::MeaningfulSpanNames,
    &prefer_node_counterparts::PreferNodeCounterparts,
    &prefer_it_effect::PreferItEffect,
    // architecture
    &no_chained_provides::NoChainedProvides,
    &no_manual_sql_transactions::NoManualSqlTransactions,
    &retry_only_retryable::RetryOnlyRetryable,
    // performance
    &schedule_hygiene::ScheduleHygiene,
    &no_unbounded_concurrency::NoUnboundedConcurrency,
    // v4 migration
    &v4_no_gen_adapter::V4NoGenAdapter,
    &v4_renames::V4Renames,
    &v4_imports::V4Imports,
];
