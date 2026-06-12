mod common;

use common::{assert_fires_v4, lint, lint_v4};

#[test]
fn v4_rules_are_off_without_v4_profile() {
    let source = r#"
import { Effect } from "effect"
const handled = Effect.succeed(1).pipe(Effect.catchAll(() => Effect.succeed(0)))
"#;
    assert_eq!(common::count(&lint(source), "v4-catch-renames"), 0);
    assert_eq!(common::count(&lint_v4(source), "v4-catch-renames"), 1);
}

#[test]
fn catch_family_renames() {
    let source = r#"
import { Effect } from "effect"
const a = Effect.succeed(1).pipe(Effect.catchAll(() => Effect.succeed(0)))
const b = Effect.succeed(1).pipe(Effect.catchAllCause(() => Effect.succeed(0)))
const c = Effect.succeed(1).pipe(Effect.catchSome(() => undefined))
"#;
    assert_fires_v4(source, "v4-catch-renames", 3);
}

#[test]
fn fork_renames() {
    assert_fires_v4(
        r#"
import { Effect } from "effect"
const fiber = Effect.fork(Effect.succeed(1))
const daemon = Effect.forkDaemon(Effect.succeed(1))
"#,
        "v4-fork-renames",
        2,
    );
}

#[test]
fn context_tag_to_service() {
    assert_fires_v4(
        r#"
import { Context } from "effect"
export class Db extends Context.Tag("Db")<Db, { query: () => string }>() {}
"#,
        "v4-context-service",
        1,
    );
}

#[test]
fn option_from_nullable() {
    assert_fires_v4(
        r#"
import { Option } from "effect"
export const value = Option.fromNullable(null)
"#,
        "v4-option-renames",
        1,
    );
}

#[test]
fn cause_renames() {
    assert_fires_v4(
        r#"
import { Cause } from "effect"
export const check = Cause.isFailType
export const find = Cause.failureOption
"#,
        "v4-cause-flattened",
        2,
    );
}

#[test]
fn layer_scoped_and_scope_extend() {
    let source = r#"
import { Layer, Scope, Effect } from "effect"
export const layer = Layer.scoped
export const extend = Scope.extend
"#;
    assert_fires_v4(source, "v4-layer-scoped-to-effect", 1);
    assert_fires_v4(source, "v4-scope-provide", 1);
}

#[test]
fn schema_variadic_shapes() {
    let source = r#"
import { Schema } from "effect"
export const Literals = Schema.Literal("a", "b")
export const Choice = Schema.Union(Schema.String, Schema.Number)
export const Pair = Schema.Tuple(Schema.String, Schema.Number)
export const Lookup = Schema.Record({ key: Schema.String, value: Schema.Number })
"#;
    assert_fires_v4(source, "v4-schema-renames", 4);
}

#[test]
fn schema_v4_array_forms_are_fine() {
    let source = r#"
import { Schema } from "effect"
export const Choice = Schema.Union([Schema.String, Schema.Number])
export const Single = Schema.Literal("only")
export const Lookup = Schema.Record(Schema.String, Schema.Number)
"#;
    assert_eq!(common::count(&lint_v4(source), "v4-schema-renames"), 0);
}

#[test]
fn schema_member_renames() {
    assert_fires_v4(
        r#"
import { Schema } from "effect"
export const Err = Schema.TaggedError
export const decode = Schema.decodeUnknown
export const picked = Schema.pick
"#,
        "v4-schema-renames",
        3,
    );
}

#[test]
fn gen_with_this_binding() {
    assert_fires_v4(
        r#"
import { Effect } from "effect"
class Service {
  load() {
    return Effect.gen(this, function* () {
      return yield* Effect.succeed(1)
    })
  }
}
"#,
        "v4-gen-self-options",
        1,
    );
}

#[test]
fn fiberref_import_removed() {
    let source = r#"
import { FiberRef } from "effect"
export const ref = FiberRef
"#;
    assert_fires_v4(source, "v4-fiberref-removed", 1);
}

#[test]
fn package_consolidation_and_unstable() {
    let source = r#"
import { HttpClient } from "@effect/platform"
import { Rpc } from "@effect/rpc"
import { HttpApi } from "effect/unstable/httpapi"
"#;
    assert_fires_v4(source, "v4-package-consolidation", 2);
    assert_fires_v4(source, "v4-unstable-import-awareness", 1);
}

#[test]
fn gen_adapter_param_flagged_even_without_v4_profile() {
    assert_eq!(
        common::count(
            &lint(
                r#"
import { Effect } from "effect"
const program = Effect.gen(function* (_) {
  return yield* Effect.succeed(1)
})
"#
            ),
            "v4-no-gen-adapter"
        ),
        1
    );
}

#[test]
fn platform_node_is_not_consolidated() {
    let source = r#"
import { NodeRuntime } from "@effect/platform-node"
import { Effect } from "effect"
export const noop = Effect.void
"#;
    assert_eq!(common::count(&lint_v4(source), "v4-package-consolidation"), 0);
}
