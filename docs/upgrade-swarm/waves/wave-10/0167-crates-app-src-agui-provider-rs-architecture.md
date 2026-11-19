# AgUiProvider — v0.4.0 Architecture Proposal (worker #167)

- **Module:** `crates/app/src/agui_provider.rs` (195 lines, read this session).
- **Lens:** architecture / structure.
- **Requirement:** REQ-059 (external agents over standard agent protocols, MUST); see ADR-026.
- **Status convention:** observations are CONFIRMED by reading this file, `crates/app/src/ports.rs`,
  `crates/app/src/worker.rs`, `spec/01-requirements.md` and `docs/adr/026-external-agents.md` this
  session; numbered items are forward-looking PROPOSALs, not verified behaviour.

## Current structure (CONFIRMED)

Two public structs (`AgUiExecutor` config/bearer, `AgUiProvider` = reqwest client + `Arc<executor>`),
three private free functions (`agui_body`, `parse_agui_stream`, plus `new`), and one `ModelProvider`
impl. The whole external-protocol stack — JSON assembly, SSE parsing, HTTP transport, auth and
error mapping — lives in the `app` crate, while other outbound adapters sit in `infra`
(`crates/infra/src/provider.rs`, `openai.rs`). Per call: one POST, whole body buffered, whole reply
parsed at once, then a `ProviderTurn` of chunks or tool calls. The bearer is pulled from the vault
in `worker.rs` and defaults to an **empty** secret when the named secret is missing. Notably:
`reqwest::Client::new()` has no timeout, the body is unbounded, every failure collapses to
`AppError::Unavailable`, malformed tool args are fabricated as `{"arguments": ...}`, and each
`TOOL_CALL_ARGS` event pushes a *separate* `ToolCallRequest` instead of accumulating deltas by id.
`ProviderRequest` carries `run_id`, memory and skills, but `agui_body` sends none of them, despite
ADR-026 requiring them. No in-module tests exist; wire tests live in `crates/testkit/src/build.rs`.

## Proposed upgrades

### 1. Split transport from protocol; make the codec pure and typed (PROPOSAL)
Keep `AgUiProvider` as the thin `ModelProvider` adapter, but move the reqwest/HTTP layer behind an
infra boundary and lift `agui_body`/`parse_agui_stream` into a pure `agui` codec module exposing a
typed `AgUiEvent` enum and `decode_stream(&str) -> Result<Vec<AgUiEvent>, AppError>` +
`reduce(Vec<AgUiEvent>) -> ProviderTurn`. The transport then only does POST + status + text.
This restores the repo's layer rule (app = use cases/ports, infra = adapters) and makes the
external-format parser unit-testable without a socket.
- **Risk:** changes crate dependency direction and the `worker.rs` construction site; keep the
  public `agui_provider` module path re-exported so callers do not break. A crate move may need an
  ADR; do it as one commit with the REQ-059 wire tests as the regression gate.

### 2. Correct tool-call assembly with a per-id state machine (PROPOSAL)
Replace the flat `open_ids: Vec<(String, String)>` and one-push-per-ARGS loop with a
`HashMap<tool_call_id, {name, args: String}>` driven by START / ARGS / END: append each ARGS
`delta`, parse JSON once at END, emit one `ToolCallRequest` per id. Reject a missing START, a
duplicate id, or invalid JSON with a typed error rather than the `unwrap_or_else` fabrication.
- **Risk:** this is a defect fix, not a deviation, but stub expectations in `testkit` that assume
  one call per delta must be updated together; confirm the original executor's delta granularity
  with a differential capture before changing the wire behaviour. Unknown event types must stay
  ignored for forward compatibility.

### 3. Harden the single outbound call (PROPOSAL)
Give the client a connect + total timeout comfortably below the run lease/timeout, cap the response
body, classify failures (transport / non-2xx / malformed SSE / `RUN_ERROR` / timeout) into a small
typed error, and surface a legible, safe reason per REQ-059's second criterion. Add a bounded retry
with backoff only for connection-level idempotent cases, and a request id in logs.
- **Risk:** POST is not inherently idempotent — retrying a half-completed turn could duplicate
  effects; default to no retry or require an idempotency key from the executor. Timeout/retry
  values become new `BOMA_*` config, so validate at startup; never log the bearer, the full URL
  credentials, or tool arguments in warnings.

### 4. Complete the protocol payload and fail fast on config (PROPOSAL)
Add `runId`, `userId` and the memory/skills digest to `agui_body`, carry tool results back as tool
messages, and change the bearer from `SecretString::from("")` fallback to `Option<SecretString>`
with a hard error at construction when an external bot has no usable credential. Validate
`base_url` as an absolute http(s) URL with no userinfo at startup, and prefer the repo's rustls
TLS stack over OpenSSL defaults.
- **Risk:** this changes the executor-visible wire format — a DEVIATION entry and owner approval
  are required before shipping; verify stub/adapter executors ignore unknown fields. Stricter URL
  validation must not reject endpoints the current build accepts, and a missing secret must fail
  closed with a legible reason, not silently downgrade to an unauthenticated call.

### 5. Give the external-format parser real coverage (PROPOSAL)
Add property/fuzz tests for `decode_stream` (arbitrary bytes, partial lines, interleaved events,
huge deltas — assert no panic and a typed outcome) plus golden wire tests naming REQ-059: happy
chunk stream, multi-delta tool call + approval round-trip, unreachable endpoint, non-2xx,
malformed SSE and `RUN_ERROR`. Keep every assertion in test names so `tools/trace_check.py` sees
the id, and add a benchmark for the large-stream path.
- **Risk:** fuzz harness and golden fixtures cost time and can go flaky if they depend on a live
  clock; seed them deterministically and keep them behind the existing testkit. Do not weaken any
  gate to make coverage numbers pass.

## Preserved invariants / non-goals

- One HTTP call per turn, no second run loop; run lifecycle, approval gate and memory stay ours.
- The AG-UI thread is our thread (thread id preserved); no remote thread bookkeeping.
- Bearer comes from the vault at call time, is never persisted, logged or sent to any other client.
- No externally visible API/data-format change without a DEVIATION entry and owner approval.
- Keep the frozen REQ-001..058 floor untouched; REQ-059 traceability stays in test declarations.
