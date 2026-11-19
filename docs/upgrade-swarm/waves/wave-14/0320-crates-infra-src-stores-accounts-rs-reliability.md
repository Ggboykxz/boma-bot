# Worker 320 — `crates/infra/src/stores/accounts.rs`, lens: reliability (v0.4.0)

## Evidence and scope

Read in `before-m5` this session: `accounts.rs`, `tenant.rs`, `stores/users.rs`,
`app/src/ports.rs`, `app/src/accounts.rs`, `domain/src/account.rs`, migrations `0002_m1.sql`
and `0051_lowercase_users_email.sql`. Source facts below are CONFIRMED; concurrency outcomes
are reasoned from the absence of locks (GUESS until the proposed tests run). No code changed.

## Failure modes

**F1 — TOCTOU on "sole owner" (CONFIRMED code shape, GUESS outcome).**
`delete_account` (accounts.rs:207-219) decides which organizations to delete with a
`SELECT ... NOT EXISTS (other member)` subquery, then deletes in a second statement. Neither
the parent `organizations` row nor the `organization_members` rows are locked
(`SELECT ... FOR UPDATE`, advisory lock, or SERIALIZABLE). A concurrent invitation acceptance
inserting a second member can commit between the decision and the `DELETE`, so the org — and its
spaces — disappear while a live member remains. `scim_deprovision` (accounts.rs:174-184) deletes
*all* of a user's org/space memberships with no owner check at all, so the same path can leave an
org ownerless. Conversely, `create_account` hardcodes `role = 'owner'` (accounts.rs:73-81,
96-105) and the schema (0002_m1.sql:93-99,133-139) has no constraint guaranteeing at least one
owner per org/space: the invariant "every tenant has an owner" exists only in application
read-then-write code. `scim_set_deactivated` (accounts.rs:238-283) kills sessions but leaves
memberships, so the two deprovision paths disagree about ownership state.

**F2 — password hash races (CONFIRMED code shape, GUESS outcome).**
`UserStore::set_password` (users.rs:73-102) is an unconditional
`UPDATE users SET password_hash = $2 ... WHERE id = $1`: no expected old value, no version, no
row lock, and it runs in a *separate* `as_system` transaction from the reset-token `consume`
(app/src/accounts.rs:294-313). The gap between `consume` and `set_password` contains an Argon2
`await`; any concurrent password write (admin reset, sign-in request, SCIM) can land there and be
silently overwritten — last writer wins, while the losing token is already burnt. Sign-in reads
the hash with a plain `fetch_optional` (users.rs:53-63) with no transaction and no re-check, so a
hash changed during verification can still yield a session; the later `delete_for_user`
(accounts.rs:314) can be racing a session created after it. `password_hash` is a plain `String`
carried in `(User, String)` tuples (users.rs:67-69, ports.rs:32): no zeroize, no `SecretString`,
and any `Debug` log of the tuple leaks it.

**F3 — email normalization is split across two runtimes (CONFIRMED).**
`Email::new` uses `trim().to_ascii_lowercase()` (domain/src/account.rs:29-41), but the backfill
migration 0051 uses SQL `lower(email)`, which is locale/Unicode aware; the two disagree on
non-ASCII input (`İ`, `ẞ`) and on IDN/trailing-dot forms. `users.email` is byte-compared
`text ... UNIQUE` (0002_m1.sql:70-79) and `find_by_email` uses `WHERE email = $1`
(users.rs:58), while the mail store uses `lower(u.email) = $2` (mail.rs:225). Result (GUESS):
two case-variant rows can coexist, recovery/invitation lookup can miss a backfilled row, and
`fetch_optional` with no `ORDER BY` can return either twin at sign-in. Migration 0051 is a
one-shot backfill, so any raw-SQL writer reintroduces the split.

## Concrete hardening (v0.4.0)

1. Make the owner invariant a database invariant. Add a deferred constraint trigger on
   `organization_members` and `space_members` that aborts a transaction ending with zero
   `'owner'` rows for a parent, plus a partial unique index if exactly one owner is intended.
   `delete_account` and `scim_deprovision` then reassign ownership to another member *before*
   removing rows, or return `AppError::Conflict` when no candidate exists.
2. Serialize the sole-owner decision: in `delete_account`, lock the parent organization row
   (`SELECT ... FOR UPDATE`, membership rows ordered by key) inside the existing `tenant::as_system`
   transaction before the `NOT EXISTS` check, and take the same lock in the invitation-accept
   write path. Keep the lock order identical in both stores.
3. Turn `set_password` into a compare-and-swap: add `password_version` (or
   `password_changed_at`) and an `expected` argument to the port; `UPDATE ... WHERE id = $1 AND
   version = $2 RETURNING ...`, `Conflict` on zero rows. Move token `consume`, `revoke_all`,
   `set_password` and `delete_for_user` into one store-level transaction so no password write can
   interleave with a consumed token. After Argon2 verification, re-read `password_hash` by id and
   constant-time compare before issuing a session (do NOT hold a row lock across Argon2).
4. Wrap `password_hash` in `secrecy::SecretString` at the port boundary; no derived `Debug`, and
   a test asserting no hash appears in tracing output.
5. Give email one canonical key: a generated `email_key` column with `lower()`/NFKC normalization
   and a UNIQUE index, reused by `find_by_email`, the mail store and invitations; make
   `Email::new` use the same rule, or explicitly reject non-ASCII addresses at the boundary.
   Backfill + dedup before adding the index; re-run 0051's logic as an idempotent migration.
6. Tests (REQ-tagged, two-transaction barriers): invite-accept racing `delete_account` never
   deletes a live org; concurrent deprovisions never leave an ownerless tenant; stale
   `set_password` returns Conflict; old-password login after a reset fails; `Person@Example.COM`
   and `person@example.com` yield one row and both lookups find it.

## Risks / deviations

- The constraint trigger and the invitation-path lock touch other stores; wrong lock order risks
  deadlocks. Contention on hot org rows increases.
- Changing `set_password`'s signature ripples through app, testkit memory store and use cases;
  merging reset steps into one transaction is a larger refactor (new port method).
- Rejecting non-ASCII emails or merging duplicate accounts is externally visible: DEVIATION
  entries in `docs/PARITY.md` plus owner approval are required before shipping. A generated-column
  migration needs an ADR (alternatives: `citext` extension, application-only rule).
- `SecretString` is not `PartialEq`; fixtures need an explicit exposure helper.
- All claims here are code inspection; concurrency effects are GUESS until the barrier tests run.
