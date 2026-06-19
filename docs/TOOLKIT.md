# agent-doctor — Toolkit Architecture

The deterministic layer around AI coding agents. `agent-doctor` started as an Effect TS
health scanner; this document scopes its evolution into a **toolkit for agent building** —
the deterministic shell that wraps a nondeterministic orchestrator and its subagents.

TypeScript-first (oxc is JS/TS only). Rust for everything analytical; Zig reserved for one
niche (hermetic test sandbox) if/when we get there.

## Thesis

Agents are nondeterministic. The value we add is a **deterministic shell** that constrains
and verifies them. We already built the hard kernel for one policy domain (Effect idioms).
The toolkit is the same kernel exposed for arbitrary policy, plus three consumers and an
orchestration model.

Everything is a pure function of one content-addressed index. Same input → same verdict,
always, cached. That is what "deterministic" means here — not a vibe, a property: any agent
or CI run reproduces the exact same answer, so a flaky planner can never corrupt state.

## The kernel — an index, not a linter

Today's `crates/core` is batch: parse everything → run rules → emit → exit. The toolkit
kernel is a **persistent, incremental semantic index** (the `lsp.rs` daemon is the seed):

- parsed ASTs + oxc semantic model per file
- structural fingerprints (`structural.rs` / `fn_index.rs`) — rename/format-invariant
- a **symbol graph**: defs, references, calls (oxc semantic gives bindings + references)
- a mutable **ownership map** (leases)

Design commitment: **content-addressing.** Function body → structural hash; file → AST hash.
Everything downstream caches off these. Incremental invalidation (re-parse only the changed
file) turns 40ms-batch into sub-ms-per-query. Every layer below is a *consumer* of this index.

## Layer 1 — Code-path rules / ACL / policy

Policy declared over the diff, three tiers of power:

- **Path ACL**: `src/auth/** → agent-A only`.
- **Structural policy** (the rule engine generalized past Effect): layering rules
  ("no import from `core/` into `ui/`"), "no public-signature change", "new file needs a test".
- **Leases** (dynamic): a coordinator grants region ownership for the life of a task; a write
  outside your lease is denied *before* it lands.

Reuse: ~90% — it's the existing rule engine, Effect packs demoted to one pack among many.
Benefit: agents can't run wild; architecture holds under hundreds of parallel edits;
deterministic allow/deny. Bite: policy authoring + lease state must live somewhere small.

## Layer 2 — Impact-based test selection

Build the call/dependency graph from the symbol graph. Map each test file → symbols it
exercises. On a diff: `git_scope` (changed lines) → `fn_index` (changed functions) → graph
walk (transitive dependents) → covering tests.

Reuse: the hard inputs (`git_scope`, `fn_index`, `structural`) exist; add the graph walk +
test↔symbol map. Benefit: agent loop goes from "3000 tests or none" to "the 30 that matter"
in <1s, deterministically. Bite — **state it loudly**: static graphs under-select on dynamic
dispatch / DI / reflection. Ship an always-run safety set + (later) a coverage feedback loop.

## Layer 3 — Semantic merge driver

3-way merge at AST granularity. Parse base/ours/theirs, diff structurally:

- A adds `foo`, B adds `bar` to same file → auto-merge, zero conflict.
- Reorder / reformat → not a conflict (fingerprint is order/format-invariant).
- Both edit the same declaration body → real conflict, surfaced *with* semantic context.
- Register as a git merge driver (`.gitattributes`) or jj merge tool; line-merge fallback for
  non-TS / parse failures.

Reuse: `structural.rs` is the seed. Benefit: kills the 70%+ spurious conflicts agents
generate; merge stops being a blocking human event (pair with jj conflicts-as-data). Bite:
AST merge can be syntactically valid but semantically wrong → **must chain to L2** (re-run
impacted tests post-merge). Merge + impact-test is a safety *pair*, not two features.

## The orchestration model

The orchestrator is the one component allowed to be nondeterministic, on a deterministic
substrate. Two hats, kept rigidly separate:

- **Planner (LLM, nondeterministic):** decompose the goal, decide next step, write retries.
- **Mechanism (kernel calls, deterministic):** grant leases, gate diffs, select tests, merge.

A hallucinating planner cannot corrupt state — every mutation flows L1 gate → L2 test →
L3 merge.

### Task model: DAG + footprints + leases

Orchestrator holds a task DAG; each task has an **edit footprint** (regions it'll touch).
Disjoint footprints → parallel with disjoint leases; overlapping → serialize. Footprint is
**hybrid**: estimate from the symbol graph (task "auth" → auth module + dependents), grant a
lease, allow mid-task **lease-expansion requests**, catch un-leased grabs at the L1 gate.
Decomposition follows the kernel's module boundaries — natural task seams, maximally parallel
by construction.

### "Within the context" — the kernel as context server

The #1 failure of orchestrated agents is context: too much (dump repo → blow window, lose
focus) or too little (reimplement existing helpers — what `fn_index` catches — hallucinate
APIs). The kernel is a **context server, not a file dump**. Each subagent's window holds only
its task spec + lease footprint + a few *queried* facts:

