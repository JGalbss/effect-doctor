//! Rewrite recipes: for every rule, the anti-pattern and how to write it in
//! cleaner Effect. Single source of truth for `explain`, the JSON rule
//! export, agent fix payloads, and the docs site.

pub struct RuleExample {
    pub bad: &'static str,
    pub good: &'static str,
}

pub fn example_for(rule: &str) -> Option<RuleExample> {
    let (bad, good) = match rule {
        // ─── correctness ───
        "require-yield-star" => (
            "const user = yield getUser(id)",
            "const user = yield* getUser(id)",
        ),
        "no-try-catch-in-gen" => (
            "Effect.gen(function* () {\n  try {\n    return yield* fetchUser(id)\n  } catch (e) {\n    return null\n  }\n})",
            "fetchUser(id).pipe(\n  Effect.catchTag(\"UserNotFound\", () => Effect.succeed(null))\n)",
        ),
        "no-try-finally-in-gen" => (
            "Effect.gen(function* () {\n  const fiber = yield* Effect.fork(poller)\n  try {\n    return yield* job\n  } finally {\n    yield* Fiber.interrupt(fiber)\n  }\n})",
            "job.pipe(Effect.ensuring(Fiber.interrupt(fiber)))\n// or: Effect.race(job, poller) / Effect.acquireRelease",
        ),
        "no-throw-in-effect" => (
            "Effect.gen(function* () {\n  if (!valid) throw new Error(\"invalid input\")\n})",
            "Effect.gen(function* () {\n  if (!valid) return yield* new InvalidInput({ input })\n})",
        ),
        "no-run-inside-effect" => (
            "Effect.gen(function* () {\n  const result = Effect.runSync(computeThing())\n})",
            "Effect.gen(function* () {\n  const result = yield* computeThing()\n})",
        ),
        "schema-class-self-mismatch" => (
            "class Wrong extends Schema.Class<Account>(\"Wrong\")({ id: Schema.String }) {}",
            "class Wrong extends Schema.Class<Wrong>(\"Wrong\")({ id: Schema.String }) {}",
        ),
        "no-constructor-override-in-schema-class" => (
            "class User extends Schema.Class<User>(\"User\")({ id: Schema.String }) {\n  constructor() { super({ id: \"0\" }) }\n}",
            "class User extends Schema.Class<User>(\"User\")({ id: Schema.String }) {\n  static empty = () => User.make({ id: \"0\" })\n}",
        ),
        "no-orDie-to-silence-errors" => (
            "loadConfig.pipe(Effect.orDie)",
            "loadConfig.pipe(Effect.catchTag(\"ConfigError\", () => Effect.succeed(defaults)))",
        ),
        "no-async-callback-in-effect-combinators" => (
            "Effect.map(user, async (u) => await enrich(u))",
            "Effect.flatMap(user, (u) => Effect.tryPromise(() => enrich(u)))",
        ),
        "no-then-in-sync" => (
            "Effect.sync(() => {\n  fetchData().then((d) => use(d))\n})",
            "Effect.tryPromise(() => fetchData()).pipe(Effect.map(use))",
        ),
        "no-promise-all-in-effect" => (
            "Effect.tryPromise(() => Promise.all(ids.map(fetchUser)))",
            "Effect.forEach(ids, (id) => Effect.tryPromise(() => fetchUser(id)), {\n  concurrency: 5,\n})",
        ),
        "no-runsync-on-async-effect" => (
            "Effect.runSync(Effect.promise(() => fetch(url)))",
            "await Effect.runPromise(Effect.promise(() => fetch(url)))",
        ),
        "no-map-returning-effect" => (
            "Effect.map(user, (u) => Effect.log(u.name)) // log never runs",
            "Effect.tap(user, (u) => Effect.log(u.name))",
        ),
        "no-runcollect-on-infinite-stream" => (
            "Stream.forever(poll).pipe(Stream.runCollect)",
            "Stream.forever(poll).pipe(Stream.take(100), Stream.runCollect)\n// or Stream.runDrain when results are not needed",
        ),
        "no-object-literal-comparison" => (
            "selected.includes({ id: 1, name: \"Paul\" }) // always false",
            "selected.some((u) => Equal.equals(u, Data.struct({ id: 1, name: \"Paul\" })))",
        ),
        "no-catchall-to-null" => (
            "getUser(id).pipe(Effect.catchAll(() => Effect.succeed(null)))",
            "getUser(id).pipe(\n  Effect.catchTag(\"UserNotFound\", () => Effect.succeed(null))\n)",
        ),
        "no-string-errors" => (
            "Effect.fail(\"Something went wrong!\")",
            "class QueryError extends Data.TaggedError(\"QueryError\")<{ cause: unknown }> {}\nEffect.fail(new QueryError({ cause }))",
        ),
        "prefer-config-redacted" => (
            "const apiKey = Config.string(\"API_KEY\")",
            "const apiKey = Config.redacted(\"API_KEY\")",
        ),
        // ─── idiomatic ───
        "prefer-catch-tag" => (
            "Effect.catchAll((e) =>\n  e._tag === \"NotFound\" ? Effect.succeed(0) : Effect.fail(e)\n)",
            "Effect.catchTag(\"NotFound\", () => Effect.succeed(0))",
        ),
        "catch-to-map-error" => (
            "Effect.catchAll((e) => Effect.fail(new WrappedError({ cause: e })))",
            "Effect.mapError((e) => new WrappedError({ cause: e }))",
        ),
        "prefer-effect-void" => (
            "Effect.succeed(undefined)",
            "Effect.void",
        ),
        "prefer-as-void" => (
            "effect.pipe(Effect.map(() => undefined))",
            "effect.pipe(Effect.asVoid)",
        ),
        "prefer-flatmap-over-map-flatten" => (
            "effect.pipe(Effect.map(toEffect), Effect.flatten)",
            "effect.pipe(Effect.flatMap(toEffect))",
        ),
        "no-unnecessary-pipe" => (
            "const value = effect.pipe()",
            "const value = effect",
        ),
        "no-unnecessary-gen" => (
            "Effect.gen(function* () {\n  return yield* fetchUser(id)\n})",
            "fetchUser(id)",
        ),
        "no-unnecessary-fail-of-yieldable" => (
            "return yield* Effect.fail(new NotFound({ id }))",
            "return yield* new NotFound({ id })",
        ),
        "no-nested-gen-yield" => (
            "Effect.gen(function* () {\n  const user = yield* Effect.gen(function* () {\n    return yield* repo.byId(id)\n  })\n})",
            "Effect.gen(function* () {\n  const user = yield* repo.byId(id)\n})",
        ),
        "no-effect-fn-iife" => (
            "yield* Effect.fn(function* () { ... })()",
            "yield* Effect.gen(function* () { ... })",
        ),
        "no-unnecessary-pipe-chain" => (
            "value.pipe(Effect.map(f)).pipe(Effect.flatMap(g))",
            "value.pipe(Effect.map(f), Effect.flatMap(g))",
        ),
        "no-return-effect-in-gen" => (
            "Effect.gen(function* () {\n  return Effect.succeed(1) // success value is an Effect!\n})",
            "Effect.gen(function* () {\n  return yield* Effect.succeed(1)\n})",
        ),
        "redundant-schema-tag-identifier" => (
            "class NotFound extends Schema.TaggedError<NotFound>(\"NotFound\")(\"NotFound\", {}) {}",
            "class NotFound extends Schema.TaggedError<NotFound>()(\"NotFound\", {}) {}",
        ),
        "no-effect-do-notation" => (
            "Effect.Do.pipe(\n  Effect.bind(\"user\", () => getUser(id)),\n  Effect.bind(\"posts\", ({ user }) => getPosts(user))\n)",
            "Effect.gen(function* () {\n  const user = yield* getUser(id)\n  const posts = yield* getPosts(user)\n  return { user, posts }\n})",
        ),
        "prefer-clock-service" => (
            "Effect.gen(function* () {\n  const now = Date.now()\n})",
            "Effect.gen(function* () {\n  const now = yield* Clock.currentTimeMillis\n})",
        ),
        "prefer-random-service" => (
            "Effect.sync(() => Math.random())",
            "Random.next",
        ),
        "prefer-effect-logging" => (
            "Effect.gen(function* () {\n  console.log(\"user created\", user)\n})",
            "Effect.gen(function* () {\n  yield* Effect.logInfo(\"user created\").pipe(Effect.annotateLogs({ user }))\n})",
        ),
        "prefer-effect-timers" => (
            "Effect.sync(() => setTimeout(poll, 5000))",
            "poll.pipe(Effect.delay(\"5 seconds\"), Effect.forever)",
        ),
        "prefer-platform-fetch" => (
            "Effect.tryPromise(() => fetch(\"https://api.example.com/users\"))",
            "HttpClient.get(\"https://api.example.com/users\")\n// typed errors, tracing, interruption built in",
        ),
        "prefer-config-module" => (
            "Effect.gen(function* () {\n  const port = process.env.PORT\n})",
            "Effect.gen(function* () {\n  const port = yield* Config.integer(\"PORT\")\n})",
        ),
        "prefer-schema-over-json" => (
            "const data = JSON.parse(raw) // any, throws",
            "const data = yield* decodeUser(raw) // Schema.fromJsonString(User)",
        ),
        "prefer-decode-effect" => (
            "Effect.gen(function* () {\n  const user = Schema.decodeUnknownSync(User)(input) // throws\n})",
            "Effect.gen(function* () {\n  const user = yield* decodeUser(input) // Schema.decodeUnknownEffect(User)\n})",
        ),
        "prefer-effect-fn" => (
            "const loadUser = (id: string) =>\n  Effect.gen(function* () {\n    return yield* repo.byId(id)\n  })",
            "const loadUser = Effect.fn(\"UserRepo.loadUser\")(function* (id: string) {\n  return yield* repo.byId(id)\n})",
        ),
        "prefer-tagged-error-classes" => (
            "class HttpError extends Error {}",
            "class HttpError extends Schema.TaggedErrorClass<HttpError>()(\"HttpError\", {\n  status: Schema.Number,\n}) {}",
        ),
        "meaningful-span-names" => (
            "Effect.fn(\"run\")(function* () { ... })",
            "Effect.fn(\"OrderService.placeOrder\")(function* () { ... })",
        ),
        "prefer-it-effect" => (
            "it(\"creates a user\", () => Effect.runPromise(program))",
            "it.effect(\"creates a user\", () => program)",
        ),
        "prefer-node-effect-counterparts" => (
            "import { readFileSync } from \"node:fs\"",
            "import { FileSystem } from \"@effect/platform\"\n// const fs = yield* FileSystem.FileSystem; yield* fs.readFileString(path)",
        ),
        "no-tag-string-comparison" => (
            "if (result._tag === \"Left\") { ... }",
            "if (Either.isLeft(result)) { ... }",
        ),
        "prefer-match-over-tag-switch" => (
            "switch (event._tag) {\n  case \"OrderPlaced\": ...\n  case \"OrderShipped\": ...\n}",
            "Match.valueTags(event, {\n  OrderPlaced: (e) => ...,\n  OrderShipped: (e) => ...,\n}) // exhaustive",
        ),
        "prefer-duration-over-raw-millis" => (
            "Effect.sleep(2000) // ms? s?",
            "Effect.sleep(\"2 seconds\")",
        ),
        "prefer-succeed-over-sync-literal" => (
            "Effect.sync(() => 42)",
            "Effect.succeed(42)",
        ),
        "prefer-structured-logging-args" => (
            "Effect.log(`Results: ${JSON.stringify(results)}`)",
            "Effect.log(\"results computed\").pipe(Effect.annotateLogs({ results }))",
        ),
        "prefer-json-response-helper" => (
            "HttpServerResponse.text(JSON.stringify(user))",
            "HttpServerResponse.json(user)",
        ),
        "require-typed-catch-in-try" => (
            "Effect.tryPromise(() => fetch(url)) // UnknownException",
            "Effect.tryPromise({\n  try: () => fetch(url),\n  catch: (cause) => new FetchError({ cause }),\n})",
        ),
        "prefer-timeout-over-race-sleep" => (
            "Effect.race(fetchData, Effect.sleep(\"2 seconds\"))",
            "fetchData.pipe(Effect.timeout(\"2 seconds\"))",
        ),
        "no-fork-then-immediate-join" => (
            "const fiber = yield* Effect.fork(task)\nconst result = yield* Fiber.join(fiber)",
            "const result = yield* task",
        ),
        "avoid-long-combinator-chains" => (
            "step1().pipe(\n  Effect.flatMap(step2),\n  Effect.flatMap(step3),\n  Effect.flatMap(step4),\n  Effect.flatMap(step5)\n)",
            "Effect.gen(function* () {\n  const a = yield* step1()\n  const b = yield* step2(a)\n  const c = yield* step3(b)\n  return yield* step5(yield* step4(c))\n})",
        ),
        // ─── architecture ───
        "no-chained-provides" => (
            "program.pipe(Effect.provide(DbLayer), Effect.provide(LogLayer))",
            "program.pipe(Effect.provide(Layer.mergeAll(DbLayer, LogLayer)))",
        ),
        "no-manual-sql-transactions" => (
            "yield* sql`BEGIN`\nyield* insertOrder\nyield* sql`COMMIT`",
            "yield* insertOrder.pipe(sql.withTransaction)",
        ),
        "retry-only-retryable" => (
            "fetchData.pipe(Effect.retry(Schedule.exponential(\"100 millis\")))",
            "fetchData.pipe(Effect.retry({\n  schedule: Schedule.exponential(\"100 millis\"),\n  while: (e) => e._tag === \"TransientError\",\n}))",
        ),
        "no-layer-mergeall-megalist" => (
            "Layer.mergeAll(A, B, C, D, E, F, G, H, I, J, K, L)",
            "const CoreInfra = Layer.mergeAll(A, B, C)\nconst UserModule = Layer.mergeAll(D, E)\nLayer.mergeAll(CoreInfra, UserModule, ...)",
        ),
        // ─── performance ───
        "add-jitter-to-backoff" => (
            "Schedule.exponential(\"100 millis\")",
            "Schedule.exponential(\"100 millis\").pipe(Schedule.jittered)",
        ),
        "cap-exponential-backoff" => (
            "Schedule.exponential(\"100 millis\")",
            "Schedule.exponential(\"100 millis\").pipe(\n  Schedule.either(Schedule.spaced(\"10 seconds\"))\n)",
        ),
        "no-unbounded-concurrency" => (
            "Effect.forEach(users, notify, { concurrency: \"unbounded\" })",
            "Effect.forEach(users, notify, { concurrency: 10 })",
        ),
        "hoist-schema-codecs" => (
            "const parse = (u: unknown) => Schema.decodeUnknownEffect(User)(u)",
            "const decodeUser = Schema.decodeUnknownEffect(User)\nconst parse = (u: unknown) => decodeUser(u)",
        ),
        "effect-all-missing-concurrency" => (
            "Effect.all([fetchUser, fetchPosts]) // silently sequential",
            "Effect.all([fetchUser, fetchPosts], { concurrency: \"unbounded\" })",
        ),
        "stream-mapeffect-missing-concurrency" => (
            "stream.pipe(Stream.mapEffect(processItem))",
            "stream.pipe(Stream.mapEffect(processItem, { concurrency: 4 }))",
        ),
        "prefer-queue-bounded" => (
            "const queue = yield* Queue.unbounded<Job>()",
            "const queue = yield* Queue.bounded<Job>(100) // backpressure",
        ),
        "no-eager-chunk-stream" => (
            "Stream.fromChunk(Chunk.fromIterable(bigIterable))",
            "Stream.fromIterable(bigIterable)",
        ),
        // ─── v4 migration ───
        "v4-no-gen-adapter" => (
            "Effect.gen(function* (_) {\n  const user = yield* _(getUser(id))\n})",
            "Effect.gen(function* () {\n  const user = yield* getUser(id)\n})",
        ),
        "v4-catch-renames" => (
            "effect.pipe(Effect.catchAll(handle))",
            "effect.pipe(Effect.catch(handle))",
        ),
        "v4-fork-renames" => (
            "yield* Effect.fork(task)",
            "yield* Effect.forkChild(task)",
        ),
        "v4-context-service" => (
            "class Db extends Context.Tag(\"Db\")<Db, Shape>() {}",
            "class Db extends Context.Service<Db, Shape>()(\"Db\") {}",
        ),
        "v4-cause-flattened" => (
            "Cause.isFailType(cause)",
            "Cause.isFailReason(reason) // Cause is a flat reasons array in v4",
        ),
        "v4-runtime-removed" => (
            "const runtime = yield* Effect.runtime<R>()",
            "const services = yield* Effect.context<R>()\n// run with Effect.runForkWith(services)",
        ),
        "v4-scope-provide" => (
            "Scope.extend(effect, scope)",
            "Scope.provide(effect, scope)",
        ),
        "v4-layer-scoped-to-effect" => (
            "Layer.scoped(Tag, acquire)",
            "Layer.effect(Tag, acquire) // v4 Layer.effect handles scoping",
        ),
        "v4-gen-self-options" => (
            "Effect.gen(this, function* () { ... })",
            "Effect.gen({ self: this }, function* () { ... })",
        ),
        "v4-option-renames" => (
            "Option.fromNullable(value)",
            "Option.fromNullishOr(value)",
        ),
        "v4-schema-renames" => (
            "Schema.Union(A, B)\nSchema.Literal(\"a\", \"b\")\nSchema.TaggedError",
            "Schema.Union([A, B])\nSchema.Literals([\"a\", \"b\"])\nSchema.TaggedErrorClass",
        ),
        "v4-fiberref-removed" => (
            "import { FiberRef } from \"effect\"",
            "import { Context } from \"effect\"\n// Context.Reference for ambient config with defaults",
        ),
        "v4-package-consolidation" => (
            "import { HttpClient } from \"@effect/platform\"",
            "import { HttpClient } from \"effect/unstable/http\"",
        ),
        "v4-unstable-import-awareness" => (
            "import { HttpApi } from \"effect/unstable/httpapi\"",
            "// fine to use — unstable APIs may change in minor releases; pin exactly",
        ),
        "prefer-abort-signal-passthrough" => (
            "Effect.tryPromise({\n  try: () => fetch(url),\n  catch: (cause) => new FetchError({ cause }),\n})",
            "Effect.tryPromise({\n  try: (signal) => fetch(url, { signal }), // interruption cancels the request\n  catch: (cause) => new FetchError({ cause }),\n})",
        ),
        "prefer-gen-over-nested-flatmap" => (
            "getUser(id).pipe(\n  Effect.flatMap((user) =>\n    getAccount(user).pipe(\n      Effect.flatMap((account) => createInvoice(user, account))\n    )\n  )\n)",
            "Effect.gen(function* () {\n  const user = yield* getUser(id)\n  const account = yield* getAccount(user)\n  return yield* createInvoice(user, account)\n})",
        ),
        // ─── adoption (experimental, --adopt) ───
        "adopt-async-function" => (
            "async function loadUser(id: string) {\n  const res = await fetch(`/users/${id}`)\n  return res.json()\n}",
            "const loadUser = Effect.fn(\"loadUser\")(function* (id: string) {\n  const res = yield* Effect.tryPromise({\n    try: () => fetch(`/users/${id}`),\n    catch: (cause) => new FetchError({ cause }),\n  })\n  return yield* Effect.tryPromise(() => res.json())\n})",
        ),
        "adopt-promise-chain" => (
            "fetchUser(id).then((user) => enrich(user)).then(save)",
            "Effect.tryPromise(() => fetchUser(id)).pipe(\n  Effect.flatMap((user) => Effect.tryPromise(() => enrich(user))),\n  Effect.flatMap((user) => Effect.tryPromise(() => save(user)))\n)",
        ),
        "adopt-new-promise" => (
            "new Promise((resolve, reject) => {\n  socket.once(\"data\", resolve)\n  socket.once(\"error\", reject)\n})",
            "Effect.async<Buffer, SocketError>((resume) => {\n  socket.once(\"data\", (d) => resume(Effect.succeed(d)))\n  socket.once(\"error\", (e) => resume(Effect.fail(new SocketError({ cause: e }))))\n})",
        ),
        "adopt-promise-all" => (
            "await Promise.all(ids.map(fetchUser))",
            "yield* Effect.forEach(ids, (id) => fetchUser(id), { concurrency: 10 })",
        ),
        "adopt-await-in-loop" => (
            "for (const id of ids) {\n  await processUser(id) // strictly sequential\n}",
            "yield* Effect.forEach(ids, (id) => processUser(id), { concurrency: 5 })",
        ),
        "prefer-foreach-over-yield-loop" => (
            "Effect.gen(function* () {\n  for (const id of ids) {\n    yield* processUser(id)\n  }\n})",
            "Effect.forEach(ids, (id) => processUser(id), { concurrency: 5 })\n// or { concurrency: 1 } to stay sequential — but explicit",
        ),
        // ─── agent hygiene (experimental, --agent) ───
        "agent-no-if-else-chain" => (
            "let label\nif (level === \"error\") {\n  label = \"!\"\n} else if (level === \"warn\") {\n  label = \"?\"\n} else {\n  label = \".\"\n}",
            "const label = Match.value(level).pipe(\n  Match.when(\"error\", () => \"!\"),\n  Match.when(\"warn\", () => \"?\"),\n  Match.orElse(() => \".\")\n)",
        ),
        "agent-no-ternary" => (
            "const tone = percent >= 100 ? \"critical\" : percent >= 80 ? \"warning\" : \"neutral\"",
            "const tone = Match.value(percent).pipe(\n  Match.when((p) => p >= 100, () => \"critical\"),\n  Match.when((p) => p >= 80, () => \"warning\"),\n  Match.orElse(() => \"neutral\")\n)",
        ),
        "agent-no-string-equality-guard" => (
            "if (value.kind === \"user\") {\n  return renderUser(value)\n}",
            "if (isUser(value)) {\n  return renderUser(value)\n}\n// or: Match.value(value).pipe(Match.tag(\"user\", renderUser), ...)",
        ),
        "agent-no-raw-loop" => (
            "const rowsByScope = new Map()\nfor (const row of rows) {\n  const list = rowsByScope.get(row.scopeId) ?? []\n  list.push(row)\n  rowsByScope.set(row.scopeId, list)\n}",
            "const rowsByScope = Array.groupBy(rows, (row) => row.scopeId)\n// effectful: yield* Effect.forEach(rows, persistRow, { concurrency: 5 })",
        ),
        "agent-no-let" => (
            "let softCeiling = 0\nif (hasSoft) {\n  softCeiling = softCap + freeAllowance\n}",
            "const softCeiling = hasSoft ? softCap + freeAllowance : 0\n// better: const softCeiling = Option.match(soft, { onNone: () => 0, onSome: (c) => c + freeAllowance })",
        ),
        "agent-duplicate-function" => (
            "const usageBarFill = (p) => p >= 100 ? red : p >= 80 ? amber : gray\nconst usageText = (p) => p >= 100 ? red : p >= 80 ? amber : gray // copy-paste",
            "const usageTone = (p) => p >= 100 ? red : p >= 80 ? amber : gray\nconst usageBarFill = usageTone\nconst usageText = usageTone",
        ),
        "agent-no-mutation" => (
            "let softCeiling = 0\nif (hasSoft) {\n  softCeiling = softCap + freeAllowance // reassigned\n}\npayload.total = softCeiling // mutated in place",
            "const softCeiling = hasSoft ? softCap + freeAllowance : 0\nconst payload = { ...base, total: softCeiling } // derived once, no mutation",
        ),
        "agent-duplicate-cross-file" => (
            "// src/billing/format.ts\nexport const formatCredits = (n) => `${Math.round(n)} cr`\n\n// src/ui/Usage.tsx — agent re-created the same helper\nconst formatCredits = (n) => `${Math.round(n)} cr`",
            "// src/billing/format.ts\nexport const formatCredits = (n) => `${Math.round(n)} cr`\n\n// src/ui/Usage.tsx\nimport { formatCredits } from \"../billing/format\"",
        ),
        "agent-near-duplicate-function" => (
            "// a.ts\nexport const toTone = (p) => { const t = pick(p); return clamp(t) }\n// b.ts — lightly-edited copy\nexport const barTone = (q) => { const v = pick(q); return clamp(v) }",
            "// tone.ts — one source of truth\nexport const tone = (p) => clamp(pick(p))\n// a.ts / b.ts both: import { tone } from \"./tone\"",
        ),
        "agent-similar-function-name" => (
            "// src/a.ts\nexport const parseConfig = (raw) => Schema.decodeUnknownSync(Config)(raw)\n// src/b.ts — same name, divergent impl\nexport const parseConfig = (raw) => JSON.parse(raw)",
            "// src/config.ts — one canonical parser\nexport const parseConfig = (raw) =>\n  Schema.decodeUnknownEffect(Config)(JSON.parse(raw))",
        ),
        "agent-similar-shape" => (
            "// both take (id) and call getUser + decode + log — same job, two routes\nexport const loadUser = (id) => { ... }\nexport const fetchUser = (id) => { ... }",
            "// keep one; derive the other or delete it\nexport const loadUser = (id) => Effect.gen(function* () { ... })\nexport const fetchUser = loadUser",
        ),
        _ => return None,
    };
    Some(RuleExample { bad, good })
}
