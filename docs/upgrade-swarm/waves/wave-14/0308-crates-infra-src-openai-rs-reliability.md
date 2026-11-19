# 0308 — `crates/infra/src/openai.rs` — reliability proposal (v0.4.0)

Worker #308 · module `crates/infra/src/openai.rs` · lens: reliability · READ-ONLY review of
`.worktrees/before-m5`. Proposals only; no code changed. Claims: **CONFIRMED** = read in this tree
this session; **GUESS** = inference needing a test. Anchors: REQ-015, REQ-016, REQ-020, REQ-021,
REQ-024 (`spec/01-requirements.md:120-135,161-175,196-203`), `error.rs:43-66`, `worker.rs:436-467`,
`ports.rs:4049-4100`, `tests/openai_adapter.rs:245-296`, AGENTS.md ("timeout on every outbound
call", "typed errors", "config validated at startup", "failures are user-legible").

## Scope note

This is a one-shot BYOK adapter, not a streaming decoder: the request pins `"stream": false`
(line 179) and `complete()` buffers one `chat/completions` response. "Stream truncation" here means
**output-token truncation of a non-streamed reply** (`finish_reason: "length"`), plus refusal/empty
content; incremental client streaming is handled downstream by the worker (REQ-020). The adapter is
reliability-critical because every failure is flattened to one retryable variant, so the worker's
retry gate (`worker.rs:437-462`) cannot distinguish a 429 from a bad key.

## What is already right (keep; do not re-propose)

- Key is a `SecretString`, used only in the `Bearer` header (line 244), and excluded from `Debug`
  (lines 31-39): no secret logging (CONFIRMED).
- A 60 s total timeout exists on the client (lines 55-57), satisfying the baseline "timeout on
  every outbound call" (CONFIRMED).
- Malformed tool arguments fail the turn as `Unavailable`, not a panic (lines 203-209; see
  `req_015_malformed_tool_call_arguments_fail_the_turn_not_the_process`); id-less tool results are
  downgraded to a user note, not a malformed tool message (lines 145-155) (CONFIRMED).

## Failure modes

### F1. Truncation, refusals and empty content are silent successes (high, CONFIRMED)

`parse_message` never reads `finish_reason` (lines 184-229) although the module doc claims it is
"translated" (lines 5-6). A `finish_reason: "length"` reply is chunked and returned as a normal
`Chunks` turn (lines 218-228); a truncated `tool_calls` array is returned as complete `ToolCalls`
(lines 216). A provider refusal (`message.refusal`) is likewise ignored: null `content` becomes
`Chunks(vec![""])` (lines 218-226) — an empty reply that looks like a finished answer. REQ-021 says
failures must surface as a status plus a reason, "never as a silent stop"; this is exactly that.

### F2. Every status collapses to the one retryable error (high, CONFIRMED)

Lines 254-258 map **all** non-2xx (429, 401, 403, 400, 404, 5xx) to `AppError::Unavailable`.
`is_retryable()` matches only `Unavailable` (`error.rs:49-51`), so bad credentials, a malformed
request and a pinned-model 404 (`REQ-015`'s "model error rather than ... different model silently")
are all retried with backoff until the routine pauses (`worker.rs:457-462`): wasted work and delayed
legibility. Conversely, 429 carries no `Retry-After` signal, so the generic REQ-024 backoff may
fire before the provider's window resets and hammer the limit. `new()` and JSON-decode failures are
also `Unavailable` (lines 58, 250-253, 259-262), conflating config/startup faults with transient ones.

### F3. Tool-call assembly loses correlation and arguments (high, CONFIRMED)

Missing `id` defaults to the literal `"call"` (lines 192-196): N id-less calls collide, and because
`request_body` correlates results by `id` (lines 146-151) the next turn's results are misrouted.
`arguments` is read only via `as_str()` with `"{}"` fallback (lines 203-206); a compatible provider
returning an already-decoded object silently drops all arguments and invokes the tool empty-handed.
`name` defaults to `""` (lines 198-202), reaching the gate as an unnameable call. Parsing is also
all-or-nothing across the array (lines 190-215): one bad call discards valid siblings, with no log
naming the offending index. None of this is checked against `finish_reason == "tool_calls"`.

### F4. Coarse timing and unbounded response buffering (medium, CONFIRMED)

One blanket 60 s deadline (lines 55-57): no `connect_timeout`, so a black-holed SYN can consume the
whole budget; no separate body deadline or elapsed-time log. `response.json::<Value>()` (lines
259-262) buffers an unbounded body — a misbehaving compatible provider can force a large allocation
and hold the budget. No validation that `endpoint` is an absolute URL: a typo fails as 60 s timeouts
per request rather than fail-fast startup config (AGENTS.md). `choices.first()` only (lines 263-268)
means a first empty choice masks a valid second (GUESS: rare with default `n=1`).

## Concrete hardening for v0.4.0 (with risks)

- **H1 — Reject non-terminal replies (F1).** After extracting `choices[0]`, read `finish_reason`:
  `"length"` → `Unavailable` (retry yields a longer window) with a `tracing::warn!` naming the
  reason; `"content_filter"`/`refusal` present → a distinct non-retryable classification; only
  `"stop"`/`"tool_calls"`/`null` are accepted. *Risk:* providers omitting `finish_reason` must keep
  working — default to "accept" for absent field, not "reject"; validate against the loopback
  fixture suite. The module doc must be corrected to match behaviour.
- **H2 — Typed status taxonomy (F2).** Internal enum (transport/timeout/429/5xx/4xx/decode), folded
  into stable public variants: 429 and 5xx → `Unavailable` (retryable, preserving the
  `req_015_..._retryable` assertion); 400/401/403/404 → `InvalidCredentials`/`Rule`/`NotAvailable`
  style non-retryable variants so the UI shows a legible model error (REQ-015, REQ-021). *Risk:*
  `is_retryable()` and the api error-envelope mapping are shared — changing `error.rs` affects every
  adapter; keep the public set stable where possible and record any new variant in `docs/PARITY.md`.
- **H3 — Honor `Retry-After` (F2).** Parse the 429 header (seconds or HTTP-date) and clamp to a
  configured max; expose it as a delay hint the REQ-024 backoff can consume, don't sleep in the
  adapter. *Risk:* needs a port/struct change to carry the hint — GUESS on caller appetite; if the
  port cannot change, at least log the header so operators can diagnose.
- **H4 — Correlation-safe tool calls (F3).** Use the given `id` when present and non-empty, else
  synthesize a unique `call_{index}`; reject the array on duplicate ids with a warning naming the
  index. Accept `arguments` as either string or object via `as_str().or_else(as_object)`; keep the
  strict string parse for malformed text. Reject empty `name`. *Risk:* synthesized ids are not the
  provider's real id — the next turn's `tool_call_id` must use the same value (it does, via
  `ToolCallRequest::id`); add a two-turn wiremock test to prove round-trip correlation.
- **H5 — Phase timeouts and body cap (F4).** Add `.connect_timeout(5 s)` (GUESS; tune from data),
  log elapsed/status-class/status-code on every failure, and read the body through a capped reader
  (e.g. 1 MiB — GUESS) before `serde_json::from_slice`. Validate `endpoint` at startup. *Risk:* caps
  reject legitimately large replies and short connects reject slow networks — make both configurable
  per deployment and measure before fixing defaults.
- **H6 — Decode hardening (F3, F4).** Iterate all `choices` for the first with a usable `message`
  (fallback, not preference), and structure `parse_message` into a typed response instead of
  `serde_json::Value` chain-getting. *Risk:* strict `deny_unknown_fields` would break OpenAI-compatible
  providers that add fields — keep permissive, assert only required ones.

## Verification (test-first, REQ-named)

- Wiremock `req_020_*`: `finish_reason: "length"` and `refusal` payloads are rejected, not streamed;
  `"stop"` still yields the exact chunk vector (guard the `split_inclusive(' ')` behaviour).
- `req_021_*`: 429 stays retryable and logs `Retry-After`; 401/400/404 become non-retryable with a
  legible variant; 5xx retried; assert against `AppError::is_retryable()`.
- `req_016_*`: id-less multi-call array gets unique ids; duplicate ids rejected; object-form
  `arguments` parsed; empty `name` rejected; two-turn test proves result correlation.
- `req_015_*`: oversized body and connect timeout fail legibly with elapsed logged; endpoint
  validation fails fast. Proptest `parse_message`. Check existing `wiremock`/`proptest` dev-deps
  first. Run `cargo fmt --check`, `clippy -D warnings`, and the named tests after each step.

## Risks / non-goals

No public port or wire-shape change without a DEVIATION and operator approval (H2/H3). No token
streaming/accounting here — out of scope. Never log the key, auth headers or full bodies. No
passing-test claims in this read-only pass; line refs are against `.worktrees/before-m5`.
