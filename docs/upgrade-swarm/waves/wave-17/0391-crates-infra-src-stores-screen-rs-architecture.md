# Worker #391 — Architecture proposal: screen capability store (v0.4.0)

Lens: architecture. Module: `crates/infra/src/stores/screen.rs` (REQ-030).
Proposal only — no code changed. Claims are CONFIRMED by reading this module plus
`crates/app/src/ports.rs` (`ScreenCapability*`), `crates/app/src/usecases.rs`
(`issue_screen_capability`, `resolve_screen_capability`), `migrations/0018_screen_capabilities.sql`,
`crates/infra/src/tenant.rs`, `crates/infra/src/stores/sessions.rs`,
`crates/infra/src/stores/password_resets.rs`, `crates/infra/src/crypto.rs` and
`crates/testkit/src/memory.rs`; inferences are marked GUESS.

## Current structure (CONFIRMED)

- `PostgresScreenCapabilityStore { pool: PgPool }` is a thin adapter implementing the two-method
  `ports::ScreenCapabilityStore` port (`issue`, `find_by_hash`). It derives only `Clone` and its
  `new` is `#[must_use]` but **not** `const` — unlike every sibling store (`Debug, Clone`, `const fn new`).
- The two methods deliberately use **different tenant scopes**:
  - `issue` runs in `tenant::with_tenant(pool, context, …)` — caller-tenant scope, matching the RLS
    policy `space_id = boma.current_space_id() OR boma.is_system()`.
  - `find_by_hash` runs in `tenant::as_system(pool, SystemReason::Authentication, …)` because the
    bearer token *is* the credential (port doc: "no tenant session yet"). This cross-tenant read is
    intentional. Do not unify these scopes casually.
- `issue`: clones both newtype IDs via `.as_str().to_owned()`, moves `token_hash`, converts the
  generation with `i32::try_from(..).unwrap_or(i32::MAX)` (silent saturation), and binds `created_at: String`
  as `$5::timestamptz`. Raw parameterized `sqlx::query`; collisions on the `token_hash` PK surface as
  blanket `AppError::Unavailable` (`tenant::is_unique_violation` exists but is unused here).
- `find_by_hash`: copies the hash with `to_vec()`, selects only three columns (no `created_at`), rebuilds
  `ScreenCapability` via `ComputerId::new`/`SpaceId::new` (`?`) and converts the column back with
  `u64::try_from(..).unwrap_or(u64::MAX)`. No constant-time comparison (a stored hash, matched by exact
  PK equality). No expiry/TTL at the store — invalidation is done above, in `resolve_screen_capability`,
  by checking computer status and generation.
- Schema (`0018`): `token_hash bytea PRIMARY KEY`, FKs to `spaces`/`computers` with `ON DELETE CASCADE`,
  `screen_generation integer NOT NULL CHECK (>= 0)`, `created_at timestamptz`, index on `computer_id`,
  RLS FORCE with the policy above. There is **no purge/cleanup path** (CONFIRMED by repo-wide grep).
- Testkit twin (`memory.rs`) diverges: `issue` checks `visible(context, space_id)` and returns `NotFound`,
  stores rows in a `Vec` with no PK (a duplicate hash yields two rows; `find_by_hash` returns the first),
  and drops `created_at` entirely — whereas Postgres enforces uniqueness and lexicographic-vs-`timestamptz`
  time semantics differ from the string boundary.

## Principles to preserve

Keep the two-method port where possible, keep the asymmetric `with_tenant` / `as_system` scopes (the
bearer read is system-scoped by design), keep parameterized queries, keep `Unavailable` as the DB-facing
error, keep only the hash on disk, keep RLS isolation, and keep every REQ-030 test traceable. No feature
outside the spec without operator approval.

## Concrete upgrades for v0.4.0 (5)

### 1. Stop silently saturating `screen_generation`
`u64 → i32` with `unwrap_or(i32::MAX)` (and back with `unwrap_or(u64::MAX)`) can store a generation that
differs from the one minted, breaking the capability↔generation invariant that is the whole revocation
mechanism. Prefer widening the column to `BIGINT` and binding `i64` (still lossy only above `i64::MAX`),
or reject out-of-range generations at issuance with a typed error.
- **Why:** "make invalid states unrepresentable"; current behavior hides lossy conversion (CONFIRMED).
- **Risk:** a migration plus a contract change; a minted token that no longer matches its computer is a
  DEVIATION → ADR + `docs/PARITY.md`. GUESS impact is negligible today (generations never approach `i32::MAX`),
  so a migration may be deferred in favour of a checked conversion + test at the boundary.

