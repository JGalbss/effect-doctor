# effect-doctor — Rule Catalog (spec)

Mined from Effect-TS/skills, effect-smol MIGRATION.md, @effect/language-service diagnostics
(~75, the de facto prior art), and the v4 beta announcement. The official
@effect/eslint-plugin has only 2 rules — this catalog fills a real gap.

Detectability: `AST` = syntax + import-provenance only (Tier 1, Rust). `type` = needs the
type checker (Tier 2 via @effect/tsgo). `project` = package.json/lockfile/cross-file.

Severity: error / warn / info. Categories map to report sections.

## Correctness

| id | sev | det | summary |
|---|---|---|---|
| `require-yield-star` | error | AST | `yield` without `*` inside `Effect.gen`/`Effect.fn` generator |
| `no-try-catch-in-gen` | error | AST | `try/catch` inside Effect generator; errors flow through the typed channel (`catchTag`) |
| `no-throw-in-effect` | error | AST | `throw` inside Effect gen/callbacks becomes an untyped defect; use `Effect.fail(new Tagged...)` |
| `no-run-inside-effect` | error | AST | `Effect.runPromise/runSync/runFork` inside another Effect — detached runtime, loses context/interruption |
| `no-async-function-in-effect-code` | warn | AST | `async/await` in files importing effect; wrap boundaries with `Effect.tryPromise` / `Effect.fn` |
| `no-floating-effect` | error | type | Effect as expression statement, never yielded/run/assigned (LSP: `floatingEffect`) |
| `no-promise-in-effect-sync` | error | type | `Effect.sync` callback returns a Promise |
| `no-unsafe-effect-assertion` | error | type | `as` casts narrowing Effect/Stream/Layer types (esp. casting away error channel) |
| `no-any-in-effect-channels` | error | type | `any`/`unknown` in error or requirements channel |
| `no-raw-error-in-failure-channel` | error | AST+type | `Effect.fail(new Error(...))`, `catch: (e) => e as Error`, failing with strings — use tagged errors |
| `no-catch-on-unfailable-effect` | warn | type | error combinators on `E = never` |
| `no-orDie-to-silence-errors` | info | AST | `orDie` used to dodge recoverable errors (flag, suppressible) |
| `missing-return-yield-star` | info | type | terminal `yield* Effect.fail(...)` should be `return yield*` |
| `schema-class-self-mismatch` | error | AST | `Schema.Class<Self>` / `Context.Service<Self>` Self ≠ declaring class |
| `no-constructor-override-in-schema-class` | error | AST | custom `constructor` in Schema.Class breaks decoding |
| `schema-suspend-for-recursion` | error | AST | recursive schema self-reference without `Schema.suspend` |
| `layer-mergeall-with-dependencies` | error | type | `Layer.mergeAll(A, B)` where B requires what A provides — mergeAll does not wire deps |
| `no-it-prop-with-schema` | error | type | top-level `it.prop` with Schema arbitrary throws; only `it.effect.prop` |
| `duplicate-effect-packages` | error | project | multiple `effect` versions / misaligned `@effect/*` versions (v4 requires exact alignment) |

## Idiomatic

