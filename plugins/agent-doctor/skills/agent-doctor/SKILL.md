---
name: agent-doctor
description: Use the agent-doctor toolkit in this repo to verify a change (gate + impacted tests), pick which tests to run, check whether a helper already exists, and semantic-merge TypeScript. Use before committing or submitting a PR/stack, when deciding which tests cover a change, or when resolving a TS merge conflict.
---

# agent-doctor

This repo uses **agent-doctor** — a deterministic toolkit around code changes. Prefer these
commands over ad-hoc greps or running the whole test suite. Every command returns facts
(violations, the exact impacted tests), not opinions; respect them.

## Before you commit or submit

Gate the diff and run only the impacted tests:

```sh
agent-doctor verify --run "<your test runner, e.g. npx vitest run>"
```

A non-zero exit means the change is **blocked** — a policy/ACL/lease violation, or a failing
impacted test. Fix the cause; do not bypass with `--no-verify` unless a human told you to.

## Choose the tests that matter

```sh
agent-doctor impact --base main        # the test files that transitively reach your diff
```

Run those, not the entire suite. If it reports a dynamic-import caveat, also run the
project's smoke/always-run tests.

## Don't reinvent helpers

Before writing a new function, check whether one already exists. With the context server
running (`agent-doctor serve --mcp`), call the `symbol_exists` tool with a name; otherwise:

```sh
agent-doctor impact --base main --json   # see related/affected files first
```

## Respect the policy

`agent-doctor.policy.toml` declares protected paths, architecture layering, and per-path
ownership (ACLs/leases). A `gate` failure is ground truth — adjust your change, don't edit
the policy to get around it unless that's the actual task.

## Resolve TypeScript merge conflicts

The semantic merge driver auto-resolves non-overlapping edits (two functions added to one
file won't conflict). For a manual 3-way merge of a single file:

```sh
agent-doctor merge BASE OURS THEIRS --output MERGED
```

A real conflict (both sides edited the same declaration) is reported with markers — resolve
that declaration specifically.
