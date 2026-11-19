# Upgrade proposal — `crates/infra/src/mcp_http.rs` (architecture lens)

- Worker: 295 | Wave: 13 | Target version: v0.4.0 | Language: English
- Scope: READ-ONLY analysis. No code changed by this worker.
- Evidence: `mcp_http.rs` (258 lines), `mcp_stdio.rs`, `net_safety.rs`,
  `app/src/ports.rs` (McpServerRecord / McpTransport), `spec/01-requirements.md`
  REQ-033, `spec/05-known-defects.md` D-05, `AGENTS.md`. All claims below are
  CONFIRMED from those files in this session unless marked GUESS.

## Current structure (CONFIRMED)

- One file, one struct `HttpMcpTransport { http: reqwest::Client }` (30 s client
  timeout, no redirects), implementing the `McpTransport` port (two methods).
- Private `rpc()` owns six concerns at once: URL extraction, D-05 outbound
  guard, request build + bearer, send, response decoding (JSON or SSE), and
  error mapping. `first_sse_message()` and `decode_tools()` are free functions;
  `decode_tools` is `pub(crate)` and reused by the stdio transport.
- `list_tools` = `initialize` then `tools/list` (two POSTs, result of
  `initialize` discarded); `call_tool` = a single `tools/call` POST. Every
  request uses the fixed id `1`; the response id is never checked. No
  `notifications/initialized`, no session header, no handshake validation.
- Only 3 unit tests, all for `first_sse_message`; no HTTP-level tests.

## Proposed upgrades (v0.4.0), 5 items

### 1. Consume the guard's verdict instead of re-resolving the URL
`check_outbound_url` returns `CheckedOutbound { host, port, resolved }` but
`rpc()` drops it and calls `reqwest.post(url)`, which resolves DNS a second
time. That leaves a validation/connection TOCTOU (DNS rebinding) window and
drops the audit data the guard was built to provide (D-05 requires an audit
event per private-endpoint use). Build the request against the validated
host/port/IPs (reqwest `resolve()` or a pinned address with a preserved
`Host`/SNI), and emit a structured `tracing` event carrying server id and
`allow_private` whenever the private path is used.
- Risk: pinning must keep TLS SNI and virtual-host `Host` correct; some
  deployments may rely on re-resolution (e.g. rolling DNS). Treat any
  behaviour change for private endpoints as a DEVIATION in `docs/PARITY.md`.
- Verify: wiremock test that the guard is consulted and that a private use
  produces an observable audit event; no real third-party traffic.

### 2. Typed transport errors; stop leaking server text to clients
All failures collapse into `AppError::Unavailable` or
`AppError::Rule(InvalidName(...))`; raw JSON-RPC `error.message` is forwarded
verbatim into a client-facing validation error, and HTTP status / JSON-RPC
codes are lost. Add a local `thiserror::Error` enum (`Guard`, `Transport`,
`Status { code }`, `BodyTooLarge`, `JsonRpc { code, message }`, `Protocol`)
and map it to one stable `AppError` at the adapter boundary; log the detail
(server id, status, code) with a request id, return a generic message.
- Risk: error strings are observable; existing callers or golden tests may
  assert them — audit before changing and record a DEVIATION if so.
- Verify: unit tests per variant plus a wiremock test asserting no server
  message reaches the client error.

### 3. Validate the MCP handshake and responses (bounded, per-server)
`initialize` is fire-and-forget and `call_tool` never initializes; the fixed
id `1` is not checked against the response. Validate the initialize result
and negotiated protocol version, send `notifications/initialized`, use a
monotonic request id, check the response id, and honour `Mcp-Session-Id`
when a server returns one (re-initialize on session expiry). Keep sessions
in a bounded, per-server cache with a TTL.
- Risk: the module header declares session-scoped servers out of scope, so
  this expands REQ-033 behaviour — it needs a decision in `docs/STATE.md`
  and a DEVIATION entry before shipping. Statefulness also breaks the current
  `Clone`-per-call simplicity: concurrent callers must not race to initialize,
  and sessions add memory and expiry paths.
- Verify: wiremock handshake test (initialize → notification → call with
  session id), id-mismatch test, concurrent-init test.

### 4. Streaming body limit and a correct SSE parser
`response.bytes()` buffers the whole body before the 1 MiB check, and
`first_sse_message()` parses each `data:` line independently (so a JSON
message split across `data:` lines is missed), ignores request id and `event:`
names, and uses lossy UTF-8. Read `bytes_stream()` with an incremental cap,
and parse SSE fields properly: join multi-line `data:` payloads, honour
`event: message`, skip comments/blank/`[DONE]`, require the matching id,
decode UTF-8 strictly.
- Risk: incremental capping changes when an error surfaces; joining data
  lines changes interpretation of unusual servers (GUESS: rare). More async
  code to review for cancellation safety.
- Verify: proptest over generated SSE streams (external-format parser rule),
  plus wiremock tests for oversized and malformed bodies.

### 5. Move shared protocol code out of the HTTP adapter
`decode_tools`, `PROTOCOL_VERSION` and the JSON-RPC envelope/error handling
belong in a neutral, IO-free module (domain or a `mcp_protocol` module), not
in the HTTP transport that stdio currently imports from. This removes the
stdio→HTTP coupling and lets both transports share one tested decoder.
- Risk: crosses a crate boundary (domain must stay pure, no async/IO); wider
  diff touching two transports. `decode_tools` behaviour must stay identical.
- Verify: keep and extend the existing decode tests; `cargo clippy -D warnings`.

## Cross-cutting notes

- Add wiremock integration tests under `tests/` with REQ-033 in the name
  (trace_check only counts declarations), and a proptest for each parser.
- Centralize the duplicated `PROTOCOL_VERSION` constant as part of item 5.
- Items 1–3 are externally visible (SSRF path, error text, session use): each
  needs a `docs/PARITY.md` DEVIATION and operator sign-off before shipping.
- No feature outside REQ-033 is proposed; session support (item 3) is the only
  scope expansion and is explicitly gated on a decision.

## Out of scope (kept as-is)

Stdio transport internals, OAuth token acquisition/refresh (`oauth.rs`),
per-space allow-list schema changes beyond consuming the guard verdict, and
any third-party network target.
