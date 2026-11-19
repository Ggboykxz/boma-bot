# 0274 — `crates/infra/src/cdp.rs` — reliability proposal (v0.4.0)

Worker #274 · `crates/infra/src/cdp.rs` · lens: reliability · READ-ONLY review of `before-m5`
(v0.3.0-era source). No code changed. **CONFIRMED** = read this session; **GUESS** = needs a test.

Scope: this 387-line minimal CDP client — `Cdp`, `CdpError`, `connect`, `connect_once`, the six
tool methods, `call`, `http_get` — and `crates/infra/src/computer.rs` (:91-181, :480-514, one fresh
session per tool call). Anchors: AGENTS "timeout on every outbound call", "cancellation safety
considered", startup-validated typed config, no secrets in logs; header promise "small and testable".

## What is already right (keep)

- Both `connect` steps and the receive half of `call` are timed out (CONFIRMED :101-122, :267-272);
  the connect loop has a 5 s budget and 200 ms pause (:22-25, :67-83). `http_get` caps headers
  (64 KiB) and body (16 MiB) and reads exactly `content-length` to dodge Chromium's ignored
  `Connection: close` (CONFIRMED :326-381).
- Typed `CdpError`, no `unwrap`/`expect`/`panic` (CONFIRMED); `Exception` is separated from transport
  failures (CONFIRMED, computer.rs:123-131); non-matching event ids are discarded on purpose (:292).
  The CDP port binds `127.0.0.1` on the host (CONFIRMED, computer.rs:247-249).

## Failure modes

### F1. Websocket drops leave half-executed actions, no recovery (high)

`call` returns `Transport(ConnectionClosed)` when the stream ends (:273-277); no reconnect, retry,
or transient/permanent classification. `click` is two calls (pressed, released, :187-213): a drop
between them leaves the mouse *pressed* — state corruption, not just an error (CONFIRMED shape,
GUESS on a real drop). `type_text` sends one key event per character (:220-232): a drop at char k
leaves partial text with no idempotency or rollback, multiplying failure probability by length. With
no heartbeat, a half-dead socket is found only by the next timeout, costing 10 s (GUESS).

### F2. Unauthenticated CDP, trusted ws URL, arbitrary target (high)

The ws URL is taken verbatim from `/json/list` and passed to `connect_async` (:113-122); no scheme,
host, port or Origin check. A misconfigured/compromised local endpoint can redirect the client.
With no token or Origin allowlist configured here (CONFIRMED), any local process can attach to the
same page target and navigate, close or inject input, making results nondeterministic. Target choice
is the *first* `type == "page"` with no id pinning (:107-112): a restart with multiple tabs, or
another actor's tab, silently routes commands to the wrong page (GUESS on a live browser).
Never bypass a protection of the original product; this concerns only Boma's own browser.

### F3. Timeouts are partial, per-frame, inconsistent (high)

`send()` in `call` is **not** timed out (:263-265): a peer with a full send buffer hangs a call
forever, violating "timeout on every outbound call" (CONFIRMED). The 10 s receive timeout is
per-frame, so trickled events reset it while our id is never answered — the call is unbounded
(CONFIRMED). The two `enable` calls after the loop (:86-87) each use 10 s and sit outside the 5 s
connect budget, so `connect` can take ~25 s before `Unavailable`, contradicting its own doc comment
(CONFIRMED). `http_get` relies wholly on the caller's timeout (:313-380). Constants are hardcoded (:14-25), with
no startup-validated config; the fixed retry pause has no jitter, so many computers retry in
lockstep (GUESS on impact).

### F4. Process death / restart looks like a generic outage (medium-high)

Death is detected only as a closed socket (:273-277); no health check, restart signal or target
epoch. A crash/restart can keep the host port while invalidating the target, giving a TOCTOU between
`/json/list` and `connect_async` that surfaces as a raw error (CONFIRMED window). The adapter folds
every `CdpError` into `AppError::Unavailable` (CONFIRMED, computer.rs:96-99, :106-110, :510-513), so
callers cannot apply a "rebooting, retry once" policy. No metric for reconnect count, timeout rate
or latency, so flapping is invisible (CONFIRMED absence).

