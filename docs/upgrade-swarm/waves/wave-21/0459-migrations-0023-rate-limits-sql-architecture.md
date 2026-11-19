# Worker #459 — `migrations/0023_rate_limits.sql` — architecture (v0.4.0)

- Module: `migrations/0023_rate_limits.sql` (15 lines, D-01 / REQ-053). Lens: architecture
  (schema as contract, boundaries, invariants, coupling, migration structure).
- Mode: READ-ONLY proposal — no product code or migration changed, no tests run. CONFIRMED = direct
  read this session; GUESS = needs a command/test/oracle run.
- Read for context (CONFIRMED): `crates/infra/src/stores/rate_limits.rs`, `crates/app/src/ports.rs`
  (1981–2009), `app/src/usecases.rs::hit_rate_limit` (418–424), `crates/infra/tests/rate_limit.rs`,
  `crates/infra/src/db.rs` (`sqlx::migrate!`, pinned `public` search path), sibling migrations
  `0001_init.sql`, `0022`, `0024`, `0027`, `0059` (last), `docs/STATE.md:1116–1117` (checksum rule),
  `spec/05-known-defects.md` D-01, and wave-17 reviews #377/#378 of the store.
- No wire/API change. **Additive only:** every upgrade is a NEW forward-only migration; 0023 itself is
  never edited. Externally visible effects are DEVIATIONs for `docs/PARITY.md` + owner approval.

## 1. Current structure (CONFIRMED)

- One table: `key text NOT NULL`, `bucket bigint NOT NULL`, `hits integer NOT NULL DEFAULT 1
  CHECK (hits >= 1)`, PK `(key, bucket)` (`:10–15`). The PK is the whole concurrency contract:
  `ON CONFLICT (key,bucket) DO UPDATE ... RETURNING hits` (store :51–61) is correct only while it exists.
- The schema stores a count and an identity; **all policy lives elsewhere**: the window is baked into
  `bucket = unix_timestamp / window_seconds` (store :41, documented in 0023 :3–5), the 24-window
  retention is a `DELETE` inside the hot transaction (store :62–66), and `count > max` refusal is
  in the HTTP layer. That separation is sound and must be kept.
- Deliberately tenant-less / outside RLS, like `plugin_submissions` (0027 :5; store :78). No FK, no
  `created_at`, no index besides the PK. `COMMENT ON TABLE` is house style (0001 :17–18) but unused here.
- Migration mechanics: `static MIGRATOR = sqlx::migrate!("../../migrations")` (db.rs :20); `boma migrate`
  is one forward-only step. 0059 is the last applied number (0033 is a skipped number, not a free slot).
  Editing 0023 after it was recorded caused a `VersionMismatch(21)` incident — checksums are pinned.

## 2. Architecture findings (CONFIRMED unless marked)

1. **The retention policy has no schema support.** The hot `DELETE ... WHERE bucket < $1` cannot use the
   PK `(key, bucket)` (leftmost column is `key`) — an index-only claim in 0023 :8–9 that no index backs.
2. **Storage type ≠ domain type.** `hits integer` (i32) vs port `u64` (ports.rs 2004); the widening at
   store :72 uses `unwrap_or(0)`, a fail-open default at the storage/contract seam.
3. **The window invariant is conventional, not structural.** Nothing in the schema records which
   window a `bucket` was derived with; a second `window_seconds` silently reinterprets live rows.
4. **The key contract is unstated in SQL.** Empty key → one global bucket for all keyless callers;
   unbounded key → index/storage growth. Callers construct prefixed keys (e.g. `auth:sign-in:<ip>`,
   `rate_limit.rs:22`), so the DB relies on a convention it does not enforce.
5. **Data classification is undocumented.** Keys may embed IP/email (PII) in a tenant-less table;
   error logs omit the key (store :79–82) — good — but the schema gives no hint to future maintainers.

## 3. Proposed additive migrations for v0.4.0

