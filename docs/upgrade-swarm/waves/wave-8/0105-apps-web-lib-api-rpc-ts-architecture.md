# 0105 — apps/web/lib/api/rpc.ts — architecture lens

Target: `before-m5` (v0.3.0-rc5), module `apps/web/lib/api/rpc.ts` (1684 lines, one flat file). Read-only
review; nothing edited. Sister files read: `crates/api/src/wire.rs` (~1497 lines, the declared mirror),
`crates/api/src/rpc.rs` (registry + `runs_retry`), `docs/adr/003-interface.md` (client codegen intent),
`docs/adr/019-testing.md`, `docs/QUALITY_BAR.md`, `apps/web/AGENTS.md`. Claim status: CONFIRMED = read in
this tree; GUESS = inferred, needs verify.

## The client's role, today

`rpc.ts` is the entire typed RPC surface for the web app: one `call<T>(namespace, procedure, body)`
primitive plus ~100 exported wrappers, all wire interfaces, the SSE reader, the auth/HTTP helpers and
two module-level singletons. Invariant contract is stated in the header and is sound: session is a cookie
(`credentials: "include"`, never read or logged), space travels in `x-space-id`, failure is a typed
`RpcError{code,error,status}`. The file is explicitly Stopgap: "hand-written until the OpenAPI procedure
codegen lands (ADR-003); keep the shapes in sync with wire.rs" (CONFIRMED). ADR-003 §39-48 confirms the
end state is a generated TS client from `openapi.json` + `x-rpc-*` extensions, so the web app "never
hand-writes a URL." Today it hand-writes every URL, shape and procedure name (CONFIRMED). All debt is
structural, not behavioral — parity is otherwise intact.

## Structure after the milestone landings (rc5)

Sections are appended by milestone (M9 admin, M10 auth/artifacts/integrations/models/board/computers,
M11.3 groups, M12.2 external agents, M15.1 notifications), interleaving types, wrappers, normalizers and
constants. What that growth did not fix:

- **Confirmed drift with the mirror.** `runsRetry` declares `Promise<{ ok: boolean }>` and a comment
  claiming `runs.retry` is absent until M2; `rpc.rs:169,440` registers it and `wire.rs:224` returns
  `RunRetryReply{run_id,status,attempt}` (REQ-012). The type is wrong and the comment is stale
  (CONFIRMED). The hand-written mirror has already diverged from its source of truth.
- **Null-vs-optional mismatch.** `wire.rs` serializes `RunView.prompt_snippet/started_at/completed_at`
  and `MessagesReply/RunsReply.next_cursor` as `null` (no `skip_serializing_if`), but `rpc.ts` types them
  `foo?: string` (`string | undefined`). The `normalize*` shims happen to tolerate both; blind `as T`
  consumers do not (CONFIRMED).
- **Two error paths.** `call()` reads the envelope via `readEnvelope()` and fires `unauthorizedHandler`
  on 401; the auth helpers use a second parser `throwErr()` with an email-text sniffing fallback, which
  never fires the 401 handler. `authCapabilities`, `invitationPreview` and `computerOpenScreenFrame`
  hand-roll `fetch` again with their own cookie/base/header rules (CONFIRMED).
- **Implicit global state.** `knownSpaceId` (with localStorage fallback) and `unauthorizedHandler`
  ("last one wins") are module singletons: untestable in isolation, leak across tests, and guarded only
  by a `typeof window` check for SSR (CONFIRMED).
- **Blind casts.** Only messages/blocks are validated (`normalizeMessage`); every other reply is a bare
  `as T`, so a wire rename silently yields wrong data instead of a failing test (CONFIRMED).
- **Anonymous contracts.** Many wrappers use inline return shapes (`{executors}`, `{ok}`, `{id,status}`,
  `{connected}`, `{revoked}`) rather than named interfaces, so there is nothing to diff against `wire.rs`.
  The mirror covers only a subset of wire.rs (`memory.*`, `routines.*`, `pages.*`, `mcp.*`, `mail.*`,
  `skills.*`, `billing.*`, `audit.*` have no client yet) (CONFIRMED).

## Proposals for v0.4.0

### 1. Split the monolith into a transport core + per-namespace modules
Keep `rpc.ts` as a thin barrel re-exporting `call`, `RpcError`, `setSpaceId`, `setUnauthorizedHandler`,
`apiBase` and shared `*View`/`*Reply` types. Move wrappers into `lib/api/rpc/{threads,bots,spaces,runs,
admin,artifacts,models,board,computer,groups,notifications,misc}.ts`, grouped by the same namespace
tokens the Rust registry uses (`namespaces::*` in `rpc.rs`), and keep SSE (`threadsSubscribe`,
`parseEventStream`) and the screen-target/auth helpers in dedicated files. Risks: (a) pure, behavior-
preserving move — no URL, field, return type or error text may change, and all Playwright REQ specs must
stay green on an unchanged wire trace; (b) barrel re-exports preserve the current import paths so no
screen is touched; (c) do not create a second client or switch state/data libraries — this is file layout
only; (d) the namespace grouping must match the Rust registry names exactly or the mapping drifts again.