| Subagent asks | Kernel answers from | Prevents |
|---|---|---|
| does a helper for X exist? | `fn_index` / symbol graph | reinventing helpers |
| signature + types of Y? | semantic model | API hallucination |
| what may I touch / arch rules? | policy + lease | out-of-bounds edits |
| what tests cover this? | impact (L2) | blind/over-testing |

Small, sharp, deterministic, cacheable. The kernel is guardrail *and* context optimizer —
same index.

### Live shared frontier

`fn_index` dedupes across files on disk; in orchestration, run it across **in-flight agents'
draft state** — A and B writing the same helper concurrently → dedup before either merges.
Only the orchestrator, seeing all drafts through one index, can do this.

### Blackboard, not chat log

Shared state is a structured **task ledger** in the kernel (DAG, statuses, leases, produced
symbols, decisions) — not in any LLM window. Subagents read/write structured entries. This is
how you scale to dozens of subtasks without context pollution.

### The loop, with deterministic retry

```
orchestrator: decompose (via symbol graph) → lease disjoint footprints
  └─ per subtask (parallel):
       assemble minimal context pack (kernel queries)
       agent works in isolated workspace
       L1 gate  → fail? feed exact findings back as retry prompt
       L2 tests → fail? feed failing tests + impacted symbols back
       L3 merge → conflict? escalate only the true semantic conflict
       release lease, write results to ledger
```

Retry signal is the kernel's structured findings, not "it broke, try again" — tiny, precise,
deterministic feedback = tight convergence + small retry contexts.

### Harness-agnostic

The orchestrator is not welded to any harness. The kernel is exposed via a `server` API; any
driver (Claude Code Agent/Task tools, a custom loop, CI) plays orchestrator. This keeps us off
the single-closed-stack cliff — the deterministic substrate outlives whatever model/harness is
hot this quarter.

## Monorepo organization

The repo is a Cargo workspace organized into four conceptual **parts**. Today's `crates/core`
is a monolith mixing the shared index with the Effect linter; we split it so the linter becomes
one *product* sitting on the shared *kernel*, alongside the new toolkit layers.

```
crates/
  # ── Part 1: kernel (shared, deterministic index — depends on nothing internal) ──
  kernel/         parse, oxc semantic model, symbol graph, structural fingerprints,
                  fn-indexing, git_scope, diagnostics types, matchers, rule engine

  # ── Part 2: products (consumers of the kernel; each a distinct deliverable) ──
  effect-lint/    THE EFFECT LINTER (today's agent-doctor): Effect rule packs, scoring,
                  --agent hygiene pack, --adopt, --deep. Depends on kernel.

  # ── Part 3: toolkit layers (the agent shell; consumers of the kernel) ──
  policy/         L1  ACL / architecture / lease eval over diffs
  impact/         L2  call graph + test selection
  merge/          L3  AST 3-way merge driver
  server/         persistent incremental index + JSON-RPC/LSP API  (← lsp.rs grows up)
  orchestrator/   reference orchestrator driving the deterministic loop
  sandbox?        (Zig) hermetic test execution — the one place Zig earns its seat

  # ── Part 4: frontends (wire products + layers to users) ──
  cli/            thin dispatcher: doctor/lint · gate · impact · merge · serve · orchestrate
  wasm/           playground (kernel + effect-lint)
```

Rule of thumb for placement: **deterministic & domain-agnostic → `kernel`**; **Effect-specific
or scoring → `effect-lint`**; **agent-shell behavior → a toolkit-layer crate**. The Effect rule
packs are just the first consumer of the rule engine, not privileged.

## Server transport (P4.1 decision)

The context server is a **library `Kernel`** (warm index + dep-graph + policy + leases,
answering typed queries) behind a **line-delimited JSON dispatch** over stdio — one JSON
request per line, one JSON response per line. Rationale: zero network/async deps, trivially
testable (feed a string, assert a string), and the same engine the CLI calls. Index freshness
is **push-based**: callers send an `update_file` request when a file changes (no fs-watch
dependency, deterministic, matches the "push, don't poll" stance). A full MCP handshake and an
optional fs-watcher are later adapters over this same `Kernel`.

## Open questions

1. **Footprint estimation accuracy** — too tight → agents stall on lease requests; too loose →
   lose parallelism. Likely needs a learned-from-history prior eventually.
2. **Ledger + lease state store** — one small service holds both; resist building a backend.
3. **Workspace isolation** — git worktrees vs jj workspaces vs virtual FS; model works on all,
   pick per cost tolerance.
4. **Multi-language later** — oxc is TS-only; merge/impact could go broad via tree-sitter
   (coarser) once the TS path is proven.

## Constraint: don't break the product

`agent-doctor` is a shipped, public linter (scoring, `--agent`, `--deep`, `--adopt`, lsp,
wasm playground). Every toolkit change must keep the existing CLI and its outputs working.
The toolkit is **additive**: new crates + new subcommands, kernel extracted *under* the
current behavior, not in place of it.
