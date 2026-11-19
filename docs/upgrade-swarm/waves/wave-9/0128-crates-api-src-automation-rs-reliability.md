# Worker #128 — `crates/api/src/automation.rs` — LENS: reliability — v0.4.0 proposal

- **Status:** PROPOSAL (read-only review; no source changed). Claims marked CONFIRMED come from reading the module, `correlation.rs`, `app/src/service.rs` and usecase signatures in this worktree; anything else is GUESS.
- **Scope:** `/api/v1/automation/*` (REQ-072), six routes, static local bearer, uniform owner principal, optional `wait=true` long polls.

## Summary

The happy path is clean (constant-time hash compare, 64 KiB cap, security headers, bounded poll timeout). Reliability exposure concentrates in four areas: no rate limiting or auth observability, unbounded long-poll fan-out, permissive boundary deserialization, and two side-effect/ordering defects where an action completes before authorization or before a correct status is chosen.

## Failure modes

### Auth
- **A1 (HIGH, CONFIRMED):** `bearer_ok` is called on every route with no attempt limiting or lockout. A static local bearer can be brute-forced without bound; AGENTS.md mandates rate limiting.
- **A2 (HIGH, CONFIRMED):** `with_automation_option(Some(""))` stores an empty `SecretString`. The surface is "on" (`token.is_none()` is false) and `Authorization: Bearer ` (empty after `strip_prefix`) hashes equal → **unauthenticated access**. Whitespace-only tokens have the same shape. No startup validation rejects this.
- **A3 (MED, CONFIRMED):** `strip_prefix("Bearer ")` is case-sensitive; RFC 6750 schemes are case-insensitive, so legitimate clients using `bearer`/`BEARER` get a spurious 401.
- **A4 (MED, CONFIRMED):** 401/403 produce only the generic `boma.http` line; no auth-specific metric or log. Brute force is invisible to operators. (`correlation.rs` logs route templates only — no header/token leak; confirmed.)
- **A5 (MED, GUESS):** `agent_enabled` gates `agents_list`, `send_message`, `routine_run`, but **not** `runs_get`, `approvals_list`, `approvals_answer`. A token scoped to bot A can poll any run id and answer any card. Approvals may be intentionally deployment-wide (GUESS — verify against spec).

### Rate limits
- **R1 (HIGH, CONFIRMED):** no concurrency cap on `wait=true`. Each request holds a task and issues `runs.find` every 250 ms up to `automation_wait_timeout_seconds` (default 120 s). N concurrent callers ≈ 4N store queries/s plus N held connections — a self-inflicted DoS with a valid token.
- **R2 (MED, CONFIRMED):** cheap endpoints (lists, `runs_get`) share the same absence of limiting as A1.
- **R3 (MED, CONFIRMED):** `timeout_seconds` is bounded by config but the server cannot shed load; `routine_run` always uses the full max.

### Validation
- **V1 (MED, CONFIRMED):** `SendMessageRequest`, `RoutineRunRequest`, `ApprovalAnswerRequest` lack `#[serde(deny_unknown_fields)]` (unlike `domain/page.rs` and golden types). A typo like `"waitt": true` silently becomes `wait:false`, silently changing semantics.
- **V2 (MED, CONFIRMED):** `answer: String` goes straight to `usecases::answer_card`; no boundary check of `approve`/`deny`. Unknown values fail late as a generic `ApiError`. GUESS: usecase may accept more card kinds — check before constraining.
- **V3 (LOW, CONFIRMED):** `timeout_seconds: 0` is silently clamped to 1 s inside `await_terminal_run`; the response never reports the effective timeout.
- **V4 (LOW, CONFIRMED):** `bot_id`, `routine_id`, `card_id` are unvalidated `String`s; only `run_id` is parsed to a newtype. Invalid ids reach usecases instead of yielding a consistent 404/400.
- **V5 (LOW, GUESS):** no explicit text-length limit beyond the 64 KiB body cap; a 64 KiB single-line message is accepted. Spec may allow it.

