import { Effect, Layer, Schema } from "effect"
import { it } from "@effect/vitest"
import { readFileSync } from "node:fs"

declare const sql: (strings: TemplateStringsArray, ...values: ReadonlyArray<unknown>) => unknown
declare const AppLayer: Layer.Layer<never>
declare const LogLayer: Layer.Layer<never>

export const timed = Effect.sync(() => setTimeout(() => {}, 100))

export const fetched = Effect.gen(function* () {
  const response = yield* Effect.tryPromise(() => fetch("https://example.com"))
  const parsed = JSON.parse("{}")
  const port = process.env.PORT
  return { response, parsed, port, file: readFileSync("x") }
})

export const begin = () => sql`BEGIN`

export const provided = Effect.succeed(1).pipe(
  Effect.provide(AppLayer),
  Effect.provide(LogLayer)
)

it("runs an effect", () => Effect.runPromise(Effect.succeed(1)))

it("async test", async () => {
  return 1
})

export class Account extends Schema.Class<Account>("Account")({
  id: Schema.String,
}) {}

export class Wrong extends Schema.Class<Account>("Wrong")({
  id: Schema.String,
}) {
  constructor() {
    super({ id: "nope" })
  }
}

export const swallowed = Effect.succeed(1).pipe(Effect.orDie)
