# 0170 — `crates/app/src/error.rs` — reliability lens (v0.4.0)

Worker #170 · Lens: reliability · Target: v0.4.0 · READ-ONLY review; no code edited.
Module: 97-line `AppError` thiserror enum + two classifiers + three REQ-named unit tests.
Read here: `crates/app/src/error.rs`, `crates/domain/src/error.rs` (`Display`, 69–101),
`crates/api/src/error.rs` (`From<AppError>`, 191–214), `crates/app/src/service.rs` (585–619),
`crates/app/src/worker.rs` (437–463), `usecases.rs` / `push.rs` / `acp_provider.rs` `%{error}` call
sites, `infra/src/{openai,messaging_http}.rs` `.map_err(|_| AppError::Unavailable)` sites, sibling
proposals wave-9/0136 and wave-10/0169. Anchors: AGENTS.md "Errors"/"Observability" (never leak
internals, no secrets in logs), REQ-012, REQ-024, REQ-040, REQ-053, REQ-058, defect D-01.
CONFIRMED = read this session; GUESS = inferred; no tests were run in this pass.

## Keep (do not re-propose)

Flat workspace-exhaustive enum; `#[from]` + `#[error(transparent)]` preserves the domain reason at
the app layer; api is the single HTTP-aware mapper; `is_retryable()` is `const` and drives the REQ-024
wake loop; tests carry REQ ids in their names. The api envelope itself stays safe (two fields, fixed
messages, `no-store`) — none of the fixes below should grow it.

## Failure modes

### F1 — Context loss (high)

- `Unavailable` and `RateLimited` are unit variants. Adapters discard the real cause with
  `.map_err(|_| AppError::Unavailable)` (`infra/src/openai.rs:58,209,252,257,261,271`;
  `messaging_http.rs:26,44,49,80,85,106`; testkit mirrors this). There is no `source()`, dependency
  tag, or retry hint: `worker.rs:437–438` can only flip one boolean and log a constant string.
  Operators cannot tell timeout from TLS failure from upstream 5xx, or which dependency failed
  (CONFIRMED; GUESS that distinct transient/permanent causes are hidden here).
- The api maps *every* `Rule(_)` to a generic 400 (`api/src/error.rs:194`). The specific reason the
  app layer preserved is dropped at the transport, so `EmailAlreadyTaken`, `ArchivedBot` and a
  REQ-012 invalid transition are indistinguishable to the client. That is intentional (no internals
  in the envelope), but it means detail survives only in the `Display` path — the same path that is
  unsafe to log (F2): detail is missing where it would help, present where it must not be.

### F2 — Secret / personal-data leakage (high)

- `Rule` is `#[error(transparent)]`, so `AppError::to_string()` is `DomainError::fmt`. Several arms
  echo raw values verbatim: `UnknownStatus`, `UnknownTrigger`, `UnknownRole`, `UnknownEventType`,
  `PinnedModelUnreachable`, `UnknownSystemReason` print `{value}`; `WeakPassword { reason }`,
  `InvalidName(reason)`, `DelegationRefused(reason)` print `{reason}` (`domain/src/error.rs:69–101`).
  `InvalidEmail` deliberately does not quote the address (line 76, asserted at 137–143) — the right
  pattern, applied inconsistently (CONFIRMED).
- That string is copied into text documented "safe to log": `Failure::detail` at `service.rs:618`;
  into user/tool output via `format!("...{error}")` (`usecases.rs:1065,1177,1215,3897,3950,
  7218–7328`); and into `tracing::warn!(%error, ...)` (`worker.rs:438`, `push.rs`, `acp_provider.rs`,
  `agui_provider.rs`). A caller-chosen "status"/"role"/"name"/"reason" therefore reaches logs, audit
  records and tool output a model may repeat. `#[derive(Debug)]` on both enums is a second echo
  channel. The path is CONFIRMED; that a real secret is present today is GUESS. The leak is internal
  (logs/audit/tool bodies), not the HTTP error envelope.

### F3 — Wrong mapping (medium-high)

- `is_retryable()` matches only `Unavailable` (error.rs:49–51). `RateLimited` literally says
  "wait and try again" yet is not retryable, so a throttled REQ-024 wake (D-01 counter) burns its
  failure budget and can pause the routine instead of backing off (CONFIRMED code; GUESS on frequency).