### 2. Start the ADR-003 codegen path; add a drift guard as the interim mirror
Generate the reply/request types and URL constants from the served `openapi.json` (`x-rpc-namespace`,
`x-rpc-procedure`) so the web app stops hand-writing URLs, with a committed generated file plus a CI
check that regenerates and diffs it (same discipline as the OpenAPI-vs-spec contract test, ADR-019).
If full codegen cannot land in v0.4.0, land the guard first: a test that cross-checks every exported
`*Reply`/`*View` field set against the `wire.rs` struct list, failing in CI on drift like `runsRetry`.
Risks: (a) codegen is the ADR-003 end state and may be a larger slice — sequence the guard first so the
mirror is protected even if generation slips; (b) codegen output must be reviewed/committed, not fetched
at build time (offline/export build, `output: "export"`), and its license added to docs/DEPENDENCIES.md;
(c) a field-set diff cannot catch semantics (null-vs-optional) — pair it with proposal 5; (d) keep a
hand-written shim only for SSE and the screen-target/procedure exceptions ADR-003 names.

### 3. One request primitive for cookie, space, 401 and error envelope
Fold `call`, `throwErr` and the three ad-hoc `fetch` sites into a single `request<T>(method, path, body,
opts)` (or a small `createTransport({base, getSpaceId, onUnauthorized})`). Polarity flags live in opts:
`credentials: "include" | "omit"`, `accept: "text/event-stream"`, bearer vs cookie. One envelope parser,
one 401 handler, one base-URL normalizer. Drop the email-text sniffing hack in `throwErr` in favor of the
uniform `{code,error}` envelope; if a backend must send a plain-text 400, fix it at the wire boundary.
Risks: (a) auth 401s currently do not trigger `unauthorizedHandler` — decide explicitly whether they
should, since redirecting a failed sign-in to `/sign-in` could loop; keep current behavior if unsure and
note it in docs/STATE.md; (b) SSE and screen-target are byte/stream transports — share headers and base,
not the JSON parse/decode; (c) `invitationPreview` and the frame round must keep `credentials: "omit"`
(never leak the session to a public/bearer endpoint); (d) verify envelope code strings stay identical
across all endpoints (screens branch on `code`).

### 4. Replace module singletons with an injectable client context
Bundle `getSpaceId`/`setSpaceId`, the unauthorized handler and `apiBase` override into one `RpcContext`
created at app bootstrap and threaded through the transport (React context for screens, a default
context for scripts/tests). Keep `setSpaceId`/`setUnauthorizedHandler` as deprecated shims for v0.4.0 so
the shell keeps working; remove them in a later major after all call sites migrate. Risks: (a) touches the
app entry that wires the 401 redirect — a missed handoff leaves the redirect dead (add a test that asserts
a 401 reaches the handler); (b) SSR has no window/localStorage — the default context must stay
deterministic (null space) so hydration and prerender are unchanged; (c) test isolation improves but old
tests relying on module-global `setSpaceId` must be migrated or the shims retained; (d) avoid a new state
library — one plain object + React context is enough.

### 5. Fix confirmed drift and make the mirror strict
Correct `runsRetry` to return a named `RunRetryReply` mirror (`run_id/status/attempt`) and delete the
stale M2 comment; name every anonymous return shape after its `wire.rs` struct. Model wire-null fields as
`T | null` where serde omits no key (`RunView.*`, `*Reply.next_cursor`) and keep `?` only for genuinely
omitted fields (`SendReply.run_ids`, `MessageView.run_id`). Replace blind `as T` casts with cheap
boundary validators for the reply shapes screens branch on (status/code enums, cursor, arrays).
Risks: (a) changing `runsRetry`'s type is a type-level fix only — confirm no screen reads `.ok` first,
else it is a behavior change needing a docs/PARITY.md entry; (b) `T | null` may surface new strict-TS
errors in consumers used to `undefined` — guard with optional chaining, do not weaken the type; (c)
validators add bundle weight and must not alter normalized output used by goldens; (d) `RunTrigger`/
status values are open strings in wire.rs — validate membership without freezing future server values.

## Explicit non-goals

No new RPC procedures, no wire/field rename, no schema or spec edit, no change to cookie/omit polarity, no
second client or data library, no SSE rewrite into `EventSource`, no BoardUI Pro. Verify with: a clean
`rpc.rs`/`wire.rs` cross-check for `runsRetry`, the existing Playwright REQ suite + axe (unchanged DOM),
the OpenAPI-vs-spec contract test, and `tsc --noEmit`. Record every claim in docs/STATE.md as CONFIRMED or
GUESS as today.
