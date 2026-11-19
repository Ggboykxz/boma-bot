# Wave 15 — worker 0350 — `crates/infra/src/stores/invitations.rs` (LENS: reliability)

Target: v0.4.0. Scope: the Postgres `InvitationStore` adapter only, but findings cite the
contract, schema and call sites that it must serve. All code facts below are CONFIRMED by direct
reads in this session (`invitations.rs`, `app/src/ports.rs`, `app/src/usecases.rs`,
`migrations/0002_m1.sql`, `crates/infra/tests/m1.rs`); behavioural impact under load is GUESS where
no concurrency test was run.

## Current shape (CONFIRMED)

- `create()` inserts `(id, space_id, email, token_hash, role, expires_at, status='pending', ...)`
  in the caller's tenant via `tenant::in_space`. `token_hash` has only a **non-unique** index
  (`0002_m1.sql:297`); no UNIQUE constraint, no CHECK on `status`.
- `find_by_token_hash()` runs on a system path:
  `WHERE token_hash=$1 ORDER BY created_at DESC LIMIT 1`. **No `status` and no `expires_at`
  predicate.** It returns the newest row even if already accepted or expired.
- `accept()` takes `_now: &str` and **ignores it**; one atomic
  `UPDATE ... SET status='accepted' WHERE id=$1 AND status='pending'`, `rows_affected() > 0`.
  No expiry predicate.
- Validity is enforced only in the use case: `invite_preview` (`usecases.rs:8839`) and
  `invite_accept` (`:8884`) re-check `pending` and `expires_at <= clock.now()`. The port doc
  (`ports.rs:1079-1084`) states the bool is true exactly once under concurrent accepts.

## Failure modes

1. **Token replay / stale read (CONFIRMED code, GUESS impact).** Because the lookup filters on
   neither status nor expiry, a used or expired token still reads back a live `Invitation`.
   Correctness depends entirely on every caller repeating the app-level check. Any new caller,
   future code path, or missed check resurrects a consumed token. This splits the source of truth
   for "is this token valid" between store and use case — fragile under maintenance.
2. **Expiry bypass at write time (CONFIRMED).** `accept()` ignores `now` and never checks
   `expires_at`, so it will transition an expired invite to `accepted`. The expiry check in
   `invite_accept` happens before a multi-step write sequence, leaving a TOCTOU window: an invite
   that expires between the read (`:8881`) and the `accept()` (`:8937`) is still accepted. Contrast
   with `PasswordResetStore::consume` (`ports.rs:446-456`), which folds hash, used-at and expiry
   into one statement — the pattern this module should follow.
3. **Non-atomic accept flow (CONFIRMED).** In `invite_accept`, membership insert (`:8903-8931`)
   happens *before* `accept()`, and the bool is discarded (`let _ = ...`, `:8937`). If the
   `accept()` UPDATE fails after the membership was written, the invite stays `pending` while the
   user is already a member — replayable state with no rollback. There is no transaction spanning
   the two stores. The atomic single statement only protects the status flip, not the whole join.
4. **Duplicate audit on concurrent/replayed accept (CONFIRMED).** The audit append (`:8938-8950`)
   is unconditional, so two racing accepts both emit `InviteAccepted` for the same invite — a
   misleading audit trail for a single join event.
5. **Ambiguous hash identity (CONFIRMED).** No UNIQUE constraint on `token_hash` plus
   `ORDER BY created_at DESC LIMIT 1` means duplicate hashes are silently allowed and one row is
   picked arbitrarily. Real collisions are improbable, but malformed seeding or retries could
   create duplicates that the query resolves nondeterministically.
6. **Stringly status (CONFIRMED).** `status text` has no CHECK; any stray value makes `accept()`
   a silent no-op (`rows_affected()==0`) and the invite permanently un-acceptable, with no error.
7. **Error masking (CONFIRMED).** `tenant::unavailable` maps every SQL error to `Unavailable`,
   including constraint/serialization failures, which can mislead retry logic.

## Concrete hardening (with risks)

- **H1 — Filter validity in the read.** Change `find_by_token_hash` to accept `now: &str` and use
  `WHERE token_hash=$1 AND status='pending' AND expires_at > $2::timestamptz`. Then simplify the
  use-case checks to defence-in-depth. *Risk:* port signature change touches `MemoryInvitationStore`,
  mocks and tests; internal only, not an externally visible DEVIATION, but must keep callers passing
  the injected clock (not DB `now()`) so deterministic tests still work.
- **H2 — Enforce expiry in the write.** Use the ignored `_now`:
  `UPDATE ... WHERE id=$1 AND status='pending' AND expires_at > $2::timestamptz`, renumbering the
  bind. Add `::timestamptz` cast — lexical string comparison would be wrong. *Risk:* a caller
  passing stale `now` could accept just after real expiry; prefer the injected clock over DB
  `now()` to preserve test determinism, and document the contract.
- **H3 — UNIQUE constraint on `token_hash`** in a new migration, with a pre-cleanup step for any
  duplicates. *Risk:* migration fails on existing duplicate data; add a dedup statement first.
- **H4 — Constrain `status`** to a Postgres enum or `CHECK (status IN ('pending','accepted'))`.
  *Risk:* existing rows with other values must be migrated; app must never write other literals.
- **H5 (strongest, highest churn) — single atomic consume.** Follow `PasswordResetStore::consume`:
  `UPDATE ... WHERE token_hash=$1 AND status='pending' AND expires_at>$2 RETURNING id,space_id,email,role,...`.
  This removes the read/write TOCTOU and makes replay impossible at the storage boundary.
  *Risk:* the use case needs the email/membership check *before* consuming; reshaping the flow with
  `RETURNING` is a larger refactor and must preserve the "wrong email refuses without consuming"
  rule (REQ-042) and the uniform not-found response.
- **H6 — Idempotent audit.** Emit `InviteAccepted` only when the accept actually consumed the row
  (`if accept(...)? { append(...) }`), or add an idempotency key on `(invitation_id, kind)`.
  *Risk:* the current code intentionally audits "whoever won or was already a member"; changing
  that is a subtle audit-semantics shift — confirm against spec/05-known-defects before adopting.
- **H7 — Transaction boundary.** Wrap membership insert + accept in one transaction so a failed
  accept rolls back membership. *Risk:* high architectural cost — audit and membership stores may
  use different connections and system/RLS paths; likely out of scope for v0.4.0. Prefer H1/H2/H6.

## Recommended v0.4.0 order

H1 + H2 (small, high value: closes replay and expiry bypass at the boundary), then H3 + H4
(schema integrity), then H6 (audit fidelity). Defer H5/H7 behind an ADR — they change the flow
shape and need the spec's audit semantics confirmed first.

## Tests to add (traceability: REQ id in name)

- `req_042_find_by_token_hash_hides_accepted_and_expired` — accepted and expired rows return `None`.
- `req_042_accept_rejects_expired_invitation` — expired invite cannot flip to accepted.
- `req_042_concurrent_accept_is_single_use` — two parallel accepts, exactly one `true`, one membership,
  one audit row.
- `req_042_replay_after_accept_is_not_found` — a second accept of a consumed token fails uniformly.
- A property test over `expires_at` boundaries around the injected `now` to pin the `>` vs `>=` edge.

## Open questions for the operator (do not guess silently)

- Is a consumed/expired token's read-back required to be `NotFound` at the *store* level, or is the
  app-layer check the intended single authority? H1/H2 assume the store should enforce it.
- Should audit fire once per invite (winner only) or once per join attempt? H6 depends on the answer.
- Confirm no existing duplicate `token_hash` rows before H3 (migration safety).
