# 0274 — `crates/infra/src/cdp.rs` — reliability proposal (v0.4.0)

Worker #274 · module `crates/infra/src/cdp.rs` · lens: reliability · READ-ONLY review of
`rebuild-workspace/.worktrees/before-m5` (v0.3.0-era source). Proposals only; no code changed.
Claims: **CONFIRMED** = read in this tree this session; **GUESS** = inference needing a test.

Scope: this 387-line minimal CDP client — `Cdp`, `CdpError`, `connect`, `connect_once`, the six
tool methods, `call`, `http_get` — and its consumers: `crates/infra/src/computer.rs` (:91-181,
:480-514, one fresh session per tool call) and `crates/infra/src/lib.rs` (:41). Spec anchors:
AGENTS "timeout on every outbound call", "cancellation safety considered", typed config validated
at startup, no secrets in logs; module header promise "small and testable".

## What is already right (keep; do not re-propose)

- Both `connect` steps and the receive half of `call` are wrapped in `tokio::time::timeout`
  (CONFIRMED :101-122, :267-272); the connect loop has a 5 s budget and a fixed 200 ms pause
  (CONFIRMED :22-25, :67-83). `http_get` caps headers (64 KiB) and body (16 MiB) and reads exactly
  `content-length` to avoid Chromium's ignored `Connection: close` (CONFIRMED :326-381).
- Typed `CdpError` with no `unwrap`/`expect`/`panic` in this file (CONFIRMED); `Exception` is
  separated from transport failures and mapped to a Rule error by the adapter (CONFIRMED,
  computer.rs:123-131). Events with a non-matching id are discarded deliberately (CONFIRMED :292).
- The CDP port is bound to `127.0.0.1` on the host (CONFIRMED, computer.rs:247-249), not 0.0.0.0.

## Failure modes

### F1. Websocket drops leave half-executed actions and no recovery (high)

`call` returns `Transport(ConnectionClosed)` when the stream ends (CONFIRMED :273-277); there is no
reconnect, retry, or classification of transient vs permanent failure. `click` is two calls
(pressed then released, :187-213): a drop between them leaves the mouse *pressed* — state corruption,
not just an error (CONFIRMED shape, GUESS on a real mid-call drop). `type_text` issues one
`dispatchKeyEvent` per character (:220-232): a drop at character k leaves partial text with no
idempotency key or rollback, and multiplies failure probability by text length. No
ping/pong/heartbeat: a half-dead TCP socket is only discovered by the next `send`/receive timeout,
costing the full 10 s (GUESS — no live fault test this pass).

### F2. Unauthenticated CDP, trusted ws URL, arbitrary target (high)

The websocket URL is taken verbatim from the `/json/list` response and passed to `connect_async`
(CONFIRMED :113-122); no scheme/host/allowlist check. A misconfigured or compromised local
endpoint can redirect the client to another ws host. With no token and no Origin allowlist
configured here (CONFIRMED), any local process can attach to the same page target and navigate,
close, or inject input, making tool results nondeterministic. Target selection is
`.find(type == "page")` — the *first* page target, with no id pinning (CONFIRMED :107-112): a
restart that restores multiple tabs, or an unrelated tab opened by another actor, silently routes
every command to the wrong page (GUESS on a live multi-tab browser). Never add or bypass a
protection belonging to the original product; this concerns only Boma's own browser.

### F3. Timeouts are partial, per-frame, and internally inconsistent (high)

`send()` in `call` is **not** timed out (:263-265): a peer with a full send buffer can hang a call
forever, violating "timeout on every outbound call" (CONFIRMED). The 10 s receive timeout is
per-frame, so a browser that keeps emitting events resets it while never answering our id — the
call is unbounded (CONFIRMED). The two `enable` calls after the connect loop (:86-87) each use the
10 s timeout and are outside the 5 s connect budget, so `connect` can take ~25 s before reporting
`Unavailable`, contradicting its own doc comment (CONFIRMED). `TcpStream::connect` and the reads in
`http_get` rely entirely on the caller's timeout (CONFIRMED :313-380) — safe today, fragile if
reused. All constants are hardcoded (:14-25) with no typed, startup-validated config; the fixed
200 ms retry has no jitter, so many computers booting together retry in lockstep (GUESS on impact).

### F4. Process death / restart is indistinguishable from a generic outage (medium-high)

`Cdp` detects death only as a closed socket (CONFIRMED :273-277); there is no browser health check,
no restart signal, no target-epoch tracking. A crash/restart can keep the published host port while
invalidating the old target, creating a TOCTOU between `/json/list` and `connect_async` that
surfaces as a raw protocol/transport error (CONFIRMED race window). The adapter collapses every
`CdpError` into `AppError::Unavailable` (CONFIRMED, computer.rs:96-99, :106-110, :510-513), so a
caller cannot apply a "browser is rebooting, retry once" policy. There is no metric for reconnect
count, timeout rate, or call latency, so browser flapping is invisible (CONFIRMED absence).

