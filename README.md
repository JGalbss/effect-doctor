# effect-doctor

Health checks for Effect TS codebases — [react.doctor](https://www.react.doctor/), but for
[Effect](https://effect.website/). Scans a repo, scores it 0–100, and reports Effect
anti-patterns with file locations. Written in Rust on the [oxc](https://oxc.rs) toolchain:
~40ms for 1,100 files, ~200ms for 1,800.

```
  effect doctor  v0.1.0

  ███████████████████████████░░░  91/100 — Great

  ✖ require-yield-star  error · Correctness · 2 issues
    Inside Effect.gen, effects must be yielded with `yield*`. ...
    src/program.ts:4:17  const value = yield Effect.succeed(1)
```

## Usage

```sh
cargo build --release
effect-doctor <dir>                      # scan everything
effect-doctor <dir> --verbose --json     # full report / machine-readable
effect-doctor --scope changed            # only files changed vs main (PR mode)
effect-doctor --scope lines --base main  # only issues on lines you touched
effect-doctor rules                      # list all 100 rules
effect-doctor explain no-map-returning-effect   # why + how to rewrite it
effect-doctor rules --json               # full catalog with rewrite recipes
effect-doctor --deep                     # merge type-aware @effect/language-service findings
effect-doctor lsp                        # run as a language server (editor diagnostics)
effect-doctor --adopt --scope lines      # experimental: vanilla-TS → Effect migration
                                         # recommendations, on exactly your PR's lines
effect-doctor --agent                    # experimental "agent doctor": flag the non-Effect
                                         # slop LLM agents emit (if/else, ternaries, raw loops…)
effect-doctor --agent-strict             # same, but escalate to errors and exit non-zero (CI gate)
```

## Docs site

`site/` is an Astro site rendering the full rule catalog with side-by-side bad→good
rewrites, search, and category filters. `npm run gen` regenerates its data from
`effect-doctor rules --json`; `npm run dev` to work on it locally.

## Status

Early but real: **100 rules live** across correctness, idiomatic, architecture,
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
- `effect-doctor lsp`: stdio language server publishing the syntactic rule set as
  editor diagnostics (full-sync; rule id as the diagnostic code).
- `--adopt` (experimental): flags vanilla TS that should migrate to Effect — async
  functions, `.then()` chains, `new Promise`, `Promise.all`, sequential awaits in loops —
  each with the clean Effect rewrite. `prefer-foreach-over-yield-loop` (yield loops
  inside Effect.gen → `Effect.forEach`) is always on as info.
- `--agent` (experimental, "agent doctor"): flags the non-Effect, non-functional patterns
  LLM agents reach for by default — `if/else` chains, ternaries, `x === "literal"` guards,
  raw `for`/`while` loops, `let`/`var` mutation, reassignment / in-place payload mutation
  (intermediate states), and copy-pasted function bodies — each with the clean
  Effect/`Match`/combinator rewrite. Defaults to `warn`; `--agent-strict` escalates to `error`
  and exits non-zero so it can gate CI. It also runs a **cross-file pass**: a repo-wide index
  of named/bound functions flags ones that duplicate another by body (exact / fuzzy), name, or
  shape (params + call set) — so an agent reusing context sees "this helper already exists"
  instead of re-creating it. All duplicate/similarity findings stay info suggestions.
- Planned: suppression comments, config file, editor extension packaging, agent
  handoff, npm distribution as per-platform binaries.

## Development

Requires rustc ≥ 1.94 (`rust-toolchain.toml` pins stable via rustup; if a Homebrew rust
shadows it, `brew unlink rust` or pass `RUSTC=$HOME/.rustup/toolchains/<host>/bin/rustc`).

Reference repos for rule development are expected (gitignored) under `references/`:
`effect` (v3), `effect-v4` (effect-smol), `skills`, `language-service`, `react-doctor`.
