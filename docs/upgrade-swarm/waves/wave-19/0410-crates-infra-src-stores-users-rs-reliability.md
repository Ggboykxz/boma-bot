# v0.4.0 reliability proposal — `crates/infra/src/stores/users.rs` (worker #410, lens: reliability)

Status: proposal only (read-only review; no code changed). Claims are CONFIRMED code inspection
unless marked GUESS. Evidence read this session: `users.rs`, `stores/rows.rs` (`UserRow`,
`rows::user`), `tenant.rs` (`as_system`, `unavailable`, `is_unique_violation`),
`app/src/ports.rs` (`UserStore`), `app/src/accounts.rs` (sign-in, `reset_password`),
`domain/src/account.rs` (`Email::new`), `migrations/0002_m1.sql`,
`migrations/0051_lowercase_users_email.sql`, `spec/01-requirements.md` (REQ-040/042/045).

## Failure modes

**F1 — Email normalization is split across three rules (CONFIRMED code shape, GUESS outcome).**
`Email::new` canonicalizes with `trim().to_ascii_lowercase()` (`domain/src/account.rs:29–41`),
but the backfill (`0051_lowercase_users_email.sql:7`) uses SQL `lower(email)` (Unicode/locale
aware) and the mail store compares `lower(u.email) = $2` (`mail.rs:225`). `users.email` is a
byte-compared `text ... UNIQUE` column (`0002_m1.sql:72`), and `find_by_email` binds
`email.as_str()` into `WHERE email = $1` (`users.rs:58–60`), i.e. it assumes a canonical value
already exists. Consequences (GUESS): (a) a row backfilled/inserted with `İ`, `ẞ`, an IDN form,
trailing dot or mixed case is unreachable because no `Email` can reproduce its bytes; (b) byte
UNIQUE allows `A@x.com` and `a@x.com` to coexist as two rows with distinct `UserId`s; (c)
`fetch_optional` has no `ORDER BY`, so which twin wins is arbitrary. Because the same query
returns the password hash (`users.rs:65–70`), an arbitrary twin can pair one person's address
with another person's hash at sign-in. Migration 0051 is one-shot, so any raw-SQL writer
reintroduces the split.

**F2 — Duplicate owners via identity duplication (CONFIRMED schema, GUESS outcome).**
Membership tables are `PRIMARY KEY (org|space_id, user_id)` with `role text NOT NULL`, no CHECK
and no "exactly one owner" constraint (`0002_m1.sql:93–99,133–139`). An owner is a `UserId`, so
the two ids of a case-variant twin (F1) can both hold `role='owner'` on the same tenant: two
owner rows for one address. `delete_account`'s sole-owner `NOT EXISTS` decision (worker #320 F1)
then sees a live second owner and refuses to delete, or a deprovision removes one twin and
leaves the other. `list(limit)` (`users.rs:29–51`) is the REQ-045 SCIM directory source but has
no `deactivated_at` filter and a hard `LIMIT`; a deactivated owner is exported as present and a
truncated page silently omits people, so neither path can reconcile who still owns a tenant.
(Keeping several owners is legal per spec — the reliability defect is *ambiguous identity*, not
the count.)