- `is_client_fault()` omits `Forbidden` (error.rs:55–66) although api maps it to 403; if used for
  metrics/alerting it would count authz denials as server faults. It also calls `StdioDisabled` (a
  deployment capability) a client fault. It has no production caller (grep: only its own tests), so
  the policy drifts unchecked; the duplicated assertion at lines 87–88 hides the intended check
  (CONFIRMED; also noted by 0169).
- `service.rs:605–615` holds a fourth exhaustive classification (`reason` strings). A new variant
  must be assigned in four places; exhaustive matches compile, but a *wrong* assignment does not fail.

## Concrete hardening (v0.4.0)

- **H1 (F2) — One safe text, one diagnostic text.** Add `AppError::public_message()` returning fixed,
  variant-derived text; use it for `Failure::detail`, tool-result bodies and any client-facing string.
  Make `DomainError::fmt` follow the `InvalidEmail` precedent: fixed wording, no raw `{value}`/
  `{reason}`; expose the raw value only through an explicit `diagnostic()` used in server-side logs.
  *Risk:* `req_012_*contains("completed")` and golden tool outputs assert current text; update tests
  with the same REQ names and review goldens — a changed tool string is a DEVIATION for `docs/PARITY.md`.
- **H2 (F2) — Canary tests at the boundary.** For every String-carrying `DomainError` variant, assert
  `to_string()`, `format!("{:?}")` and `Failure::from(AppError::Rule(e)).detail` omit a seeded secret
  canary. Reuse the REQ-040 no-quote test as the template; name tests with REQ ids.
  *Risk:* if a provider reason is contractually echoed (REQ-052), pass it only as a bounded,
  allowlisted field — never through `Display`/`Debug`.
- **H3 (F1) — Preserve the cause server-side only.** Give `Unavailable`/`RateLimited` a `Copy + Eq`
  cause enum (timeout / TLS / upstream-5xx / throttle), expose it through `Error::source()`, and log the
  raw cause once *at the adapter* that discards it, never in the returned `AppError`. Keep `Display`
  constant so downstream `%{error}` fields stay safe. Coordinates with 0169 items 3–4 (cause field,
  `Retry-After`) and 0136 item 2 (stop echoing `DomainError`).
  *Risk:* touches ~40 `matches!`/constructor sites; migrate via constructors and `{ .. }` patterns;
  keep field types cloneable/eq; rate-limit the new logs to avoid volume spikes.
- **H4 (F3) — Freeze the matrix, then fix it.** Add one test over all nine variants asserting both
  helpers, the api status and the `Failure` reason; fix `Forbidden`, correct the duplicated line-88
  assertion, and decide `RateLimited` retry explicitly (promote to retryable only with bounded,
  jittered backoff and a cap — else document that a throttled wake consumes budget).
  *Risk:* flipping a classifier can change wake/retry behavior or status codes; no production caller
  exists today (CONFIRMED), but freeze the current matrix first and file any wire change as a DEVIATION.
- **H5 (F3) — Drift guard.** Have `Failure::from`, the api mapper and the two helpers derive from one
  classification table (see 0169 item 1) so a new variant is a single edit with a compiler-enforced
  mapping. *Risk:* a mis-assigned kind changes envelopes; land H4's golden test first.

## Sequencing / cross-cutting risks

- Order H1→H2→H5-guard→H4→H3: stop the leak and add tests before changing shapes or retry timing.
- Wire-visible changes (statuses, Display text in tool output, retry timings) require an ADR + entry
  in `docs/PARITY.md` and the owner's approval; add `Retry-After` in the api worker, not here.
- Never weaken `no-store`, the two-field envelope, `unsafe_code`/unwrap denies, or REQ-024 budgets.
- Verification gate after each step: `cargo fmt --check`, `clippy -D warnings`, unit + `tests/`
  integration, golden/contract suites, plus the canary and subscriber-capture tests from H2/H3.
- Out of scope: HTTP types in the `app` crate, new variants outside the spec, envelope growth, log
  pipeline configuration — those belong to api/bin workers or an ADR.
