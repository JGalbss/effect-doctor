mod common;

use common::{assert_fires, assert_silent};

#[test]
fn yield_without_star_in_gen() {
    assert_fires(
        r#"
import { Effect } from "effect"
const program = Effect.gen(function* () {
  const value = yield Effect.succeed(1)
  return value
})
"#,
        "require-yield-star",
        1,
    );
}

#[test]
fn yield_without_star_in_aliased_gen() {
    assert_fires(
        r#"
import { Effect as E } from "effect"
const program = E.gen(function* () {
  const value = yield E.succeed(1)
  return value
})
"#,
        "require-yield-star",
        1,
    );
}

#[test]
fn yield_without_star_in_module_path_import() {
    assert_fires(
        r#"
import * as Effect from "effect/Effect"
const program = Effect.gen(function* () {
  const value = yield Effect.succeed(1)
  return value
})
"#,
        "require-yield-star",
        1,
    );
}

#[test]
fn yield_star_is_fine() {
    assert_silent(
        r#"
import { Effect } from "effect"
const program = Effect.gen(function* () {
  const value = yield* Effect.succeed(1)
  return value
})
"#,
        "require-yield-star",
    );
}

#[test]
fn plain_generator_yield_is_not_flagged() {
    assert_silent(
        r#"
import { Effect } from "effect"
export const noop = Effect.void
export function* numbers() {
  yield 1
  yield 2
}
"#,
        "require-yield-star",
    );
}

#[test]
fn nested_plain_generator_inside_gen_is_not_flagged() {
    assert_silent(
        r#"
import { Effect } from "effect"
const program = Effect.gen(function* () {
  function* inner() {
    yield 1
  }
  return yield* Effect.succeed(inner)
})
"#,
        "require-yield-star",
    );
}

#[test]
fn effect_fn_generator_counts_as_gen() {
    assert_fires(
        r#"
import { Effect } from "effect"
const load = Effect.fn("load")(function* (id: string) {
  const value = yield Effect.succeed(id)
  return value
})
"#,
        "require-yield-star",
        1,
    );
}

#[test]
fn try_catch_in_gen() {
    assert_fires(
        r#"
import { Effect } from "effect"
const program = Effect.gen(function* () {
  try {
    yield* Effect.succeed(1)
  } catch {
    return 0
  }
})
"#,
        "no-try-catch-in-gen",
        1,
    );
}

#[test]
fn try_catch_outside_gen_is_fine() {
    assert_silent(
        r#"
import { Effect } from "effect"
export const noop = Effect.void
export function parse(input: string) {
  try {
    return JSON.parse(input)
  } catch {
    return undefined
  }
}
"#,
        "no-try-catch-in-gen",
    );
}

#[test]
fn throw_in_gen() {
    assert_fires(
        r#"
import { Effect } from "effect"
const program = Effect.gen(function* () {
  throw new Error("nope")
})
"#,
        "no-throw-in-effect",
        1,
    );
}

#[test]
fn throw_in_nested_function_is_not_flagged() {
    assert_silent(
        r#"
import { Effect } from "effect"
const program = Effect.gen(function* () {
  const validate = (value: number) => {
    if (value < 0) throw new Error("nope")
    return value
  }
  return yield* Effect.succeed(validate(1))
})
"#,
        "no-throw-in-effect",
    );
}

#[test]
fn run_promise_inside_gen() {
    assert_fires(
        r#"
import { Effect } from "effect"
const program = Effect.gen(function* () {
  Effect.runPromise(Effect.succeed(1))
})
"#,
        "no-run-inside-effect",
        1,
    );
}

#[test]
fn run_sync_inside_effect_callback() {
    assert_fires(
        r#"
import { Effect } from "effect"
const program = Effect.sync(() => Effect.runSync(Effect.succeed(1)))
"#,
        "no-run-inside-effect",
        1,
    );
}

#[test]
fn run_promise_at_entrypoint_is_fine() {
    assert_silent(
        r#"
import { Effect } from "effect"
const program = Effect.succeed(1)
Effect.runPromise(program)
"#,
        "no-run-inside-effect",
    );
}

#[test]
fn schema_class_self_mismatch() {
    assert_fires(
        r#"
import { Schema } from "effect"
class Account extends Schema.Class<Account>("Account")({ id: Schema.String }) {}
class Wrong extends Schema.Class<Account>("Wrong")({ id: Schema.String }) {}
"#,
        "schema-class-self-mismatch",
        1,
    );
}

#[test]
fn schema_class_constructor_override() {
    assert_fires(
        r#"
import { Schema } from "effect"
class Account extends Schema.Class<Account>("Account")({ id: Schema.String }) {
  constructor() {
    super({ id: "fixed" })
  }
}
"#,
        "no-constructor-override-in-schema-class",
        1,
    );
}

#[test]
fn ordinary_class_constructor_is_fine() {
    assert_silent(
        r#"
import { Effect } from "effect"
export const noop = Effect.void
class Plain {
  constructor(readonly id: string) {}
}
"#,
        "no-constructor-override-in-schema-class",
    );
}

#[test]
fn or_die_is_flagged_for_review() {
    assert_fires(
        r#"
import { Effect } from "effect"
const config = Effect.succeed(1).pipe(Effect.orDie)
"#,
        "no-orDie-to-silence-errors",
        1,
    );
}
