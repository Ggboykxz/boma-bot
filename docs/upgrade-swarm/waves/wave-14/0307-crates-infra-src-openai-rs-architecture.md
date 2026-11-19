# v0.4.0 architecture proposal — `crates/infra/src/openai.rs` (worker #307, lens: architecture)

Status: proposal only (read-only review). Claims below are CONFIRMED from the file and its
tests unless marked GUESS.

## Current structure (CONFIRMED)

`openai.rs` is a single ~275-line module with three concerns interleaved:

- `OpenAiConfig` (endpoint, model, `SecretString` key) + a hand-written `Debug` that omits the key.
- Two free functions over `serde_json::Value`: `request_body` (assembles messages/tools) and
  `parse_message` (maps the wire answer onto `ProviderTurn`).
- `OpenAiCompatibleAdapter` holding a `reqwest::Client` + config, with the `ModelProvider::complete`
  impl doing URL building, auth, send, status check, JSON decode and choice extraction inline.

It is wired once, in `crates/bin/src/commands.rs` (BYOK when `config.provider` is set, otherwise the
`ScriptedProvider`), and exercised by `crates/infra/tests/openai_adapter.rs` against a wiremock
loopback server. The transport siblings (`model_http.rs`, `messaging_http.rs`) already show a
stricter house style that this module does not yet follow.

## Upgrade 1 — Split wire mapping from transport (CONFIRMED gap)

Extract a private `wire` submodule (or `wire.rs` beside it) holding: typed request/response structs,
`request_body`, `parse_message`, and its tests. `complete()` then does only endpoint building, auth,
send and status handling, matching the layering of `model_http.rs`.

- **Why:** `parse_message` and `request_body` are pure and independently testable today but live
  next to async IO; a wire change currently forces re-reading the transport.
- **Risk:** low — internal refactor, no public API change. Keep the `boma_infra` re-exports in
  `lib.rs` intact. Preserve the existing REQ-015 test names verbatim (trace_check counts them).

## Upgrade 2 — Typed response model with strict-enough deserialization (CONFIRMED gap)

The answer is read through `.get(...).and_then(...)` chains with silent fallbacks:
`id -> "call"`, `name -> ""`, `arguments -> "{}"`, `content -> ""`. A provider reply with a missing
or empty tool name therefore becomes a `ToolCallRequest { name: "" }` that is handed to the
dispatcher instead of failing at the boundary. Replace the `Value` walk with a `#[derive(Deserialize)]`
`ChatCompletion` struct (explicit `#[serde(default)]` on optional fields, typed `Choice`, `Message`,
`ToolCall`, `Function`), and reject a tool call whose `name` is empty. Keep forward compatibility by
NOT using `deny_unknown_fields` on the envelope (BYOK providers add fields), but add explicit types.
Add a proptest for `parse_message` (QUALITY_BAR §23 requires property/fuzz tests on every parser of
an external format).

- **Risk:** medium — a stricter parser can reject a third-party provider that returns, e.g.,
  `content: null` or numeric IDs. Mitigate with golden wire fixtures from the loopback suite and an
  explicit decision recorded in `docs/PARITY.md` if any real provider shape is dropped.

## Upgrade 3 — Honour `finish_reason` and refusals (CONFIRMED gap, doc/impl mismatch)

The module doc says "`finish_reason` and `tool_calls` are translated", but the implementation never
reads `finish_reason`. A reply truncated with `finish_reason: "length"`, or one with
`finish_reason: "content_filter"` / a `refusal` field, is mapped to ordinary `ProviderTurn::Chunks`
and presented as a complete answer. Read the field: keep `stop`/`tool_calls` on the current path,
map `length` and `content_filter` to a distinct failure instead of pretending completeness.

- **Risk:** medium — externally visible behavior change (a formerly "successful" truncated answer
  becomes an error the worker may retry), so it needs a DEVIATION/STATE entry plus fixtures for each
  finish reason. Decide with REQ-024 in mind: a `content_filter` refusal is a *permanent* provider
  decision, not something backoff should retry, which collides with the current taxonomy (Upgrade 5).

## Upgrade 4 — Harden the HTTP client (CONFIRMED gap vs `model_http.rs`)

`new()` sets only a 60 s total timeout. Unlike `HttpModelTransport::new` and `StripeBillingProvider::new`
(both in `model_http.rs`), it does NOT set `.redirect(reqwest::redirect::Policy::none())`, has no
separate connect timeout, and caps no response body. A 30x from a misconfigured or hostile provider
endpoint can carry the `Bearer` key to a redirected host. Add: no redirects, a short connect
timeout, and a bounded read of the response body before JSON decoding (mirroring the `MAX_ERROR_BODY`
idea). Validate the config at construction (non-empty endpoint/model; endpoint parseable as URL).

- **Risk:** medium — some BYOK gateways legitimately redirect; disabling redirects may break them.
  Needs an ADR or a config flag, and golden coverage that the auth header is never re-sent cross-host.

## Upgrade 5 — Split retryable from permanent provider errors (CONFIRMED gap)

Every failure — timeout, 401, 400, 429, 5xx, unreadable JSON, malformed tool arguments — becomes
`AppError::Unavailable`, and `AppError::is_retryable()` (error.rs:49) is true only for `Unavailable`.
Consequence: a permanent invalid-key 401 is retried with backoff like a transient 429, burning the
routine's failure budget (REQ-024) and delaying a legible error. Introduce at least a distinction
between transport/throttling (429, 5xx, timeouts -> `Unavailable`) and provider rejection
(401/403/400 -> a non-retryable deployment error), with the status code carried in the log line.

- **Risk:** high — touches `AppError` and the worker retry gate, and changes run outcomes, so it is a
  DEVIATION requiring approval. Consider whether a new typed variant is warranted (a deployment
  fault is neither `Rule` nor `Conflict`); keep `api`'s stable error envelope unchanged. Do not leak
  the provider's raw error body to clients (QUALITY_BAR §27).

## Out of scope / deferred (GUESS)

Bounded history/memory sizes, `max_tokens`, SSE streaming, and applying `InfraEndpointSafety` to the
operator endpoint are reasonable follow-ups, but each changes behavior or needs a test-injection path
(the loopback tests deliberately use a private endpoint). The endpoint is deployment-configured, not
end-user-supplied, so the SSRF exposure is narrower than `push`'s (REQ-064).
