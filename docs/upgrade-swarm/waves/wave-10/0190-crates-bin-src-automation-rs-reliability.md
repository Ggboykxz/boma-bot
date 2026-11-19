# Worker #190 — `crates/bin/src/automation.rs` — LENS: reliability — v0.4.0 proposal

- **Status:** PROPOSAL (read-only review; no source changed). CONFIRMED = read from this module, `config.rs`, `telemetry.rs`, `cli.rs`, `main.rs` and spec REQ-072 in this worktree; anything else is GUESS.
- **Scope:** the *operator CLI client* `boma automation` (REQ-072), six commands over `/api/v1/automation/*`. This is the sibling of wave-9's server-side `crates/api/src/automation.rs`; the two surfaces must be hardened together.

## Summary

The module is a thin, single-request-per-invocation client (~218 lines). Happy path is tidy, but reliability exposure concentrates in three places the doc comment itself promises to avoid ("never a silent success"): **auth token handling** (raw, unvalidated, untrimmed env bearer), **network errors** (unencoded path ids, silent empty-body fallback, coarse status reporting, no retry), and **partial runs** (`wait=true` timeout loses the run id; no idempotency key; no client/server timeout consistency check). `main() -> anyhow::Result` prints the error chain and exits 1, so the *quality of the error message* is the operator's only recovery signal.

## Failure modes

### Auth token handling
- **A1 (HIGH, CONFIRMED):** `token()` (85–88) returns the raw `BOMA_AUTOMATION_TOKEN` with no trim and no length/shape check. The server (`config.rs:662`) requires >= 16 chars and trims via `optional()` (430–434). A trailing newline/space from a shell or `.env` copy → bearer mismatch → 401 with no hint about the cause.
- **A2 (MED, CONFIRMED):** the bearer is held as a plain `String` and passed to `.bearer_auth(...)` (101), not wrapped in `secrecy::SecretString` as `ProviderConfig` does (config.rs:227). No current log prints it, so exposure is latent, but it violates the "secrets wrapped in secrecy" standard.
- **A3 (MED, CONFIRMED):** no fail-fast validation at CLI startup. `automation::run` bypasses `load_config()` (cli.rs:85 vs 93–97), so the client never checks token presence/length before the request; a missing token fails only after `init_output()` and client construction.
- **A4 (MED, CONFIRMED):** `endpoint()` (77–82) accepts any value from `BOMA_AUTOMATION_ENDPOINT` with no scheme/host validation. A mistaken non-loopback or plain-`http://` remote value sends the bearer off-machine. Doc says "loopback by default" but nothing enforces it.
- **A5 (LOW, CONFIRMED):** status 401/404/429/503 are indistinguishable to the caller: all become `bail!("the automation surface refused the call ({status})")` (117–118). A wrong token, a disabled surface, a rate limit and an overloaded server produce the same one-line error.

### Network errors
- **N1 (HIGH, CONFIRMED):** path ids are interpolated raw into the URL (`bot` 176, `card` 194, `bot`/`routine` 203, `run` 212) via `format!("{}{path}", endpoint())` (98). An id containing `/`, `?`, `#` or whitespace produces a wrong/malformed target — possibly addressing another resource — with no percent-encoding.
- **N2 (HIGH, CONFIRMED):** `response.text().await.unwrap_or_default()` (112) swallows body-read errors (reset mid-body) as `""`. On a 2xx with a truncated body the client logs `body=""` and returns `Ok(())` — a silent success with no answer, exactly what the module comment (90–91) says it will not do.
- **N3 (MED, CONFIRMED):** `client_timeout_seconds()` (122–128) silently falls back to 300 s on an unparsable/zero value, unlike the server's fail-fast `optional_parsed` (config.rs:436–449). A typo in the timeout var is invisible.
- **N4 (MED, CONFIRMED):** no retry/backoff for transient connect failures, 429 or 503 (108–118). A deployment restart during a call = hard failure, even for a read-only `Agents`/`Approvals`.
- **N5 (MED, CONFIRMED):** the whole response body is logged on both success (114) and failure (117). Non-2xx bodies may contain run content; AGENTS.md says no personal data in logs. The JSON format deliberately exposes `body`, so any change here needs a parity note.
- **N6 (LOW, CONFIRMED):** only a whole-request timeout is set; no separate connect timeout, and `reqwest::Client::new()` uses defaults. No cancellation test for a dropped `wait=true` call.

