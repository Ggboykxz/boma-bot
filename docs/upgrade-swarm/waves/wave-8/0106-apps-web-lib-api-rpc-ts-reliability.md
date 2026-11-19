# 0106 — apps/web/lib/api/rpc.ts — reliability (v0.4.0)

Worker #106 · `apps/web/lib/api/rpc.ts` · lens: reliability · READ-ONLY review of `before-m5`.
Facts CONFIRMED by reading `rpc.ts`, `crates/api/src/{error,rpc,wire}.rs`, `spec/03-api-contract.md`
and consumer call sites this session. Proposals/risks are GUESS (nothing implemented or tested).

Spec anchors: §3.1 (RPC framing, `x-<product>-space-id`, error envelope, `no-store`), §4.8 (degrade
on realtime loss, never silent staleness), REQ-001/008/009/010/017/027/067. ADR-003 is the standing
excuse for this hand-written mirror of `crates/api/src/wire.rs`.

## Current behavior (CONFIRMED)

- `call()` (245-272) is the only envelope-aware path: `credentials: "include"`, `x-space-id` header,
  `AbortSignal` only when supplied, no default timeout.
- `readEnvelope()` (225-238) accepts a failure only when BOTH `code` and `error` are strings; else
  `call()` fabricates `INTERNAL` + "The request could not be completed." (264-268).
- `throwErr()` (937-956), used by every `/api/auth/*` mutation and `authCapabilities`, parses the
  same envelope DIFFERENTLY: it accepts any JSON object (missing fields fall back to `INTERNAL`) and
  sniffs non-JSON text with `text.includes("email") && text.length < 300` as `{error: text}`. The
  comment at 934 claiming it is "solved exactly like `call`'s path" is false.
- On `!ok`, `call()` fires `unauthorizedHandler?.()` for 401 (260-262) before reading the envelope or
  throwing; direct-fetch endpoints (auth helpers, `invitationPreview`, `computerOpenScreenFrame`) never do.
- `setUnauthorizedHandler` (923-925) is last-write-wins module state with no getter — see 0074.
- Success bodies are blind-cast `as T` (271, 828, 992, ...). Only messages/blocks/reactions pass
  through runtime normalizers (295-396); every other reply is trusted verbatim.
- `threadsSubscribe` (627-677) reads SSE with no idle timeout, heartbeat or reconnect;
  `parseEventStream` (575-611) discards comment-only frames (incl. a bare `heartbeat`).
- `computerOpenScreenFrame` (1347-1375) returns `null` for any non-ok (incl. 401) via one rigid regex; no timeout.

## Failure modes

F1. **Timeouts (GUESS).** A hung `runs.list`/`threads.head` holds its skeleton forever — the error
phase never renders (echoed by 0016/F2). A half-open SSE socket makes `reader.read()` never resolve,
so `onClose` never fires and the UI reports "live" over stale data (violates §4.8). A caller-supplied
`AbortSignal` throws a bare `DOMException` (`AbortError`/`TimeoutError`), not `RpcError`, so screens
that only catch `RpcError` leak an unhandled rejection. `AbortSignal.timeout` is unavailable in some
target runtimes; naive `AbortSignal.any` composition can silently drop the caller's cancel signal.

