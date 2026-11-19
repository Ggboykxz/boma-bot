# Wave 10 — worker 0168 — `crates/app/src/agui_provider.rs` — reliability (v0.4.0)

Target: `AgUiProvider`, the stateless adapter that POSTs an AG-UI run to an external executor and
coarses its `text/event-stream` reply into a `ProviderTurn` (REQ-059). Scope: stream errors,
protocol drift, retry. This is a proposal only; no code was changed.

All findings below are **CONFIRMED by reading** the module, `ports.rs` (`ModelProvider`,
`ProviderTurn`, `ToolCallRequest`, `ProviderRequest::run_id`), `error.rs` (`AppError::is_retryable`)
and `crates/app/Cargo.toml`, unless marked **GUESS**.

## Failure modes

**F0 — Hard contract.** REQ-059 requires: "endpoint unreachable or protocol malformed ... the run
fails with a legible reason and nothing is fabricated into the room". Today malformed/truncated
input frequently becomes a *successful* empty or partial turn, i.e. the opposite. This is the
reliability bug to fix first.

**F1 — No deadline on the outbound call (P0).** `reqwest::Client::new()` (l.40-45) plus `.send()` /
`.text().await` (l.181-192) carry no timeout and no connect timeout. One hung executor holds a run
worker forever; with a bounded worker pool the whole queue stalls. Violates AGENTS.md ("a timeout
on every outbound call").

**F2 — Whole-body buffering, no cap (P1).** `response.text().await` (l.192) accumulates the entire
SSE body with no byte limit and no idle-read timer. A chatty, infinite or drip-feeding executor
causes unbounded memory growth and an unbounded wait. `reqwest` in `Cargo.toml` l.27 has features
`["rustls", "json"]` only — `bytes_stream()` is not available yet.

**F3 — Truncated stream accepted as success (P0).** `parse_agui_stream` (l.100-162) never requires
a terminal event: there is no `RUN_FINISHED`, `TEXT_MESSAGE_END` or `TOOL_CALL_END` check. A
connection cut mid-stream returns `Ok(ProviderTurn::Chunks(partial))`; an empty stream returns
`Ok(Chunks(vec![""]))` (l.161). Both are fabricated answers and break F0.

**F4 — Corrupt frames silently dropped (P1).** Malformed JSON data is skipped (`Err(_) => continue`,
l.108-111) and unknown event types ignored (`_ => {}`, l.155) with no counter, metric or error.
Protocol drift (a renamed `delta`/`toolCallId` field) degrades a reply to an empty success instead
of a legible failure. `RUN_ERROR` (l.151-154) is the only explicit failure and discards its detail.

**F5 — Tool-call assembly is wrong (P0).** Each `TOOL_CALL_ARGS` event pushes a *new*
`ToolCallRequest` (l.140-149) and tries `serde_json::from_str` on a single fragment, falling back to
`{"arguments": "<fragment>"}`. AG-UI streams argument deltas that must be concatenated until
`TOOL_CALL_END` before one parse. Effects: duplicate calls with duplicate ids, wrong name when
ARGS precedes START (`ok_or(AppError::Unavailable)`, l.146), corrupt arguments reaching the
dispatcher/approval gate (REQ-016/017), and a START with no ARGS is silently dropped. `open_ids`
(l.103, l.129) is never removed, so reused ids can bind to a stale name.

**F6 — SSE framing too strict (P2).** `strip_prefix("data: ")` (l.105) requires the space; SSE
permits `data:` with no space and multiple consecutive `data:` lines joined by `\n`. Comments and
`event:`/`id:`/`retry:` fields are ignored — acceptable, but multi-line data currently gets treated
as two separate events.

**F7 — Error classification blocks sensible retry (P1).** Every failure collapses to
`AppError::Unavailable` (l.124, 135, 138, 146, 153, 185, 190, 192). `error.rs` l.49-51 makes only
`Unavailable` retryable, so a 401/403 (bad bearer) or 400 (bad body) is retried by REQ-024/012 the
same way as a 503. Conversely the response body with the diagnostic is discarded (l.188-191), so
the run failure reason is not legible (F0).

**F8 — Retry placement / idempotency (P1, GUESS).** `complete()` has no retry and should not gain a
naive one: a retried turn can re-trigger remote write effects through tool calls. `ProviderRequest`
already carries `run_id` (ports.rs l.4099) but `agui_body` ignores it; sending it as an idempotency
key would let the engine's retries dedupe remotely. Whether the target executor honours such a key
is unknown — confirm before relying on it.

**F9 — Secret lifetime / log hygiene (P2).** `format!("Bearer {}", expose_secret(...))` (l.173-178)
allocates an unzeroized `String` per call; `tracing::warn!(?event, ...)` on `RUN_ERROR` (l.152) logs
the whole event, which may echo arguments. Adjacent to reliability but cheap to fix.

**F10 — No startup validation (P2).** `base_url` is any string; `AgUiProvider::new` never validates
it, so a malformed URL only surfaces as `Unavailable` at run time, not at config load.

## Hardening plan (with risks)

1. **Build a configured client**: `ClientBuilder::timeout`, `connect_timeout`, bounded pool, and a
   User-Agent; add the `stream` reqwest feature. Risk: a fixed overall timeout may be too short for
   long agent runs — make it configurable (`BOMA_*`) with a conservative default.
2. **Stream incrementally** with `bytes_stream()`: accumulate bytes against a hard cap (e.g. 8 MiB)
   and reset an idle-read timeout on each chunk. Abort with an error past the cap. Risk: needs the
   new feature and a careful line-buffer that keeps a trailing partial `data:` frame at EOF.
3. **Make the parser a state machine** with an explicit terminal state: require `RUN_FINISHED`
   (fallback: `TEXT_MESSAGE_END` / all tool calls `END`) before returning `Ok`. Key open calls by
   `toolCallId`: START stores name, ARGS concatenates deltas, END parses once and finalizes; finalize
   any still-open call at end of stream. Risk: executors that omit `RUN_FINISHED` would now fail —
   verify against the real executor; if compatibility is required, gate leniency behind a config
   flag and record it as a DEVIATION in `docs/PARITY.md`.
4. **Fail on drift**: count malformed data frames and unknown event types above a small threshold;
   log bounded, redacted diagnostics (never the bearer, never full arguments) and emit a metric.
   Risk: legitimate non-JSON SSE sentinels need an explicit allow-list.
5. **Classify errors**: 408/429 and 5xx/network/timeouts → `Unavailable` (retryable); other 4xx →
   a non-retryable fault; include a short, bounded status/body excerpt in the run failure reason.
   Risk: a new `AppError` variant touches the API error envelope — prefer reusing an existing
   client-fault variant unless a distinct public reason is required.
6. **Keep retries at the run layer** (REQ-024/012); add the `run_id` idempotency key if, and only if,
   the executor supports it. Risk: see F8.
7. **Tests (test-first, names carry REQ-059)**: proptest/fuzz the SSE parser over random frames,
   truncated streams, reordered START/ARGS/END, duplicate ids and malformed JSON (AGENTS.md requires
   property/fuzz tests for every external-format parser); wiremock cases for timeout, 401, 503,
   `RUN_ERROR`, oversized body and slow drip; golden parity test for a well-formed multi-tool turn.

Priority order: F3, F5, F1 → F2, F4, F7 → F6, F8, F9, F10.
