# Kernel latency — sample results

Run with `bench/run.sh` (clones the projects, builds release, runs the harness).
Numbers below: Apple Silicon, `--release`, p50 over repeated runs.

| project | files | build p50 | incr p50 | impact p50 | gate p50 | merge p50 |
|---------|------:|----------:|---------:|-----------:|---------:|----------:|
| zustand |    33 |   2.46ms  |  45.9µs  |   0.2µs    |  53.2µs  |  20.7µs   |
| zod     |   404 |   7.82ms  |  24.9µs  |   0.2µs    |   4.0µs  |  18.8µs   |

Columns (all five toolkit layers):

- **build** — cold index build: walk + parallel parse of every TS file. ~20µs/file.
- **incr** — warm single-file re-parse (`Index::update_file`); tens of µs, size-independent.
- **impact** (L2) — warm impact selection with the `DepGraph` prebuilt; sub-µs (a graph walk).
- **gate** (L1) — policy evaluation incl. a layering rule. Resolves imports only for the
  *changed* files, so it's flat in repo size (it judges the diff, not the repo).
- **merge** (L3) — semantic 3-way merge of one file (parse ×3 + declaration merge).

Two perf findings the harness drove out, both fixed by precomputing/scoping work:

- impact selection: **1300µs → 0.2µs** by building the reverse-dependency `DepGraph` once
  instead of per call.
- gate: **1193µs → 4µs** on zod by resolving only the changed files' imports in the layering
  check instead of the whole repo's edges.

CI runs `agent-doctor-bench --max-us-per-file 5000 fixtures` as a regression gate (P7.5).