F2. **Error envelope mapping (GUESS).** Same HTTP failure yields different `code`s by endpoint:
`readEnvelope` rejects `{error: "..."}` (a shape the server's own public/auth surfaces use), while
`throwErr` accepts it; screens branching on `code` get `INTERNAL` in one path and the real code in
another. `invitationPreview` (989-991) flattens every failure — including 500/429 — to `NOT_FOUND`,
and `computerOpenScreenFrame` flattens 401/503 to `null`: auth and outage signals disappear. Even a
valid-JSON non-object body falls through to INTERNAL, discarding the server's safe message.

F3. **Wire mirror drift (CONFIRMED at the enum).** Server `ErrorCode` has eight variants; the client
union (154-161) has seven — `COMPUTERS_UNAVAILABLE` (503, REQ-027) is missing, yet
`components/thread/computer-panel.tsx:47` already branches on that exact string. The type is a lie:
`RpcError.code` is `RpcErrorCode | string`, so any typo compiles and any rename passes silently.
Blind `as T` casts mean a `wire.rs` rename (e.g. `runId`→`run_id`, patched for blocks but not replies)
surfaces as `undefined` with no thrown error. The two documented drifts (`artifact_id`, `card_id`)
are patched locally, but ADR-003 codegen has not landed, so drift accumulates per milestone.

F4. **Auth 401 (CONFIRMED behavior; GUESS impact).** The handler fires only from `call()`, so a
session expiry discovered by `authCapabilities` or a subscribe stream never redirects. It is called
synchronously before the `RpcError` is built — if the host handler throws, the typed error is lost.
It fires on every 401, including a background poll or an idempotent replay, giving redirect storms;
guard logic lives in the host, not here. `setSpaceId(null)` does not clear `localStorage`, so a
stale/foreign space id is resent on every call and the server's tenant-as-404 (§3.1) looks like a
normal missing resource with no recovery path.

## Concrete hardening for v0.4.0 (GUESS)

P1. Add a single `request()` core used by `call`, auth helpers and subscribe: wrap caller signal
with a default timeout (`AbortSignal.timeout(10_000)` combined via `AbortSignal.any` when available,
else a manual controller); map timeout/abort to a typed `RpcError` carrying `isTimeout`/`isAborted`
flags rather than a new public code (a new code is a DEVIATION). *Risk:* `AbortSignal.any` needs a
feature-detected fallback in jsdom/older browsers; a 10s cap may break legitimate large artifact
downloads — expose a per-call override and keep subscribe on an idle timeout, not a hard cap.

P2. Reconcile envelope parsing: make `readEnvelope` the single parser, accept a partial
`{code}`/`{error}` object, validate `code` against a runtime set, and have `throwErr` delegate to it.
Delete the `"email"` text sniff (safe server messages are already guaranteed by `error.rs`).
*Risk:* accepting a partial envelope changes which safe message screens display for malformed bodies;
assert against the golden fixtures and mark any message change in `docs/PARITY.md`.

P3. Keep error semantics per endpoint: `invitationPreview` may map 404→"Not found." but must surface
5xx/429 as `RpcError` with their real status/code; `computerOpenScreenFrame` must throw on
401/503 (or return a tagged result) instead of bare `null`. *Risk:* callers of these two functions
currently branch on `null`/a flat message — update `accept-screen.tsx` and the computer panel in the
same wave or the change breaks them.

P4. Close the enum drift: add `COMPUTERS_UNAVAILABLE` to `RpcErrorCode` (matching `error.rs` and its
consumer at `computer-panel.tsx:47`) and add a contract test that diffs the client union against a
generated list from the server enum. Keep `| string` for forward compatibility but log unknown
codes through the telemetry hook. *Risk:* this is a type-only widening, not a wire change, but the
contract test must be generated at build time or it will itself drift.

P5. Guard success bodies at the boundary for the highest-value replies (`SendReply`, `ThreadHeadReply`,
`ArtifactGetReply`, auth capabilities) with small runtime validators, reusing the existing
normalizers' "never throw, degrade" style. *Risk:* validators add bundle bytes and duplicate the
schema they are meant to mirror — treat as a stopgap until ADR-003 codegen, and keep them dev-gated
or in a single shared module so they are cheap to delete.

P6. Make 401 handling safe: expose a getter, return-and-restore the previous handler on unregister
(coordinate with 0074), guard handler invocation by `typeof window` and an already-on-auth-route
check, and invoke it AFTER the `RpcError` is constructed so a throwing handler cannot swallow it. De-dupe
per navigation epoch. *Risk:* changes redirect timing that Playwright auth-flow tests depend on;
re-run the sign-in redirect tests and record any observable change as a DEVIATION.

P7. Separate session-scope from call-scope: `setSpaceId(null)` should clear both the module var and
`localStorage`; validate the space id format and, on a 404 that follows an `x-space-id` write, clear
it once and retry the call once. *Risk:* auto-retry can double a non-idempotent mutation — restrict
the retry to read procedures and require `clientNonce` on writes.

P8. Give subscribe explicit liveness: treat the spec's `heartbeat` event as a keepalive timestamp,
close and reconnect with backoff after an idle gap, and always call `onClose` on abort/timeout so the
screen can show "not live" (§4.8). *Risk:* reconnect + optimistic replay can duplicate frames —
require consumers to de-dupe on `seq` and cap reconnects; injectable clock needed for tests.

## Constraints and rollout

Keep the header's three invariants: cookie session (never read/logged), space in header, typed failure.
No public wire change — procedure names, reply shapes and `{code, error}` stay fixed; externally visible
changes are DEVIATIONs in `docs/PARITY.md` pending approval. Out of scope: ADR-003 codegen, server
lease/fence work, SSE replacement, renderer redesign.
Wave A: P2, P3, P4, P6 (small, locally revertable, mockable). Wave B: P1, P7, P8 (Playwright + fake
clock). Acceptance: no unguarded `as T` on the five named replies; a contract test proves the code
unions match; a killed SSE stream reaches `onClose` within the idle bound; every endpoint's 401
reaches the host handler exactly once per epoch.
