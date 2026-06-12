// Rules ported from @effect/language-service syntactic diagnostics.
mod common;

use common::{assert_fires, assert_silent};

#[test]
fn nested_gen_yield() {
    assert_fires(
        r#"
import { Effect } from "effect"
const program = Effect.gen(function* () {
  const user = yield* Effect.gen(function* () {
    return yield* Effect.succeed(1)
  })
  return user
})
"#,
        "no-nested-gen-yield",
        1,
    );
}

#[test]
fn effect_fn_iife() {
    assert_fires(
        r#"
import { Effect } from "effect"
const value = Effect.fn(function* () {
  return yield* Effect.succeed(1)
})()
"#,
        "no-effect-fn-iife",
        1,
    );
}

#[test]
fn curried_effect_fn_is_fine() {
    assert_silent(
        r#"
import { Effect } from "effect"
const load = Effect.fn("load")(function* (id: string) {
  return yield* Effect.succeed(id)
})
"#,
        "no-effect-fn-iife",
    );
}

#[test]
fn chained_pipes() {
    let source = r#"
import { Effect, pipe } from "effect"
const a = Effect.succeed(1).pipe(Effect.map((n) => n)).pipe(Effect.map((n) => n + 1))
const b = pipe(pipe(Effect.succeed(1), Effect.map((n) => n)), Effect.map((n) => n + 1))
"#;
    assert_fires(source, "no-unnecessary-pipe-chain", 2);
}

#[test]
fn single_pipe_is_fine() {
    assert_silent(
        r#"
import { Effect } from "effect"
const a = Effect.succeed(1).pipe(Effect.map((n) => n), Effect.map((n) => n + 1))
"#,
        "no-unnecessary-pipe-chain",
    );
}

#[test]
fn return_bare_effect_in_gen() {
    assert_fires(
        r#"
import { Effect } from "effect"
const program = Effect.gen(function* () {
  return Effect.succeed(1)
})
"#,
        "no-return-effect-in-gen",
        1,
    );
}

#[test]
fn return_yield_star_is_fine() {
    assert_silent(
        r#"
import { Effect } from "effect"
const program = Effect.gen(function* () {
  return yield* Effect.succeed(1)
})
"#,
        "no-return-effect-in-gen",
    );
}

#[test]
fn redundant_tag_identifier() {
    let source = r#"
import { Schema } from "effect"
class NotFound extends Schema.TaggedError<NotFound>("NotFound")("NotFound", {}) {}
class Distinct extends Schema.TaggedError<Distinct>("app/Distinct")("Distinct", {}) {}
"#;
    assert_fires(source, "redundant-schema-tag-identifier", 1);
}

#[test]
fn context_tag_self_mismatch() {
    assert_fires(
        r#"
import { Context } from "effect"
export class Db extends Context.Tag("Db")<Other, { query: () => string }>() {}
declare class Other {}
"#,
        "schema-class-self-mismatch",
        1,
    );
}

#[test]
fn context_tag_matching_self_is_fine() {
    assert_silent(
        r#"
import { Context } from "effect"
export class Db extends Context.Tag("Db")<Db, { query: () => string }>() {}
"#,
        "schema-class-self-mismatch",
    );
}

#[test]
fn try_promise_ignoring_abort_signal() {
    let source = r#"
import { Effect } from "effect"
const ignored = Effect.tryPromise({
  try: () => fetch("https://api.example.com/data"),
  catch: (cause) => ({ _tag: "FetchError", cause }),
})
const passed = Effect.tryPromise({
  try: (signal) => fetch("https://api.example.com/data", { signal }),
  catch: (cause) => ({ _tag: "FetchError", cause }),
})
"#;
    assert_fires(source, "prefer-abort-signal-passthrough", 1);
}

#[test]
fn non_signal_aware_promise_is_fine() {
    assert_silent(
        r#"
import { Effect } from "effect"
declare const db: { query: () => Promise<unknown> }
const queried = Effect.tryPromise({
  try: () => db.query(),
  catch: (cause) => ({ _tag: "DbError", cause }),
})
"#,
        "prefer-abort-signal-passthrough",
    );
}

#[test]
fn nested_flatmap_pipes() {
    assert_fires(
        r#"
import { Effect } from "effect"
declare const getUser: (id: string) => ReturnType<typeof Effect.succeed<{ id: string }>>
declare const getAccount: (u: unknown) => ReturnType<typeof Effect.succeed<{ n: number }>>
declare const createInvoice: (u: unknown, a: unknown) => ReturnType<typeof Effect.succeed<string>>
const program = getUser("1").pipe(
  Effect.flatMap((user) =>
    getAccount(user).pipe(Effect.flatMap((account) => createInvoice(user, account)))
  )
)
"#,
        "prefer-gen-over-nested-flatmap",
        1,
    );
}

#[test]
fn single_flatmap_is_fine() {
    assert_silent(
        r#"
import { Effect } from "effect"
const program = Effect.succeed(1).pipe(Effect.flatMap((n) => Effect.succeed(n + 1)))
"#,
        "prefer-gen-over-nested-flatmap",
    );
}