| id | sev | det | summary |
|---|---|---|---|
| `prefer-effect-fn` | warn | AST | `(args) => Effect.gen(...)` → `Effect.fn("name")(function*(args){...})` (free spans) |
| `no-fnUntraced-by-default` | info | AST | `Effect.fnUntraced` without measured hot-path reason |
| `no-effect-fn-iife` | warn | AST | `Effect.fn(...)()` immediately invoked → `Effect.gen` |
| `no-unnecessary-gen` | info | AST | gen body is single `return (yield* op)` → use op directly (LSP: `unnecessaryEffectGen`) |
| `no-nested-gen-yield` | info | AST | `yield* Effect.gen(...)` inside a generator — inline it |
| `prefer-tagged-error-classes` | warn | AST | `class X extends Error` → `Data.TaggedError` / `Schema.TaggedErrorClass` |
| `prefer-catch-tag` | warn | AST | catch handler branching on `e._tag` → `catchTag`/`catchTags` |
| `catch-to-map-error` | info | AST | catch handler that always `Effect.fail`s → `mapError` |
| `no-unnecessary-fail-of-yieldable` | info | AST | `yield* Effect.fail(new Tagged(...))` → `return yield* new Tagged(...)` |
| `prefer-effect-void` | info | AST | `Effect.succeed(undefined)` → `Effect.void` |
| `prefer-as-void` | info | AST | `Effect.map(() => undefined)` → `asVoid`; `map(() => c)` → `as(c)` |
| `prefer-flatmap-over-map-flatten` | info | AST | `map` + `flatten` → `flatMap` |
| `no-effect-do-notation` | info | AST | `Effect.Do`/`bind` pipelines → `Effect.gen` |
| `no-unnecessary-pipe` | info | AST | empty `.pipe()`, nested `pipe(pipe(...))` |
| `prefer-clock-service` | warn | AST | `Date.now()`/`new Date()` in Effect code → `Clock`/`DateTime` (TestClock-able) |
| `prefer-random-service` | warn | AST | `Math.random()`/`crypto.randomUUID()` → `Random` |
| `prefer-effect-logging` | warn | AST | `console.*` in Effect code → `Effect.log*` |
| `prefer-platform-fetch` | warn | AST | global `fetch` → `HttpClient` |
| `prefer-effect-timers` | warn | AST | `setTimeout`/`setInterval` → `Effect.sleep`/`Schedule` |
| `prefer-config-module` | warn | AST | `process.env.X` → `Config` |
| `prefer-node-effect-counterparts` | info | AST | `node:fs`/`node:path` imports where `FileSystem`/`Path` services exist |
| `prefer-schema-is` | info | type | `instanceof SchemaClass` → `Schema.is` |
| `prefer-schema-over-json` | info | AST | raw `JSON.parse/stringify` at boundaries → `Schema.fromJsonString` |
| `prefer-decode-effect` | warn | AST | `decodeUnknownSync` inside Effect code → `decodeUnknownEffect` |
| `prefer-schema-class-for-named-models` | info | AST | exported reused `Schema.Struct` → `Schema.Class` |
| `prefer-tagged-struct` | info | AST | `Struct({_tag: Literal("X")})` → `TaggedStruct` (LSP: `schemaStructWithTag`) |
| `schema-union-of-literals` | info | AST | `Union([Literal,...])` → `Literals([...])` |
| `prefer-optional-key` | info | AST | `Schema.optional` vs `optionalKey` intent |
| `prefer-brand-for-ids` | info | AST | bare `Schema.String` for `*Id` fields → `Schema.brand` |
| `prefer-make-over-new` | info | type | `new SchemaClass({...})` → `.make({...})` |
| `meaningful-span-names` | info | AST | `Effect.fn("helper"/"run"/"process")` — span names should be business ops |
| `no-duplicate-schemas` | info | project | structurally-identical schemas differing in one field's encoding → one schema + mapFields |

### Testing

| id | sev | det | summary |
|---|---|---|---|
| `prefer-it-effect` | warn | AST | `it(..., () => Effect.runPromise(...))` / async bodies → `it.effect` |
| `no-provide-in-test-bodies` | warn | AST | repeated `Effect.provide(L)` in test bodies → `layer(L)(...)` / `it.layer` |
| `no-it-live-by-default` | info | AST | `it.live` without need for real Clock/Console |
| `prefer-assert-in-effect-tests` | info | AST | `expect` inside `it.effect` → `assert` from @effect/vitest |

## Architecture

| id | sev | det | summary |
|---|---|---|---|
| `no-local-provide` | warn | AST | `Effect.provide(Layer)` inside reusable business fns — provide once at entrypoint |
| `no-chained-provides` | warn | AST | multiple `Effect.provide` in one pipe — compose layers first (LSP: `multipleEffectProvide`) |
| `prefer-managed-runtime` | info | project | many `runPromise(x.pipe(provide(AppLayer)))` sites → one `ManagedRuntime` |
| `no-layer-factory-recall` | warn | AST | layer-returning fn called >1× — breaks memoization-by-reference, resource built twice |
| `layer-succeed-pure-only` | warn | AST | effectful construction inside `Layer.succeed` → `Layer.effect` (+ acquireRelease) |
| `prefer-acquire-release` | warn | AST | manual open/close in `finally`/`tap` → `Effect.acquireRelease` |
| `no-thin-service-accessors` | info | AST | exported Effect.fn that only forwards one service method |
| `no-leaking-impl-requirements` | warn | type | service method `R` includes impl-internal services (LSP: `leakingRequirements`) |
| `deterministic-service-keys` | warn | AST+project | tag/error id strings should match class name, app-namespaced, project-unique |
| `compose-layers-locally` | info | AST | deeply nested inline Layer.provide/mergeAll → named subsystem layers |
| `prefer-effect-sql` | info | project | raw `pg`/`mysql2`/`better-sqlite3` imports in domain code when @effect/sql-* fits |
| `no-manual-sql-transactions` | warn | AST | `` sql`BEGIN` `` / COMMIT / ROLLBACK → `sql.withTransaction` |
| `no-as-cast-on-rows` | warn | AST | `rows[0] as Row` → Schema decode / SqlSchema |
| `prefer-structured-retry` | warn | AST | hand-rolled retry loops (recursion + sleep) → `Effect.retry({...})` |
| `retry-only-retryable` | info | AST | bare `Effect.retry(schedule)` without while/until/tag filter |
| `validate-at-boundaries` | warn | AST+type | external input consumed without `Schema.decodeUnknown*`; `as` casts at boundaries |

