# 0460 — `migrations/0023_rate_limits.sql` reliability proposal (v0.4.0)

Module: `migrations/0023_rate_limits.sql`
Lens: reliability
Spec anchor: D-01, REQ-053 (shared abuse counters); spec/05-known-defects.md §Notes (D-04/D-01
compound); REQ-043/ADR-002 (RLS contract); spec/02-data-model.md §RateLimit.

Status of claims: "Current behavior" is CONFIRMED by reading this migration,
`crates/infra/src/stores/rate_limits.rs`, `crates/app/src/ports.rs`, `0002_m1.sql`, `0027_plugin_submissions.sql`
and `crates/infra/tests/rate_limit.rs` this session. Proposals and risks are GUESS (design inference,
not implemented or tested).

## Current behavior (CONFIRMED)

- One table: `key text NOT NULL`, `bucket bigint NOT NULL`, `hits integer NOT NULL DEFAULT 1 CHECK
  (hits >= 1)`, `PRIMARY KEY (key, bucket)`. No `created_at`, no upper bound on `hits`, no length
  bound on `key`, no index on `bucket`, no `GRANT`/`REVOKE`, no RLS policy.
- Deliberately tenant-less (no `space_id`), like `plugin_submissions` (see 0027 comment and the
  `unavailable()` note in the store). No migration in `migrations/` grants or revokes any privilege.
- The store computes `bucket = at.unix_timestamp() / window` in Rust from a caller-supplied RFC 3339
  `now` **and a per-call `window_seconds`** (callers use 60, 300 and 3600). It upserts
  `hits = hits + 1` and, in the same transaction, runs `DELETE FROM rate_limits WHERE bucket < $1`
  with `$1 = bucket - 24` — no window scoping, no `LIMIT`.

## Failure modes

1. **Constraint ceiling and key shape.** `hits integer` has a hard ceiling of 2,147,483,647. Under a
   sustained single-key flood the upsert eventually raises `22003 integer out of range`, which
   `unavailable()` maps to `AppError::Unavailable` (fail-closed), but it also stops recording hits
   for that key: the transaction aborts and the row is stuck with no way to lower it (`CHECK hits>=1`)
   until the bucket rolls. `key` is unbounded text inside the PK: a long/attacker-influenced key
   bloats the PK index, makes every compare O(len), and stores plaintext IP-derived keys (personal
   data) with no schema-level bound. `CHECK (hits >= 1)` constrains only the lower side, so an
   over-limit row keeps taking a write on every request — a hot row exactly during an attack.
2. **Window atomicity.** Because `bucket` is `epoch / window_seconds`, different windows live in
   different integer namespaces: for 2026 a 60 s window is ~2.8e7, a 300 s window ~5.7e6, a 3600 s
   window ~4.9e5. The cleanup `bucket < this_bucket - 24` is a bare cross-namespace comparison, so
   a single 60 s hit deletes every 300 s and 3600 s bucket for **all** keys immediately (quota reset,
   widened abuse window, silent loss of shared state) — the opposite of the "24 windows" comment.
   The shared counter also depends on replica clocks agreeing on `now`; skew splits one logical
   window across two PK rows and doubles the effective allowance. The cleanup `DELETE` has no
   supporting index (PK leads with `key`), so it sequential-scans the whole table inside the hit
   transaction on every hit; lock hold time grows with table size, making the limiter the bottleneck
   under the flood it is meant to stop. The delete is unbounded.
3. **RLS / privilege boundary.** Non-RLS is intentional and correct for cookieless hits. But with no
   `GRANT`/`REVOKE`, every role able to connect via `BOMA_DATABASE_URL` (GUESS) can read all keys,
   i.e. all client IPs — no defense in depth. Naively "fixing" this by adding a policy like the
   tenant tables' `space_id = boma.current_space_id() OR boma.is_system()` would break every
   anonymous rate-limited endpoint: `boma.current_space_id()` is unset for cookieless requests
   (0002_m1.sql), so the upsert would be denied and all public routes would 500. The limiter's own
   `Err` path is therefore only as reliable as the privileges of the connection that runs it.

## Concrete hardening for v0.4.0 — additive only (new migration, no edit to 0023)

1. Widen `hits` to `bigint` with `ALTER TABLE rate_limits ALTER COLUMN hits TYPE bigint` (additive:
   no data loss, removes the 2^31 ceiling); optionally add a documented upper-bound `CHECK` after
   measuring real maxima. Add `created_at timestamptz NOT NULL DEFAULT now()` for age-based cleanup.
   *Risk:* an upper-bound check could reject legitimate traffic — derive the bound from spec limits
   first; verify the type change against a populated test DB.
2. Add `CHECK (char_length(key) BETWEEN 1 AND 256)` after confirming no existing row exceeds it.
   *Risk:* a legitimate internal key longer than 256 makes the migration fail — query `max(char_length(key))`
   first (GUESS: current keys are `kind:<ip>`, short, so this should pass).
3. `CREATE INDEX CONCURRENTLY rate_limits_bucket_idx ON rate_limits (bucket)` to support the cleanup
   scan without blocking writers. *Risk:* `CONCURRENTLY` cannot run inside a transaction block — run
   it as its own statement in the additive migration; it also adds write overhead per hit (measure).
4. Make cleanup window-safe and bounded: with `created_at` + the bucket index available, recommend
   (in the owning module) replacing `bucket < this_bucket - 24` with an age predicate
   (`created_at < now() - interval '2 hours'`) executed in bounded `LIMIT` batches outside the hot
   hit transaction, and deriving the bucket from DB time rather than caller `now` to remove clock
   skew. *Risk:* this crosses into `rate_limits.rs` (another module's lens) and changes quota-reset
   timing — gate as a DEVIATION if externally visible, and test cross-window keys explicitly.
5. Privilege hardening without RLS: `REVOKE ALL ON rate_limits FROM PUBLIC`, grant only the DML the
   limiter needs to the actual application role, and add a `COMMENT ON TABLE` recording that the
   table is tenant-less by design so no future migration adds a `current_space_id()` policy.
   *Risk:* the role name comes from the connection URL (GUESS) — resolve it from config and test with
   a real DB; an over-broad revoke turns every hit into `Unavailable` (all public routes fail).

## Constraints

- Additive only: a new migration after 0059; never rewrite 0023 or mutate existing rows destructively.
- Preserve the shared-counter invariant (two replicas, one limit — `d_01_two_instances_enforce_one_shared_limit`)
  and the tenant-less, cookieless design (no `space_id`, no RLS policy).
- No externally visible quota/reset change without a `docs/PARITY.md` DEVIATION and approval; test-first
  per REQ id, and mark every new claim CONFIRMED/GUESS in docs.