### F5. Result-level reliability (medium)

`evaluate` reads the exception text from `result["result"]["description"]` (:141-146), but CDP puts
it in `exceptionDetails.exception.description` — diagnostics likely empty (CONFIRMED code, GUESS on
the frame). `returnByValue: true` without `awaitPromise: true` (:134-139) returns a Promise object
for async expressions — silently wrong (CONFIRMED shape). `navigate` ignores `errorText` (:177-180),
so a refused navigation reports success (CONFIRMED). `screenshot` decodes base64 with no decoded
size cap (:161-169).

## Concrete hardening (v0.4.0)

1. **P1 — absolute per-call deadline.** Time out `send()` and give the whole `call` one `Instant`
   used by every `next()`; fold `enable` calls into the connect budget. Risk R3.
2. **P2 — typed config.** `CdpConfig { call_timeout, connect_budget, retry_pause, body_cap }` from
   `BOMA_*`, validated at startup; add ±jitter to retries. Risk R6.
3. **P3 — single-flight actions.** Make `click` always attempt a release; replace per-char typing
   with one `Input.insertText` where parity allows. Risks R4, R7.
4. **P4 — reconnect + bounded retry.** `CdpError::is_retryable()`; retry a dropped socket once with
   a fresh session for idempotent methods only (`evaluate`, `screenshot`, `dom_snapshot`); never
   auto-retry `click`/`type_text`/`navigate`. Risk R7.
5. **P5 — validate ws URL.** Require `ws://127.0.0.1`/`localhost` and the published port; pin the
   target `id` and re-resolve after reconnect. Risk R5.
6. **P6 — target epoch.** Track target id + browser start time; a change is a new epoch — fail fast
   rather than reuse a stale session. Risk R5.
7. **P7 — result correctness.** Read `exceptionDetails.exception`, add `awaitPromise: true`, surface
   `navigate`'s `errorText` as `Protocol`. Risk R3.
8. **P8 — heartbeats.** CDP/tungstenite ping-pong on an idle interval; mark dead sockets before the
   next call. Risk R1.
9. **P9 — observability + tests.** Structured spans and counters for calls, timeouts, reconnects and
   latency; fault-injection tests (mid-click drop, event trickle, stale target, browser restart)
   plus `proptest` for the `http_get` parser. Risks R1, R8.

## Risks / trade-offs

- R1. Heartbeats/retries must not double an in-flight action; re-audit cancellation safety if
  sessions are ever pooled. R2. Origin/token may be rejected by old Chromium — make opt-in and never
  log the ws URL or token. R3. Absolute deadline and stricter error text change latency/fixtures;
  keep the RPC body byte-identical or record a DEVIATION in docs/PARITY.md. R4. `insertText` is not
  equivalent to per-key events (no key handlers) — validate goldens and ask before shipping.
- R5. Target pinning/allowlist may need a spec decision for multi-tab deployments; default strict
  localhost, record in docs/STATE.md. R6. New `BOMA_*` keys need a typed config entry and ADR.
  R7. Retries risk duplicate clicks/keystrokes — restrict to read-only methods, add a test.
  R8. Metrics need no new dependency (reuse `tracing`). All edits in English; keep `cargo fmt
  --check`, `clippy -D warnings`, `req_*` tests green per step.

## Explicit non-goals

No CDP surface expansion, remote browsers, in-module browser restart, bypass of original
protections, connection pool (cancellation desync), or HTTP DTO changes.

## Suggested rollout

Wave A (revertable): P1, P2, P7, P8 + timeout/parser tests. Wave B: P3, P4, P5 with fault injection
(mid-click drop, event trickle, stale target) and differential checks vs the original. Wave C: P6,
P9 after the config ADR. Acceptance: no unbounded call, a drop never leaves a pressed mouse or
half-typed text, ws endpoints constrained to localhost with a pinned target, and restarts surface as
a distinct retryable signal. All facts CONFIRMED by reading this tree; GUESS items need the tests.
Nothing executed this pass.