## Performance

| id | sev | det | summary |
|---|---|---|---|
| `add-jitter-to-backoff` | info | AST | `Schedule.exponential` without `.jittered` (thundering herd) |
| `cap-exponential-backoff` | info | AST | `Schedule.exponential` without cap/union |
| `no-unbounded-concurrency` | info | AST | `{ concurrency: "unbounded" }` over potentially large collections |
| `hoist-schema-codecs` | info | AST | `Schema.decodeUnknownEffect(S)` built per-call inside fn bodies → hoist to module scope |
| `prefer-module-imports` | info | AST | v3 barrel imports defeating tree-shaking (low priority in v4) |

## v4-migration

All AST-matchable by name + import provenance; profile-gated (only fire for v4 targets or
`--migrate` audits). Source of truth: effect-smol `MIGRATION.md` + LSP `outdatedApi`.

| id | summary |
|---|---|
| `v4-context-service` | `Context.Tag`/`GenericTag`/`Effect.Tag`/`Effect.Service` → `Context.Service` |
| `v4-no-service-accessors` | static accessor proxies removed → `yield*` or `Service.use` |
| `v4-effect-service-dependencies-removed` | `dependencies: []` + `.Default` gone → explicit `static layer` |
| `v4-layer-naming-convention` | `.Default`/`.Live` → `.layer` (`layerTest`, `layerConfig`) |
| `v4-catch-renames` | `catchAll`→`catch`, `catchAllCause`→`catchCause`, `catchSome`→`catchFilter`, … |
| `v4-fork-renames` | `fork`→`forkChild`, `forkDaemon`→`forkDetach`; `forkAll` removed |
| `v4-yieldable-not-effect` | `yield* ref/deferred/fiber` → `Ref.get`/`Deferred.await`/`Fiber.join`; Option/Result need `.asEffect()` (type) |
| `v4-fiberref-removed` | `FiberRef*`/`Differ` → `Context.Reference`/`References.*` |
| `v4-cause-flattened` | flat `reasons` array; `isFailType`→`isFailReason`, `failureOption`→`findErrorOption`, `*Exception`→`*Error`, `Cause.sequential/parallel`→`combine` |
| `v4-runtime-removed` | `Runtime<R>`/`Effect.runtime` → `Effect.context` + `runForkWith` |
| `v4-scope-provide` | `Scope.extend` → `Scope.provide` |
| `v4-layer-scoped-to-effect` | `Layer.scoped` → `Layer.effect` (inverts the v3 LSP suggestion — version-aware!) |
| `v4-gen-self-options` | `Effect.gen(this, fn)` → `Effect.gen({ self: this }, fn)` |
| `v4-no-gen-adapter` | `Effect.gen(function*(_){ yield* _(op) })` adapter → yield directly |
| `v4-package-consolidation` | `@effect/platform`/`rpc`/`cluster` imports → `effect` / `effect/unstable/*` |
| `v4-option-renames` | `Option.fromNullable` → `fromNullishOr`, etc. |
| `v4-equal-structural-default` | `Equal.equivalence`→`asEquivalence`; structural-by-default semantics note |
| `v4-schema-renames` | large family (autofixable): `annotations`→`annotate`, `TaggedError`→`TaggedErrorClass`, `decodeUnknown`→`decodeUnknownEffect`, `*FromSelf` drops, variadic→array (`Union(A,B)`→`Union([A,B])`, `Literal("a","b")`→`Literals([...])`), `Record({key,value})`→`Record(key,value)`, `pick/omit`→`mapFields`, `filter`→`check`/`refine`, removed: `validate*`, `keyof`, `Schema.Data`, … |
| `v4-context-reference-shape` | `Context.Reference<Self>()(id, {defaultValue})` class form → value form |
| `v4-unstable-import-awareness` | report `effect/unstable/*` imports (info) |

## Scoring surfaces

Following react-doctor: every diagnostic carries surfaces (`cli`, `prComment`, `score`,
`ciFailure`). Style/info rules can be CLI-only so they never tank the score. Score counts
distinct rules fired, not occurrences.
