//! Experimental `--agent` mode ("agent doctor"): the non-Effect, non-functional
//! "slop" patterns LLM agents reach for by default — `if/else` chains, ternaries,
//! string-equality guards, raw loops, `let` mutation — plus copy-pasted function
//! bodies. Every finding has a cleaner Effect / `Match` / combinator form.
//!
//! The whole family is opt-in (`--agent`) and defaults to `warn`; `--agent-strict`
//! escalates each to `error` so the scan can gate CI. `agent-duplicate-function`
//! is the exception — it stays an `info` suggestion regardless of `--agent-strict`.

use std::collections::HashMap;

use oxc_ast::ast::{
    Argument, ArrowFunctionExpression, AssignmentExpression, AssignmentTarget, BinaryExpression,
    CallExpression, ConditionalExpression, Expression, Function, FunctionBody, IfStatement,
    ImportDeclaration, ImportDeclarationSpecifier, ImportExpression, ModuleExportName, Statement,
    TSAsExpression, TSType, TSTypeName, ThrowStatement, TryStatement, UnaryExpression,
    VariableDeclaration, VariableDeclarationKind,
};
use oxc_span::Span;
use oxc_syntax::operator::BinaryOperator;

use crate::diagnostics::{Category, RuleMeta, Severity};
use crate::matchers::{ident_name, static_member, unwrap_parens};
use crate::rules::{FileCtx, Rule};
use crate::structural;

static IF_ELSE_CHAIN: RuleMeta = RuleMeta {
    id: "agent-no-if-else-chain",
    severity: Severity::Warn,
    category: Category::AgentHygiene,
    help: "if/else (and else-if) chains hide branching. Prefer early returns, a lookup map, or Match.value(...).pipe(Match.when(...), Match.exhaustive) for declarative, exhaustive dispatch.",
};

static TERNARY: RuleMeta = RuleMeta {
    id: "agent-no-ternary",
    severity: Severity::Warn,
    category: Category::AgentHygiene,
    help: "Ternaries hide branching. Extract a named helper, or use Match.value(...).pipe(Match.when(...), Match.orElse(...)) so each branch is named and exhaustive.",
};

static STRING_EQUALITY_GUARD: RuleMeta = RuleMeta {
    id: "agent-no-string-equality-guard",
    severity: Severity::Warn,
    category: Category::AgentHygiene,
    help: "Comparing against a string literal (`x === \"user\"`) is a stringly-typed guard. Use a type guard / predicate (isUser(x)) or Match.when on a tagged value for exhaustiveness.",
};

static RAW_LOOP: RuleMeta = RuleMeta {
    id: "agent-no-raw-loop",
    severity: Severity::Warn,
    category: Category::AgentHygiene,
    help: "Raw for/while loops mutate and obscure intent. Use Array combinators (map/filter/reduce) for pure data, or Effect.forEach / Effect.reduce for effectful iteration (concurrency + interruption included).",
};

static LET_BINDING: RuleMeta = RuleMeta {
    id: "agent-no-let",
    severity: Severity::Warn,
    category: Category::AgentHygiene,
    help: "`let`/`var` signal mutation. Bind with `const` and build values functionally — Array.reduce, Match for conditional values, or an Effect pipeline instead of reassigning.",
};

static DUPLICATE_FUNCTION: RuleMeta = RuleMeta {
    id: "agent-duplicate-function",
    severity: Severity::Info,
    category: Category::AgentHygiene,
    help: "Two functions in this file share a structurally identical body — agents copy-paste instead of extracting. Hoist the shared logic into one reusable helper.",
};

static MUTATION: RuleMeta = RuleMeta {
    id: "agent-no-mutation",
    severity: Severity::Warn,
    category: Category::AgentHygiene,
    help: "Reassigning a binding or mutating a payload in place creates intermediate states that are hard to follow. Derive the final value in one expression (const + Match/reduce/pipe) instead of building it up by mutation.",
};

static INLINE_IMPORT: RuleMeta = RuleMeta {
    id: "agent-no-inline-import",
    severity: Severity::Warn,
    category: Category::AgentHygiene,
    help: "Inline `await import(...)` / `require(...)` hides a module's dependencies mid-body — agents add them to dodge a missing top-level import. Hoist to a static top-level `import` (keep dynamic import only for deliberate code-splitting).",
};

