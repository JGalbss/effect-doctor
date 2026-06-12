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
./target/release/effect-doctor <dir>        # scan
./target/release/effect-doctor <dir> --verbose
./target/release/effect-doctor <dir> --json
```

## Status

Early. 9 rules live (generator hygiene, runtime misuse, Clock/Random/logging services,
v4 adapter migration). The full ~90-rule spec is in [docs/RULES.md](docs/RULES.md);
architecture and roadmap in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

- Import-aware matching: `import { Effect as E } from "effect"` and
  `import * as Effect from "effect/Effect"` both resolve correctly.
- Score model: penalty per distinct rule fired (errors 1.5, warnings 0.75), info rules
  never affect the score.
- Planned: `--scope changed` CI deltas, test-file downgrades, suppression comments,
  `--deep` type-aware tier via `@effect/tsgo`, agent handoff, npm distribution as
  per-platform binaries.

## Development

Requires rustc ≥ 1.94 (`rust-toolchain.toml` pins stable via rustup; if a Homebrew rust
shadows it, `brew unlink rust` or pass `RUSTC=$HOME/.rustup/toolchains/<host>/bin/rustc`).

Reference repos for rule development are expected (gitignored) under `references/`:
`effect` (v3), `effect-v4` (effect-smol), `skills`, `language-service`, `react-doctor`.
