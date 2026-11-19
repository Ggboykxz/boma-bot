# Worker 0154 — `crates/api/src/screen.rs` — Reliability (v0.4.0 proposal)

Scope: `screen_target` handler only. Evidence tags follow AGENTS.md — CONFIRMED = read in
this session, GUESS = inferred runtime behaviour. No code was changed (auditor role).

## Failure modes

**FM-1 — Missing frame / "asset" passes as success (CONFIRMED).** `screen_frame` returns
`Vec<u8>`; the handler base64-encodes it unconditionally (lines 84-87). There is no
check for zero length or for the PNG signature (`\x89PNG\r\n\x1a\n`). A backend that
returns empty or corrupt bytes still yields `200` with `event: frame` and a non-empty
`pngBase64` (empty bytes encode to `""`). The client renders a broken/absent image while
the API reports success: a silent failure, the root of a client-side retry/fallback loop
that can never converge because the server never signals a retryable cause.

**FM-2 — Backend/DB failures are disguised as 403 (CONFIRMED).** Line 86 collapses every
`screen_frame` error with `map_err(|_| ApiError::forbidden())`. `AppError::Unavailable`
(no computer backend, `provider()=="none"`, missing `external_id`, screenshot command
failure — usecases.rs 6346-6365) becomes `403`, not `503`. A client cannot distinguish
"re-mint the capability" from "backend down, retry later"; it will spin minting fresh
capabilities against a broken backend. Risk: capability-minting retry storm / fallback
loop. Note the handler's own rustdoc (line 50) promises `503` for this case — code and
docs already disagree.

**FM-3 — Inconsistent semantics between two sequential backend reads (CONFIRMED).**
`resolve_screen_capability` uses `?` (line 80), so its `AppError::Unavailable` maps via
`From<AppError>` to `ApiError::unavailable()` = `503` (error.rs 164-170, 211). The very
same DB outage at the *frame* step is a `403` (FM-2). The status therefore depends on
which of two adjacent calls fails, which is non-deterministic under partial outages.

**FM-4 — No timeout on the screenshot call (CONFIRMED).** `runtime.screenshot(...).await`
(usecases.rs 6365) is unwrapped with no `tokio::time::timeout`. A hung backend pins the
request and its connection forever, violating the AGENTS.md "timeout on every outbound
call" rule. GUESS: under load this cascades into pooled-connection exhaustion and a
readiness/liveness false-positive, since the request never completes to be counted.

**FM-5 — Unbounded response memory (CONFIRMED).** Only the *request* body is capped at
16 KiB (line 72); the frame size is uncapped. The whole PNG is buffered, base64-expanded
(~1.33x) and copied again into the `format!` SSE `String` (lines 87-91). A large or
hostile backend frame can spike memory per request. GUESS: concurrent large frames are a
credible OOM/degradation vector on a low-end host.

**FM-6 — Generation metadata can go stale (TOCTOU, CONFIRMED code path).** Resolution
checks `computer.screen_generation == capability.screen_generation` (usecases.rs 6328),
but `screen_frame` re-fetches the computer and screenshots it separately. The `meta`
event reports `capability.screen_generation` (the mint-time value, line 90), while the
PNG may be from a later generation if the screen advanced between the two calls. The
module doc's promise that a client can compare `meta` to its capability is then false;
a strict client may loop retrying forever on the mismatch. GUESS: narrow race, but
exactly at the moment screens change most (active use).

**FM-7 — No observability on failure (CONFIRMED).** The handler emits no `tracing`
events; the `map_err(|_| ...)` also discards the underlying error. FM-2/3 are therefore
invisible in logs, delaying detection of a backend outage.

## Concrete hardening (with risks)

1. **Validate the frame before encoding.** Reject empty buffers and require the 8-byte PNG
   magic; bound the frame to a documented max (e.g. 8 MiB). On failure return a retryable
   error, never a fake `200`. *Risk:* a backend emitting a valid-but-unusual PNG wrapper
   would be rejected; pin the exact signature check and add a fixture test. Purely
   additive behaviour — no contract change for valid frames.

2. **Preserve the retryable/permanent distinction.** Stop flattening `screen_frame` errors
   to 403: map `AppError::Unavailable` to `ApiError::unavailable()` (503) while keeping
   `403` for genuine capability failures, and add a `tracing::warn!` carrying the error
   kind (never the token). *Risk / parity gate:* `spec/openapi.yaml` lists only 200/403/413
   for this operation. Returning 503 is an externally visible DEVIATION and needs an ADR,
   a spec/openapi.yaml edit and owner approval before shipping (per AGENTS.md). Until then,
   the parity-safe interim is to keep 403 on the wire but log the true cause and record
   the deficiency in docs/PARITY.md — this fixes diagnosability (FM-7) without changing
   the contract.

3. **Wrap the screenshot in a timeout** (`tokio::time::timeout`, bounded e.g. 5 s) and map
   the elapsed case to the same retryable path as FM-2. *Risk:* a legitimately slow capture
   on a loaded VM gets cut off; make the duration configurable in `HttpConfig` and test
   the boundary. Internally invisible on success.

4. **Cap frame memory.** Stream or pre-check the length before base64, and build the SSE
   body without a second full copy. *Risk:* larger refactor of the response path; verify
   the exact SSE bytes (golden) are unchanged for frames under the cap.

5. **Make generation metadata honest.** Re-read the generation atomically with the frame
   (extend the port to return `(generation, png)`), or re-validate after capture and fail
   retryably if it moved. *Risk:* touches `ports.rs`/`usecases.rs`/infra adapters and is a
   wire-visible field change in the `meta` event → DEVIATION + approval. Lower-risk
   alternative: keep the field but document that it is mint-time, so clients stop
   treating a mismatch as a permanent error.

6. **Tests first (REQ-030 naming):** empty/corrupt frame → non-200; oversized frame →
   bounded; screenshot timeout → retryable/logged; `provider()=="none"` → documented
   status; generation race → `meta` matches captured frame. Extend the contract test so
   the generated OpenAPI document and `spec/openapi.yaml` stay in lockstep after item 2.

## Recommended order for v0.4.0

Ship 1 + 3 + 6 and the logging half of 2 first (no wire change). Treat the status-code
change (2 full), the frame-size cap surface (4) and the generation field change (5) as
DEVIATION candidates requiring approval in the same release train.