static ANY_TYPE: RuleMeta = RuleMeta {
    id: "agent-no-any",
    severity: Severity::Warn,
    category: Category::AgentHygiene,
    help: "`any` opts out of type checking and silently spreads. Use a precise type, `unknown` + narrowing, or a Schema decode at the boundary. (opencode: \"avoid using the any type\".)",
};

static IMPORT_ALIAS: RuleMeta = RuleMeta {
    id: "agent-no-import-alias",
    severity: Severity::Warn,
    category: Category::AgentHygiene,
    help: "Aliasing an import (`import { x as y }`) hides the real name and breaks grep-ability. Import under the original name. (opencode: \"never alias imports\"; effect imports are exempt.)",
};

static NAMESPACE_IMPORT: RuleMeta = RuleMeta {
    id: "agent-no-namespace-import",
    severity: Severity::Warn,
    category: Category::AgentHygiene,
    help: "Star imports (`import * as x`) pull in the whole module and obscure what's used. Import the named bindings you need. (opencode: \"never use star imports\"; effect's `import * as Effect` idiom is exempt.)",
};

static TRY_CATCH: RuleMeta = RuleMeta {
    id: "agent-no-try-catch",
    severity: Severity::Info,
    category: Category::AgentHygiene,
    help: "Avoid try/catch where possible — it produces untyped errors and tangled control flow. Model failures in the typed channel (Effect.try/tryPromise + catchTag) or return a Result. (opencode: \"avoid try/catch where possible\".)",
};

static DEFAULT_EXPORT: RuleMeta = RuleMeta {
    id: "agent-no-default-export",
    severity: Severity::Warn,
    category: Category::AgentHygiene,
    help: "Default exports rename freely at each import site and resist refactors/auto-import. Use a named export.",
};

static AS_CAST: RuleMeta = RuleMeta {
    id: "agent-no-as-cast",
    severity: Severity::Warn,
    category: Category::AgentHygiene,
    help: "An `as` cast asserts a type the compiler can't verify, silently hiding mismatches. Narrow with a type guard, or decode with Schema at the boundary instead of casting. (`as const` is fine.)",
};

static UNBOUNDED_PROMISE_ALL: RuleMeta = RuleMeta {
    id: "agent-no-unbounded-promise-all",
    severity: Severity::Warn,
    category: Category::AgentHygiene,
    help: "`Promise.all(array.map(...))` fans out the whole list at once — it can exhaust the DB pool, trip rate limits, or cascade one slow dependency. Cap it with p-limit, or use Effect.forEach with an explicit `concurrency`.",
};

static LOOSE_EQUALITY: RuleMeta = RuleMeta {
    id: "agent-no-loose-equality",
    severity: Severity::Warn,
    category: Category::AgentHygiene,
    help: "`==` / `!=` coerce operands and hide bugs. Use `===` / `!==` (or `Equal.equals` for structural equality). `== null` / `!= null` are exempt — that's the one idiomatic nullish check.",
};

static NON_NULL_ASSERTION: RuleMeta = RuleMeta {
    id: "agent-no-non-null-assertion",
    severity: Severity::Warn,
    category: Category::AgentHygiene,
    help: "The non-null assertion `x!` asserts a value is present without proof — it crashes at runtime when wrong. Narrow with a guard, handle the `undefined` case, or model absence with Option.",
};

static TS_ENUM: RuleMeta = RuleMeta {
    id: "agent-no-enum",
    severity: Severity::Warn,
    category: Category::AgentHygiene,
    help: "TS `enum` emits runtime code and has surprising semantics. Use a union of string literals (or `Schema.Literals(...)`) and derive the type — it's erasable and decodes cleanly.",
};

static TS_NAMESPACE: RuleMeta = RuleMeta {
    id: "agent-no-ts-namespace",
    severity: Severity::Info,
    category: Category::AgentHygiene,
    help: "TS `namespace` is a legacy module system that emits runtime code and nests scope awkwardly. Use ES modules (one file = one module) and named exports.",
};