### Partial runs
- **P1 (HIGH, CONFIRMED):** with `wait=true`, a client timeout (default 300 s) or a server 408 leaves a run in flight server-side. The response body is *only logged*, never parsed (114), so the operator has no run id to pass to `RunStatus` (208–215). The dedicated `RunStatus` command exists but cannot be chained from a failed `send`/`routine_run` — a message was sent, yet the CLI reports only "refused the call (408)".
- **P2 (MED, CONFIRMED):** `Answer` (190–198) sends no idempotency key; a retry after a network error can double-record or 409. The spec REQ-072 acceptance text implies "same authorization rules", GUESS whether dedupe is specified.
- **P3 (MED, CONFIRMED):** client default 300 s vs server wait default 120 s (config.rs:649). If an operator raises the server wait timeout above 300 s without also raising `BOMA_AUTOMATION_CLIENT_TIMEOUT_SECONDS`, the client cuts off a still-running wait with no warning. No startup consistency check.
- **P4 (LOW, CONFIRMED):** `timeout_seconds` is forwarded as `body["timeoutSeconds"]` (170–171) with no client validation (0/huge pass through; server clamps — wave-9 V3). The client never reports the effective timeout.

## Concrete hardening (prioritized for v0.4.0)

- **P0 — correctness:** fix N2 (propagate body-read error; never return `Ok(())` on an incomplete body); fix N1 (percent-encode each path segment via `url`/`Url::path_segments_mut`, or newtype ids with validated charset); fix A1 (trim + reject short/blank token once, with an actionable error naming the variable).
- **P1 — observability/recovery:** classify status into typed client errors (401/404/408/429/5xx with distinct, non-secret messages — A5); on `send`/`routine_run`, parse `runId` from the JSON body and emit it (stderr/log field) even when `wait` times out, so `RunStatus` can recover (P1); wrap the bearer in `secrecy::SecretString` (A2) and move validation into a typed `AutomationClientConfig` built once (A3).
- **P2 — resilience:** bounded retry with jitter for connect errors/429/503 on idempotent commands only (`Agents`, `Approvals`, `RunStatus`), never for `Send`/`Answer`/`RoutineRun` without a key (N4, P2); make `client_timeout_seconds` fail fast like other config (N3) and warn when it is below the server wait timeout (P3); optional `Idempotency-Key` for `Send`/`Answer` pending spec agreement (P2).
- **P3 — polish:** validate `BOMA_AUTOMATION_ENDPOINT` as loopback/http(s) unless an explicit override flag is set (A4); truncate/redact bodies in error logs per log-format policy (N5); validate `timeout_seconds` client-side and echo the effective value (P4); add a cancellation/disconnect test for `wait=true` (N6).

## Risks & mitigations

- **DEVIATION exposure:** A3/A4 validation, typed status errors, run-id emission and idempotency headers alter externally visible behavior → record in `docs/PARITY.md` and obtain operator approval before shipping (AGENTS.md).
- **Idempotency keys** require server support (wave-9 P2); adding a header alone is inert — sequence both changes in the same release, gated on spec agreement.
- **Retrying on 503** during a `wait=true` long poll can duplicate work if the server already accepted the request but timed out responding; keep retries strictly to read-only commands in v0.4.0.
- **Parsing `runId`** couples the CLI to a response field; call it a stable contract only if `spec/openapi.yaml` defines it, otherwise treat as best-effort log field and keep "call HTTP directly" as the script interface.
- **Endpoint restriction** may break legitimate remote operators; make it opt-out via config rather than a hard refusal, and document it.

## Acceptance tests (REQ-072 names)

- `req_072_automation_trims_and_validates_token` — A1/A3.
- `req_072_automation_rejects_unencoded_path_id` — N1 (assert encoded target via wiremock).
- `req_072_automation_body_read_error_is_not_success` — N2.
- `req_072_automation_wait_timeout_reports_run_id` — P1.
- `req_072_automation_status_is_classified` — A5.
- `req_072_automation_client_timeout_fails_fast` — N3/P3.
- `req_072_automation_read_retry_is_bounded` — N4.

## Decisions needed (GUESS → operator/spec)

1. Is a run id guaranteed in every `wait=true` response (P1), or must the CLI stay raw-log-only?
2. Does REQ-072 specify idempotency/dedupe for `Send`/`Answer` (P2)?
3. May `BOMA_AUTOMATION_ENDPOINT` ever be non-loopback (A4)?
4. Are stricter CLI validation and status classification DEVIATIONS needing parity approval?