### 2. Add a retention/purge path for dead capabilities
Generation bumps and computer restarts invalidate capabilities logically but never delete them; rows
accumulate until an FK cascade removes the computer/space. Add an internal `purge_before(&self, cutoff)`
(system reason `Authentication`) deleting rows older than a documented window, driven by the existing
maintenance schedule.
- **Why:** unbounded table/index growth on busy deployments; no cleanup exists (CONFIRMED).
- **Risk:** new behavior + schedule needs an ADR and operator sign-off; too aggressive a cutoff could drop
  a row still useful for incident review. Choose a conservative window (GUESS: 30 days) and test the exact
  `cutoff` boundary.

### 3. Typed time at the port boundary
Replace `created_at: String` with a domain `Instant` newtype (`chrono::DateTime<Utc>`, RFC 3339 at the
HTTP edge); bind it directly and drop the `::timestamptz` casts; make the testkit row hold the same type.
- **Why:** removes the Postgres `timestamptz` vs memory string comparison split and rejects malformed
  timestamps before they become a spurious `Unavailable` (CONFIRMED divergence; impact GUESS — low).
- **Risk:** signature changes ripple through `boma-app`, `boma-infra`, `boma-testkit` and callers — a
  DEVIATION needing an ADR/STATE entry and a full test run; legacy `0018` rows must parse under one
  canonical format.

### 4. Handle `token_hash` PK collisions explicitly
Match the INSERT error with the existing `tenant::is_unique_violation` helper and map it to a distinct
typed outcome (or a bounded app-layer retry that mints a fresh token) instead of a blanket `Unavailable`.
- **Why:** a collision currently masquerades as a database outage; the helper and pattern exist in-crate
  (CONFIRMED).
- **Risk:** SHA-256 collisions are cryptographically negligible — do not add an unbounded retry loop or a
  new timing oracle; a new `AppError` variant is a documented contract change. Clarity over complexity.

### 5. Trim boilerplate and align the adapter with its siblings
Derive `Debug`, make `new` a `const fn`, and extract a private `system<T,F>` helper owning the
`as_system` + `Box::pin` wrap so `find_by_hash` carries only SQL and binds (a parallel tenant-scope helper
can serve `issue`). In the same pass consider `query!`/`query_as!` with committed `.sqlx` offline metadata.
- **Why:** structural consistency with `sessions.rs`/`password_resets.rs` and compile-time SQL checking
  (the AGENTS.md default); CONFIRMED inconsistency.
- **Risk:** HRTB lifetime plumbing is easy to get wrong — land the helper alone first, behavior unchanged.
  Offline metadata must be regenerated and CI kept on `--locked`; sibling stores use runtime `query`, so
  macros are a local inconsistency needing an ADR — likely defer macro adoption to v0.4.x.

## Considered and rejected

- **Constant-time comparison of `token_hash`:** reject. It is a stored SHA-256 hash matched by exact PK
  equality, not a raw secret; the exact DB lookup leaks nothing usable (consistent with the sibling
  password-reset proposal). GUESS, but standard practice.
- **Widening `find_by_hash` to `in_space`:** reject — the bearer token is the credential and must resolve
  before any tenant session exists; RLS system scope is correct here.
- **Adding a store-level expiry check:** reject for v0.4.0 — validity is defined by computer status +
  generation and already enforced in `resolve_screen_capability`; duplicating it risks divergent rules.

## Acceptance checklist for v0.4.0

- [ ] Each change lands with `cargo fmt --check`, clippy `-D warnings`, and the REQ-030 tests
      (`crates/api/tests/screen.rs`, `crates/infra/tests/m1.rs`, `crates/infra/tests/computers.rs`) green.
- [ ] DEVIATIONs (items 1–3) recorded in `docs/PARITY.md` with justification and approval.
- [ ] ADRs for items 2, 3 and 5 (purge schedule, typed time, offline SQL) noted in `docs/STATE.md`.
- [ ] Memory/Postgres twins verified to agree on uniqueness, time and generation boundaries.
- [ ] Every claim re-marked CONFIRMED/GUESS after implementation (AGENTS.md).
