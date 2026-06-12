import { Cause, Context, Effect, Layer, Option, Schema, Scope } from "effect"
import { FiberRef } from "effect"
import { HttpClient } from "@effect/platform"

export const caught = Effect.succeed(1).pipe(Effect.catchAll(() => Effect.succeed(0)))

export const forked = Effect.fork(Effect.succeed(1))

export const nullable = Option.fromNullable(null)

export const extended = (scope: Scope.Scope) => Scope.extend(Effect.succeed(1), scope)

export const scopedLayer = Layer.scoped

export const oldCause = Cause.isFailType

export class Db extends Context.Tag("Db")<Db, { query: () => string }>() {}

export const OldLiteral = Schema.Literal("a", "b")

export const OldUnion = Schema.Union(Schema.String, Schema.Number)

export const OldTagged = Schema.TaggedError

export const refUsage = FiberRef

export const client = HttpClient
