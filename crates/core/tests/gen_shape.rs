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
