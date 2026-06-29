use oxc_ast::ast::{
    ArrowFunctionExpression, AssignmentExpression, BinaryExpression, CallExpression, Class,
    ConditionalExpression, ExportDefaultDeclaration, Function, IfStatement, ImportDeclaration,
    ImportExpression, NewExpression, ReturnStatement, Statement, StaticMemberExpression,
    SwitchStatement, TSAsExpression, TSInterfaceDeclaration, TSNonNullExpression, TSType,
    TaggedTemplateExpression, ThrowStatement, TryStatement, VariableDeclaration, YieldExpression,
};
use oxc_span::Span;

use crate::diagnostics::{RawDiagnostic, RuleMeta, Severity};
use crate::effect_imports::EffectImports;

mod adopt;
mod agent_hygiene;
mod catch_idioms;
mod composition_limits;
mod concurrency_idioms;
mod equality_idioms;
mod error_modeling;
mod function_metrics;
mod gen_shape;
mod globals_in_effect;
mod idiom_shortcuts;
mod interruption;
mod literal_idioms;
mod logging_security;
mod map_misuse;
mod meaningful_span_names;
mod module_conventions;
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
mod oop_to_effect;
mod prefer_clock_service;
mod prefer_effect_fn;
mod prefer_effect_logging;
mod prefer_it_effect;
mod prefer_node_counterparts;
mod prefer_random_service;
mod prefer_tagged_error_classes;
mod promise_interop;
mod require_yield_star;
mod retry_only_retryable;
mod run_sync_async;
mod schedule_hygiene;
mod schema_class_hygiene;
mod schema_usage;
mod stream_hygiene;
pub(crate) mod ts_safety;
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
    /// `(structural fingerprint, span)` per non-trivial function body — the
    /// agent-hygiene duplicate detector flags repeated fingerprints at file end.
    pub fn_fingerprints: Vec<(u64, Span)>,
    /// Spans of `else if` links already covered by a reported chain head, so
    /// the agent if/else rule reports each chain exactly once.
    pub if_chain_skip: Vec<u32>,
    /// Single-method interfaces seen in this file `(name, span)` — the Strategy
    /// rule cross-references these against [`Self::implemented_interfaces`] at
    /// file end (a single-method interface with ≥2 implementers is a Strategy).
    pub single_method_interfaces: Vec<(String, Span)>,
    /// Interface names appearing in class `implements` clauses (one entry per
    /// occurrence, so duplicates count implementers).
    pub implemented_interfaces: Vec<String>,
}

/// Per-file context handed to rules: import provenance, the function-frame
/// stack maintained by the runner, profile flags, and the diagnostics sink.
pub struct FileCtx {
    pub imports: EffectImports,
    pub stack: Vec<FrameKind>,
    pub scratch: Scratch,
    pub raw: Vec<RawDiagnostic>,
    v4_active: bool,
    adopt_active: bool,
    agent_active: bool,
    agent_strict: bool,
}

