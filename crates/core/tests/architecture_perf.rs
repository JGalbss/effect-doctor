mod common;

use common::{assert_fires, assert_silent};

#[test]
fn chained_provides_in_one_pipe() {
    assert_fires(
        r#"
import { Effect, Layer } from "effect"
declare const A: Layer.Layer<never>
declare const B: Layer.Layer<never>
const program = Effect.succeed(1).pipe(Effect.provide(A), Effect.provide(B))
"#,
        "no-chained-provides",
        1,
    );
}

#[test]
fn single_provide_is_fine() {
    assert_silent(
        r#"
import { Effect, Layer } from "effect"
declare const A: Layer.Layer<never>
const program = Effect.succeed(1).pipe(Effect.provide(A))
"#,
        "no-chained-provides",
    );
}

#[test]
fn manual_sql_transaction() {
    let source = r#"
import { Effect } from "effect"
declare const sql: (strings: TemplateStringsArray) => unknown
export const begin = Effect.sync(() => sql`BEGIN`)
export const commit = Effect.sync(() => sql`COMMIT`)
export const rollback = Effect.sync(() => sql`ROLLBACK`)
"#;
    assert_fires(source, "no-manual-sql-transactions", 3);
}

#[test]
fn ordinary_sql_query_is_fine() {
    assert_silent(
        r#"
import { Effect } from "effect"
declare const sql: (strings: TemplateStringsArray) => unknown
export const users = Effect.sync(() => sql`SELECT * FROM users`)
"#,
        "no-manual-sql-transactions",
    );
}

#[test]
fn bare_retry_without_filter() {
    assert_fires(
        r#"
import { Effect, Schedule } from "effect"
const policy = Schedule.exponential("100 millis").pipe(Schedule.jittered, Schedule.either(Schedule.spaced("5 seconds")))
const retried = Effect.succeed(1).pipe(Effect.retry(policy))
"#,
        "retry-only-retryable",
        1,
    );
}

#[test]
fn retry_with_while_filter_is_fine() {
    assert_silent(
        r#"
import { Effect } from "effect"
const retried = Effect.succeed(1).pipe(
  Effect.retry({ times: 3, while: (error) => error._tag === "Transient" })
)
"#,
        "retry-only-retryable",
    );
}

#[test]
fn exponential_without_jitter_or_cap() {
    let source = r#"
import { Effect, Schedule } from "effect"
export const policy = Schedule.exponential("100 millis")
export const noop = Effect.void
"#;
    assert_fires(source, "add-jitter-to-backoff", 1);
    assert_fires(source, "cap-exponential-backoff", 1);
}

#[test]
fn jittered_and_capped_backoff_is_fine() {
    let source = r#"
import { Effect, Schedule } from "effect"
export const policy = Schedule.exponential("100 millis").pipe(
  Schedule.jittered,
  Schedule.either(Schedule.spaced("10 seconds"))
)
export const noop = Effect.void
"#;
    assert_silent(source, "add-jitter-to-backoff");
    assert_silent(source, "cap-exponential-backoff");
}

#[test]
fn unbounded_concurrency() {
    assert_fires(
        r#"
import { Effect } from "effect"
const all = Effect.all([Effect.succeed(1)], { concurrency: "unbounded" })
"#,
        "no-unbounded-concurrency",
        1,
    );
}

#[test]
fn bounded_concurrency_is_fine() {
    assert_silent(
        r#"
import { Effect } from "effect"
const all = Effect.all([Effect.succeed(1)], { concurrency: 10 })
"#,
        "no-unbounded-concurrency",
    );
}
