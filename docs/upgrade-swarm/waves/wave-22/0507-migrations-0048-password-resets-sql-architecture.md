# Worker #507 — Architecture proposal: `migrations/0048_password_resets.sql` (v0.4.0)

Lens: architecture. Module: `migrations/0048_password_resets.sql` (REQ-040).
Proposal only — no code changed. CONFIRMED by reading 0048, 0049, 0050, 0052, 0055,
`0002_m1.sql`, `crates/infra/src/db.rs`, `crates/infra/src/crypto.rs`,
`crates/infra/src/stores/password_resets.rs` and the wave-17 store proposal; GUESS where noted.
Constraint: **additive only** — every item below is a new forward-only migration file that
CREATEs / ADDs / COMMENTs. Nothing here drops, rewrites or back-fills 0048 objects in place.

## Current structure (CONFIRMED)

- One table: `password_reset_tokens(id text PK, user_id text FK→users ON DELETE CASCADE,
  token_hash bytea NOT NULL UNIQUE, expires_at timestamptz NOT NULL, used_at timestamptz,
  created_at timestamptz DEFAULT now())`. `user_id` FK gives account-delete cleanup (REQ-044).
- Indexes: `(user_id, created_at DESC)` and a partial `(token_hash) WHERE used_at IS NULL`.
- RLS: ENABLE + FORCE + `password_reset_tokens_system_only` using `boma.is_system()` — rows are
  deployment-scoped, no space tenant, matching the non-tenant block in `0002_m1.sql`.
- `crypto.rs` mints opaque tokens and stores SHA-256 → exactly 32 bytes (CONFIRMED).
- `db.rs` embeds `sqlx::migrate!("../../migrations")`; migrations are forward-only and applied
  once, with an idempotency/replay test in `crates/infra/tests/database.rs` (CONFIRMED).
- The partial `(token_hash)` index is largely subsumed by `UNIQUE(token_hash)` for point lookups;
  the consume predicate is actually `used_at IS NULL AND expires_at > now` (CONFIRMED).

## Principles to preserve

Keep the system-only RLS envelope, hashed-at-rest tokens, single-use `used_at` semantics,
parameterized SQL and forward-only, replay-safe migrations. No new feature outside REQ-040
without owner approval. No destructive change to an existing migration.

## Additive upgrades for v0.4.0 (5)

### U1 — Promote "one live link per account" to a database invariant
New migration: `CREATE UNIQUE INDEX password_reset_tokens_one_live ON password_reset_tokens
(user_id) WHERE used_at IS NULL;`. Today the invariant lives only in app discipline; the store
proposal (wave-17 up2) notes `request_password_reset` can leave several live links (CONFIRMED).
- Why: REQ-043 says enforce in the data layer; a partial unique index makes "one live link"
  unrepresentable-elsewhere and makes revoke/issue paths O(1) per user.
- Risk: externally visible behavior change (a second request collides or invalidates the first).
  Pre-flight `SELECT user_id, count(*) … WHERE used_at IS NULL HAVING count(*) > 1` must be empty;
  the app must revoke prior live rows in the same transaction or map the unique violation, or
  two rapid legitimate requests will surface as a spurious `Unavailable`. Record as DEVIATION.

### U2 — Add an expiry-aware partial index for consume and future purge
New migration: `CREATE INDEX password_reset_tokens_expiry ON password_reset_tokens (expires_at)
WHERE used_at IS NULL;`. This serves `expires_at > now` filtering and a cutoff-based purge;
leave the existing partial `(token_hash)` index in place (additive rule).
- Why: the current indexes do not cover the expiry predicate; consumed/expired rows accumulate
  with no cleanup path (CONFIRMED), so a purge primitive will need this access path.
- Risk: one more index write per INSERT/consume; a small table may see no planner benefit.
  A purge *schedule/policy* is a separate owner decision (ADR). Do not tie the cutoff to the
  RECOVERY_TTL without review — incident/audit review may need recent rows.

### U3 — Add structural CHECK constraints
New migration: `ALTER TABLE password_reset_tokens ADD CONSTRAINT … CHECK` for
`octet_length(token_hash) = 32` (SHA-256, CONFIRMED), `expires_at > created_at`,
`used_at IS NULL OR used_at >= created_at`, and `length(btrim(id)) > 0`.
- Why: malformed digests and inverted lifetimes are currently storable; the constraint turns
  crypto/schema assumptions into enforced facts instead of silent lookup misses.
- Risk: forward-only — a single pre-existing violating row aborts a deploy; run the same
  predicates as a pre-flight SELECT. The 32-byte constant is coupled to SHA-256: if the hash
  primitive changes, this migration must change too. Keep messages free of token material.

### U4 — Document the security contract in the database itself
New migration: `COMMENT ON TABLE password_reset_tokens` and `COMMENT ON COLUMN` for
`token_hash`, `expires_at`, `used_at`; `COMMENT ON POLICY password_reset_tokens_system_only`.
State: hash-at-rest only, single-use, bounded TTL, system-scoped RLS, and stdout delivery via
`BOMA_AUTH_PRINT_RESET_LINK` for mail-less operators (from 0048's header).
- Why: the schema is the contract operators and auditors read first; the constraints in U1/U3
  are non-obvious without it. Zero functional risk (CONFIRMED: comments do not affect plans).
- Risk: comments drift if the app's TTL/delivery changes; tie wording to REQ-040 / spec §8.2
  and keep it anti-enumeration neutral (no wording that distinguishes expired from used).

### U5 — Add a guarded purge function, not a purge schedule
New migration: `CREATE OR REPLACE FUNCTION boma.password_reset_purge(before timestamptz)
RETURNS bigint` that returns `NULL`/raises unless `boma.is_system()`, deletes rows with
`expires_at < before`, and returns `row_count`. Pair with U2's index.
- Why: gives a reviewable, bounded cleanup primitive in the data layer with the same RLS
  discipline as the rest of `boma.*` (CONFIRMED pattern in `0002_m1.sql`).
- Risk: deletion vs audit/incident needs; cutoff and cadence need owner sign-off (GUESS: 30 days
  past expiry) before any job calls it. A SQL function is a new callable surface — document it
  and test `expires_at = before` as a boundary. Do not wire a scheduler in this migration.

## Considered and rejected

- Do **not** drop the redundant partial `(token_hash)` index or widen RLS to `in_space` — both
  break the additive rule / the no-tenant design. Do **not** add a `token_hash` UNIQUE change:
  it already exists. Do **not** add constant-time comparison in the database; it is a stored
  SHA-256 digest, not a raw secret. Defer typed-time/collision mapping to the store-layer
  proposal (wave-17) so this migration stays structural.

## Acceptance checklist for v0.4.0

- [ ] Each item ships as its own numbered, forward-only migration; 0048 is untouched.
- [ ] `cargo fmt --check`, clippy `-D warnings`, and `crates/infra/tests/database.rs`
      (migrations apply + replay) green after each step.
- [ ] Pre-flight queries for U1/U3 are clean; DEVIATION for U1 recorded in `docs/PARITY.md`;
      ADRs for U2 purge policy and U5 recorded in `docs/STATE.md`.
- [ ] Every claim re-marked CONFIRMED/GUESS after implementation (AGENTS.md).
