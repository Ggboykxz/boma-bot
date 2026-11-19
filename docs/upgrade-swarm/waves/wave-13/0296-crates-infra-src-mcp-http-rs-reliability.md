# Worker #296 — Reliability proposal for `crates/infra/src/mcp_http.rs` (v0.4.0)

**Lens:** reliability · **Status:** proposal (read-only audit) · **Target:** v0.4.0
**Source reviewed:** `crates/infra/src/mcp_http.rs` (258 lines) at `before-m5`.
All statements are CONFIRMED from that file unless tagged GUESS.

## Scope
`HttpMcpTransport` is the streamable-HTTP JSON-RPC client: one POST per
`initialize`/`tools/list`/`tools/call` call, a 30 s whole-call timeout, no
redirects, a 1 MiB body cap, the outbound guard (D-05), and optional bearer auth.
Session-scoped streams are explicitly out of scope (STATE.md). This proposal
keeps that contract and hardens failure handling without adding features (no
DEVIATION intended).

## Failure modes

### 1. SSE parsing (`first_sse_message`, lines 138-154)
- CONFIRMED: each `data:` line is parsed independently, so one JSON-RPC object
  split across multiple `data:` lines (valid SSE) is dropped and the call
  returns `Unavailable` — a silent lost answer.
- CONFIRMED: no `id` correlation — the first parseable JSON object wins even
  with a mismatched JSON-RPC `id` or a `notification`/`error` frame.
- CONFIRMED: `event:` type is ignored, so a non-`message` event carrying JSON
  can be mistaken for the reply.
- CONFIRMED: the `data:` prefix check is case-sensitive; SSE field names are not.
- CONFIRMED: `String::from_utf8_lossy` hides invalid UTF-8 instead of failing.
- CONFIRMED: the whole SSE body is buffered by `response.bytes()` before parsing,
  so a server that keeps the stream open holds the slot for the full 30 s.

### 2. Timeouts
- CONFIRMED: a single 30 s `Client::timeout` covers connect+headers+body; there
  is no separate connect timeout, so a stalled connect looks like a slow call.
- CONFIRMED: no body/stream deadline beyond the global one; a slow-loris SSE
  response occupies one call for up to 30 s.
- CONFIRMED: every transport, TLS and timeout failure collapses into
  `AppError::Unavailable` (lines 89-92, 100, 114) — no classification and no
  capture of `Retry-After`.

### 3. Session state
- CONFIRMED: the `initialize` result is discarded (line 208); any
  `Mcp-Session-Id` response header is ignored and never echoed back on
  `tools/list` or `tools/call`.
- GUESS: spec-compliant servers that require that header will reject the second
  call with 400/404, which today surfaces as a validation-looking
  `AppError::Rule(InvalidName("...status..."))` — misleading and non-retryable.
- CONFIRMED: `PROTOCOL_VERSION` is pinned to `2024-11-05`, with no fallback.

### 4. Reconnect
- CONFIRMED: there is no retry or backoff anywhere; one transient 5xx, timeout
  or connection reset fails the whole `list_tools` (two POSTs) or `call_tool`.
- CONFIRMED: `initialize` and `tools/list` are independent POSTs, so a failure in
  the second leaves no resumable state and forces a full re-initialize.
- CONFIRMED: the hard-coded `id: 1` (line 79) gives no protection against
  replayed or interleaved responses.

## Concrete hardening (v0.4.0)
1. Stream SSE with `bytes_stream()` and stop at the first complete message;
   accumulate the `data:` lines of each event, join them with `\n`, then parse
   once with `serde_json::from_str`. Require the response `id` to match the
   request and accept only `result`/`error` frames. Match field names and the
   `data:` prefix case-insensitively. Fail on invalid UTF-8.
2. Split timeouts: `.connect_timeout(5s)` plus the 30 s total, and wrap body
   streaming in a bounded `tokio::time::timeout` (e.g. 10 s to the first
   message) so a stalled stream cannot ride the coarse client-level clock.
3. Enforce the 1 MiB cap as a streaming budget: abort as soon as accumulated
   bytes exceed it, never buffering past the limit. Treat `202 Accepted` with
   no body as a valid notification-style response.
4. Capture and echo `Mcp-Session-Id`: store it from `initialize`, send it on
   later calls, and re-initialize once (single bounded retry) if a call is
   rejected with 400/404. Keep this opt-in per server to respect the documented
   out-of-scope decision; flag in STATE.md as GUESS pending spec confirmation.
5. Introduce a typed `McpTransportError` (`Timeout`, `Transport`, `Tls`,
   `HttpStatus(code)`, `Protocol(msg)`, `TooLarge`) mapped to `AppError`
   variants, so a 5xx is not mislabelled as a client validation error. Log via
   `tracing` with server name, method, status and elapsed — never the bearer or
   session id.
6. Add bounded retry for idempotent calls (`initialize`, `tools/list`): two
   attempts, exponential backoff with jitter (e.g. 100/300 ms), retrying only
   on timeouts, connection resets and 429/502/503/504 while honoring
   `Retry-After`. Do NOT auto-retry `tools/call` (side effects) — leave that to
   the caller. Give each attempt a fresh monotonic JSON-RPC `id` and verify it.
7. Tests first, with REQ-033 in the names: multi-line `data:` event; mismatched
   id rejected; non-`message` event ignored; case-insensitive fields; oversized
   body aborts early; `202` accepted; session id echoed plus one re-initialize;
   status classification; retry on 503 with `Retry-After`, none on `tools/call`.
   Use `wiremock` for HTTP behavior and `proptest` for the SSE parser.

## Risks
- Items 4 and 6 alter externally visible behavior and likely need a DEVIATION
  entry or explicit spec confirmation before shipping (STATE.md decisions).
- The streaming SSE parser is more complex and could regress the three passing
  unit tests; mitigate with those tests plus the new ones above.
- Retries can amplify load on a struggling server; keep attempts low and honor
  `Retry-After`.
- Capturing headers must never log the bearer or session id (secrecy gate).
- New reqwest configuration options must be checked against the installed
  reqwest API on docs.rs (per AGENTS.md) — do not type APIs from memory.
