# Worker #377 — `crates/infra/src/stores/rate_limits.rs` architecture proposal (v0.4.0)

Lens: architecture (structure, layering, boundaries, invariants, error model). Read-only review;
no source changed. Layout claims are **CONFIRMED** from a source read of `rate_limits.rs`,
`app/src/ports.rs` (`RateLimitStore`, `RateLimitDecision`), `app/src/usecases.rs::hit_rate_limit`,
`app/src/service.rs`, `infra/tests/rate_limit.rs`, `testkit/src/memory.rs::bucket_of`,
`migrations/0023_rate_limits.sql`, `api/src/authroutes.rs` behaviour notes in the wave-9 review,
and `docs/openbot-swarm/waves/wave-4/0039-contact-us-in-privacy.md`. Impact notes are **GUESS**
until a build/test run verifies them.

## Current structure (as built)

- One 82-line adapter `PostgresRateLimitStore { pool: PgPool }` implementing the single-method port
  `RateLimitStore::hit(&self, key: &str, now: &str, window_seconds: u64) -> RateLimitDecision`.
  CONFIRMED.
- Layering is sound: the port and DTO live in `app`, the clock is injected in `usecases.rs` (423–424),
  the limit/key policy and the `count > max` refusal live in the HTTP layer, and the store only
  persists a counter. The store is deliberately tenant-less (no RLS), like `plugin_submissions`.
  CONFIRMED; this boundary should be kept.
- `hit` mixes two responsibilities in one transaction: the atomic upsert (52–61) and lazy pruning
  `DELETE ... WHERE bucket < bucket - 24` (62–66). CONFIRMED.
- Time handling is duplicated: this file parses RFC 3339 and computes `unix_timestamp()/window`
  (35–41); the in-memory adapter repeats both in `bucket_of` (memory.rs 3055, 3079+). CONFIRMED.
- Weak invariants / error model: (a) bad `now` or non-positive `window` maps to `AppError::Unavailable`,
  conflating caller input with a DB outage (36–40); (b) `window` is not part of row identity, so
  reusing a key with a different window silently reinterprets existing rows; (c) DB `hits` is
  `i32` while the port returns `u64`, and `unwrap_or(0)` (72) turns a failed widening into a false
  zero count; (d) every `sqlx::Error` collapses to `Unavailable` with a `warn!` that logs only the
  driver error (78–81) — no typed taxonomy, no client leak (good), no detail tier. CONFIRMED.
- `key.to_owned()` (42) is a borrow-lifetime workaround for the async bind; likely unnecessary but
  harmless. CONFIRMED code, **GUESS** on whether `.bind(key)` compiles with the `'static` query.

## Proposed upgrades for v0.4.0

**U1 — Narrow the port to typed inputs; compute the bucket once.** Move RFC-3339 parse + bucket
division into `usecases::hit_rate_limit` (clock already there) and make the port
`hit(key: &str, bucket: i64)`. Delete `bucket_of` duplication in the memory adapter. One formula,
one owner; adapters become dumb counters.
*Risk:* port change ripples to infra, testkit memory, and all `Arc<dyn RateLimitStore>` wiring
(bin/commands.rs, api tests, infra tests). Preserve the exact `ts/window` formula to keep D-01
behaviour; verify goldens. **GUESS** on call-site count (grep suggests ~8).

**U2 — Separate maintenance from the hit path.** Remove the `DELETE` from the hot transaction and
expose an idempotent `prune` (port method or dedicated job) so `hit` is one atomic upsert. Pure
separation of concerns: a counter write should not decide retention.
*Risk:* pruning must exist and be scheduled *before* removal or the table grows unbounded; keep a
threshold fallback. Adding a port method touches every adapter. **GUESS** on scheduler availability.

**U3 — Fix the error model at the boundary, not in the store.** Validate `now`/`window` in the app
layer as a typed reject; in the adapter map only DB failures to `Unavailable`, log driver detail at
`debug!` behind a private `StoreError`, keep the public error format unchanged (REQ-054).
*Risk:* changing a bad-input code from `Unavailable` is externally visible — check no caller relies
on it; never surface raw SQL text to clients. **CONFIRMED** current collapse.

**U4 — Align counter types end to end.** Versioned migration: `hits BIGINT` (safe widening of the
`CHECK (hits >= 1)` column); carry `i64` through `RETURNING` and map to `u64` with an explicit error
on impossible overflow instead of `unwrap_or(0)`. Use a named `FromRow` hit struct, consistent with
sibling stores, rather than `query_scalar::<_, i32>`.
*Risk:* migration on a live table needs a rollback plan; a false `0` today can under-count a hot key
and let an attacker through — fixing it changes observable counts under overflow (record in
`docs/PARITY.md`). **GUESS** on real overflow frequency (very low at 2^31).

**U5 — Make the window an explicit invariant.** Encode `window_seconds` in the key namespace
(e.g. `w300:...`) or store it as a column and assert it matches, so one key cannot switch window
silently. Pair with the U3 positive-window validation.
*Risk:* changing row identity resets live counters on deploy — externally visible; stage as a
documented invariant + assertion first, gate any key change behind approval. **GUESS** on callers
that reuse a key across windows.

## Cross-cutting notes (not separate upgrades)

- Keep fail-closed behaviour and the `count > max` decision in the HTTP layer; the store must stay
  policy-free. The hot-counter and boundary-burst concerns from the wave-9 reliability review are
  performance/policy work, not architecture defects — coordinate, do not duplicate, here.
- Keys may embed client IPs (PII): the existing `warn!` logs only the error, not `key`. Preserve that;
  if span context is added, hash or truncate the key.

## Priority / verification

Order U1 → U3 → U4 → U2 → U5 (schema and visible changes last). After each step run
`cargo fmt --check`, `cargo clippy -- -D warnings` and the REQ-053/D-01 infra integration tests;
mark claims **CONFIRMED** only with command output. Record visible changes (U4, U5) in
`docs/PARITY.md` and open decisions where the spec is silent in `docs/STATE.md`.