### Partial runs
- **P1 (HIGH, CONFIRMED):** `routine_run` calls `routines_run_now` **before** checking `outcome.routine.bot_id != bot_id`. A token allowed for bot A can name bot B's routine: **bot B's routine actually fires**, then the caller receives 403. Authorization must precede the side effect.
- **P2 (HIGH, CONFIRMED):** `send_message` passes `client_nonce: None` and a `wait=true` timeout returns 408 while the run stays in flight. A retrying client creates duplicate messages/runs. Stores already support `client_nonce` dedupe (`stores/messages.rs`, `stores/runs.rs`), but the surface never exposes it.
- **P3 (MED, CONFIRMED):** empty `outcome.runs` returns **HTTP 200** with `{"error":"no run was created"}` — success status carrying a failure body.
- **P4 (MED, CONFIRMED):** `await_terminal_run` store errors and `runs_get` errors collapse to a bare 503 with no body and no log — the cause is lost.
- **P5 (LOW, CONFIRMED):** `Ok(None)` mid-wait ("run vanished") returns a bare 503; likely a retention race, so callers retry blindly.
- **P6 (LOW, GUESS):** inner deadline and outer `TimeoutLayer`(+5 s grace) race; verify by test that disconnect cancels polling and leaves no orphan tasks.

## Concrete hardening (prioritized for v0.4.0)

- **P0 — correctness/access:** fix P1 (precheck routine ownership before fire; add an app-layer lookup or pass expected `bot_id` into `routines_run_now`); fix A2 (fail fast at startup on empty/whitespace automation token); fix P3 (return 502/500 + structured error, not 200); add `deny_unknown_fields` (V1).
- **P1 — resilience:** in-process fixed-window limiter on bearer failures (A1) and all routes (R2) with `429` + `Retry-After`; a bounded semaphore (e.g. configurable, default 16) around `await_terminal_run` returning 429/503 when saturated (R1, R3); accept an idempotency key (`Idempotency-Key` header or `clientNonce` body) and pass it as `client_nonce` (P2); emit a `boma_automation_auth_failed_total` counter with no token data (A4) and traced store errors with request id (P4).
- **P2 — polish:** case-insensitive `Bearer` + single-SP parsing (A3); boundary enum for `answer` (V2); echo effective timeout or reject 0 (V3); newtypes for path ids (V4); config-driven max text length (V5); gate `runs_get` by run→bot ownership if spec permits (A5); explicit `run_gone` outcome (P5); cancellation test (P6).

## Risks & mitigations

- **DEVIATION exposure:** P2 (new field/header), P3 (status change), V1 (strict rejection), A5 (new gate) alter externally visible behavior → record each in `docs/PARITY.md` with justification and obtain operator approval before shipping (AGENTS.md).
- **In-memory limiter** is per-process and resets on restart; acceptable for a local single-machine surface (GUESS — confirm deployment topology), make thresholds configurable and generous to avoid lockouts.
- **Idempotency replay** must return the original status/run, not just dedupe writes; requires agreement with store semantics.
- **Strict DTOs** may break callers sending extra fields; roll out with the parity note above.
- **Semaphore** introduces queueing/fairness questions; keep it transparent via 429 + `Retry-After`.

## Acceptance tests (REQ-072 names)

- `req_072_empty_automation_token_fails_startup` — A2.
- `req_072_bearer_scheme_is_case_insensitive` — A3.
- `req_072_cross_agent_routine_refusal_creates_no_run` — P1 (assert store count unchanged).
- `req_072_duplicate_idempotency_key_dedupes` — P2.
- `req_072_no_run_returns_5xx_not_200` — P3.
- `req_072_unknown_field_is_rejected` — V1.
- `req_072_auth_failure_rate_limit_returns_429` — A1/R2.
- `req_072_long_poll_saturation_is_bounded` — R1.
- `req_072_store_error_is_traced_with_request_id` — P4.

## Decisions needed (GUESS → operator/spec)

1. Are approvals deployment-wide, or should the agent allow-list apply to `answer`/`runs_get` (A5)?
2. Full set of valid `answer` values across card kinds (V2)?
3. Is an idempotency key or a specific max text length specified for this surface (P2, V5)?
4. Spec-defined rate-limit numbers and whether they are a DEVIATION (A1/R1)?
