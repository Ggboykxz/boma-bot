# 0378 — rate_limits.rs (reliability), v0.4.0 proposal

Module: `crates/infra/src/stores/rate_limits.rs` (lens: reliability).
Status: PROPOSAL — no code changed (READ-ONLY pass). Claims marked CONFIRMED / GUESS per AGENTS.md.

## What the code does (CONFIRMED)

`PostgresRateLimitStore::hit` parses a caller-supplied RFC-3339 `now`, derives
`bucket = unix_timestamp / window_seconds` (41), then in one transaction runs an atomic
`INSERT ... ON CONFLICT (key,bucket) DO UPDATE SET hits = hits + 1 RETURNING hits` (51-61),
a `DELETE ... WHERE bucket < bucket - 24` cleanup (62-66), and commits (67-70). Any error
becomes `AppError::Unavailable` (79-82). The use case passes `app.services.clock.now()`
(usecases.rs 423-424), so today `now` is a server clock, not a raw request header.
Schema: `key text`, `bucket bigint`, `hits integer CHECK (hits >= 1)`, PK `(key,bucket)`
(migration 0023). The counter is correct by construction **only** while that PK exists.
Scope note: API-level concerns (per-IP key choice, 429 headers, trusted proxies) are already
tracked in wave-9/0126; this proposal stays on the store's own reliability.

## Failure modes

### 1. Atomicity of counters
- **Increment itself is sound (CONFIRMED).** One upsert under the PK is the classic shared
  counter; `RETURNING` reports the post-increment count so refusal is honest under races
  (51-61, test `rate_limit.rs` 14-47). The PK is a schema invariant: drop it and `ON CONFLICT`
  fails closed to 503 — a migration guard/regression test is missing (CONFIRMED, GUESS impact).
- **Cleanup rides the hot transaction (CONFIRMED, 46-70).** Every hit also runs a `DELETE` on the
  same table and holds the freshly incremented row lock until commit. Under a storm this is
  write amplification and lock contention; concurrent hits in distinct windows can
  deadlock/serialize and abort, surfacing as auth 503 (CONFIRMED code, GUESS load impact;
  overlaps wave-9/0126 #6). The design couples all authentication to one table's availability.
- **Partial failure is fail-closed (CONFIRMED).** If the `DELETE` or `COMMIT` fails, the
  transaction rolls back and the hit is lost; because `hit` returns `Err`, callers fail closed.
  Acceptable, but a slow cleanup directly suppresses legitimate logins.
- **Fail-open conversion (CONFIRMED, 72).** `u64::try_from(i64::from(count)).unwrap_or(0)`
  maps any negative/out-of-range value to `0` — the caller sees "no hits" and grants access.
  Unreachable today (`CHECK hits >= 1`, i32 → i64 → u64 is lossless) but a latent fail-open and
  a bad default for a security counter.
- **i32 ceiling (CONFIRMED, migration 0023).** `hits integer` + `hits + 1` throws "integer out
  of range" near 2^31, turning a hot key into permanent 503 (fail-closed DoS; GUESS likelihood).

### 2. Clock skew
- **Replica skew splits a window (CONFIRMED mechanics, GUESS likelihood).** Each replica calls
  its own `clock.now()`; a node skewed past a bucket boundary writes to the neighbouring
  bucket, handing out a fresh allowance. The effective limit becomes "per skewed node", not
  "per key".
- **Port trusts arbitrary `now` (CONFIRMED, ports.rs 2004-2009).** Any caller can pass a chosen
  timestamp: a future `now` yields an empty bucket → unlimited hits. The only production
  caller today is safe, but the store offers no guard — a latent fail-open for any future
  caller or test harness (GUESS likelihood).
- **No cross-replica clock authority (CONFIRMED).** Buckets are derived in app code; nothing
  pins them to a single clock, so NTP drift, VM resume, or a bad clock changes limits directly.
- **Pre-epoch / config edges (CONFIRMED code, GUESS impact).** Rust integer division truncates
  toward zero, so timestamps in `(-window,0)` share bucket 0 with `[0,window)`; `bucket - 24`
  can overflow for extreme values. Changing `window_seconds` silently reinterprets all stored
  rows (no window stored per row), resetting or shifting limits.

### 3. Bypass via key normalization
- **Verbatim key (CONFIRMED, 42).** `key.to_owned()` is stored as-is and the PK is exact text,
  so `"User"`/`"user"`, NFC/NFD unicode, trailing whitespace, or v4/v6 spellings of one IP are
  distinct rows → split counter → bypass. The store has no canonicalization hook; it is the
  caller's job and currently unenforced.
- **No key validation (CONFIRMED).** Empty key → all keyless callers share one global bucket
  (lockout); unbounded key length → index bloat / storage DoS (GUESS impact).
- **Raw key at rest (CONFIRMED, migration 0023).** If the key embeds IP/email it is PII in a
  tenant-less table with no RLS. Error logs omit the key (79-82) — good — but storage is plain.
- **SQL injection is NOT a concern (CONFIRMED):** binds are parameterized (57-58).

## Concrete hardening for v0.4.0 (with risks)

1. **Authoritative time (high value).** Derive the bucket from `clock_timestamp()` inside the
   upsert (or reject any `now` outside ±window of server time). Removes replica skew and the
   caller-controlled-`now` bypass at once. *Risk:* port/input change and testkit determinism;
   keep an injectable clock at the use case for tests, DB clock in the adapter. Internal port
   change, not a public HTTP DEVIATION, but contract tests must be updated.
2. **Split cleanup from the hit (high value).** Make a hit a single upsert; move the 24-window
   `DELETE` to a periodic/batched janitor. *Risk:* pruning no longer happens every hit — pick a
   schedule and add an index on `bucket`; verify table growth under load before merging.
3. **Fail closed on conversion (low risk).** Replace `unwrap_or(0)` with `Err(Unavailable)`.
   No behavior change on reachable inputs.
4. **Widen `hits` to `bigint` (medium).** Removes the i32 ceiling. *Risk:* `ALTER COLUMN TYPE`
   rewrites the table; schedule a maintenance window, or keep i32 and accept 503 at overflow.
5. **Canonicalize + validate keys at the boundary (medium).** Add a `RateLimitKey` newtype:
   trim, lowercase, Unicode NFC, reject empty, cap length; build it from trusted attributes.
   *Risk:* changes effective keys and orphans existing rows — a DEVIATION needing approval plus
   a migration/dual-read plan. Minimal v0.4.0 step: validate + canonicalize in the API layer,
   keep raw storage, document the invariant. Optional keyed HMAC (no PII, fixed length) is a
   larger DEVIATION — defer.
6. **Schema guard (low risk).** Add a regression test asserting the `(key,bucket)` PK, and store
   the window per row if `window_seconds` is ever configurable at runtime.

## Acceptance tests (test-first, REQ id in name)

- Concurrent hits across two pools equal N (extend `d_01_two_instances_enforce_one_shared_limit`).
- Skewed-clock replicas land in one bucket; out-of-tolerance `now` is rejected.
- proptest: equivalent normalized keys produce one bucket; empty/oversized keys rejected.
- Forced near-overflow count yields an error, never `count = 0`.

Bottom line: the increment is atomic and fail-closed today; the real reliability gaps are the
cleanup-coupled transaction, untrusted/skewed time, and unnormalized keys. Items 1-3 are safe
v0.4.0 wins; items 4-5 need an ADR/approval because they alter behavior or the on-disk format.