**U1 — `0060_rate_limits_bucket_index.sql`: give retention an access path.**
`CREATE INDEX IF NOT EXISTS rate_limits_bucket ON rate_limits (bucket);`
*Risk:* every counter insert now maintains a second index (small write cost, more bloat on hot keys).
No behavior change. Pairs with U3; do not remove the inline `DELETE` until a pruner is scheduled
(coordinate with #460 / 0377 U2 — this proposal does not duplicate that work).

**U2 — `0061_rate_limits_hits_bigint.sql`: align storage with the port.**
`ALTER TABLE rate_limits ALTER COLUMN hits TYPE bigint USING hits::bigint;`
The `CHECK (hits >= 1)` survives; widening is value-preserving, no data loss.
*Risk:* `ALTER COLUMN TYPE` takes `ACCESS EXCLUSIVE` and may rewrite the table — schedule a window.
Store code must read `i64` (a cross-module change owned by #377 U4). Removing the i32 ceiling is
externally visible only near 2^31 — record in `docs/PARITY.md`; GUESS the frequency is negligible.

**U3 — `0062_rate_limits_prune_function.sql`: one SQL owner for retention.**
`CREATE OR REPLACE FUNCTION boma_rate_limits_prune(bucket_min bigint) RETURNS bigint LANGUAGE sql AS $$
WITH gone AS (DELETE FROM rate_limits WHERE bucket < bucket_min RETURNING 1)
SELECT count(*) FROM gone $$;` — idempotent, safe to call twice. Later the store/janitor calls it
instead of the inline `DELETE`, so the statement lives in one place.
*Risk:* an unused schema object until Rust calls it; grants/`search_path` must be explicit; if the
job is wired before it is scheduled, the table grows unbounded. Keep the inline `DELETE` as the
fallback during the transition (two owners temporarily, by design, then one).

**U4 — `0063_rate_limits_key_check.sql`: enforce the key contract, in two phases.**
`ALTER TABLE rate_limits ADD CONSTRAINT rate_limits_key_check CHECK (key <> '' AND length(key) <= 512)
NOT VALID;` then a later migration `VALIDATE CONSTRAINT rate_limits_key_check;` — the additive pattern
that avoids blocking writers on a live table.
*Risk:* `VALIDATE` fails if legacy rows violate it — run a read-only count first (GUESS: none, all
observed keys are prefixed). The cap must exceed the longest real key (IPv6 + route prefix + a
future hashed key); rejecting empty keys changes behavior only for a caller that uses one (GUESS: none).

**U5 — `0064_rate_limits_schema_contract.sql`: document the invariant at the boundary.**
`COMMENT ON TABLE rate_limits IS 'Tenant-less D-01 counters; keys may embed IPs (PII); no RLS by
design.'` and `COMMENT ON COLUMN rate_limits.bucket IS 'floor(epoch_seconds / window_seconds);
window owned by the application, not stored here.'` (house style: 0001 :17–18).
*Risk:* comments do not enforce anything and can rot — pair with a regression test asserting the PK
and the `ts/window` formula. Do **not** add a `window_seconds` column / new PK in v0.4.0: it changes
row identity, resets live counters on deploy, and is a DEVIATION needing an ADR + approval (0377 U5).

## 4. Cross-cutting / verification

- Order U1 → U2 → U3 → U4 → U5; one concern per file, D-01/REQ-053 header comments like 0022/0024,
  `IF NOT EXISTS`/`IF EXISTS` for local dev even though runtime is forward-only. Never reuse the 0033 gap.
- No PII in comments or logs; keep the table outside RLS; keep the `count > max` decision in HTTP.
- Verify: apply all new files to a fresh DB via `boma migrate`; re-run `d_01_two_instances_enforce_one_shared_limit`;
  add a read-only assertion for the PK and index (`pg_indexes`); refresh sqlx offline metadata if a
  compile-time-checked query changes. Mark CONFIRMED only with command output; open decisions in `docs/STATE.md`.