static THROW: RuleMeta = RuleMeta {
    id: "agent-no-throw",
    severity: Severity::Info,
    category: Category::AgentHygiene,
    help: "Throwing outside Effect produces an untyped exception that escapes the type system. Model failure explicitly — return a Result/Either, or `Effect.fail` a tagged error inside Effect code.",
};

static DELETE_OP: RuleMeta = RuleMeta {
    id: "agent-no-delete",
    severity: Severity::Info,
    category: Category::AgentHygiene,
    help: "`delete` mutates an object in place and deoptimizes it. Build a new object without the key (destructure-and-rest, or `Struct.omit`) instead.",
};

static DEEP_NESTING: RuleMeta = RuleMeta {
    id: "agent-deep-nesting",
    severity: Severity::Warn,
    category: Category::AgentHygiene,
    help: "Deeply nested control flow is hard to follow. Flatten with guard clauses / early returns, extract the inner block into a named helper, or replace branching with Match / Array combinators.",
};

static HIGH_COMPLEXITY: RuleMeta = RuleMeta {
    id: "agent-high-complexity",
    severity: Severity::Warn,
    category: Category::AgentHygiene,
    help: "This function has high cyclomatic complexity (too many branches/loops/conditions). Split it into smaller named functions, or replace the branching with Match / a lookup / an Effect pipeline.",
};

static TOO_MANY_PARAMS: RuleMeta = RuleMeta {
    id: "agent-too-many-params",
    severity: Severity::Warn,
    category: Category::AgentHygiene,
    help: "A long positional parameter list is error-prone at the call site. Pass a single named options object (or a Schema-validated input).",
};

static DEEP_RELATIVE_IMPORT: RuleMeta = RuleMeta {
    id: "agent-deep-relative-import",
    severity: Severity::Info,
    category: Category::AgentHygiene,
    help: "A deep relative import (`../../../`) is brittle and signals a module reaching across the architecture. Use a path alias / package entry point, or move the shared code closer.",
};

/// ESLint `max-depth` default — flag nesting deeper than this.
const MAX_DEPTH: u32 = 4;
/// SonarJS cognitive-complexity default — flag functions above this.
const MAX_COMPLEXITY: u32 = 15;
/// Flag functions with more positional parameters than this.
const MAX_PARAMS: usize = 5;
/// Flag relative imports climbing at least this many directories.
const MAX_PARENT_HOPS: usize = 3;

static INLINE_TYPE_IMPORT: RuleMeta = RuleMeta {
    id: "agent-no-inline-type-import",
    severity: Severity::Warn,
    category: Category::AgentHygiene,
    help: "Inline `import(\"...\").Foo` type references are hard to read and move. Use a top-level `import type { Foo } from \"...\"` — it's erased at compile time, so it can't cause circular-dependency issues. (rogo)",
};

static SAFE_PARSE: RuleMeta = RuleMeta {
    id: "agent-prefer-safe-parse",
    severity: Severity::Warn,
    category: Category::AgentHygiene,
    help: "`Schema.parse()` throws far from the call site. Prefer `.safeParse()` (Zod) / a decode-to-Either (Effect Schema) so the failure path is handled explicitly — surface a 400, fall back, or log and skip.",
};

fn is_loose_equality(operator: BinaryOperator) -> bool {
    matches!(
        operator,
        BinaryOperator::Equality | BinaryOperator::Inequality
    )
}

fn is_null_literal(expr: &Expression) -> bool {
    matches!(unwrap_parens(expr), Expression::NullLiteral(_))
}

/// `<X>Schema.parse(...)` — a Zod/Schema parse whose receiver name ends in
/// `Schema`, distinguishing it from `JSON.parse` / `Number.parse` / etc.
fn is_schema_parse(call: &CallExpression) -> bool {
    matches!(
        static_member(&call.callee),
        Some((object, "parse")) if ident_name(object).is_some_and(|name| name.ends_with("Schema"))
    )
}

fn is_const_assertion(ty: &TSType) -> bool {
    matches!(
        ty,
        TSType::TSTypeReference(reference)
            if matches!(&reference.type_name, TSTypeName::IdentifierReference(id) if id.name == "const")
    )
}