**F3 — Stale password and identity reads (CONFIRMED code shape, GUESS outcome).**
`set_password` (`users.rs:73–102`) is an unconditional `UPDATE ... WHERE id = $1`: no expected
old hash, version or row lock; `now` is discarded as `_now` and the statement carries the no-op
`email_verified = email_verified`. There is no `updated_at`/`password_changed_at` column
(`0002_m1.sql:70–79`), so writes cannot be ordered or detected and last-writer-wins clobbers a
concurrent reset: `reset_password` burns its token (`accounts.rs:297`) and spends an Argon2
`await` (`accounts.rs:309`) in gaps another writer can occupy (workers #409/#320 concur).
`find_by_email`/`find` use plain `fetch_optional(&self.pool)` with no transaction, so a hash
changed after the read can still validate a session built on the old hash. All three reads
return `deactivated_at` (via `boma.rfc3339`) with no filter: freshness of "active" status depends
on every caller repeating the checks at `accounts.rs:143` and `usecases.rs:93`; a new caller that
forgets gets an identity that should have been revoked.

## Concrete hardening (v0.4.0)

1. **Detect ambiguity before trusting it (low risk, no port change).** Replace `find_by_email`'s
   `fetch_optional` with a `LIMIT 2` query; if two rows come back, return `AppError::Conflict`
   and a `tracing::error!` carrying both ids — never an arbitrary sign-in. Add a test/fixture
   asserting no case-variant twins exist. This alone turns F1's silent wrong-account sign-in
   into a loud, diagnosable failure.
2. **One canonical key for a person (structural fix for F1/F2).** Add a generated `email_key`
   column normalized by the exact `Email::new` rule, backfill + dedup, `UNIQUE (email_key)`, and
   route `find_by_email`, `mail.rs` and invitations through it; keep `users.email` as display
   value. Re-run 0051's logic as an idempotent migration and explicitly reject the non-ASCII
   forms where SQL `lower` disagrees with `to_ascii_lowercase`. Before deleting a twin, reassign
   FKs (`organization_members`, `space_members`, `sessions`, `bots`) to the surviving id.
3. **Make ownership drift visible (operator signal, not a hard reject).** Add
   `CHECK (role IN ('owner','admin','member'))` on both membership tables and a monitoring query
   that flags tenants with more than one owner whose `user_id`s share an `email_key`; alert the
   operator rather than rejecting the write, since multiple owners are allowed.
4. **CAS password writes (fix F3).** Add `password_changed_at timestamptz NOT NULL DEFAULT now()`
   (or a `bigint` version) and extend `set_password` with an expected value:
   `UPDATE ... WHERE id=$1 AND password_changed_at=$expected RETURNING ...`, `Conflict` on zero
   rows. Move `consume` + `revoke_all` + `set_password` + `delete_for_user` into one store-level
   transaction; after Argon2, re-read the hash by id and constant-time compare before issuing a
   session (never hold a row lock across Argon2). Give `now` a real write and drop the no-op
   assignment only after confirming no audit trigger depends on the UPDATE firing.
5. **Fresh SCIM projection (fix stale directory).** In `list`, clamp `limit` to a positive bounded
   value, keep `ORDER BY created_at, id`, expose `deactivated_at` explicitly, and add `updated_at`
   so REQ-045 sync can reconcile changed/removed people instead of re-listing a capped snapshot.

## Tests (REQ-tagged, two-transaction barriers)

- `REQ-040_case_variant_email_is_one_identity_and_lookup_is_unambiguous`: both casings resolve to
  one row; a raw-SQL mixed-case twin yields Conflict, not a sign-in.
- `REQ-040_stale_set_password_is_conflict_and_old_password_login_fails`; a racing reset loser
  never overwrites the winner's hash.
- `REQ-042_twin_owner_ids_alert_and_deprovision_never_leaves_a_tenant_ownerless`.
- `REQ-045_scim_list_rejects_negative_limit_and_marks_deactivated_person`.

## Risks / deviations

- Items 2–5 touch schema, `UserStore` signatures, `mail.rs`, invitations and testkit memory store:
  ADRs + `docs/PARITY.md` entries + owner approval before v0.4.0. Dedup merges two people and
  reassigns FKs — externally visible and easy to get wrong (wrong order orphans sessions).
- Rejecting non-ASCII addresses, and generated-column vs `citext` vs app-only rule, each need an
  ADR; `email_key` must be indexed before the monitoring query is enabled on every sync.
- The audit-trigger role of `email_verified = email_verified` is GUESS until migrations/triggers
  are inspected; removing it could silently drop password-change audit events.
- A stricter `set_password` surfaces as a 409 to clients (DEVIATION) and ripples through app,
  testkit and use cases.
- All concurrency outcomes above are GUESS until the barrier tests run.
