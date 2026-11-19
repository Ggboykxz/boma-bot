# v0.4.0 reliability proposal — `crates/infra/src/messaging_http.rs`

- Worker: 300 (upgrade swarm, wave 14). Lens: reliability. Target: v0.4.0.
- Scope: read-only review; this document is the only file written.
- Claim markers: **CONFIRMED** = read from source in this session; **GUESS** = inference, verify before shipping.

## 1. Current behaviour (CONFIRMED)

`HttpMessagingTransport` wraps one `reqwest::Client`: 30s total timeout, no redirects. Three
port methods (REQ-048): `get_json`, `post_form`, `post_json`. Only `post_json` calls
`crate::net_safety::check_outbound_url(url, allow_private)`. Every transport failure collapses
to `AppError::Unavailable`; `post_json` returns the raw status code and callers log `status >= 400`
themselves. Callers embed secrets: Slack bearer header, Telegram bot token in the URL path,
Slack OAuth `client_secret` in the form body, generic webhook URL as the target.

## 2. Failure modes

### 2.1 Timeouts and hangs
- **CONFIRMED**: one 30s `timeout()` covers connect + body; no `connect_timeout`, so a slow
  connect consumes the entire budget.
- **CONFIRMED**: `check_outbound_url` runs `tokio::net::lookup_host` *before* the client call,
  with no timeout of its own — a stalled DNS answer sits outside the 30s client budget.
- **CONFIRMED**: `.json::<Value>().await` buffers the whole response with no size cap; a broken
  or hostile endpoint can exhaust memory (`get_json`, `post_form`).
- **CONFIRMED**: no per-host concurrency bound or circuit breaker; a down platform makes each
  delivery wait up to 30s, which can exhaust tasks/connections under fan-out.
- **GUESS**: 30s exceeds an interactive reply SLA; a shorter delivery-specific budget is likely
  warranted, but pick it from measured latency, not intuition.

### 2.2 SSRF
- **CONFIRMED**: only `post_json` is guarded. `get_json` (Telegram `getMe`) and `post_form`
  (Slack `oauth.v2.access`) currently target fixed hosts, so there is no live hole today, but the
  port accepts arbitrary URLs and a future generic path inherits no guard (**GUESS** on callers).
- **CONFIRMED**: `CheckedOutbound.resolved` is returned by the guard but never used; reqwest
  resolves again at connect time. A DNS-rebinding name can pass with a public IP and connect to a
  private IP. The guard doc's "per-call resolution" is not equivalent to pinning.
- **CONFIRMED**: refusal maps to `AppError::Rule(InvalidName(...))` (a client fault), so the
  REQ-024 retry wrapper (`is_retryable() == Unavailable`) will not retry it — correct, but the
  reason is an unstructured string, invisible to metrics.
- **GUESS**: IPv6/edge range gaps in `is_private_address` belong in `net_safety`, not this module;
  flag to that owner rather than duplicating the policy here.

### 2.3 Retries
- **CONFIRMED**: zero retries. The port doc deliberately forbids a loop: "a platform refusing the
  call is a logged failure, never a retry loop".
- **CONFIRMED**: `post_json` carries no idempotency key; `chat.postMessage` / `sendMessage` have no
  content dedupe, so a blind retry can duplicate a user-visible message.
- **CONFIRMED**: `AppError::Unavailable` is `is_retryable()` (REQ-024), so an upper-layer retry
  can replay a delivery that actually succeeded but whose response was lost — at-least-once
  becomes "maybe twice" without dedupe.
- **GUESS**: Slack/Telegram 429 uses `Retry-After`; ignoring it turns rate limiting into a fast
  failure loop upstream. Verify against pinned platform docs before adding any retry.

### 2.4 Auth and secret handling
- **CONFIRMED**: `tracing::warn!(%error, ...)` logs reqwest errors on all three paths, and the
  Telegram token sits in the URL. If reqwest's `Display` includes the URL (**GUESS** — verify in
  the pinned version), the bot token is written to logs on every failure, breaking the
  "no secrets in logs" rule.
- **CONFIRMED**: headers are caller-supplied and forwarded verbatim; no allow-list binds a
  credential to a host. A member-controlled URL plus a bearer header is an exfiltration path if
  such a caller is ever added (**GUESS** on future callers).
- **CONFIRMED**: no status check in `get_json` / `post_form`; a 401/403 with a JSON error body is
  parsed as success and only caught by callers inspecting `"ok"`. Inconsistent with `post_json`.

## 3. Concrete hardening (v0.4.0)

H1. Split timeouts: `connect_timeout(5s)`, keep total `timeout(30s)` (shorter for delivery), and
    wrap `check_outbound_url` in `tokio::time::timeout(2s, ...)`.
    *Risk*: more timeouts on poor networks; tune from measured data.
H2. Pin DNS: connect via the `resolved` set from `CheckedOutbound` (reqwest `resolve()`) and refuse
    if a candidate drifts private. Add a pinning helper in `net_safety`; do not duplicate policy.
    *Risk*: breaks rotating-DNS/CDN hosts unless all A/AAAA answers are pinned.
H3. Guard all three methods, not just `post_json` (`allow_private = false` for `get_json` /
    `post_form`). *Risk*: one extra DNS lookup per call; only cache with a TTL if rebinding stays closed.
H4. Bound response bodies: check `content-length`, then stream-read with a cap (e.g. 1 MiB) before
    `.json()`. *Risk*: legitimate large payloads truncate; derive the limit from the spec.
H5. Consistent status handling: `error_for_status()` in `get_json` / `post_form`; introduce a typed
    internal error (`thiserror`: Timeout / Transport / HttpStatus / Refused) mapped to `AppError`
    at the edge. *Risk*: callers relying on parsing error JSON now get `Unavailable` — audit them.
H6. Retries only where safe: none for `post_json` until an idempotency key exists; for idempotent
    GETs, 2 attempts with jittered backoff and `Retry-After`, plus a per-host budget.
    *Risk*: idempotency keys change the outbound wire format → DEVIATION, needs approval.
H7. Secret hygiene: never log full URLs or reqwest `Display`; log method, host, status, request id,
    and a redacted error kind. Enforce a host→credential allow-list where headers are attached (or
    document fixed-host pinning as a caller obligation). *Risk*: less detail in triage; compensate
    with structured fields.
H8. Saturation + observability: per-call span with request id + platform, metrics for
    timeout/5xx/429/refused counts and an in-flight gauge, bounded per-host concurrency with a
    small circuit breaker. *Risk*: extra config/dependency; keep it in-process.

## 4. Test plan (REQ-tagged)

- `req_048_timeout_is_bounded`: `wiremock` stall → error within budget, `connect_timeout` path hit.
- `req_048_response_body_is_capped`: oversized body refused, no unbounded buffer.
- `req_033_get_json_and_post_form_are_ssrf_checked`: private URL refused through all three methods.
- `req_033_rebinding_is_pinned`: DNS answer flips public→private between check and connect → refused.
- `req_048_no_duplicate_delivery_on_retry`: scripted transport counts one call on lost response.
- `req_048_no_secret_in_logs`: capture tracing output on failure, assert token/header absent.

## 5. Deviations / questions to owner

- H2/H6 may alter observable behaviour (host resolution, outbound wire format): record any change in
  `docs/PARITY.md` and wait for approval before shipping.
- Confirm whether generic webhook delivery should ever accept member URLs; if yes, idempotency and
  credential host-binding become mandatory, not optional.
- Confirm 429/503 handling policy with the platform docs before enabling any retry.
