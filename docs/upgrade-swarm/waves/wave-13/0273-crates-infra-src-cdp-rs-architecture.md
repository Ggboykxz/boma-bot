# Wave 13 — Worker 0273 — `crates/infra/src/cdp.rs` (architecture) — v0.4.0 proposal

Scope: read-only architecture review of `crates/infra/src/cdp.rs` (387 lines) and its single consumer `crates/infra/src/computer.rs`. All claims below are CONFIRMED by direct reading unless marked GUESS. No behavior change is proposed unless explicitly noted. Nothing else was modified.

## Current bridge structure (CONFIRMED)

The file is a flat, single-struct CDP adapter with four concerns interleaved in one namespace:

- **Discovery**: `connect_once` fetches `http://127.0.0.1:{port}/json/list` via a hand-written HTTP/1.1 client (`http_get`), picks the first `type == "page"` target, reads `webSocketDebuggerUrl`.
- **Transport**: `Cdp { ws: WebSocketStream<MaybeTlsStream<TcpStream>>, next_id: u64 }`, `tokio-tungstenite`, `futures_util` sink/stream.
- **Protocol framing**: private `call` increments `next_id`, sends a JSON message, loops until a frame with matching `id` arrives, and deliberately drops every event frame.
- **Domain API**: `evaluate`, `screenshot`, `navigate`, `click`, `type_text`, `scroll` — each a thin wrapper building `json!` params.

Policy is hard-coded as module constants: call timeout 10 s, connect budget 5 s, retry pause 200 ms. Errors are one `thiserror` enum (`Transport(Box<..>)`, `Protocol(String)`, `Exception(String)`, `Timeout`); `computer.rs` maps every variant to `AppError::Unavailable` except `Exception`, which is inspected specially (line 123). `lib.rs` re-exports `Cdp`/`CdpError`; the only consumer is `DockerComputerRuntime::cdp` (resolves the published host port from Docker inspection, then `Cdp::connect(host_port)`). `connect` enables `Page` and `Runtime` and returns a session that is fresh per tool call — an explicit isolation choice documented in the type's rustdoc.

## What is fragile (CONFIRMED)

- **No seam for testing or substitution**: `computer.rs` names `Cdp` concretely; a fake browser cannot be injected, so the Docker-to-CDP path has no unit-level differential coverage.
- **A second, home-grown parser lives in the driver**: `http_get` manually parses the request line, `Host`, `\r\n\r\n`, `content-length`, chunked reads and a 16 MB cap. It is an external-format parser inside a protocol adapter, with no fuzz/property tests here.
- **Transport/protocol/domain not separated**: `call` owns ID allocation, timeout, send, receive, error translation and event dropping in one loop; adding CDP sessions, event subscriptions or a second target means editing this one method.
- **Policy cannot be configured or tested**: host is fixed to `127.0.0.1`, budgets are constants, and the fresh-per-call lifecycle is implicit rather than a named policy.
- **Untyped boundary and lossy errors**: methods take/return `serde_json::Value`; `Protocol(String)`/`Exception(String)` lose the method name and CDP error code, which is why the consumer must string-match `Exception`.
- **I/O shape**: `type_text` performs one websocket round trip per character and `scroll` hard-codes origin `(100, 100)` — efficiency and geometry concerns embedded in the domain layer.

## Proposed upgrades for v0.4.0

1. Split the flat file into a `cdp/` module tree behind a facade.
   - `cdp::discovery` (`http_get`, target selection, `Target` newtype), `cdp::transport` (websocket + IDs + timeout), `cdp::protocol` (request/response envelope and `call` matching), `cdp::domains` (Page/Runtime/Input operations), `cdp::error`. Keep `cdp.rs` as a `pub use` facade so `crate::Cdp` paths still compile.
   - Risk: LOW — mechanical, no behavior change; preserve every REQ-tagged rustdoc verbatim. One module per commit, compile between steps.

2. Introduce a client trait at the computer boundary for injection.
   - Define `BrowserBridge` with the six operations and implement it for `Cdp`; have `DockerComputerRuntime::cdp` return `impl BrowserBridge` / `Arc<dyn BrowserBridge>` so tests can supply an in-memory fake. Keep the trait in `infra` if only `computer.rs` consumes it; if a use case must fake it, place the port in `app::ports` (GUESS — layering decision for the architect).
   - Risk: LOW-MEDIUM — one consumer (CONFIRMED), so signature churn is contained; no wire format changes.

3. Move HTTP discovery behind a maintained client and make the parser testable.
   - Replace hand-rolled `http_get` with a properly added HTTP dependency (cargo add, docs.rs API check, `docs/DEPENDENCIES.md`, cargo-deny), keeping the exact `/json/list` request shape Chromium accepts. Move target selection into a pure function `select_page_target(&[Value]) -> Result<&str, CdpError>` with property/fuzz tests for the external JSON format.
   - Risk: MEDIUM — new dependency and license check; a request-shape drift would break discovery, so keep the curl-compatible headers until an oracle test proves equivalence.

4. Make policy and session lifecycle explicit and configurable.
   - Add `CdpConfig { host, port, connect_budget, call_timeout, retry_pause }` (typed durations, validated) and a named `SessionPolicy` whose default remains `FreshPerCall` (the current, deliberate isolation). Pooling/reuse becomes an opt-in variant, not an accident.
   - Risk: MEDIUM — touches `Cdp::connect` and the consumer's call site; must preserve the current failure semantics (down computer → `Unavailable`, 5 s ceiling) or record a DEVIATION.

5. Type the boundary and stop dropping protocol information.
   - Give `call` a method-name parameter in errors; split `CdpError` into structured variants (`Protocol { method, code, message }`, `Exception { method, text }`, …) so `computer.rs` no longer string-matches. Replace raw `Value` returns with small typed results (`EvaluateResult`, screenshot bytes wrapper) and let `call` expose events through a bounded channel instead of discarding them.
   - Risk: MEDIUM-HIGH — touches every domain method and the consumer's error mapping; the event channel must be bounded and drained to avoid backpressure stalls. Wire format unchanged, so no external DEVIATION if signatures stay internal.

## Sequencing and gates

Order: 1 → 2 → 3 → 4 → 5 (cheapest and purely internal first; error/wire-shape changes last). Each step: `cargo fmt --check`, clippy `-D warnings`, then tests; add a mock DevTools endpoint in `tests/` for discovery and `call` behavior. Any externally visible change (config env names, API errors) goes to `docs/PARITY.md` for approval before shipping. Strict deserialization and rustdoc on public items follow the workspace charter throughout.
