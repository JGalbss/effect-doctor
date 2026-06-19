# agent-doctor Toolkit — Build Plan (loop-driven)

Execution checklist for turning `agent-doctor` into the deterministic agent toolkit.
Design lives in [TOOLKIT.md](./TOOLKIT.md) — read the referenced section before each task.

## Status (live)

- **Phase 0** — `P0.0` crate split *deferred* (extract the kernel once consumers prove the
  boundary; rename is mechanical later). `P0.2`–`P0.5` ✅: content-addressing, symbol graph,
  warm `Index`, incremental update.
- **Phase 1** — ✅ `crates/policy`: glob, TOML schema, ACL/layering/protected/lease gate +
  `agent-doctor gate` (deny exits non-zero).
- **Phase 2** — ✅ `crates/impact`: reverse-dep selection, `DepGraph`, dynamic-import caveat +
  `agent-doctor impact`.
- **Phase 3** — ✅ `crates/merge`: decl-level 3-way semantic merge (reorder/format-invariant,
  changed-symbols, safe fallback) + `agent-doctor merge` git driver, **proven e2e**.
- **Phase 4** — ✅ `crates/server`: warm `Kernel` (symbol_exists/impact/gate/context_pack) +
  line-delimited JSON dispatch + `agent-doctor serve`.
- **Phase 5** — ✅ `crates/orchestrator`: ledger/DAG, footprint estimation, live frontier dedup,
  and the deterministic run loop (lease → context → execute → gate → retry-with-findings).
