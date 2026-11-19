# v0.4.0 architecture proposal — `crates/infra/src/stores/accounts.rs` (worker #319, lens: architecture)

Status: proposal only (read-only review). Claims below are CONFIRMED from the module, its port,
`crates/infra/src/tenant.rs` and `migrations/0002_m1.sql` / `0030_audit_events.sql` unless marked GUESS.

## Current structure (CONFIRMED)

`accounts.rs` is a thin `PostgresAccountStore { pool: PgPool }` implementing the five-method
`boma_app::ports::AccountStore`. Every method opens a transaction through `tenant` and writes raw
parameterized SQL inside a boxed `TenantFuture`; no helper layer exists between the port and SQL.

- `create_account`: `in_space(new_space_id)` — five sequential INSERTs (users, organizations,
  organization_members, spaces, space_members) — then a hand-built `Account`. Only the users
  INSERT maps a unique violation to `AppError::Conflict`; the other four map to `tenant::unavailable`.
- `create_person`: `as_system(ScimDirectorySync)` — one users INSERT, same unique mapping, same
  hand-built `User` shape (REQ-045).
- `scim_deprovision`: `as_system(ScimDirectorySync)` — DELETE sessions / space_members /
  organization_members, then UPDATE `deactivated_at`. No row-count or existence check.
- `delete_account`: `as_system(AccountExport)` — DELETE organizations where the caller is the sole
  member, then DELETE the user; everything else is left to `ON DELETE CASCADE`. Ignores `_now`.
- `scim_set_deactivated`: `as_system(ScimDirectorySync)` — optional session DELETE, then two
  variants of UPDATE `deactivated_at`. Ignores `_now`.

The policy decision "which `SystemReason` justifies crossing tenants" is made inline in this adapter
(`ScimDirectorySync` vs `AccountExport`), and all destructive semantics live in the migration FKs,
not in code. Returned structs restate DB defaults in Rust (`email_verified: false`,
`deactivated_at: None`, `locale`) instead of reading them back.

## Upgrade 1 — Give deletion its own system reason, not `AccountExport` (CONFIRMED gap)

`delete_account` runs an irreversible cascade under `SystemReason::AccountExport` (REQ-044 is an
export path). A deletion is a different exception with a different retention meaning, and the reason
is written into the audit trail via `boma.system_reason`. Add a distinct `SystemReason::AccountDeletion`
(touching `domain/src/tenancy.rs`: enum, `ALL`, `as_str`, `parse`, its round-trip test) and use it here.

- **Risk:** medium — the closed enum is a cross-crate contract; adding a variant requires updating the
  `ALL` array and every exhaustive match or CI breaks, and any consumer that snapshots `as_str()`
  values changes. If a DEVIATION is not wanted, the minimum alternative is a documented ADR that
  deliberately reuses `AccountExport` for both, so the choice stops looking accidental.

## Upgrade 2 — Deduplicate the user INSERT and the unique-violation mapping (CONFIRMED gap)

`create_account` and `create_person` carry the same users INSERT and the same
`is_unique_violation -> AppError::Conflict` / `unavailable` match. Extract a private
`insert_user(conn, &NewUser) -> Result<User, AppError>` helper inside the module (or a small
`users_insert.rs` sibling) and call it from both. This is the one row shared by every entry point.

- **Risk:** low — internal refactor, no port or SQL change. Must preserve the REQ-042 / REQ-045
  integration test names verbatim (trace_check counts declaration lines). Keep the helper private so
  it cannot become a second public way to create a person.

## Upgrade 3 — Read writes back instead of restating DB defaults in Rust (CONFIRMED gap)

Both creation methods construct `User` / `Account` from their inputs, duplicating the column defaults
in `migrations/0002_m1.sql`. Use `INSERT ... RETURNING` (or one re-SELECT in the same transaction)
for the fields that come back, and drop the hardcoded `false` / `None` / `locale` copies. This also
gives a place to assert the invariant that `space.organization_id == organization.id` before the
transaction commits, instead of trusting the caller.

- **Risk:** medium — an extra round-trip or a `RETURNING` clause per insert; RLS scope inside
  `in_space(new_space_id)` must still see the rows (global users/organizations have no policy, so
  CONFIRMED likely safe, but verify with the integration suite). A `RETURNING` that omits a field
  silently changes the reply shape -> cover with the existing golden/parity tests.

## Upgrade 4 — Classify non-unique constraint failures instead of blanket `Unavailable` (CONFIRMED gap)

Every error on the org / membership / space INSERTs — FK violation, CHECK failure, invalid timestamp
cast — becomes `AppError::Unavailable`, which `AppError::is_retryable()` treats as a transient 503.
A malformed `created_at` or a broken org/space id is therefore retried with backoff and finally
reported as "service unavailable" rather than a data error. Introduce a small local error-mapping
helper that separates unique violations (`Conflict`), driver/connection failures (`Unavailable`)
and constraint/serialization failures (a non-retryable typed error, e.g. `AppError::Rule` or a new
variant).

- **Risk:** high — touches the shared error taxonomy and the worker retry gate (REQ-024), so it is a
  DEVIATION needing a `docs/PARITY.md` entry and approval. Keep the client-facing error envelope in
  `crates/api` unchanged and never leak the raw driver message (REQ-054).

## Upgrade 5 — Make destructive and deactivation semantics explicit and testable (CONFIRMED gap + GUESS)

`delete_account` delegates the whole cascade to schema FKs, and its doc comment claims audit events
keep a NULL actor — but `audit_events.actor_user_id` is plain text with no FK, and
`audit_events.space_id` is `ON DELETE CASCADE`, so those rows disappear with the space (CONFIRMED).
`scim_deprovision` and `scim_set_deactivated` both delete sessions and update `deactivated_at` with
overlapping logic and no existence check. Consolidate the deactivation primitives into one helper,
and add a schema-backed integration test (testcontainers) that proves, for a seeded account, exactly
which rows survive deletion and records the real behaviour in the comment. Add a `tracing::info`
span for the multi-row delete (ids only, never email/hash).

- **Risk:** medium/high — deletion is irreversible and its blast radius is the whole tenant tree; a
  test that asserts the wrong expectation could "fix" code toward data loss. The consolidation must
  keep `scim_deprovision`'s extra membership deletions distinct from a plain deactivation (REQ-045).
  GUESS: no tracing span exists today — `tenant::unavailable` logs only on failure (CONFIRMED).

## Out of scope / deferred (GUESS)

Typing timestamps end-to-end (the `created_at` / `now` fields are `String` bound as `::timestamptz`,
and `_now` is ignored in two methods) is a cross-crate ports change; worthwhile but larger than this
module. An idempotency key for `delete_account` and a soft-delete/tombstone stage would change
externally visible behaviour and need an ADR before v0.4.0.
