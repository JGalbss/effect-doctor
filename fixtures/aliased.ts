import { Effect as E } from "effect"

export const aliased = E.gen(function* () {
  const x = yield E.succeed(1)
  return x
})

export const callbackClock = E.sync(() => Date.now())
