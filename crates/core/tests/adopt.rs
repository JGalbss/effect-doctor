// Experimental --adopt mode: vanilla TS → Effect migration recommendations.
mod common;

use common::{assert_fires, assert_fires_adopt, assert_silent, count, lint};

#[test]
fn adopt_rules_are_off_by_default() {
    let source = r#"
import { Effect } from "effect"
export const noop = Effect.void
export async function load(id: string) {
  return await fetch(`/u/${id}`)
}
"#;
    assert_eq!(count(&lint(source), "adopt-async-function"), 0);
    assert_fires_adopt(source, "adopt-async-function", 1);
}

#[test]
fn async_arrow_fires_under_adopt() {
    assert_fires_adopt(
        r#"
import { Effect } from "effect"
export const noop = Effect.void
export const load = async (id: string) => fetch(`/u/${id}`)
"#,
        "adopt-async-function",
        1,
    );
}

#[test]
fn promise_chain_reports_once_per_chain() {
    assert_fires_adopt(
        r#"
import { Effect } from "effect"
export const noop = Effect.void
declare const fetchUser: (id: string) => Promise<{ name: string }>
export const result = fetchUser("1").then((u) => u.name).then((n) => n.length)
"#,
        "adopt-promise-chain",
        1,
    );
}

#[test]
fn new_promise_fires_outside_effect() {
    let source = r#"
import { Effect } from "effect"
export const noop = Effect.void
export const wait = new Promise((resolve) => setTimeout(resolve, 100))
"#;
    assert_fires_adopt(source, "adopt-new-promise", 1);
}

#[test]
fn promise_all_outside_effect() {
    let source = r#"
import { Effect } from "effect"
export const noop = Effect.void
declare const ids: ReadonlyArray<string>
declare const fetchUser: (id: string) => Promise<unknown>
export const users = Promise.all(ids.map(fetchUser))
"#;
    assert_fires_adopt(source, "adopt-promise-all", 1);
}

#[test]
fn promise_all_inside_effect_wrapper_is_covered_elsewhere() {
    // Inside Effect.tryPromise, no-promise-all-in-effect owns the report —
    // the adopt rule stays quiet to avoid double-reporting.
    let source = r#"
import { Effect } from "effect"
declare const ids: ReadonlyArray<string>
declare const fetchUser: (id: string) => Promise<unknown>
export const users = Effect.tryPromise(() => Promise.all(ids.map(fetchUser)))
"#;
    assert_fires_adopt(source, "adopt-promise-all", 0);
    assert_fires_adopt(source, "no-promise-all-in-effect", 1);
}

#[test]
fn await_in_loop() {
    assert_fires_adopt(
        r#"
import { Effect } from "effect"
export const noop = Effect.void
declare const ids: ReadonlyArray<string>
declare const processUser: (id: string) => Promise<void>
export async function run() {
  for (const id of ids) {
    await processUser(id)
  }
}
"#,
        "adopt-await-in-loop",
        1,
    );
}

#[test]
fn loop_without_await_is_fine() {
    let source = r#"
import { Effect } from "effect"
export const noop = Effect.void
export function total(values: ReadonlyArray<number>) {
  let sum = 0
  for (const value of values) {
    sum += value
  }
  return sum
}
"#;
    assert_fires_adopt(source, "adopt-await-in-loop", 0);
}

#[test]
fn yield_loop_in_gen_fires_without_adopt() {
    assert_fires(
        r#"
import { Effect } from "effect"
declare const ids: ReadonlyArray<string>
declare const processUser: (id: string) => ReturnType<typeof Effect.succeed<void>>
export const program = Effect.gen(function* () {
  for (const id of ids) {
    yield* processUser(id)
  }
})
"#,
        "prefer-foreach-over-yield-loop",
        1,
    );
}

#[test]
fn loop_in_gen_without_yield_is_fine() {
    assert_silent(
        r#"
import { Effect } from "effect"
export const program = Effect.gen(function* () {
  const values = yield* Effect.succeed([1, 2, 3])
  let sum = 0
  for (const value of values) {
    sum += value
  }
  return sum
})
"#,
        "prefer-foreach-over-yield-loop",
    );
}
