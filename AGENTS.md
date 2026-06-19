# Agent coding conventions (enforced)

These are the TypeScript/Effect conventions agents must follow in this repo. A
`CLAUDE.md`/`AGENTS.md` alone is necessary but **not sufficient** for large,
ambitious changes — guidance gets ignored under load. So every convention below
that can be checked with confidence is also an **agent-doctor rule** (the
`--agent` tier), and CI can gate on it with `--agent-strict`.

Run it on your changes:

```sh
agent-doctor --agent --scope lines      # just the lines you touched
agent-doctor --agent-strict             # hard-fail on violations (CI gate)
```

The `--agent` tier only inspects files that import `effect` (this is an Effect
codebase). Each rule below is annotated with its id and the source it was mined
from: **[oc]** = opencode `AGENTS.md`, **[rogo]** = the Rogo TS conventions,
**[std]** = the personal coding standards.

## Control flow

- **No `if/else` (or `else if`) chains.** Prefer early returns, a lookup map, or
  `Match.value(...).pipe(Match.when(...), Match.exhaustive)`. — `agent-no-if-else-chain` [oc][std]
- **No ternaries.** Extract a named helper or use `Match.when`/`orElse`.
  `agent-no-ternary` [std] — note this *overrides* opencode's "use ternaries
  instead of reassignment"; named branches win here.
- **No string-equality guards** (`x === "user"`). Use a type guard / predicate
  (`isUser(x)`) or `Match`. — `agent-no-string-equality-guard` [std]

## Variables & data flow

- **No `let`/`var`.** Bind with `const`; build values functionally. — `agent-no-let` [oc][rogo][std]
- **No reassignment or in-place payload mutation.** Derive the final value once
  rather than building up intermediate states. — `agent-no-mutation` [oc][std]
- **No `delete`.** Build a new object without the key (rest / `Struct.omit`). —
  `agent-no-delete`

## Iteration & concurrency

- **No raw `for`/`while` loops.** Use `Array.map/filter/reduce`, or
  `Effect.forEach`/`Effect.reduce` for effectful iteration. — `agent-no-raw-loop` [oc][rogo]
- **No unbounded `Promise.all(array.map(...))`.** Cap with `p-limit`, or
  `Effect.forEach` with an explicit `concurrency`. A fixed tuple is fine. —
  `agent-no-unbounded-promise-all` [rogo]

## Types

- **No `any`.** Use a precise type, `unknown` + narrowing, or a Schema decode. —
  `agent-no-any` [oc][rogo]
- **No `as` casts** (except `as const`). Narrow with a guard or decode with
  Schema at the boundary. — `agent-no-as-cast` [rogo][std]
- **No non-null assertions (`x!`).** Narrow with a guard or model absence with
  Option. — `agent-no-non-null-assertion` [std]
- **No `@ts-ignore` / `@ts-expect-error` / `@ts-nocheck`.** `strict: true` is on
  everywhere; fix the type. — `agent-no-ts-ignore` [rogo][std]
- **No TS `enum`.** Use a union of string literals / `Schema.Literals` and derive
  the type. — `agent-no-enum`
- **No `==` / `!=`** (except the idiomatic `== null`). Use `===` / `!==` or
  `Equal.equals`. — `agent-no-loose-equality`
- **Prefer `.safeParse()` over `Schema.parse()`** — handle the failure path
  explicitly. — `agent-prefer-safe-parse` [rogo]

## Imports & exports

- **Named exports only**, no `export default`. — `agent-no-default-export` [rogo][std]
- **Never alias imports** (`import { X as Y }`). — `agent-no-import-alias` [oc]
- **Never star-import** (`import * as X`) — except Effect's idiomatic
  `import * as Effect from "effect"`, which is exempt. — `agent-no-namespace-import` [oc]
- **No inline `import()` / `require()`** mid-body; hoist to a static top-level
  `import` (dynamic import only for deliberate code-splitting). —
  `agent-no-inline-import` [rogo]
- **No inline `import("...").Foo` type refs.** Use a top-level `import type`. —
  `agent-no-inline-type-import` [rogo]

## Modules

- **No TS `namespace`.** Use ES modules (one file = one module) + named
  exports. — `agent-no-ts-namespace`

## Error handling

- **Avoid `try/catch`.** Model failure in the typed channel (`Effect.try` +
  `catchTag`) or return a `Result`. — `agent-no-try-catch` [oc][rogo]
- **No `throw` outside Effect.** Return a Result/Either or `Effect.fail` a
  tagged error. — `agent-no-throw`

## Functions & duplication

- **Don't extract single-use helpers.** An exported helper imported by exactly
  one module is usually premature extraction — inline or co-locate it unless it
  hides a genuinely complex boundary. — `agent-no-single-use-helper` [oc]
- **Don't copy-paste logic.** Structurally identical / near-identical / same-name
  / same-shape functions are flagged so you reuse instead of re-implementing. —
  `agent-duplicate-function`, `agent-duplicate-cross-file`,
  `agent-near-duplicate-function`, `agent-similar-function-name`,
  `agent-similar-shape`

## Control flow & architecture

- **Don't nest control flow > 4 deep.** Use guard clauses / early returns or
  extract the inner block. — `agent-deep-nesting`
- **Keep cyclomatic complexity ≤ 15.** Split big branchy functions; replace
  branching with Match / a lookup. — `agent-high-complexity`
- **≤ 5 positional parameters.** Pass a named options object. — `agent-too-many-params`
- **No `../../../` imports.** Use a path alias or move shared code closer. —
  `agent-deep-relative-import`
- **No import cycles.** Break them with a leaf module or by inverting a
  dependency. — `agent-circular-import`

## Effect specifics

- Pass an explicit `concurrency` to `Effect.forEach` / `Effect.all` (or
  `"inherit"`); never leave the default `"unbounded"` on a list whose size you
  don't control. — `no-unbounded-concurrency`
- Decode untrusted JSON with Schema helpers (e.g. `Schema.fromJsonString`)
  rather than `JSON.parse`. — `prefer-schema-over-json`

## Not enforced (guidance only)

These were mined but are too intent-dependent to enforce as AST rules without
noise — follow them, but they aren't checked: explicit return types on exports
(fights Effect inference), "avoid unnecessary destructuring", "happy-path
structure", barrel-import avoidance, snake_case DB columns. See `docs/RULES.md`
for the full catalog.