### F5. Result-level reliability (medium)

`evaluate` reads the exception description from `result["result"]["description"]` (:141-146), but
CDP puts it in `exceptionDetails.exception.description` — diagnostics are likely empty/misleading
(CONFIRMED code, GUESS on the real frame). `returnByValue: true` without `awaitPromise: true`
(:134-139) returns a Promise object for async expressions — a silently wrong result (CONFIRMED
shape). `navigate` ignores `errorText` in the response (:177-180), so a refused navigation reports
success (CONFIRMED). `screenshot` trusts base64 and decodes without a decoded-size cap
(CONFIRMED :161-169).

## Concrete hardening (v0.4.0)

1. **P1 — absolute per-call deadline.** Put `send()` under `tokio::time::timeout` and give the whole
   `call` one `Instant` deadline used by every `next()`, replacing the per-frame timer; fold
   `enable` calls into the connect budget. Risk R3.
2. **P2 — typed config.** `CdpConfig { call_timeout, connect_budget, retry_pause, body_cap }` from
   `BOMA_*` env, validated at startup; add ±jitter to the retry pause. Risk R6.
3. **P3 — single-flight operations.** Replace the two `dispatchMouseEvent` calls with one
   press+release sequence guarded so a release is always attempted (or use a single CDP gesture),
   and replace per-character typing with one `Input.insertText` where parity allows. Risk R4/R7.
4. **P4 — reconnect + bounded retry.** Add `CdpError::is_retryable()`; retry a dropped/closed socket
   once with a fresh session for idempotent methods only (`evaluate`, `screenshot`, `dom_snapshot`);
   never auto-retry `click`/`type_text`/`navigate`. Risk R7.
5. **P5 — validate the ws URL.** Require `ws://127.0.0.1`/`localhost` and the published port, reject
   other schemes/hosts; pin the chosen target's `id` and re-resolve it after a reconnect. Risk R5.
6. **P6 — target epoch.** Record target id + browser start time in `/readyz`-style state; treat a
   change as a new epoch and fail fast instead of reusing a stale session. Risk R5.
7. **P7 — result correctness.** Read the exception text from `exceptionDetails.exception`, add
   `awaitPromise: true`, and surface `navigate`'s `errorText` as `Protocol`. Risk R3.
8. **P8 — heartbeats.** Send a CDP-level ping or rely on tungstenite ping/pong with a configurable
   idle interval; mark dead sockets before the next tool call. Risk R1.
9. **P9 — observability + tests.** Structured `tracing` fields and counters for calls, timeouts,
   reconnect count and latency; fault-injection tests (drop mid-`click`, trickle events, stale
   target, browser restart) plus `proptest` for `http_get`'s header/body parser. Risks R1, R8.

## Risks / trade-offs

- R1. Heartbeats/keepalive need care: dropped frames during a live call must not trigger a retry that
  doubles an action; cancel-safety must be re-audited if sessions are ever pooled.
- R2. An Origin/token header may be rejected by older Chromium; keep it opt-in behind config and
  never log the ws URL or any token (AGENTS: no secrets in logs).
- R3. An absolute deadline and stricter error text can change latency/fixtures; keep the RPC body
  shape byte-identical or record a DEVIATION in docs/PARITY.md.
- R4. `Input.insertText` is not equivalent to per-character key events (no per-key handlers); a
  swap is a behavior change — validate against the golden paths and ask before shipping.
- R5. Target pinning/allowlist may need a spec/config decision for deployments with multiple tabs;
  default to strict localhost and document the choice (DEC-style entry in docs/STATE.md).
- R6. New `BOMA_*` keys need a typed config entry and an ADR deviation from the hardcoded defaults.
- R7. Retries risk duplicate clicks/keystrokes; restrict them to read-only methods and add a test.
- R8. Metrics need no new dependency (use the existing `tracing` stack). All edits in English; keep
  `cargo fmt --check`, `clippy -D warnings` and all `req_*` tests green after each step.

## Explicit non-goals

No CDP command surface expansion, no attaching to remote/non-local browsers, no browser process
management/restart in this module, no auth bypass of any original protection, no connection pool
(with its cancellation-desync hazard), no change to HTTP tool DTOs.

## Suggested rollout

Wave A (local, revertable): P1, P2, P7, P8 + timeout/parser tests. Wave B: P3, P4, P5 with
fault-injection (mid-click drop, event trickle, stale target) and differential checks against the
original. Wave C: P6, P9 once the config ADR lands. Acceptance: no call can hang unbounded, a drop
never leaves a pressed mouse or half-typed text, ws endpoints are constrained to localhost with a
pinned target, and browser restarts surface as a distinct, retryable signal. All facts CONFIRMED by
reading this tree; timing/impact marked GUESS needs the tests above. Nothing executed this pass.
