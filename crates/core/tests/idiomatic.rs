mod common;

use common::{assert_fires, assert_silent};

#[test]
fn date_now_in_gen() {
    assert_fires(
        r#"
import { Effect } from "effect"
const program = Effect.gen(function* () {
  return Date.now()
})
"#,
        "prefer-clock-service",
        1,
    );
}

#[test]
fn new_date_in_callback() {
    assert_fires(
        r#"
import { Effect } from "effect"
const program = Effect.sync(() => new Date())
"#,
        "prefer-clock-service",
        1,
    );
}

#[test]
fn new_date_with_args_is_parsing_not_clock() {
    assert_silent(
        r#"
import { Effect } from "effect"
const program = Effect.sync(() => new Date(1718000000000))
"#,
        "prefer-clock-service",
    );
}

#[test]
fn date_now_at_module_level_is_fine() {
    assert_silent(
        r#"
import { Effect } from "effect"
const startedAt = Date.now()
export const noop = Effect.void
"#,
        "prefer-clock-service",
    );
}

#[test]
fn math_random_in_effect_code() {
    assert_fires(
        r#"
import { Effect } from "effect"
const program = Effect.gen(function* () {
  return Math.random()
})
"#,
        "prefer-random-service",
        1,
    );
}

#[test]
fn console_log_in_effect_code() {
    assert_fires(
        r#"
import { Effect } from "effect"
const program = Effect.gen(function* () {
  console.log("hello")
})
"#,
        "prefer-effect-logging",
        1,
    );
}

#[test]
fn timers_and_fetch_and_env_and_json() {
    let source = r#"
import { Effect } from "effect"
const program = Effect.gen(function* () {
  setTimeout(() => {}, 100)
  const data = yield* Effect.tryPromise(() => fetch("https://x"))
  const parsed = JSON.parse("{}")
  return process.env.NODE_ENV
})
"#;
    assert_fires(source, "prefer-effect-timers", 1);
    assert_fires(source, "prefer-platform-fetch", 1);
    assert_fires(source, "prefer-schema-over-json", 1);
    assert_fires(source, "prefer-config-module", 1);
}

#[test]
fn succeed_undefined() {
    assert_fires(
        r#"
import { Effect } from "effect"
export const done = Effect.succeed(undefined)
"#,
        "prefer-effect-void",
        1,
    );
}

#[test]
fn succeed_with_value_is_fine() {
    assert_silent(
        r#"
import { Effect } from "effect"
export const one = Effect.succeed(1)
"#,
        "prefer-effect-void",
    );
}

#[test]
fn map_to_undefined_and_constant() {
    let source = r#"
import { Effect } from "effect"
export const a = Effect.map(Effect.succeed(1), () => undefined)
export const b = Effect.map(Effect.succeed(1), () => "done")
"#;
    assert_fires(source, "prefer-as-void", 2);
}

#[test]
fn map_using_value_is_fine() {
    assert_silent(
        r#"
import { Effect } from "effect"
export const doubled = Effect.map(Effect.succeed(1), (value) => value * 2)
"#,
        "prefer-as-void",
    );
}

#[test]
fn map_then_flatten_nested_and_piped() {
    let source = r#"
import { Effect } from "effect"
const nested = Effect.flatten(Effect.map(Effect.succeed(1), (n) => Effect.succeed(n)))
const piped = Effect.succeed(1).pipe(
  Effect.map((n) => Effect.succeed(n)),
  Effect.flatten
)
"#;
    assert_fires(source, "prefer-flatmap-over-map-flatten", 2);
}

#[test]
fn unnecessary_gen() {
    assert_fires(
        r#"
import { Effect } from "effect"
export const passthrough = Effect.gen(function* () {
  return yield* Effect.succeed(1)
})
"#,
        "no-unnecessary-gen",
        1,
    );
}

#[test]
fn gen_with_real_body_is_fine() {
    assert_silent(
        r#"
import { Effect } from "effect"
export const program = Effect.gen(function* () {
  const a = yield* Effect.succeed(1)
  return a + 1
})
"#,
        "no-unnecessary-gen",
    );
}

#[test]
fn fail_of_new_tagged_error_in_yield() {
    assert_fires(
        r#"
import { Effect, Data } from "effect"
class NotFound extends Data.TaggedError("NotFound")<{}> {}
const program = Effect.gen(function* () {
  return yield* Effect.fail(new NotFound({}))
})
"#,
        "no-unnecessary-fail-of-yieldable",
        1,
    );
}

