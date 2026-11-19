# v0.4.0 architecture proposal — `crates/infra/src/stores/users.rs` (worker #409, lens: architecture)

Status: proposal only (read-only review). Claims are CONFIRMED from the module, `crates/app/src/ports.rs`
(`UserStore`), `crates/infra/src/stores/rows.rs`, `crates/infra/src/tenant.rs`,
`crates/infra/src/stores/sessions.rs`, `migrations/0002_m1.sql` and the `reset_password` call site in
`crates/app/src/accounts.rs`, unless marked GUESS.

## Current structure (CONFIRMED)

`users.rs` is a thin `PostgresUserStore { pool: PgPool }` (`Debug, Clone`, `const new`) implementing the
four-method `UserStore` port. There is no helper layer: each method writes its own SQL, and conversion
to the domain type is delegated to `rows::user()`.

- `list(limit)`: `tenant::as_system(ScimDirectorySync)` → boxed `TenantFuture` → one `SELECT` with
  `ORDER BY created_at ASC, id ASC LIMIT $1`; maps every row through `rows::user`.
- `find_by_email(email)`: direct `fetch_optional(&self.pool)`, no tenant wrapper. Returns
  `(User, String)`: the hash is cloned off `UserRow` before `rows::user()` discards it, so sign-in
  never re-queries for the hash.
- `find(id)`: direct `fetch_optional(&self.pool)`, same projection, no tenant wrapper.
- `set_password(id, hash, now)`: `tenant::as_system(Authentication)` →
  `UPDATE users SET password_hash = $2, email_verified = email_verified WHERE id = $1`; zero rows
  affected → `AppError::NotFound`. `now` is bound to `_now` and unused.

The module doc says people are global rows ("not tenant data ... queries run without a tenant
context"), and `migrations/0002_m1.sql` confirms `users` has no RLS. Reads therefore fall back to RLS
only being a no-op; the `as_system` calls add a transaction, set `boma.system_reason`, and carry a
reason that differs per method. The users table has exactly: `id, email, name, email_verified,
locale, password_hash, created_at, deactivated_at` — no `updated_at` / `password_changed_at`.

## Upgrade 1 — Make the tenant policy for people explicit and uniform (CONFIRMED gap)

Two of four methods open a system transaction (with *different* reasons: `ScimDirectorySync` for a
read, `Authentication` for a write); two query the pool directly. Pick one documented rule:
(a) global rows are read/written directly and `as_system` is used only where the audit reason is
needed, or (b) every method goes through `tenant` with a reason chosen by a tiny private helper.
Write the chosen invariant in the module doc and add a test that asserts `boma.system_reason` is
recorded (or not) consistently.

- **Risk:** medium — users have no RLS, so behaviour does not change functionally, but the audit
  trail does. Changing the reason or dropping a transaction changes observables in audit tests;
  GUESS: the `Authentication` reason on `set_password` may be relied on by an audit golden, verify
  before removal.

## Upgrade 2 — Centralize the user `SELECT` projection (CONFIRMED gap)

The same seven-column list, including `boma.rfc3339(deactivated_at) AS deactivated_at`, is pasted into
`list`, `find_by_email` and `find`. This contradicts `rows.rs`'s own promise that a column change
breaks in one place: the *shape* is centralized but the *projection* is not. Define one private
`const USER_COLUMNS` (or a `fetch_user_row(conn, key)` helper) and use it in all three paths; keep the
`ORDER BY ... LIMIT` and the `rows::user()` mapping unchanged.

- **Risk:** low — these are runtime `sqlx::query_as` calls, not the compile-time `query_as!` macro, so
  a string constant is valid and offline metadata is unaffected. A missing column would still fail
  every read at once; cover with the existing integration tests and keep REQ test names verbatim
  (`trace_check` counts declaration lines).

## Upgrade 3 — Do not load `password_hash` on paths that do not need it (CONFIRMED gap)

`UserRow` contains `password_hash` and derives `Debug`, so `list` and `find` pull a secret into memory
and into a struct whose `Debug` would print it; only `find_by_email` actually needs it. Split the row
into a narrow public projection (no hash) used by `list`/`find` and an auth-only row used by
`find_by_email`, and adjust `rows::user` / a new `rows::auth_user` mapper. Keep the port's
`(User, String)` return type intact so this stays internal.

- **Risk:** medium — touches `rows.rs` and all three SELECT lists; a mismatched mapper signature
  breaks other store modules at compile time (safe failure). Do **not** change `UserStore::find_by_email`
  to return a different type: that is a cross-crate signature change and a DEVIATION requiring an ADR.
  Verify no log/tracing call ever formats a `UserRow` (GUESS: none does today).

## Upgrade 4 — Validate the `limit` argument of `list` (CONFIRMED gap)

`limit: i64` is bound verbatim; Postgres treats a negative `LIMIT` as no limit, so a negative value
silently dumps the whole people table on a SCIM-sync path. Clamp to zero or reject non-positive
values at the top of `list` (and consider a sensible upper cap), with a test for `limit = -1`.

- **Risk:** low/medium — only inputs outside the documented "capped list" contract change. GUESS: no
  caller passes a negative limit (the call site was not enumerated here); if one does, changing it is
  a behaviour change and needs a `docs/PARITY.md` note.

## Upgrade 5 — Resolve the ignored `now` and the no-op column update (CONFIRMED gap + GUESS)

`set_password` discards `now`, passes it from `reset_password`'s clock, and uses the odd
`email_verified = email_verified` self-assignment. Either add a `password_changed_at timestamptz`
column in a versioned migration and write `boma.rfc3339(now)` into it (dropping the self-assignment),
or — if no timestamp is wanted — remove the parameter from the port and its call site. As an interim
zero-risk step, document *why* `now` is unused and why the self-assignment exists.

- **Risk:** high — a schema + port change is externally visible and needs an ADR plus a
  `docs/PARITY.md` entry and approval before v0.4.0. GUESS: the self-assignment may exist to force an
  audit trigger to fire; inspect the migrations/triggers before deleting it, or the audit trail may
  silently lose password-change events.

## Out of scope / deferred (GUESS)

Moving user creation (`create_person` / `create_account` in `accounts.rs`) into this store, typing the
timestamp strings end-to-end, and adding read-side tracing spans all touch other crates or the port
contract. They are worthwhile but larger than a single module and should be tracked as separate
proposals in `docs/STATE.md` rather than bundled here.
