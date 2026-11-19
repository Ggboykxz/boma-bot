# 0169 — `crates/app/src/error.rs` — architecture (v0.4.0)

- Worker: 169 · Lens: architecture · Target: v0.4.0 · Read-only review; no code edited.
- Module: 97-line `AppError` enum + two classification helpers + 3 unit tests. Spec anchors:
  `AGENTS.md` "Errors" (typed errors with thiserror, one stable API error format, never leak
  internals), REQ-024 (retryable wake backoff), REQ-058 (Forbidden), REQ-029 (ScreenTaken),
  REQ-040 (Conflict / credentials), defect D-01 (RateLimited).
- Read here: `crates/app/src/error.rs`, `crates/domain/src/error.rs`, `crates/api/src/error.rs`
  (mapping, lines 191–214), `crates/app/src/worker.rs:437`, usages across `crates/infra/tests/*`
  and `crates/testkit/src/{build,scripts}.rs`, `docs/upgrade-swarm/waves/wave-7/0101…md` (format).
- CONFIRMED = read in this session; GUESS = inferred.

## Structure today (CONFIRMED)

`AppError` is a flat, non-`Clone`, non-`PartialEq` thiserror enum of nine variants: one payload
variant `Rule(DomainError)` (`#[from]`, `#[error(transparent)]`) and eight unit variants —
`Conflict`, `StdioDisabled`, `RateLimited`, `ScreenTaken`, `InvalidCredentials`, `Unavailable`,
`NotFound`, `Forbidden`. Its doc comment states the intent well: the variant names the *layer*
that failed, and `crates/api/src/error.rs::From<AppError> for ApiError` is the single place that
knows HTTP status codes. That api mapping is exhaustive and compiler-checked across the workspace —
a real strength that should survive v0.4.0.

Two helpers encode policy: `is_retryable()` (only `Unavailable`; consumed by the REQ-024 wake loop
at `worker.rs:437`) and `is_client_fault()`. The architecture debt concentrates in three findings:

1. `is_client_fault()` omits `Forbidden`, yet the api maps `Forbidden` to 403 (a client fault).
   The two sources of truth disagree today (CONFIRMED by reading both; GUESS which one is wrong).
2. `is_client_fault()` has **no production caller** (grep: only its own tests) — a public method
   that drifts unverified and is either dead surface or the natural home for canonical policy.
3. The same policy is encoded a third time in the api `From<AppError>` match, so each new variant
   is a coordinated multi-crate edit; and `Unavailable` / `RateLimited` carry no cause or retry
   hint, so the wake loop cannot honour `Retry-After` and operators cannot see *why* a dependency
   failed.

Also: `req_040_a_taken_address_is_a_conflict_and_not_a_server_error` asserts `is_client_fault()`
twice (error.rs:87–88) — a copy-paste defect that leaves the intended second assertion unwritten
(CONFIRMED).

## Concrete upgrades for v0.4.0 (each with a risk)

1. **Single-source the classification.** Add a canonical `AppError::kind()` (small
   `enum ErrorKind { Client, Conflict, Deployment, Throttled }`) and compute both helpers from it;
   have `api::From<AppError>` consult the same kind instead of a parallel match. Risk: a mis-assigned
   kind would change status codes / error envelopes, which are externally visible — freeze the
   current code matrix in a golden test first and treat any change as a DEVIATION.

2. **Correct the `Forbidden` classification.** Include `Forbidden` in `is_client_fault()` (403 is a
   caller fault) and add a full matrix test over all nine variants asserting both helpers plus the
   api status for each. Risk: if a hidden caller ever treated `Forbidden` as server-side, flipping
   it changes wake/retry behavior; grep shows no production consumer today (CONFIRMED), so impact
   should be nil — re-grep after merge. Fix the duplicated assertion at line 88 with real content.

3. **Give `Unavailable` a cause without leaking it.** Change the unit variant to carry a reason,
   e.g. `Unavailable { reason: &'static str }` (or a small `Dependency` enum), keep `Display` and
   the api envelope byte-identical, and log the reason via `tracing` at the boundary. Add a
   constructor `AppError::unavailable(reason)` so the many call sites (testkit, infra adapters)
   need not spell the struct. Risk: shape-breaking across those constructors and `matches!`
   patterns; keep the reason `Clone + Eq` (`&'static str`/enum, not `Arc<dyn Error>`) or upgrade #5
   becomes impossible.

4. **Surface a retry hint for `RateLimited` (D-01).** Carry `retry_after: Option<Duration>` so the
   api can emit `Retry-After` and the REQ-024 loop can back off correctly; keep `RateLimited` a
   client fault and do **not** auto-promote it to `is_retryable()` by default. Risk: changing retry
   timing is externally visible and can turn a slow-down into a retry storm — gate behind an ADR,
   preserve today's no-retry behavior on the default path, and contain the new field via the same
   constructor approach as #3.

5. **Derive `Clone, PartialEq, Eq` and classify exhaustively in tests.** `DomainError` already
   derives all three (CONFIRMED), so the derive is cheap; it lets tests compare errors directly
   instead of `matches!`, and a nine-arm `match` in one test makes silent policy drift a compile
   error. Risk: land only *after* #3/#4 with cloneable/eq field types — `Eq` holds for `Duration`
   but not for `Arc<dyn Error>`, so the field-type choice in #3 gates this.

## Explicit non-goals

No grouping into nested `Client`/`Deployment` sub-enums: that would ripple through the ~40
`matches!(Err(AppError::Conflict))`-style assertions across `infra/tests` and `testkit` for little
gain, and the flat workspace-exhaustive enum is a strength. No `#[non_exhaustive]` — it would weaken
the compiler-checked cross-crate mapping. No HTTP types or status codes in the `app` crate, no
internal messages in client envelopes, no new variants outside the spec, and no behavior change
shipped without an ADR / `docs/PARITY.md` entry.
