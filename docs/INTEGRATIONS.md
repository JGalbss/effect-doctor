# Integrations — Claude Code plugin, Graphite, git hooks, CI

## Claude Code plugin (recommended)

The repo is also a Claude Code **plugin marketplace**. Installing the plugin wires up
everything at once — the skill, the MCP server, slash commands, and a commit-gate hook:

```
/plugin marketplace add JGalbss/agent-doctor
/plugin install agent-doctor@agent-doctor
```

What you get (source: `plugins/agent-doctor/`):
- **Skill** — teaches the agent to use `verify`/`impact`/`gate`/`merge` in your repo.
- **MCP server** — `agent-doctor serve --mcp` via `npx` (zero local install), exposing the
  kernel's ground-truth tools (`symbol_exists`, `impact`, `gate`, `context_pack`).
- **Slash commands** — `/agent-doctor:verify`, `/agent-doctor:impact`, `/agent-doctor:gate`.
- **Commit-gate hook** — a `PreToolUse(Bash)` hook that runs `agent-doctor verify` whenever
  the agent runs `git commit` / `git push` / `gt submit`, and **blocks** it on a real
  failure (policy/lease violation or a failing impacted test). It no-ops if the toolkit
  isn't installed and never blocks for setup reasons — only genuine, deterministic failures.

For the CLI it shells out to (`verify`/`gate`/`impact`), install the binary too:
`npm i -g @jgalbsss/agent-doctor` (the MCP server already runs via `npx`).

Validate the plugin locally with `claude plugin validate ./plugins/agent-doctor`.

---

# Integrations — Graphite, git hooks, CI

agent-doctor has no proprietary plugin to install into Graphite, and Graphite has no
plugin SDK to target. It doesn't need one: Graphite runs **standard git hooks**, and
`gt submit` pushes — so a **pre-push hook fires automatically on submit**. The single
command that ties it together is:

```sh
agent-doctor verify   # gate the diff, then select (and optionally run) the impacted tests
```

`verify` exits non-zero if the diff violates policy/ACL/leases, or (with `--run`) if the
impacted tests fail. It's fast because it runs *only* the tests reaching your change.

## Graphite (`gt`) — verify on every submit

Install the hook once (also done by `agent-doctor init --hooks`):

```sh
agent-doctor init --hooks
```

This writes `.git/hooks/pre-push`:

```sh
#!/bin/sh
exec agent-doctor verify
```

Now `gt submit` (which pushes) runs `verify` first. If the gate fails or impacted tests
fail, the submit is blocked — locally, in seconds, before CI ever runs.

Run the impacted tests too (not just list them):

```sh
# in .git/hooks/pre-push
exec agent-doctor verify --run "npx vitest run"
```

The selected test files are appended to that command, so only the relevant tests run.

Notes:
- `gt submit --no-verify` bypasses hooks (Graphite honors the standard flag) — for the
  rare escape hatch.
- Graphite respects per-repo hooks; nothing Graphite-specific is required.

## CI — the same check as a GitHub Action

Graphite surfaces GitHub checks in its UI, so run `verify` server-side too. This replaces
"submit and wait for the full suite" with an impact-scoped check:

```yaml
# .github/workflows/verify.yml
name: verify
on: pull_request
jobs:
  verify:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with: { fetch-depth: 0 }          # need history for the diff base
      - uses: actions/setup-node@v4
      - run: npm ci
      - run: npm i -g @jgalbsss/agent-doctor
      - run: agent-doctor verify --base "origin/${{ github.base_ref }}" --run "npx vitest run"
```

## Wrapper alias (optional)

If you'd rather verify explicitly than via a hook:

```sh
# ~/.zshrc  — verify, then submit the stack
gship() { agent-doctor verify --run "npx vitest run" && gt submit "$@"; }
```

## What verify is (and isn't)

It's a deterministic **fact check** — policy/lease violations and the exact impacted
tests — not a style opinion. It composes the same `gate` and `impact` the agents use, so
humans and agents pass through the identical bar.
