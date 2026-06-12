import { Effect } from "effect"

export const program = Effect.gen(function* () {
  const value = yield Effect.succeed(1)
  try {
    yield* Effect.succeed(2)
  } catch {
    // swallowed
  }
  if (!value) {
    throw new Error("bad")
  }
  const now = Date.now()
  const when = new Date()
  const roll = Math.random()
  console.log("hi", when, roll)
  Effect.runPromise(Effect.succeed(3))
  return now
})

export const adapter = Effect.gen(function* (_) {
  return yield* Effect.succeed(1)
})

export const pointless = Effect.gen(function* () {
  return yield* Effect.succeed(2)
})
