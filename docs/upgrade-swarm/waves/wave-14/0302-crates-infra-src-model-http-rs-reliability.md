# 0302 — `crates/infra/src/model_http.rs` — reliability proposal (v0.4.0)

Worker #302 · lens: reliability · READ-ONLY review of `.worktrees/before-m5`. Proposals only; no code
changed. **CONFIRMED** = read in this tree this session; **GUESS** = inference needing a test. Anchors:
REQ-051 (Stripe checkout), REQ-052 (`/models` probe), `ports.rs:4007-4033`, `usecases.rs:3598-3616`,
AGENTS.md ("timeout on every outbound call", "idempotency keys where the spec has retries or
payments", "request body limits", "secrets never logged", typed errors).

## Scope note

Two clients, not an inference stream: `HttpModelTransport::get_json` is a one-shot GET used only to
validate a BYOK credential; `StripeBillingProvider::create_checkout` is a one-shot POST opening a
Checkout Session (CONFIRMED). Neither streams nor counts tokens, so "stream truncation" and "token
accounting" are treated as scope/adjacency findings (F6); real streams live in the wave-10 ACP code.
Both clients are gates: a false `Unavailable` blocks a valid key or a checkout, a false `Ok` stamps
a bad key as valid.

## What is already right (keep)

- 30 s total timeout and `redirect(Policy::none())` on both clients (lines 19-22, 92-95).
- Stripe secret is `SecretString`, passed via `basic_auth`, excluded from `Debug` (lines 67-80, 150);
  neither path logs auth headers or the secret.
- Error body capped at `MAX_ERROR_BODY = 4 KiB` (line 28) — extend this to the success path.

## Failure modes

**F1 — No idempotency key on a payment call (high, CONFIRMED).** `create_checkout` POSTs with no
`Idempotency-Key` (lines 143-153), against AGENTS.md's payments rule. A timeout after Stripe created
the session is ambiguous: the client returns `Unavailable` (line 156); any caller retry would create
a second session. No retry loop today, so the current risk is a stranded session, but H2 would turn
it into a duplicate.

**F2 — No retries, no transient/permanent distinction (high, CONFIRMED).** Every `send()` error maps
straight to `Unavailable` (lines 41-44, 154-157): a reset, DNS blip or 502 forces the member to
retype a key or loses the checkout. The error type has only `Http`/`Unavailable`
(`ports.rs:4023-4033`), so callers cannot tell "retry" from "bad key". Blind retry of the POST is
unsafe until F1 is fixed.

**F3 — One total timeout, no connect or idle timeout (medium, CONFIRMED).** `Client::timeout(30 s)`
is a whole-request deadline; no `connect_timeout`, so a black-holed SYN burns the full budget, and
no body deadline independent of connect. The same cap covers a fast probe and a payment POST, and
no attempt deadline is logged (GUESS: this is what "validation hung" reports look like).

**F4 — Unbounded success-body buffering (medium, CONFIRMED).** Both success paths call
`.json::<Value>().await` uncapped (lines 56-59, 162-165), unlike the 4 KiB error cap. A misbehaving
provider or proxy can force a large allocation and hold the budget; only `url`/`id` are used
(lines 166-175).

**F5 — Lossy error classification and body handling (medium, CONFIRMED).** Decode failures are
discarded with no `tracing` (lines 59, 165). `status >= 400` misses 3xx (redirects disabled), which
falls into `.json()` and returns `Unavailable` without status (lines 45-59). The error body is cut by
byte index then `from_utf8_lossy` (line 51), which can split a multi-byte char into U+FFFD;
`usecases.rs:3606` shows the first 200 chars to the member. Stripe's non-success branch logs only
status and drops Stripe's error body (lines 158-161).

**F6 — Stream truncation / token accounting: not in this module (scope, CONFIRMED).** No streaming
response and no token counter exist here. The reliability link is `usecases.rs:3604` accepting any
`Ok(_)`: a captive portal or proxy returning any 2xx JSON (even `{}`) validates the credential, and
the first real inference stream then fails with no trace to this probe — the classic misattributed
"stream failed" report. Fix by shape-checking at the caller, not by adding accounting here.

**F7 — Thin observability (low, CONFIRMED).** `tracing::warn!` carries no URL, status class, attempt
or elapsed time; no request-id correlation or latency/timeout counters.

## Concrete hardening for v0.4.0 (with risks)

- **H1 — Checkout idempotency key (F1).** Generate one `Uuid` per call and send `Idempotency-Key`,
  reusing it across internal retries. *Risk:* Stripe caches 24 h, so a caller legitimately wanting a
  new session may get the old one; cross-call dedup needs a port change (persist per attempt) —
  record as DEVIATION and seek approval first.
- **H2 — Bounded retry for the idempotent GET only (F2).** Retry `get_json` 2-3 times on connect/
  reset/timeout and 429/5xx, capped backoff + jitter inside the 30 s budget; never retry the POST
  until H1's cross-call design is approved. *Risk:* load amplification and stretched worst-case
  latency — cap by total budget.
- **H3 — Phase timeouts (F3).** Add `.connect_timeout(5 s)` (GUESS; validate) and log elapsed on
  failure. *Risk:* too short rejects slow-but-valid networks; tune from data.
- **H4 — Body caps (F4).** Stream and cap success bodies (1 MiB `/models`, 256 KiB session — both
  GUESSes), then `serde_json::from_slice`; apply the 4 KiB cap to Stripe errors too. *Risk:* a large
  model list would be rejected; validate against a big provider first.
- **H5 — Typed, logged failure taxonomy (F5, F7).** Internal enum (transport/connect/timeout/status/
  oversize/decode) for logging, folded into the unchanged public variants; decode error bodies with
  `from_utf8` + char-boundary fallback; log redacted host, status, elapsed, attempt. *Risk:* never log
  auth headers, secrets, userinfo URLs or full bodies — redact explicitly.
- **H6 — Caller-side shape validation (F6).** Require `data[*].id` / `models[*]` in `usecases.rs`
  before `Ok`. *Risk:* a provider with a valid-but-different 2xx body would now fail — DEVIATION
  candidate in `docs/PARITY.md`, gate on sign-off.
- **H7 — Status hygiene (F5).** Use `error_for_status()` so non-2xx (including 3xx) uniformly return
  status+body. *Risk:* a 3xx-JSON server would stop validating; unlikely on fixed hosts.

## Verification (test-first, REQ-named)

- Wiremock `req_052_*`: 503-then-200; timeout/refused retried the expected count; oversized body
  rejected; truncated multi-byte error body decodes cleanly; 2xx `{}` rejected after H6.
- `req_051_*`: `Idempotency-Key` present and stable across an internal retry; Stripe 500 body logged;
  oversized session JSON rejected.
- Proptest the error-body decoder over arbitrary bytes. Check existing `wiremock`/`proptest` dev-deps
  before adding (GUESS: present); wide timing margins; `cargo fmt --check`, `clippy -D warnings`, and
  REQ-051/052 tests after each step.

## Risks / non-goals

No public port or JSON shape change without a DEVIATION and approval (H1 cross-call, H6). Do not add
streaming or token accounting here, and do not log secrets or header values. No passing-test claims in
this read-only pass; line refs are against `.worktrees/before-m5`.
