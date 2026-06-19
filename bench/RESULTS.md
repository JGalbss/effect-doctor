# Kernel latency — sample results

Run with `bench/run.sh` (clones the projects, builds release, runs the harness).
Numbers below: Apple Silicon, `--release`, p50/p95 over repeated runs.

| project | files | build p50 | build p95 | incr p50 | incr p95 | impact p50 |
|---------|------:|----------:|----------:|---------:|---------:|-----------:|
| zustand |    33 |   2.53ms  |   2.56ms  |  51.4µs  | 274.0µs  |   0.2µs    |
| zod     |   404 |   8.09ms  |   8.27ms  |  28.2µs  |  56.4µs  |   0.2µs    |

Reading the columns:

- **build** — cold index build: walk + parallel parse of every TS file. Scales
  ~linearly with file count (~20µs/file here).
- **incr** — warm single-file re-parse (`Index::update_file`); tens of µs,
  independent of repo size — this is what keeps a long-lived index cheap.
- **impact** — warm impact selection with the `DepGraph` prebuilt (what a server
  reuses). Sub-µs: a graph walk, not a re-resolution. The one-shot `select()`
  that rebuilds the `DepGraph` is ~`build`-of-DepGraph + this.

The impact column is the headline result: once the dependency graph is warm,
selecting the impacted tests for a change is effectively free.
