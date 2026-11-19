# Worker 0508 — `migrations/0048_password_resets.sql` — reliability lens (v0.4.0 proposal)

Status: proposal, additive only. Claims tagged CONFIRMED were verified by reading the named files in
this session; proposals/effects are GUESS (inferred, not yet measured). No existing object is dropped,
retyped or rewritten: every item below is `ADD CONSTRAINT ... NOT VALID`, `CREATE INDEX`, or
`CREATE FUNCTION`. No row is modified at migration time.

## Current shape (CONFIRMED)

- `password_reset_tokens(id text PK, user_id text FK users ON DELETE CASCADE, token_hash bytea UNIQUE,
  expires_at timestamptz, used_at timestamptz NULL, created_at timestamptz DEFAULT now())`.
- Indexes: `(user_id, created_at DESC)` and partial `(token_hash) WHERE used_at IS NULL`.
- `ENABLE` + `FORCE ROW LEVEL SECURITY`, one policy `USING/WITH CHECK (boma.is_system())`.
- `is_system()` reads transaction-scoped GUC `boma.system_reason` set via `set_config(..., true)`
  (0002_m1.sql:38-42; tenant.rs:120-126) — privilege is reverted at transaction end.
- Store: `create` is a plain INSERT; `consume` is one conditional `UPDATE ... WHERE token_hash=$1 AND
  used_at IS NULL AND expires_at > $2 RETURNING user_id`; `revoke_all_for_user` sets `used_at` for all
  outstanding rows (stores/password_resets.rs). App `reset_password` consumes first, then calls
  revoke and `set_password`, ignoring their errors (accounts.rs:294-313).

## Failure modes

**Token uniqueness.** (U1) The UNIQUE index spans *all* rows forever. A hash collision (bad RNG or a
degenerate 0-length hash) makes `create` fail, and the store maps the DB error to
`AppError::Unavailable` (503) — a retry-able client error becomes an opaque outage (CONFIRMED code,
GUESS impact). (U2) No length/format CHECK: a truncated or empty `token_hash` can be stored once and
then satisfy bytea equality for a degenerate lookup. (U3) `id text PK` has no non-empty constraint, so
a blank id is a legal row. (U4) Because consumed/expired rows are never removed, the unique index and
the partial "active" index grow without bound; inserts get slower and the hash namespace can never be
reclaimed (GUESS: measurable after ~10^6 issuances).

**Expiry.** (E1) No `CHECK (expires_at > created_at)`: a clock-skew or negative-TTL bug stores a token
that is never redeemable, with no DB signal. (E2) No upper bound: a far-future `expires_at` yields an
effectively non-expiring credential. (E3) `consume` compares against the *application-supplied* `now`
string, not the DB clock (CONFIRMED); skew between app instances means one instance honors a token
another considers expired. (E4) The partial index predicate is `used_at IS NULL`, so expired-but-unused
rows stay in the "active" index forever; expiry is not part of any index and no cleanup exists
(CONFIRMED) — bloat plus a full-scan risk for any future purge. (E5) Expiry is enforced only in
`consume`; there is no DB-level assertion that a consumed row was actually unexpired.

**RLS.** (R1) Deny-by-default is correct (CONFIRMED). (R2) `FORCE` also binds the table owner: any
future maintenance/purge run as owner (without the GUC) silently affects 0 rows. (R3) `is_system()`
depends on a GUC set with `true` (transaction scope) — safe today (CONFIRMED), but a future call using
session scope would leak "system" across pooled connections; nothing in this migration guards that.
(R4) `consume` + `revoke_all_for_user` + `set_password` are separate statements with swallowed errors
(CONFIRMED), so a failure after consume burns the token and can leave sibling links live after a
password change.

## Concrete additive hardening (GUESS — proposed SQL)

1. Integrity CHECKs, added `NOT VALID` then `VALIDATE` to avoid a long exclusive lock:
   `ADD CONSTRAINT password_reset_tokens_ttl_positive CHECK (expires_at > created_at) NOT VALID;`
   `ADD CONSTRAINT password_reset_tokens_hash_sized CHECK (octet_length(token_hash) = 32) NOT VALID;`
   `ADD CONSTRAINT password_reset_tokens_id_not_blank CHECK (length(btrim(id)) > 0) NOT VALID;`
   Risk: existing corrupt rows block `VALIDATE` — pre-flight `SELECT` to count offenders; digest
   width must match the configured hasher (32 assumed for SHA-256, verify before merging).

2. Optional max-TTL ceiling (needs ADR/config, not forced):
   `CHECK (expires_at <= created_at + interval '24 hours') NOT VALID`.
   Risk: rejects legitimate longer TTLs; exposes a policy decision, so keep it behind an ADR.

3. Purge support: `CREATE INDEX password_reset_tokens_expires ON password_reset_tokens (expires_at);`
   plus `CREATE FUNCTION boma.purge_expired_password_resets(retention interval, batch int)` that
   deletes in bounded batches (`... AND expires_at < statement_timestamp() - retention LIMIT batch`),
   `SECURITY DEFINER`, `SET search_path = pg_catalog, boma`, `REVOKE EXECUTE ... FROM PUBLIC`.
   Risk: SECURITY DEFINER is a privilege-escalation surface — scope to this table only; deletion is
   a runtime data change (retention policy) that needs explicit owner approval, not a silent default.

4. Optional atomic consume-and-revoke function returning `user_id`, so consume + revoke happen in one
   statement/transaction; the app still performs `set_password` separately.
   Risk: changes error semantics/test doubles (DEVIATION if observable), so gate behind the ADR.

5. Verification (test names carry REQ-040): `req_040_rejects_nonpositive_ttl`,
   `req_040_rejects_wrong_hash_length`, `req_040_purge_keeps_unexpired_rows`,
   `req_040_owner_without_guc_sees_no_rows`, `req_040_reset_is_single_use_under_race`.

## Recommendation

Ship items 1 and 3-index first (low risk, no behavior change); treat max-TTL, purge execution and the
atomic consume function as separate ADR-backed decisions. Explicitly do **not** rely on a session-scoped
GUC or on app-clock equality for expiry.
