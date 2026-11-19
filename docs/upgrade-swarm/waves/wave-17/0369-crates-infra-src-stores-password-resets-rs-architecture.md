# Worker #369 — Architecture proposal: password reset store (v0.4.0)

Lens: architecture. Module: `crates/infra/src/stores/password_resets.rs` (REQ-040).
Proposal only — no code changed. Claims are CONFIRMED by reading this module plus
`crates/app/src/ports.rs`, `migrations/0048_password_resets.sql`, `crates/infra/src/tenant.rs`,
`crates/app/src/accounts.rs`, `crates/testkit/src/memory.rs` and sibling stores in this
worktree; inferences are marked GUESS.

## Current structure (CONFIRMED)

- `PostgresPasswordResetStore { pool: PgPool }` is a thin adapter (`Debug, Clone`, `const fn new`)
  implementing the three-method `ports::PasswordResetStore` port: `create`, `consume`,
  `revoke_all_for_user`.
- Each method clones owned arguments into an HRTB closure and runs it via
  `tenant::as_system(pool, SystemReason::Authentication, |conn| Box::pin(async move { … }))`.
  This is right for REQ-043: rows are deployment-scoped and the RLS policy
  `password_reset_tokens_system_only` requires `boma.is_system()`. Preserve this scoping.
- `create`: raw `sqlx::query` INSERT, five binds, `map_err(tenant::unavailable)`.
- `consume`: one atomic `UPDATE … WHERE token_hash=$1 AND used_at IS NULL AND
  expires_at > $2::timestamptz RETURNING user_id`, `fetch_optional`, then `UserId::new` with
  failure mapped to `AppError::Unavailable`. Concurrent submissions cannot both win.
- `revoke_all_for_user`: `UPDATE … SET used_at=$2 WHERE user_id=$1 AND used_at IS NULL`,
  returns `rows_affected()`.
- Schema: `token_hash bytea NOT NULL UNIQUE`, partial index `(token_hash) WHERE used_at IS NULL`,
  index `(user_id, created_at DESC)`. No retention or cleanup path exists.
- The testkit twin compares timestamps as strings (`expires_at.as_str() > now`); Postgres
  compares `timestamptz`. `tenant::is_unique_violation` is used by cards/accounts/mcp/messages/
  runs but not here.

## Principles to preserve

Keep the port shape where possible, keep system-scoped RLS, keep single-statement consume, keep
parameterized queries, keep `Unavailable` as the only database-facing error, and keep every
REQ-040 test name traceable. No new feature outside the spec without operator approval.

## Upgrades for v0.4.0 (5)

### 1. Typed time at the port boundary
Replace `now: &str` / `expires_at: String` with a domain `Instant` (`chrono::DateTime<Utc>`
newtype, `FromStr` + RFC 3339 formatting at the HTTP edge). Bind the typed value directly and
drop the `$n::timestamptz` casts; make the testkit store compare the same type.
- Why: removes the Postgres-vs-memory semantic split (lexicographic string compare vs
  `timestamptz`) and rejects malformed timestamps before they reach the driver as a spurious
  `Unavailable` (CONFIRMED divergence; impact GUESS — low without mixed offsets).
- Risk: port signature changes in `boma-app`, `boma-infra`, `boma-testkit` and callers — a
  DEVIATION requiring an ADR/STATE entry and a full test run; any stored legacy string must
  parse under one canonical format or migration `0048` rows become unreadable.

### 2. Enforce a single live token atomically at issuance
In `create`, wrap "mark prior active rows used" and the INSERT in the existing
`as_system` transaction: `UPDATE … SET used_at=$now WHERE user_id=$uid AND used_at IS NULL`
before the insert, returning the count. Keep `revoke_all_for_user` for post-reset use.
- Why: `accounts.rs::request_password_reset` creates a link but never revokes earlier ones, so
  several live links can coexist; the port doc says an account should hold no second live link
  (CONFIRMED).
- Risk: externally visible behavior change (a previously issued link stops working) — record as
  DEVIATION or get spec confirmation first. The extra UPDATE must stay in the same transaction
  or a crash leaves the user with no link; verify with a test that two requests leave one live row.

### 3. Handle the `token_hash` UNIQUE collision explicitly
Mirror the established sibling pattern: match the INSERT error with
`tenant::is_unique_violation(&error)` and map to a distinct typed outcome (or a bounded retry
at the app layer that mints a fresh token), instead of the current blanket `Unavailable`.
- Why: a collision currently looks like a database outage and can be logged/acted on as one;
  the helper and the pattern already exist in this crate (CONFIRMED).
- Risk: a retry loop must be bounded and must not change response shape or add a timing oracle;
  `AppError` may need a new variant, which is a contract change to document. Hash collisions are
  cryptographically negligible — prefer clarity over complexity here.

### 4. Add a retention/purge path
Add an internal `purge_expired(&self, cutoff)` (system reason `Authentication`) deleting rows
with `expires_at < cutoff`, driven by the existing scheduled job or a maintenance command, with
a documented window (GUESS: 30 days past expiry). Extend the partial index or add
`(expires_at) WHERE used_at IS NULL` if the planner needs it.
- Why: consumed and expired rows accumulate forever; the schema has no cleanup (CONFIRMED), so
  storage and index bloat grow with reset traffic.
- Risk: new behavior+schedule needs operator decision and an ADR; an aggressive cutoff could
  delete rows still needed for an audit or incident review — choose the window conservatively
  and test boundary rows at exactly `cutoff`.

### 5. Cut boilerplate with a private system-scope helper and offline-checked SQL
Extract a private `async fn system<T, F>(&self, body: F) -> Result<T, AppError>` that owns the
`as_system` + `Box::pin` wrapping, so each method only carries its SQL and binds. In the same
pass, consider `query!`/`query_scalar!` with committed `.sqlx` offline metadata so column and
type drift fails at compile time (AGENTS.md default).
- Why: three near-identical closures obscure the actual data access; compile-time SQL matches
  the project's SQLx standard (CONFIRMED duplication).
- Risk: HRTB lifetime plumbing is easy to get wrong — land the helper alone first with no
  behavior change. Offline metadata must be regenerated and CI kept on `--locked`/offline;
  sibling stores use runtime `query`, so adopting macros here is a local inconsistency that
  needs an ADR and may be better deferred to v0.4.x.

## Considered and rejected

Do **not** add constant-time comparison for `token_hash`: it is a stored hash, not a raw secret;
the DB equality check is fine and the testkit's `Vec<u8>` compare need not change (GUESS, but
standard practice). Do not widen tenant scoping to `in_space` — these rows have no space tenant
by design.

## Acceptance checklist for v0.4.0

- [ ] Each change lands with `cargo fmt --check`, clippy `-D warnings`, and the REQ-040 tests
      (`crates/infra/tests/m1.rs`, `crates/api/tests/http_surface.rs`) green.
- [ ] DEVIATIONs from items 1–2 are recorded in `docs/PARITY.md` with justification and approval.
- [ ] New typed-error variant (item 3) documented in `docs/STATE.md`; ADR for items 4–5.
- [ ] Every claim in this doc re-marked CONFIRMED/GUESS after implementation (AGENTS.md).