/// `<array>.map(...)` — the unbounded fan-out source for `Promise.all`.
fn is_map_call(expr: &Expression) -> bool {
    matches!(
        unwrap_parens(expr),
        Expression::CallExpression(call) if matches!(static_member(&call.callee), Some((_, "map")))
    )
}

/// `effect`, `effect/...`, `@effect/...` — the namespace/alias import rules
/// exempt these so Effect's idiomatic `import * as Effect from "effect"` and
/// occasional aliases aren't punished.
fn is_effect_module(specifier: &str) -> bool {
    specifier == "effect" || specifier.starts_with("effect/") || specifier.starts_with("@effect/")
}

fn export_name<'a>(name: &'a ModuleExportName) -> &'a str {
    match name {
        ModuleExportName::IdentifierName(id) => id.name.as_str(),
        ModuleExportName::IdentifierReference(id) => id.name.as_str(),
        ModuleExportName::StringLiteral(s) => s.value.as_str(),
    }
}

fn is_equality(operator: BinaryOperator) -> bool {
    matches!(
        operator,
        BinaryOperator::Equality
            | BinaryOperator::Inequality
            | BinaryOperator::StrictEquality
            | BinaryOperator::StrictInequality
    )
}

fn is_string_literal(expr: &Expression) -> bool {
    matches!(unwrap_parens(expr), Expression::StringLiteral(_))
}

/// `<x>._tag` — owned by the equality-idioms tag rules; the agent guard rule
/// defers to those so tag comparisons aren't double-reported.
fn is_tag_member(expr: &Expression) -> bool {
    matches!(
        unwrap_parens(expr),
        Expression::StaticMemberExpression(member) if member.property.name == "_tag"
    )
}

fn keyword_span(span: Span, len: u32) -> Span {
    Span::new(span.start, span.start + len)
}

/// `=` (or a compound `+=`…) onto a plain identifier is a reassignment; onto a
/// member/index target it mutates an object in place. Both are intermediate
/// state — describe which for the message.
fn mutation_kind(target: &AssignmentTarget) -> &'static str {
    match target {
        AssignmentTarget::AssignmentTargetIdentifier(_) => "reassignment",
        _ => "in-place mutation",
    }
}

pub struct AgentHygiene;

impl AgentHygiene {
    /// Per-function checks that share the single structural walk: parameter
    /// count, then (for non-trivial bodies) the dup fingerprint plus the
    /// nesting-depth and cyclomatic-complexity metrics.
    fn check_function(
        &self,
        body: Option<&FunctionBody>,
        param_count: usize,
        span: Span,
        ctx: &mut FileCtx,
    ) {
        if !ctx.agent_active() {
            return;
        }
        if param_count > MAX_PARAMS {
            ctx.report_agent(
                &TOO_MANY_PARAMS,
                keyword_span(span, 1),
                format!("{param_count} parameters — pass a single named options object"),
            );
        }
        let Some(body) = body else { return };
        let Some(shape) = structural::analyze(body, param_count) else {
            return;
        };
        ctx.scratch
            .fn_fingerprints
            .push((shape.identity_hash(), span));
        if shape.max_depth > MAX_DEPTH {
            ctx.report_agent(
                &DEEP_NESTING,
                keyword_span(span, 1),
                format!(
                    "nested {} levels deep — flatten with guard clauses or extract a helper",
                    shape.max_depth
                ),
            );
        }
        if shape.complexity > MAX_COMPLEXITY {
            ctx.report_agent(
                &HIGH_COMPLEXITY,
                keyword_span(span, 1),
                format!(
                    "cyclomatic complexity {} — split into smaller functions or use Match",
                    shape.complexity
                ),
            );
        }
    }
}

