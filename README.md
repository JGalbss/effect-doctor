# agent-doctor

Health checks for Effect TS codebases — [react.doctor](https://www.react.doctor/), but for
[Effect](https://effect.website/). Scans a repo, scores it 0–100, and reports Effect
anti-patterns with file locations. Written in Rust on the [oxc](https://oxc.rs) toolchain:
~40ms for 1,100 files, ~200ms for 1,800.

```
  agent doctor  v0.1.0

  ███████████████████████████░░░  91/100 — Great

  ✖ require-yield-star  error · Correctness · 2 issues
    Inside Effect.gen, effects must be yielded with `yield*`. ...
    src/program.ts:4:17  const value = yield Effect.succeed(1)
```

## Usage

```sh
cargo build --release
agent-doctor <dir>                      # scan everything
agent-doctor <dir> --verbose --json     # full report / machine-readable
agent-doctor --scope changed            # only files changed vs main (PR mode)
agent-doctor --scope lines --base main  # only issues on lines you touched
agent-doctor rules                      # list all 116 rules
agent-doctor explain no-map-returning-effect   # why + how to rewrite it
agent-doctor rules --json               # full catalog with rewrite recipes
agent-doctor --deep                     # merge type-aware @effect/language-service findings
agent-doctor --no-react                 # skip the React tier (on by default, see below)
agent-doctor lsp                        # run as a language server (editor diagnostics)
agent-doctor --adopt --scope lines      # experimental: vanilla-TS → Effect migration
                                         # recommendations, on exactly your PR's lines
agent-doctor --agent                    # experimental "agent doctor": flag the non-Effect
                                         # slop LLM agents emit (if/else, ternaries, raw loops…)
agent-doctor --agent-strict             # same, but escalate to errors and exit non-zero (CI gate)
```

## React tier — all of react-doctor, automatically

When agent-doctor detects a React project (a `react` dependency in package.json), it runs
[react-doctor](https://www.react-doctor.com/)'s full rule set automatically and merges its
findings into the report as `rd/*` rules in a **React** category — no flag or config needed.
This mirrors the `--deep` tier: agent-doctor orchestrates react-doctor, it doesn't reimplement
it. Install react-doctor so the tier can run (`npm i -D react-doctor`); a missing react-doctor
is a silent no-op. Opt out per-run with `--no-react`.

## Claude Code plugin

This repo doubles as a Claude Code marketplace. Installing the plugin ships the
**agent-doctor skill** so coding agents run the linter on their own TypeScript before
committing. Full setup: [docs/INTEGRATIONS.md](docs/INTEGRATIONS.md).

## Docs site

`site/` is an Astro site rendering the full rule catalog with side-by-side bad→good
rewrites, search, and category filters. `npm run gen` regenerates its data from
`agent-doctor rules --json`; `npm run dev` to work on it locally.

## Status

Early but real: **116 rules live** across correctness, idiomatic, architecture,
performance, and v4-migration categories — every rule ships with a bad→good rewrite
recipe (`explain`), and 120+ integration tests cover the catalog (bad patterns fire,
clean code stays silent; example coverage is test-enforced). Rule sources: the Effect-TS
skills repo, the @effect/language-service diagnostic catalog, the effect-smol v4
MIGRATION guide, and the EffectPatterns community corpus (304 patterns). The full spec
is in [docs/RULES.md](docs/RULES.md); architecture and roadmap in
[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

- Import-aware matching: `import { Effect as E } from "effect"` and
  `import * as Effect from "effect/Effect"` both resolve correctly.
- Version-aware profiles: effect major detected from package.json; v4-migration rules
  fire on v4 codebases automatically, or on v3 with `--migrate`.
- Test-file classification: findings in `*.test.ts` / `test/` paths stay in the report
  but don't count toward the score (except test-specific rules).
- Score model: penalty per distinct rule fired (errors 1.5, warnings 0.75), info rules
  never affect the score.
- Diff scoping: `--scope changed` (files) / `--scope lines` vs `--base` (defaults to the
  merge-base with main) — untracked files count as fully changed.
- `--deep` tier: merges the ~78 type-aware diagnostics from
  `@effect/language-service` (its headless `diagnostics --format json` CLI) as `ls/*`
  rules — we never reimplement type analysis.
- `agent-doctor lsp`: stdio language server publishing the syntactic rule set as
  editor diagnostics (full-sync; rule id as the diagnostic code).
- `--adopt` (experimental): flags vanilla TS that should migrate to Effect — async
  functions, `.then()` chains, `new Promise`, `Promise.all`, sequential awaits in loops —
  each with the clean Effect rewrite. `prefer-foreach-over-yield-loop` (yield loops
  inside Effect.gen → `Effect.forEach`) is always on as info.
- `--agent` (experimental, "agent doctor"): flags the non-Effect, non-functional patterns
  LLM agents reach for by default — `if/else` chains, ternaries, `x === "literal"` guards,
  raw `for`/`while` loops, `let`/`var` mutation, inline `import()`/`require()`, reassignment / in-place payload mutation
  (intermediate states), and copy-pasted function bodies — each with the clean
  Effect/`Match`/combinator rewrite. Defaults to `warn`; `--agent-strict` escalates to `error`
  and exits non-zero so it can gate CI. It also runs a **cross-file pass**: a repo-wide index
  of named/bound functions flags ones that duplicate another by body (exact / fuzzy), name, or
  shape (params + call set) — so an agent reusing context sees "this helper already exists"
  instead of re-creating it. All duplicate/similarity findings stay info suggestions.
- OOP → Effect (under `--agent`): flags hand-rolled Gang-of-Four patterns that Effect replaces
  with a first-class primitive — Singleton → `Context.Tag`/`Layer`, Observer → `PubSub`/`Stream`,
  Strategy (single-method interface, ≥2 impls) → a function type, Visitor → `Match.exhaustive`,
  Chain of Responsibility → `Effect.orElse`/`catchTag` — each with the idiomatic rewrite.
- Type safety (always-on): flags the escape hatches agents use to silence the compiler —
  `any`, non-null `!`, double-casts (`as unknown as`), empty `catch {}`, `@ts-ignore` —
  plus per-function maintainability metrics (too many parameters, deep nesting, high cognitive
  complexity). All `warn`; `--agent-strict` escalates them to a hard CI gate.
- Planned: suppression comments, config file, editor extension packaging, agent
  handoff, npm distribution as per-platform binaries.

## Development

Requires rustc ≥ 1.94 (`rust-toolchain.toml` pins stable via rustup; if a Homebrew rust
shadows it, `brew unlink rust` or pass `RUSTC=$HOME/.rustup/toolchains/<host>/bin/rustc`).

Reference repos for rule development are expected (gitignored) under `references/`:
`effect` (v3), `effect-v4` (effect-smol), `skills`, `language-service`, `react-doctor`.