#[test]
fn catch_all_dispatching_on_tag() {
    assert_fires(
        r#"
import { Effect } from "effect"
const handled = Effect.succeed(1).pipe(
  Effect.catchAll((error) => (error._tag === "NotFound" ? Effect.succeed(0) : Effect.fail(error)))
)
"#,
        "prefer-catch-tag",
        1,
    );
}

#[test]
fn catch_all_with_uniform_handler_is_fine() {
    assert_silent(
        r#"
import { Effect } from "effect"
const handled = Effect.succeed(1).pipe(Effect.catchAll(() => Effect.succeed(0)))
"#,
        "prefer-catch-tag",
    );
}

#[test]
fn catch_all_that_always_refails() {
    assert_fires(
        r#"
import { Effect } from "effect"
const wrapped = Effect.succeed(1).pipe(
  Effect.catchAll((error) => Effect.fail({ wrapped: error }))
)
"#,
        "catch-to-map-error",
        1,
    );
}

#[test]
fn effect_do_notation() {
    assert_fires(
        r#"
import { Effect } from "effect"
const program = Effect.Do.pipe(Effect.bind("a", () => Effect.succeed(1)))
"#,
        "no-effect-do-notation",
        1,
    );
}

#[test]
fn function_wrapping_single_gen() {
    assert_fires(
        r#"
import { Effect } from "effect"
export const load = (id: string) =>
  Effect.gen(function* () {
    return yield* Effect.succeed(id)
  })
"#,
        "prefer-effect-fn",
        1,
    );
}

#[test]
fn thunk_wrapping_gen_is_fine() {
    assert_silent(
        r#"
import { Effect } from "effect"
export const lazy = Effect.suspend(() =>
  Effect.gen(function* () {
    return yield* Effect.succeed(1)
  })
)
"#,
        "prefer-effect-fn",
    );
}

#[test]
fn class_extending_error() {
    assert_fires(
        r#"
import { Effect } from "effect"
export const noop = Effect.void
export class HttpError extends Error {}
"#,
        "prefer-tagged-error-classes",
        1,
    );
}

#[test]
fn tagged_error_class_is_fine() {
    assert_silent(
        r#"
import { Data, Effect } from "effect"
export const noop = Effect.void
export class HttpError extends Data.TaggedError("HttpError")<{ status: number }> {}
"#,
        "prefer-tagged-error-classes",
    );
}

#[test]
fn generic_span_name() {
    assert_fires(
        r#"
import { Effect } from "effect"
export const run = Effect.fn("run")(function* () {
  return yield* Effect.succeed(1)
})
"#,
        "meaningful-span-names",
        1,
    );
}

#[test]
fn descriptive_span_name_is_fine() {
    assert_silent(
        r#"
import { Effect } from "effect"
export const create = Effect.fn("UserRepo.create")(function* () {
  return yield* Effect.succeed(1)
})
"#,
        "meaningful-span-names",
    );
}

#[test]
fn sync_decode_in_effect_code() {
    assert_fires(
        r#"
import { Effect, Schema } from "effect"
const User = Schema.Struct({ name: Schema.String })
const program = Effect.gen(function* () {
  return Schema.decodeUnknownSync(User)({ name: "x" })
})
"#,
        "prefer-decode-effect",
        1,
    );
}

#[test]
fn codec_built_per_call() {
    assert_fires(
        r#"
import { Schema, Effect } from "effect"
const User = Schema.Struct({ name: Schema.String })
export const decode = (input: unknown) => Schema.decodeUnknownEffect(User)(input)
export const noop = Effect.void
"#,
        "hoist-schema-codecs",
        1,
    );
}

#[test]
fn hoisted_codec_is_fine() {
    assert_silent(
        r#"
import { Schema, Effect } from "effect"
const User = Schema.Struct({ name: Schema.String })
const decodeUser = Schema.decodeUnknownEffect(User)
export const decode = (input: unknown) => decodeUser(input)
export const noop = Effect.void
"#,
        "hoist-schema-codecs",
    );
}

#[test]
fn it_running_effect_run_promise() {
    assert_fires(
        r#"
import { Effect } from "effect"
import { it } from "vitest"
it("works", () => Effect.runPromise(Effect.succeed(1)))
"#,
        "prefer-it-effect",
        1,
    );
}

#[test]
fn it_effect_is_fine() {
    assert_silent(
        r#"
import { Effect } from "effect"
import { it } from "@effect/vitest"
it.effect("works", () => Effect.succeed(1))
"#,
        "prefer-it-effect",
    );
}

#[test]
fn node_fs_import_in_effect_file() {
    assert_fires(
        r#"
import { Effect } from "effect"
import { readFileSync } from "node:fs"
export const noop = Effect.void
"#,
        "prefer-node-effect-counterparts",
        1,
    );
}