impl Rule for AgentHygiene {
    fn metas(&self) -> &'static [&'static RuleMeta] {
        static METAS: &[&RuleMeta] = &[
            &IF_ELSE_CHAIN,
            &TERNARY,
            &STRING_EQUALITY_GUARD,
            &RAW_LOOP,
            &LET_BINDING,
            &MUTATION,
            &INLINE_IMPORT,
            &ANY_TYPE,
            &IMPORT_ALIAS,
            &NAMESPACE_IMPORT,
            &TRY_CATCH,
            &DEFAULT_EXPORT,
            &AS_CAST,
            &UNBOUNDED_PROMISE_ALL,
            &LOOSE_EQUALITY,
            &NON_NULL_ASSERTION,
            &TS_ENUM,
            &SAFE_PARSE,
            &INLINE_TYPE_IMPORT,
            &TS_NAMESPACE,
            &THROW,
            &DELETE_OP,
            &DEEP_NESTING,
            &HIGH_COMPLEXITY,
            &TOO_MANY_PARAMS,
            &DEEP_RELATIVE_IMPORT,
            &DUPLICATE_FUNCTION,
        ];
        METAS
    }

    fn on_ts_namespace(&self, span: Span, ctx: &mut FileCtx) {
        if !ctx.agent_active() {
            return;
        }
        ctx.report_agent(
            &TS_NAMESPACE,
            keyword_span(span, 9),
            "TS namespace — use ES modules and named exports".to_string(),
        );
    }

    fn on_throw(&self, throw_stmt: &ThrowStatement<'_>, ctx: &mut FileCtx) {
        // Inside Effect code, `no-throw-in-effect` (error) owns this.
        if !ctx.agent_active() || ctx.in_effect_code() {
            return;
        }
        ctx.report_agent(
            &THROW,
            keyword_span(throw_stmt.span, 5),
            "throw — return a Result/Either or Effect.fail a tagged error".to_string(),
        );
    }

    fn on_unary(&self, unary: &UnaryExpression<'_>, ctx: &mut FileCtx) {
        if !ctx.agent_active() || !unary.operator.is_delete() {
            return;
        }
        ctx.report_agent(
            &DELETE_OP,
            keyword_span(unary.span, 6),
            "delete mutates in place — build a new object without the key".to_string(),
        );
    }

    fn on_ts_import_type(&self, span: Span, ctx: &mut FileCtx) {
        if !ctx.agent_active() {
            return;
        }
        ctx.report_agent(
            &INLINE_TYPE_IMPORT,
            span,
            "inline `import(\"...\").Foo` type — use a top-level `import type`".to_string(),
        );
    }

    fn on_ts_non_null(&self, span: Span, ctx: &mut FileCtx) {
        if !ctx.agent_active() {
            return;
        }
        ctx.report_agent(
            &NON_NULL_ASSERTION,
            span,
            "non-null assertion `!` — narrow with a guard or model absence with Option".to_string(),
        );
    }

    fn on_ts_enum(&self, span: Span, ctx: &mut FileCtx) {
        if !ctx.agent_active() {
            return;
        }
        ctx.report_agent(
            &TS_ENUM,
            keyword_span(span, 4),
            "TS enum — use a union of string literals / Schema.Literals and derive the type"
                .to_string(),
        );
    }

    fn on_export_default(&self, span: Span, ctx: &mut FileCtx) {
        if !ctx.agent_active() {
            return;
        }
        ctx.report_agent(
            &DEFAULT_EXPORT,
            keyword_span(span, 6),
            "default export — use a named export".to_string(),
        );
    }

    fn on_ts_as_expression(&self, as_expr: &TSAsExpression<'_>, ctx: &mut FileCtx) {
        if !ctx.agent_active() || is_const_assertion(&as_expr.type_annotation) {
            return;
        }
        ctx.report_agent(
            &AS_CAST,
            as_expr.span,
            "`as` cast — narrow with a type guard or decode with Schema instead".to_string(),
        );
    }

    fn on_ts_any(&self, span: Span, ctx: &mut FileCtx) {
        if !ctx.agent_active() {
            return;
        }
        ctx.report_agent(
            &ANY_TYPE,
            span,
            "`any` — use a precise type, `unknown` + narrowing, or a Schema decode".to_string(),
        );
    }

    fn on_import(&self, import: &ImportDeclaration<'_>, ctx: &mut FileCtx) {
        let source = import.source.value.as_str();
        if !ctx.agent_active() || is_effect_module(source) {
            return;
        }
        if source.split('/').filter(|segment| *segment == "..").count() >= MAX_PARENT_HOPS {
            ctx.report_agent(
                &DEEP_RELATIVE_IMPORT,
                import.source.span,
                format!("`{source}` reaches across the tree — use a path alias or move the shared code closer"),
            );
        }
        let Some(specifiers) = import.specifiers.as_ref() else {
            return;
        };
        for specifier in specifiers {
            match specifier {
                ImportDeclarationSpecifier::ImportNamespaceSpecifier(namespace) => {
                    ctx.report_agent(
                        &NAMESPACE_IMPORT,
                        namespace.span,
                        format!(
                            "`import * as {}` — import the named bindings you use",
                            namespace.local.name
                        ),
                    );
                }
                ImportDeclarationSpecifier::ImportSpecifier(named) => {
                    let imported = export_name(&named.imported);
                    if imported != named.local.name.as_str() {
                        ctx.report_agent(
                            &IMPORT_ALIAS,
                            named.span,
                            format!(
                                "`{} as {}` — import under its real name",
                                imported, named.local.name
                            ),
                        );
                    }
                }
                ImportDeclarationSpecifier::ImportDefaultSpecifier(_) => {}
            }
        }
    }

    fn on_try(&self, try_stmt: &TryStatement<'_>, ctx: &mut FileCtx) {
        // Inside an Effect generator, `no-try-catch-in-gen` (error) owns this.
        if !ctx.agent_active() || ctx.in_effect_gen() {
            return;
        }
        ctx.report_agent(
            &TRY_CATCH,
            keyword_span(try_stmt.span, 3),
            "try/catch — model failure in the typed channel (Effect.try + catchTag) or return a Result".to_string(),
        );
    }

    fn on_if(&self, if_stmt: &IfStatement<'_>, ctx: &mut FileCtx) {
        if !ctx.agent_active() {
            return;
        }
        // Each `else if` is itself an IfStatement nested in a parent's
        // `alternate`; the runner visits the chain head first, where we mark the
        // descendant links so the chain reports exactly once.
        if let Some(position) = ctx
            .scratch
            .if_chain_skip
            .iter()
            .position(|start| *start == if_stmt.span.start)
        {
            ctx.scratch.if_chain_skip.swap_remove(position);
            return;
        }
        let Some(alternate) = if_stmt.alternate.as_ref() else {
            return;
        };
        let mut link = Some(alternate);
        while let Some(Statement::IfStatement(inner)) = link {
            ctx.scratch.if_chain_skip.push(inner.span.start);
            link = inner.alternate.as_ref();
        }
        ctx.report_agent(
            &IF_ELSE_CHAIN,
            keyword_span(if_stmt.span, 2),
            "if/else chain — use early returns, a lookup map, or Match.exhaustive".to_string(),
        );
    }

    fn on_conditional(&self, conditional: &ConditionalExpression<'_>, ctx: &mut FileCtx) {
        if !ctx.agent_active() {
            return;
        }
        ctx.report_agent(
            &TERNARY,
            conditional.span,
            "ternary — extract a named helper or use Match.when/orElse".to_string(),
        );
    }

    fn on_binary(&self, binary: &BinaryExpression<'_>, ctx: &mut FileCtx) {
        if !ctx.agent_active() || !is_equality(binary.operator) {
            return;
        }
        // `==` / `!=` (except the idiomatic `== null` / `!= null`) — the
        // operator itself is the smell, independent of the string-guard check.
        if is_loose_equality(binary.operator)
            && !is_null_literal(&binary.left)
            && !is_null_literal(&binary.right)
        {
            ctx.report_agent(
                &LOOSE_EQUALITY,
                binary.span,
                format!(
                    "`{}` coerces — use `{}=` (or Equal.equals)",
                    binary.operator.as_str(),
                    binary.operator.as_str()
                ),
            );
        }
        // `_tag` comparisons belong to the equality-idioms tag rules.
        if is_tag_member(&binary.left) || is_tag_member(&binary.right) {
            return;
        }
        if !is_string_literal(&binary.left) && !is_string_literal(&binary.right) {
            return;
        }
        ctx.report_agent(
            &STRING_EQUALITY_GUARD,
            binary.span,
            format!(
                "`{}` against a string literal — use a type guard / predicate or Match.when",
                binary.operator.as_str()
            ),
        );
    }

    fn on_loop(&self, loop_span: Span, _body: &Statement<'_>, ctx: &mut FileCtx) {
        if !ctx.agent_active() {
            return;
        }
        ctx.report_agent(
            &RAW_LOOP,
            keyword_span(loop_span, 3),
            "raw loop — use Array.map/filter/reduce or Effect.forEach / Effect.reduce".to_string(),
        );
    }

    fn on_var_decl(&self, decl: &VariableDeclaration<'_>, ctx: &mut FileCtx) {
        if !ctx.agent_active() {
            return;
        }
        if !matches!(
            decl.kind,
            VariableDeclarationKind::Let | VariableDeclarationKind::Var
        ) {
            return;
        }
        ctx.report_agent(
            &LET_BINDING,
            keyword_span(decl.span, decl.kind.as_str().len() as u32),
            format!(
                "`{}` signals mutation — bind with const and build values functionally",
                decl.kind.as_str()
            ),
        );
    }

    fn on_assignment(&self, assignment: &AssignmentExpression<'_>, ctx: &mut FileCtx) {
        if !ctx.agent_active() {
            return;
        }
        let kind = mutation_kind(&assignment.left);
        ctx.report_agent(
            &MUTATION,
            assignment.span,
            format!("{kind} — derive the final value once (const + Match/reduce/pipe) instead of building it up"),
        );
    }

    fn on_import_expression(&self, import: &ImportExpression<'_>, ctx: &mut FileCtx) {
        if !ctx.agent_active() {
            return;
        }
        ctx.report_agent(
            &INLINE_IMPORT,
            keyword_span(import.span, 6),
            "inline dynamic import() — hoist to a top-level `import` unless this is deliberate code-splitting".to_string(),
        );
    }

    fn on_call(&self, call: &CallExpression<'_>, ctx: &mut FileCtx) {
        if !ctx.agent_active() {
            return;
        }
        if ident_name(&call.callee) == Some("require") {
            ctx.report_agent(
                &INLINE_IMPORT,
                call.span,
                "require(...) — use a static top-level ESM `import` instead".to_string(),
            );
            return;
        }
        if is_schema_parse(call) {
            ctx.report_agent(
                &SAFE_PARSE,
                call.span,
                "Schema.parse() throws — prefer .safeParse() / decode-to-Either and handle the failure".to_string(),
            );
            return;
        }
        // `Promise.all(arr.map(...))` / `Promise.allSettled(arr.map(...))` —
        // unbounded fan-out over a list whose size isn't fixed.
        let promise_combinator = static_member(&call.callee).filter(|(object, prop)| {
            ident_name(object) == Some("Promise") && matches!(*prop, "all" | "allSettled")
        });
        if let Some((_, prop)) = promise_combinator {
            let maps = call
                .arguments
                .first()
                .and_then(Argument::as_expression)
                .is_some_and(is_map_call);
            if maps {
                ctx.report_agent(
                    &UNBOUNDED_PROMISE_ALL,
                    call.span,
                    format!("Promise.{prop}(arr.map(...)) — cap with p-limit or Effect.forEach {{ concurrency }}"),
                );
            }
        }
    }

    fn on_function(&self, function: &Function<'_>, ctx: &mut FileCtx) {
        self.check_function(
            function.body.as_deref(),
            function.params.items.len(),
            function.span,
            ctx,
        );
    }

    fn on_arrow(&self, arrow: &ArrowFunctionExpression<'_>, ctx: &mut FileCtx) {
        self.check_function(Some(&arrow.body), arrow.params.items.len(), arrow.span, ctx);
    }

    fn on_file_end(&self, ctx: &mut FileCtx) {
        if !ctx.agent_active() {
            return;
        }
        let mut by_hash: HashMap<u64, Vec<Span>> = HashMap::new();
        for (hash, span) in std::mem::take(&mut ctx.scratch.fn_fingerprints) {
            by_hash.entry(hash).or_default().push(span);
        }
        let mut duplicate_spans: Vec<Span> = by_hash
            .into_values()
            .filter(|spans| spans.len() > 1)
            .flatten()
            .collect();
        duplicate_spans.sort_by_key(|span| span.start);
        for span in duplicate_spans {
            ctx.report(
                &DUPLICATE_FUNCTION,
                keyword_span(span, 1),
                "function body is structurally identical to another in this file — extract a shared helper".to_string(),
            );
        }
    }
}