- **Phase 7** — ✅ `crates/bench` + `bench/run.sh` + `bench/RESULTS.md`.
- **Phase 6** — ✅ `crates/vcs`: `Vcs` trait + `GitVcs`, content-addressed op-log (agent/task
  attribution, deterministic undo/revert-task), `Repo` facade. `P6.5` fully-native storage
  engine is the explicitly-deferred long tail (≈ jj's 112k test LOC), drop-in behind the trait.
- **MCP** — ✅ `serve --mcp` exposes the kernel's ground-truth tools to agent harnesses.
- **Totals:** 10 crates, **247 workspace tests passing**, clean build; merge driver / serve /
  MCP / gate proven e2e; perf wins (impact 1300→0.2µs, gate 1193→4µs) caught by the harness.
- **Done:** Phases 0–7 (P0.0 crate-split and P6.5 native-storage are the two documented,
  intentional deferrals).

## How to run this with `/loop`

Trigger with:

```
/loop work the next unchecked task in docs/PLAN.md
```

### Per-iteration protocol (do exactly this, once)

1. Read this file. Pick the **first unchecked `[ ]` task** in the earliest incomplete phase.
   Respect phase order — don't start phase N+1 while phase N has open tasks, unless the task is
   tagged `(parallel-ok)`.
2. Read the TOOLKIT.md section the phase references for design intent.
3. Implement **only that task** (plus trivially-coupled lines). One task = one iteration.
4. Verify (Definition of Done is per-task; the baseline gate every time):
   - `cargo build --workspace` is clean.
   - `cargo test --workspace` passes.
   - **Regression**: existing product still works — `cargo run -p agent-doctor -- fixtures`
     produces a score and findings (no panic, no changed exit semantics).
5. Check the box `[x]` and append a one-line note: what landed + any deviation from plan.
6. Commit on branch `toolkit` (create it on first run; `git checkout -b toolkit`).
   Message: `toolkit(<phase>.<n>): <summary>`. **Do not** add a Co-Authored-By trailer
   (repo convention). Do not push unless asked.
7. Stop. The next loop iteration takes the next task.

If a task is blocked or wrong, **don't force it**: leave it unchecked, add a `> BLOCKED: …`
note under it explaining why, and move to the next unblocked task in the same phase.

### Conventions

- **TS-first**: the toolkit analyzes TypeScript/JavaScript only for now.
- Rust idioms (match, Result, typed errors) — mirror the existing `crates/core` style.
- Additive only: new crates + subcommands. Never regress the shipped linter.
- Keep tasks small enough that one fits comfortably in a single context window.

---

## Phase 0 — Monorepo reorg + kernel foundations (design → TOOLKIT.md §Monorepo organization, §kernel)

Goal: split the `core` monolith into a shared `kernel` + the `effect-lint` product, then
establish "kernel = content-addressed index". Behavior of the shipped linter must not change.

- [ ] **P0.0a** Carve `crates/core` into two crates without behavior change:
  - `crates/kernel` — domain-agnostic, deterministic: `walk`, `diagnostics`, `matchers`,
    `structural`, `fn_index` (the *indexing*), `git_scope`, `engine` (rule dispatch).
  - `crates/effect-lint` — Effect-specific: `rules/`, `lint`, `score`, `runner`, `examples`,
    `effect_imports`, `deep`, `adopt`, and the `--agent`/`--adopt` packs. Depends on `kernel`.
  Update workspace members + `cli`/`wasm` dependencies. DoD: `cargo build --workspace` clean;
  `agent-doctor fixtures` produces the **same score and findings** as before the split.
  > Note: if `fn_index`'s diagnostic emission resists separation from its indexing, keep the
  > rule emission in `effect-lint` and the function index in `kernel`; record the seam taken.
- [ ] **P0.0b** Rename internal crate identifiers consistently (`agent_doctor_core` →
  `agent_doctor_kernel` + `agent_doctor_effect_lint`) and fix imports. DoD: workspace builds;
  no dangling `core` references; published crate/package names unchanged externally.
- [x] **P0.1** Add `docs/TOOLKIT.md` cross-link to `README.md` and `docs/ARCHITECTURE.md`
  (one line each pointing at the toolkit direction). DoD: links render, no other change.
- [ ] **P0.2** Introduce content-addressed identity types in `crates/kernel`: `FileId`,
  `ContentHash` (stable hash of source), and an `AstHash`/reuse of `structural::Shape` hash for
  function bodies. Pure types + hashing, unit-tested. DoD: tests assert same input → same hash,
  whitespace-only change → same `structural` hash, different source → different `ContentHash`.
- [ ] **P0.3** Build a **symbol graph** module `crates/kernel/src/symbol_graph.rs`: per file,
  extract definitions (named/exported fns, classes, consts) and references, using
  `oxc_semantic` bindings + references. Output: `SymbolId → {def site, ref sites}` and
  `file → imported symbols`. DoD: unit test over a 2-file fixture shows cross-file import edge.
- [ ] **P0.4** Expose an in-memory `Index` struct that holds parsed files + symbol graph +
  fn_index, built once from a directory (wrap existing walk/parse). DoD: `Index::build(dir)`
  returns populated index; test asserts symbol + duplicate counts on `fixtures`.
- [ ] **P0.5** Add incremental update hook `Index::update_file(path, new_source)` that re-parses
  one file and patches the symbol graph/fn_index for it. DoD: test mutates one file, asserts the
  graph reflects the change and untouched files' entries are unchanged (identity preserved).

## Phase 1 — L1 Policy / ACL engine (design → TOOLKIT.md §Layer 1)

Goal: deterministic gate over a diff. New crate `crates/policy`.

- [ ] **P1.1** Scaffold `crates/policy` crate (workspace member, depends on `agent_doctor_kernel`).
  DoD: empty crate builds in the workspace.
- [ ] **P1.2** Define the policy schema (TOML) + loader: path-ACL rules, layering rules
  (`forbid import from <glob> into <glob>`), and a list of protected globs. Parse with serde,
  typed errors. DoD: round-trip test parses a sample `agent-doctor.policy.toml`.
- [ ] **P1.3** Diff → footprint: reuse `git_scope::DiffInfo` + `fn_index` span mapping to turn
  changed lines into changed **files and symbols**. DoD: test over a staged change in a fixture
  repo returns the expected changed-symbol set. `(parallel-ok)` with P1.2.
- [ ] **P1.4** Path-ACL evaluator: given a footprint + actor id + policy, return allow/deny +
  reasons. DoD: deny when actor writes outside its allowed globs; allow otherwise.
- [ ] **P1.5** Layering/architecture evaluator: detect new imports that cross a forbidden
  boundary (reuse `effect_imports` walker patterns). DoD: fixture with a `core→ui` import is
  flagged; legal imports pass.
- [ ] **P1.6** CLI subcommand `agent-doctor gate --base <ref> [--actor <id>] [--policy <path>]`:
  evaluate working diff, print findings, exit non-zero on deny. DoD: exit codes + JSON output
  (`--json`) verified on a fixture.
- [ ] **P1.7** Lease model: `Lease { actor, globs, task_id }` + a file-backed store
  (`.agent-doctor/leases.json`) with acquire / release / list. DoD: acquire then conflicting
  acquire is rejected; release frees it; concurrent-safe via file lock.
- [ ] **P1.8** Wire leases into `gate`: a write outside the actor's active lease is a deny.
  DoD: test shows leased-region write passes, un-leased write denied.

## Phase 2 — L2 Impact-based test selection (design → TOOLKIT.md §Layer 2)

Goal: diff → minimal test set. New crate `crates/impact`.

- [ ] **P2.1** Scaffold `crates/impact` crate (depends on kernel). DoD: builds in workspace.
- [ ] **P2.2** Call/dependency graph: from the symbol graph, build symbol→symbol reference
  edges (who references whom). DoD: test asserts transitive dependents of a symbol in a fixture.
- [ ] **P2.3** Test↔symbol map: identify test files (config: globs like `**/*.test.ts`) and
  record the symbols each imports/references. DoD: test maps a `*.test.ts` to the symbols under
  test in a fixture.
- [ ] **P2.4** Impact query: diff → changed symbols → transitive dependents → covering tests.
  Include a configurable **always-run** safety set. DoD: changing a leaf util selects exactly
  the tests that reach it + always-run set.
- [ ] **P2.5** Under-selection caveat: detect dynamic-dispatch / dynamic-import / DI signals in
  changed files and emit a `coverage-risk` warning so selection is never silently incomplete.
  DoD: fixture with `import()` / index-signature dispatch raises the warning.
- [ ] **P2.6** CLI subcommand `agent-doctor impact --base <ref> [--json]` → list of test files
  (+ caveats). DoD: deterministic output on a fixture; stable ordering.

## Phase 3 — L3 Semantic merge driver (design → TOOLKIT.md §Layer 3)

Goal: AST 3-way merge. New crate `crates/merge`.

- [ ] **P3.1** Scaffold `crates/merge` crate (depends on kernel). DoD: builds in workspace.
- [ ] **P3.2** Top-level declaration diff: parse two sources, diff at the
  declaration-statement granularity (added / removed / modified decls), keyed by name + shape.
  DoD: tests for add-only, remove-only, modify cases.
- [ ] **P3.3** 3-way merge: base/ours/theirs over top-level decls. Non-overlapping edits
  auto-merge; both-modified-same-decl → structured conflict with semantic labels. DoD: tests:
  (a) A adds fn / B adds fn → clean merge; (b) both edit same fn body → conflict reported.
- [ ] **P3.4** Reorder/format invariance: decl reordering or pure formatting between sides is
  not a conflict. DoD: test reorders decls on one side → clean merge.
- [ ] **P3.5** Line-merge fallback: non-TS files, parse failure, or sub-decl overlap fall back
  to a standard 3-way line merge (shell `git merge-file` or a vendored 3-way). DoD: a non-TS
  fixture merges via fallback; a TS parse-failure path falls back without panic.
- [ ] **P3.6** git merge-driver integration: `agent-doctor merge %O %A %B %P` interface +
  documented `.gitattributes` / `git config` setup. DoD: a real `git merge` using the driver
  on a fixture repo auto-resolves an additive conflict.
- [ ] **P3.7** Compose with L2: `merge` exposes the changed-symbol set of a resolved merge so a
  caller can run impacted tests post-merge. DoD: merge result carries the symbol delta; unit
  test asserts it.

## Phase 4 — Server / context server (design → TOOLKIT.md §kernel, §context server)

Goal: persistent incremental index behind an API. Grow `crates/cli/src/lsp.rs` or new `server`.

- [ ] **P4.1** Decide transport (extend LSP custom requests vs standalone JSON-RPC) and record
  the decision in TOOLKIT.md. DoD: one paragraph added; no code.
- [ ] **P4.2** Long-lived index process: build `Index` once, watch files, call
  `Index::update_file` on change (debounced). DoD: process stays warm; edits reflected without
  full rebuild (log shows single-file reparse).
- [ ] **P4.3** Query endpoints: `symbol-exists` (fn_index), `signature`, `policy-for-path`,
  `impact-for-diff`. DoD: each returns deterministic JSON; integration test hits all four.
- [ ] **P4.4** Context-pack endpoint: given a task spec (intent + target globs), return the
  minimal pack (relevant symbols, existing-helper hits, applicable policy, covering tests).
  DoD: test asserts the pack for a sample task contains the expected helper-reuse hit and omits
  unrelated files.

## Phase 5 — Orchestrator (design → TOOLKIT.md §orchestration model)

Goal: reference orchestrator driving the deterministic loop. New crate `crates/orchestrator`.

- [ ] **P5.1** Scaffold `crates/orchestrator` + task model: `Task { id, intent, deps,
  footprint, status }` and a file-backed **ledger** (`.agent-doctor/ledger.json`). DoD: create /
  read / update tasks; DAG cycle detection.
- [ ] **P5.2** Footprint estimation: from a task intent + target globs, estimate touched
  symbols/files via the symbol graph. DoD: "auth" task estimate includes auth module +
  dependents in a fixture.
- [ ] **P5.3** Lease coordinator: grant disjoint leases for ready tasks, support
  expansion-requests, release on completion. DoD: two disjoint tasks both get leases; two
  overlapping tasks serialize.
- [ ] **P5.4** Subtask dispatch contract: define the in/out protocol (spec + context pack in →
  diff + summary out) as typed structs; provide a stub executor for tests (no LLM). DoD: stub
  round-trips a task through dispatch.
- [ ] **P5.5** Integration loop: for each task — assemble context (P4.4) → run executor →
  L1 gate → L2 tests → L3 merge → on failure feed structured findings back as retry input
  (bounded retries). DoD: end-to-end test on a fixture: a stub task that violates policy is
  denied and retried with the findings attached.
- [ ] **P5.6** Live frontier dedup: run `fn_index` across in-flight task drafts; flag two tasks
  writing the same helper before merge. DoD: test with two stub tasks adding identical helpers
  raises the cross-draft duplicate.

## Phase 6 — Agent-native VCS (design → TOOLKIT.md §agent-native VCS)

Goal: our own model on a git-compatible backend — the novel agent-native layer, not a git
rewrite. Built behind a trait so a fully-native storage engine can swap in later.

- [x] **P6.1** `Vcs` trait + `GitVcs` adapter: changed-files, file-at-ref, isolated worktrees,
  semantic `merge_file` (default method over the trait). DoD: tested vs a temp git repo.
- [x] **P6.2** Content-addressed **operation log** (`OpLog`): agent/task attribution, parent
  chain, monotonic seq. DoD: deterministic ids, attribution queries.
- [x] **P6.3** Deterministic **undo / revert-task** on the op-log (jj-style, agent-scoped).
  DoD: undo moves head back + audits; revert-task restores pre-task state.
- [x] **P6.4** `Repo` facade: composes git storage + op-log + first-class leases + semantic
  merge; persists under `.agent-doctor/`. DoD: snapshot/lease/undo persist and reload.
- [ ] **P6.5** *(deferred long tail)* Fully-native storage engine (own object store / wire
  protocol / virtual working copy) replacing the git backend — ≈ jj's 112k test-LOC of
  correctness work. Tracked, intentionally not built; the trait makes it a drop-in later.

## Phase 7 — Benchmark / latency harness (design → bench/RESULTS.md)

Goal: measure kernel latency across many real projects so regressions and scaling are visible.

- [x] **P7.1** `crates/bench` binary: take project dirs, measure cold build / warm incremental
  update / warm impact select, report p50/p95. DoD: runs on a dir, prints a table.
- [x] **P7.2** `bench/run.sh`: clone a tiered set of real TS repos (zustand→zod→trpc→effect)
  into `bench/projects/` (gitignored) and run the harness. DoD: idempotent re-runs.
- [x] **P7.3** Capture sample numbers in `bench/RESULTS.md`. DoD: table with real measurements.
- [x] **P7.4** Add gate (L1) and merge (L3) latency rows once those layers land.
- [x] **P7.5** CI job: fail if cold-build µs/file regresses beyond a threshold on a pinned repo.

## Milestones

- **M1 (Phase 0–1):** deterministic gate usable in CI — `agent-doctor gate` blocks bad agent diffs.
- **M2 (Phase 2):** `agent-doctor impact` cuts agent test loops to the relevant set.
- **M3 (Phase 3):** semantic merge driver kills spurious agent conflicts.
- **M4 (Phase 4–5):** context server + reference orchestrator run the full deterministic loop.

## Definition of done (whole plan)

All boxes checked; `cargo build/test --workspace` clean; the shipped `agent-doctor` linter
unchanged in behavior; `gate`, `impact`, `merge` subcommands documented in README; TOOLKIT.md
reflects any design deviations discovered during the build.