impl FileCtx {
    pub fn new(
        imports: EffectImports,
        v4_active: bool,
        adopt_active: bool,
        agent_active: bool,
        agent_strict: bool,
    ) -> Self {
        FileCtx {
            imports,
            stack: Vec::new(),
            scratch: Scratch::default(),
            raw: Vec::new(),
            v4_active,
            adopt_active,
            agent_active,
            agent_strict,
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

    /// Experimental adoption rules fire only under --adopt.
    pub fn adopt_active(&self) -> bool {
        self.adopt_active
    }

    /// Experimental agent-hygiene rules fire only under --agent (or --agent-strict).
    pub fn agent_active(&self) -> bool {
        self.agent_active
    }

    pub fn report(&mut self, meta: &'static RuleMeta, span: Span, message: String) {
        self.raw.push(RawDiagnostic {
            meta,
            span,
            message,
            severity: None,
        });
    }

    /// Report an agent-hygiene finding, escalating its severity to `error`
    /// when `--agent-strict` is set (otherwise the rule's declared `warn`).
    pub fn report_agent(&mut self, meta: &'static RuleMeta, span: Span, message: String) {
        let severity = self.agent_strict.then_some(Severity::Error);
        self.raw.push(RawDiagnostic {
            meta,
            span,
            message,
            severity,
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
    fn on_if(&self, _if_stmt: &IfStatement<'_>, _ctx: &mut FileCtx) {}
    fn on_conditional(&self, _conditional: &ConditionalExpression<'_>, _ctx: &mut FileCtx) {}
    fn on_var_decl(&self, _decl: &VariableDeclaration<'_>, _ctx: &mut FileCtx) {}
    fn on_assignment(&self, _assignment: &AssignmentExpression<'_>, _ctx: &mut FileCtx) {}
    fn on_import_expression(&self, _import: &ImportExpression<'_>, _ctx: &mut FileCtx) {}
    fn on_return(&self, _return_stmt: &ReturnStatement<'_>, _ctx: &mut FileCtx) {}
    /// Any loop statement (for / for-of / for-in / while / do-while).
    fn on_loop(&self, _loop_span: Span, _body: &Statement<'_>, _ctx: &mut FileCtx) {}
    fn on_yield(&self, _yield_expr: &YieldExpression<'_>, _ctx: &mut FileCtx) {}
    fn on_try(&self, _try_stmt: &TryStatement<'_>, _ctx: &mut FileCtx) {}
    fn on_throw(&self, _throw_stmt: &ThrowStatement<'_>, _ctx: &mut FileCtx) {}
    fn on_class(&self, _class: &Class<'_>, _ctx: &mut FileCtx) {}
    fn on_interface(&self, _interface: &TSInterfaceDeclaration<'_>, _ctx: &mut FileCtx) {}
    fn on_ts_type(&self, _ts_type: &TSType<'_>, _ctx: &mut FileCtx) {}
    fn on_ts_as(&self, _as_expr: &TSAsExpression<'_>, _ctx: &mut FileCtx) {}
    fn on_ts_non_null(&self, _non_null: &TSNonNullExpression<'_>, _ctx: &mut FileCtx) {}
    fn on_export_default(&self, _export: &ExportDefaultDeclaration<'_>, _ctx: &mut FileCtx) {}
    fn on_import(&self, _import: &ImportDeclaration<'_>, _ctx: &mut FileCtx) {}
    fn on_tagged_template(&self, _template: &TaggedTemplateExpression<'_>, _ctx: &mut FileCtx) {}
    fn on_function(&self, _function: &Function<'_>, _ctx: &mut FileCtx) {}
    fn on_arrow(&self, _arrow: &ArrowFunctionExpression<'_>, _ctx: &mut FileCtx) {}
    fn on_file_end(&self, _ctx: &mut FileCtx) {}
}

/// All rule metadata across the registry, for listings and export. Includes the
/// cross-file agent rules, which fire from the engine pass rather than the
/// per-file [`Rule`] dispatch.
pub fn all_metas() -> Vec<&'static RuleMeta> {
    RULES
        .iter()
        .flat_map(|rule| rule.metas().iter().copied())
        .chain(crate::fn_index::cross_file_metas().iter().copied())
        .chain(crate::file_length::file_length_metas().iter().copied())
        .chain(ts_safety::comment_metas().iter().copied())
        .collect()
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
    &interruption::Interruption,
    &composition_limits::CompositionLimits,
    &gen_shape::GenShape,
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
    // adoption (experimental, --adopt; prefer-foreach-over-yield-loop is always on)
    &adopt::Adopt,
    // agent hygiene (experimental, --agent)
    &agent_hygiene::AgentHygiene,
    // OOP → Effect (experimental, --agent): hand-rolled design patterns Effect replaces
    &oop_to_effect::Singleton,
    &oop_to_effect::Observer,
    &oop_to_effect::Strategy,
    &oop_to_effect::Visitor,
    &oop_to_effect::ChainOfResponsibility,
    // type safety (always-on): escape hatches that defeat the type checker
    &ts_safety::NoExplicitAny,
    &ts_safety::NoNonNullAssertion,
    &ts_safety::NoUnsafeDoubleCast,
    &ts_safety::NoEmptyCatch,
    // maintainability metrics (always-on)
    &function_metrics::FunctionMetrics,
    // module conventions (experimental, --agent)
    &module_conventions::NoDefaultExport,
];
