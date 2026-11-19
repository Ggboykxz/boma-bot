# Wave 17 — worker 0370 — `crates/infra/src/stores/password_resets.rs` (LENS: reliability)

Target: v0.4.0. Scope: the Postgres `PasswordResetStore` adapter, with the port, schema and
call sites cited as evidence. All code facts below are CONFIRMED by direct reads in this session
(`password_resets.rs`, `app/src/ports.rs:418-466`, `app/src/accounts.rs:221-336`,
`migrations/0048_password_resets.sql`, `crates/infra/tests/m1.rs`). Impact under load is GUESS:
no concurrency or fault-injection test was run.

## Current shape (CONFIRMED)

- All three methods run through `tenant::as_system(..., SystemReason::Authentication, ...)`; the
  table is RLS-protected with `FORCE` and a system-only policy (`0048:18-22`) — a good boundary.
- `create()` inserts `(id, user_id, token_hash, expires_at, created_at)`; `token_hash bytea` is
  UNIQUE (`0048:9`), with no CHECK on hash length or `expires_at > created_at`. `consume()` is one
  atomic statement:
  `UPDATE … SET used_at=$2 WHERE token_hash=$1 AND used_at IS NULL AND expires_at > $2 RETURNING user_id`.
  Hash, single-use and expiry are folded into the write — the right pattern.
- `revoke_all_for_user()` is a separate statement
  `UPDATE … SET used_at=$2 WHERE user_id=$1 AND used_at IS NULL` with no expiry predicate.
- `tenant::unavailable` maps **every** SQL error (including the unique violation on `token_hash`
  and serialization failures) to `AppError::Unavailable`. A malformed returned `user_id` also
  maps to `Unavailable` (`password_resets.rs:78`).
- Call sites: `request_password_reset` mints, hashes, inserts and (opt-in) logs the raw link at
  WARN (`accounts.rs:234-258`); `reset_password` does `consume()` → `revoke_all_for_user()`
  (result discarded) → `set_password()` → `delete_for_user()` → audit → session grant
  (`accounts.rs:294-335`). Each store call is its own `as_system` unit of work.

## Failure modes

1. **Same-token replay is correctly prevented (CONFIRMED code).** The conditional UPDATE takes a
   row lock; Postgres EvalPlanQual rechecks `used_at IS NULL` after a concurrent winner commits, so
   exactly one racer gets the `user_id`. This is the module's strongest property — preserve it.
   Residual risk (GUESS): correctness still depends on the caller passing a monotonic, trusted
   `now`; a clock that jumps backwards reopens an already-expired token.
