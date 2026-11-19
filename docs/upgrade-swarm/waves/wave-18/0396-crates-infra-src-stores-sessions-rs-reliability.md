# v0.4.0 Reliability proposal — `crates/infra/src/stores/sessions.rs`

Worker #396 · lens: reliability · status: proposal only (no code changed)
Scope: the Postgres `SessionStore` adapter and its consumer
(`crates/app/src/usecases.rs::resolve_session`, `crates/app/src/accounts.rs`).

Statements tagged CONFIRMED were read from the worktree this session; GUESS marks
inference. No tests were run (READ-ONLY review).

## Current behavior (CONFIRMED)
- `create` inserts (id, user_id, token_hash, expires_at, created_at). `id` and `token_hash`
  are UNIQUE in `migrations/0002_m1.sql:104-110`. Every SQL error maps to
  `AppError::Unavailable` through `tenant::unavailable` (`tenant.rs:153-156`).
- `find_by_token_hash` selects by exact `token_hash = $1`, then re-checks with
  `constant_time_eq` (lines 47-62). There is **no `expires_at` filter in SQL**.
- `delete` deletes by id and returns `()` (lines 84-91). `delete_for_user` deletes all rows
  of a user via `tenant::as_system(SystemReason::Authentication)` and returns a count
  (lines 64-82) — the only method using a system tenant context.
- Expiry is enforced only in the app layer, by RFC 3339 **string** comparison against the
  app clock (`usecases.rs:82-97`); `authenticate` discards the returned expiry (`auth.rs:75,86`).
- No `rotate`, no `revoked_at`, no purge job in this module.

## Failure modes
1. **Server-side revocation gap (CONFIRMED).** The store equates "row exists" with "session
   valid". Expiry and deactivation are checked only in `resolve_session`, a single app-layer
   choke point; any other consumer of `find_by_token_hash` would accept expired sessions.
   Revocation is physical deletion: no durable tombstone or audit, and no way to revoke one
   token while keeping the session for forensics. Reads span several separate queries with no
   snapshot: a concurrent `delete_for_user` (password reset, REQ-040) can land between the
   session lookup and the user lookup, so a revoked token can authorize one more request
   (TOCTOU). `delete` returns no count, so a caller cannot confirm rev occurred.
2. **Expiry (CONFIRMED).** There is no SQL-side expiry predicate and no purge: expired rows
   accumulate until a delete by id or user. The app compares `expires_at` as text with second
   precision; that only holds while every writer emits canonical UTC `Z` strings
   (`crypto.rs:159-191`). A row written with an offset or fractional timestamp, or clock skew
   between app instances computing `now + ttl`, breaks the comparison. TTL comes from config
   and the app clock, never Postgres `now()`. GUESS: no scheduled cleanup exists in the
   workspace (not found by grep of this module or `migrations/`).
3. **Token fixation (CONFIRMED).** The port exposes create/find/delete/delete_for_user only —
   no rotation primitive. `grant_session` always mints a new session; there is no atomic way
   to replace a token hash on privilege change (re-auth, password or email change), so a
   pre-elevation token can stay valid. The stored hash is fast unsalted SHA-256 (`crypto.rs`):
   safe against guessing (256 bits), but a DB-only disclosure permits instant offline
   verification. The extra `constant_time_eq` runs *after* the exact DB lookup, so it is
   defence-in-depth rather than the primary comparison; it also silently drops a row whose
   bytea encoding differs from the input.

Supporting risks (CONFIRMED): `tenant::unavailable` reports unique violations, FK/invalid
timestamp errors and connection loss identically as `Unavailable`, so permanent errors are
retried like transient outages; `tenant::is_unique_violation` already exists but is unused
here. `create` has no `ON CONFLICT`, so a retry after a collision fails permanently.
`delete_for_user`'s system context is inconsistent with the other three methods (sessions
carry no `space_id`, so RLS should not apply — GUESS for any other policy).

## Concrete hardening for v0.4.0
- H1. Add `AND expires_at > now()` to the `find_by_token_hash` SELECT so the store itself
  refuses expired sessions; keep the app check as a second line. Prefer a `timestamptz`
  comparison in SQL over string comparison, and stop relying on text ordering in
  `resolve_session`. Low risk; query change only.
- H2. Add a bounded periodic purge `DELETE FROM sessions WHERE expires_at < now()` (batched),
  with a partial index on `expires_at` if the table is large. Risk: purging erases the record
  of who was signed in; keep a short grace and a backlog metric.
- H3. Add `rotate(id, old_hash, new_hash, new_expires_at) -> Result<bool, AppError>` to the
  `SessionStore` port, implemented as `UPDATE sessions SET token_hash = $3, expires_at = $4
  WHERE id = $1 AND token_hash = $2`, returning `rows_affected() == 1`. Call it on
  re-authentication/elevation; `false` means a concurrent rotation or revocation, so force
  re-login. This compare-and-swap closes the fixation gap. Risk: the port change touches
  `app`, `api` and the `testkit` memory store — coordinate, and record as a DEVIATION in
  `docs/PARITY.md` if externally visible.
- H4. Add nullable `revoked_at timestamptz`, filter `revoked_at IS NULL` in
  `find_by_token_hash`, and make deletes soft revokes cleaned up by H2. Gives durable
  revocation audit and instant invalidation. Risk: a schema migration plus every reader
  adopting the predicate atomically; one missed reader is a security hole. Ship H3 first;
  treat H4 as optional for 0.4.0.
- H5. Classify SQL errors: unique violation → typed conflict (at least non-retryable),
  connection/timeout → `Unavailable`; reuse `tenant::is_unique_violation`. Add
  `ON CONFLICT (token_hash) DO NOTHING RETURNING ...` to `create` with a warn metric on
  collision. Risk: a new `AppError` variant is an API-surface change; keep the wire error
  shape unchanged.
- H6. Make tenant context consistent: either wrap all four methods in `as_system` (sessions
  are global, per the module doc) or document why only `delete_for_user` is. Removes a latent
  RLS surprise. Risk: none expected; verify against the real policies before changing.
- H7. Observability: counters for create/find/revoke and expired-on-read (H1) hits, plus a
  gauge for the expired-row backlog. Never log token hashes or session ids.

## Acceptance (tests, REQ ids in names)
- `REQ-040_find_by_token_hash_refuses_expired_row` — insert an expired session, assert `None`.
- `REQ-040_rotate_is_compare_and_swap` — a stale `old_hash` returns `false` and the token no
  longer resolves.
- `REQ-040_delete_for_user_revokes_immediately` — after the delete, lookup yields `None`.
- `REQ-040_duplicate_token_hash_is_not_unavailable` — a collision is a typed conflict, not a retry.
- `REQ-054_no_secret_in_session_logs` — capture logs, assert no token hash appears.

## Decisions needed (default marked GUESS → owner confirms)
- H4 scope: default H1, H2, H3, H5, H6, H7 for 0.4.0 and defer H4 (schema + soft delete).
- Does any policy outside `migrations/0002_m1.sql` touch `sessions` (affects H6)? Default: no.
- Where the purge job runs: reuse an existing scheduler if present, else a tokio interval in
  the binary, with an ADR. Default: reuse existing scheduler.
