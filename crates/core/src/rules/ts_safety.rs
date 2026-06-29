//! TypeScript escape hatches that defeat the type checker — the moves an agent
//! reaches for to make a red squiggle go away instead of fixing the types.
//! Always-on (every scanned file), `warn`, escalating to `error` under
//! `--agent-strict`.

use oxc_ast::ast::{Expression, Program, TSAsExpression, TSNonNullExpression, TSType, TryStatement};

use crate::diagnostics::{Category, RawDiagnostic, RuleMeta, Severity};
use crate::matchers::unwrap_parens;
use crate::rules::{FileCtx, Rule};

static NO_EXPLICIT_ANY: RuleMeta = RuleMeta {
    id: "no-explicit-any",
    severity: Severity::Warn,
    category: Category::TypeSafety,
    help: "`any` opts the value out of type checking entirely and silently spreads. Use `unknown` + a Schema/narrowing decode at the boundary, a generic, or the precise type.",
};

static NO_NON_NULL_ASSERTION: RuleMeta = RuleMeta {
    id: "no-non-null-assertion",
    severity: Severity::Warn,
    category: Category::TypeSafety,
    help: "The `!` non-null assertion lies to the compiler and throws at runtime when wrong. Narrow with a guard, use optional chaining, or model absence with Option.",
};

static NO_UNSAFE_DOUBLE_CAST: RuleMeta = RuleMeta {
    id: "no-unsafe-double-cast",
    severity: Severity::Warn,
    category: Category::TypeSafety,
    help: "`x as Y as Z` (or `as unknown as`) forces an unrelated type — the type system can no longer help you. Decode/validate the value (Schema) or fix the source type instead.",
};

static NO_EMPTY_CATCH: RuleMeta = RuleMeta {
    id: "no-empty-catch",
    severity: Severity::Warn,
    category: Category::TypeSafety,
    help: "An empty catch silently swallows failures. Handle the error, log it, or model it as a typed failure (Effect.catchTag / Effect.tapError) — never discard it.",
};

static NO_TS_IGNORE: RuleMeta = RuleMeta {
    id: "no-ts-ignore",
    severity: Severity::Warn,
    category: Category::TypeSafety,
    help: "`@ts-ignore` / `@ts-expect-error` suppress a real type error rather than fixing it. Address the underlying type; if a suppression is truly unavoidable, prefer `@ts-expect-error` with an explanatory comment.",
};

/// Metas for the comment-scanned `no-ts-ignore` rule, which fires from the
/// engine pass (comments are not AST nodes), so it is appended to the catalog
/// here rather than via a [`Rule`] impl.
pub fn comment_metas() -> &'static [&'static RuleMeta] {
    static METAS: &[&RuleMeta] = &[&NO_TS_IGNORE];
    METAS
}

/// Scan a parsed file's comments for `@ts-ignore` / `@ts-expect-error`
/// suppressions. Returns raw diagnostics the caller folds into the file's set.
pub fn ts_ignore_findings(program: &Program<'_>, source: &str) -> Vec<RawDiagnostic> {
    program
        .comments
        .iter()
        .filter_map(|comment| {
            let text = source.get(comment.span.start as usize..comment.span.end as usize)?;
            if !text.contains("@ts-ignore") && !text.contains("@ts-expect-error") {
                return None;
            }
            Some(RawDiagnostic {
                meta: &NO_TS_IGNORE,
                span: comment.span,
                message: "type-checker suppression comment — fix the underlying type error".to_string(),
                severity: None,
            })
        })
        .collect()
}

pub struct NoExplicitAny;

impl Rule for NoExplicitAny {
    fn metas(&self) -> &'static [&'static RuleMeta] {
        static METAS: &[&RuleMeta] = &[&NO_EXPLICIT_ANY];
        METAS
    }

    fn on_ts_type(&self, ts_type: &TSType<'_>, ctx: &mut FileCtx) {
        if let TSType::TSAnyKeyword(any) = ts_type {
            ctx.report_agent(&NO_EXPLICIT_ANY, any.span, "`any` defeats type checking — use `unknown` + decode, a generic, or the precise type".to_string());
        }
    }
}

pub struct NoNonNullAssertion;

impl Rule for NoNonNullAssertion {
    fn metas(&self) -> &'static [&'static RuleMeta] {
        static METAS: &[&RuleMeta] = &[&NO_NON_NULL_ASSERTION];
        METAS
    }

    fn on_ts_non_null(&self, non_null: &TSNonNullExpression<'_>, ctx: &mut FileCtx) {
        ctx.report_agent(&NO_NON_NULL_ASSERTION, non_null.span, "non-null assertion `!` — narrow with a guard, optional chaining, or Option".to_string());
    }
}

pub struct NoUnsafeDoubleCast;

impl Rule for NoUnsafeDoubleCast {
    fn metas(&self) -> &'static [&'static RuleMeta] {
        static METAS: &[&RuleMeta] = &[&NO_UNSAFE_DOUBLE_CAST];
        METAS
    }

    fn on_ts_as(&self, as_expr: &TSAsExpression<'_>, ctx: &mut FileCtx) {
        // `x as Y as Z` (incl. `(x as unknown) as Z`): inner is itself an `as` cast.
        if matches!(unwrap_parens(&as_expr.expression), Expression::TSAsExpression(_)) {
            ctx.report_agent(&NO_UNSAFE_DOUBLE_CAST, as_expr.span, "double type assertion forces an unrelated type — decode/validate instead".to_string());
        }
    }
}

pub struct NoEmptyCatch;

impl Rule for NoEmptyCatch {
    fn metas(&self) -> &'static [&'static RuleMeta] {
        static METAS: &[&RuleMeta] = &[&NO_EMPTY_CATCH];
        METAS
    }

    fn on_try(&self, try_stmt: &TryStatement<'_>, ctx: &mut FileCtx) {
        let Some(handler) = &try_stmt.handler else {
            return;
        };
        if handler.body.body.is_empty() {
            ctx.report_agent(&NO_EMPTY_CATCH, handler.span, "empty catch swallows the error — handle it or model it as a typed failure".to_string());
        }
    }
}