2. **Consume and revoke are not one transaction (CONFIRMED).** They are separate `as_system`
   calls and the revoke result is ignored (`let _ = …`, `accounts.rs:303-307`). A failure or crash
   between them leaves *other* live links for the same account; a completed password reset does
   not reliably invalidate them. This defeats the stated REQ-040 intent ("every outstanding link
   dies with this recovery").
3. **Cross-token reset race (CONFIRMED code, GUESS impact).** Two *different* valid tokens for one
   user can both pass `consume()` concurrently; both proceed to `set_password()` and both grant a
   session. There is no per-user lock, no compare-and-set on a password version, and revoke cannot
   undo the first consume. Last writer wins on the password; two sessions survive.
4. **Clock and time representation (CONFIRMED).** `now`, `expires_at` and `created_at` cross the
   port as `String`; only the SQL casts (`$n::timestamptz`) keep the comparison correct. A malformed
   string becomes a DB error surfaced as `Unavailable`. `used_at` is stamped with the *caller's*
   `now`, so clock skew can record a consumption before creation. Boundary is `>` (CONFIRMED);
   the exact-expiry instant is refused — pin that edge with a test.
5. **Duplicate hash / retry misclassification (CONFIRMED).** `create()` has no idempotency key. A
   retried request with the same token violates the UNIQUE constraint, which `tenant::unavailable`
   reports as `Unavailable`, inviting pointless retries and misleading metrics. No CHECK constrains
   hash length, so a truncated hash could match an unintended row (improbable with a sound hasher).
6. **No retention or GC (CONFIRMED).** `0048` has no cleanup job or partition; used and expired rows
   accumulate forever, the partial index grows, and the UNIQUE constraint permanently reserves
   every hash ever issued. Deletes happen only via `users` ON DELETE CASCADE. Audit history lives
   in the separate audit table, so row deletion does not erase the security record.
7. **Secret/hash leakage surface (CONFIRMED code, GUESS likelihood).** The raw recovery link and
   user id are written to process output at WARN when `BOMA_AUTH_PRINT_RESET_LINK=true`
   (`accounts.rs:248-258`) — an operator mailbox by design, but it puts a live credential in logs,
   contrary to the repo's "no secrets in logs" rule. The stored `token_hash` is a high-entropy
   digest (never the raw token — good), but DB equality matching is not constant-time and the
   schema carries no hash-algorithm version for future rotation. RLS prevents tenant reads.
8. **Error-class masking (CONFIRMED).** Unknown/used/expired token collapse to `None` — intentional
   and good for uniform refusal — but a unique violation, a serialization failure and a corrupt
   `user_id` are all reported as `Unavailable`, so callers cannot distinguish retryable from
   permanent failures.

## Concrete hardening (with risks)

- **H1 — Keep the atomic consume; tighten its contract.** Document that single-use relies on the
  conditional UPDATE, add `AND used_at IS NULL` recheck coverage in tests, and bind `now` once.
  *Risk:* none functionally; the main danger is a future refactor back to SELECT-then-UPDATE.
- **H2 — Make consume + revoke atomic in one system transaction.** Expose a single port method (or
  a transaction callback) so `consume` marks the used token and revokes the user's other tokens in
  one `as_system` unit; stop discarding the revoke result. *Risk:* port signature change touches
  `MemoryPasswordResetStore`, mocks and tests (internal, not an externally visible DEVIATION), and
  needs a defined error path if revoke fails after consume.
- **H3 — Serialize per-user resets.** Add a Postgres advisory lock on `hashtextextended(user_id)`
  around consume+revoke, or a `password_version` compare-and-set in the users row adopted by
  `set_password`. *Risk:* advisory locks add connection/lifetime care under pooling; a version
  column is a migration + caller change. Higher churn than H1/H2 — weigh against the low
  probability of two live links being used simultaneously.
- **H4 — Typed time at the boundary.** Change `now`/`expires_at`/`created_at` to `DateTime<Utc>`
  (or a domain newtype) in the port and bind typed values, dropping stringly timestamptz. Add
  `CHECK (expires_at > created_at)` and a hash-length CHECK in a migration. *Risk:* touches port,
  adapter and memory store; beneficial churn but must keep deterministic test clocks.
- **H5 — Classify unique violations.** Match SQLSTATE `23505` in `create()` and return a distinct
  domain error (or log-and-treat-as-success for an identical reissue) instead of `Unavailable`;
  map corrupt `user_id` to a logged invariant failure, not `Unavailable`. *Risk:* error taxonomy
  change may ripple into API mapping; confirm the uniform-refusal contract for REQ-040 first.
- **H6 — Retention sweep.** Add a periodic DELETE of `used_at IS NOT NULL OR expires_at < now()`
  rows (batched, with `LIMIT` and a sleep between batches), or range-partition by `created_at`.
  *Risk:* does not change external behavior, but must confirm no downstream consumer reads raw
  reset rows for analytics; audit events are the authoritative record.
- **H7 — Reduce link exposure.** Keep `BOMA_AUTH_PRINT_RESET_LINK` off by default (CONFIRMED), add
  a prominent warning that output is sensitive, and store a `hash_version` column for rotation.
  *Risk:* print-link is an intentional operator fallback in the spec — do not remove it; only
  harden logging hygiene. Hash-version migration is additive and low risk.

## Recommended v0.4.0 order

H1 + H2 (protect the core single-use guarantee and make revocation reliable), then H5 + H4
(error clarity and type safety), then H6 + H7 (hygiene). Defer H3 behind an ADR — it changes the
reset flow shape and needs the REQ-040 concurrency expectation confirmed.

## Tests to add (traceability: REQ id in the test name)

- `req_040_concurrent_consume_of_same_hash_is_single_use` — two parallel consumes, one `Some`.
- `req_040_consume_refuses_expired_and_used_tokens` — covers `>` vs `>=` at the injected `now`.
- `req_040_revoke_all_for_user_invalidates_every_other_live_link` — after reset, no other token
  consumes; pin this across the H2 transaction boundary.
- `req_040_create_duplicate_hash_returns_distinct_error` — H5 classification.
- `req_040_expired_rows_are_removed_by_retention_sweep` — H6, with audit rows preserved.

## Open questions for the operator (do not guess silently)

- Is `password_version`/advisory-lock serialization (H3) in scope, given REQ-040 is broad and two
  simultaneous resets of one account are unlikely but not impossible?
- Does any downstream consumer read raw `password_reset_tokens` rows, or are audit events the only
  required record (H6)?
- Confirm D-08 ("auth delegated wholesale to a library") does not already cover H7's logging
  concern before opening a new defect entry.
