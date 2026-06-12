import { Clock, Effect } from "effect"

export const program = Effect.gen(function* () {
  const now = yield* Clock.currentTimeMillis
  yield* Effect.logInfo("tick", now)
  return now
})

const outsideEffect = Date.now()

export function* plainGenerator() {
  yield outsideEffect
}
