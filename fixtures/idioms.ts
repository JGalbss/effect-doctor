import { Effect, Schedule, Schema } from "effect"

export const voidish = Effect.succeed(undefined)

export const constant = Effect.map(Effect.succeed(1), () => "done")

export const flattened = Effect.succeed(Effect.succeed(1)).pipe(
  Effect.map((inner) => inner),
  Effect.flatten
)

export const doNotation = Effect.Do

export const tagDispatch = Effect.succeed(1).pipe(
  Effect.catchAll((error) => (error._tag === "NotFound" ? Effect.succeed(0) : Effect.fail(error)))
)

export const refail = Effect.succeed(1).pipe(
  Effect.catchAll((error) => Effect.fail({ wrapped: error }))
)

export const backoff = Schedule.exponential("100 millis")

export const retried = Effect.succeed(1).pipe(Effect.retry(backoff))

export const tooMany = Effect.all([Effect.succeed(1)], { concurrency: "unbounded" })

const User = Schema.Struct({ name: Schema.String })

export const decodePerCall = (input: unknown) => Schema.decodeUnknownEffect(User)(input)

export const badSpan = Effect.fn("run")(function* (id: string) {
  return yield* Effect.succeed(id)
})

export const wrapper = (id: string) =>
  Effect.gen(function* () {
    yield* Effect.logInfo(id)
    return id
  })

export class HttpError extends Error {}
