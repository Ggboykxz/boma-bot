# Worker #156 — `crates/api/src/state.rs` reliability proposal (v0.4.0)

Lens: reliability. Posture: READ-ONLY analysis; no source changes here.

Evidence (CONFIRMED): read `crates/api/src/state.rs` (105 lines), `config.rs`, `auth.rs`,
`authcap.rs`, `health.rs`, `error.rs`; `crates/app/src/health.rs`, `metrics.rs`, `service.rs`
(`App`/`Services`); `crates/infra/src/db.rs`, `config.rs`, `events.rs`; `crates/bin/src/commands.rs`
(`serve` 42-98); `AGENTS.md`. Failure modes and hardening are GUESS (design inference, not
implemented, not tested).

## Role of this module
`AppState` is the one object Axum clones into every handler (`State<AppState>`). It is built once
from already-validated parts and holds five `Arc`s: `Readiness`, `Capabilities`, `AuthCapabilities`,
`HttpConfig`, and `App`. Its only I/O is `authenticate()` (DB-backed `resolve_session` /
`resolve_actor`, plus lazy local-owner creation in local-first mode). It owns no lock directly, but
it is the sole shared gateway to every interior-mutable handle in `App.services`. Reliability here
is therefore about what the state *exposes* and *guards*, not about fields it declares.

## Failure modes

### 1. Poisoned locks (process-wide, reachable through `state.app()`)
- FM-1 `AppState` has no `Mutex`/`RwLock`; its own `Debug` only reads lengths/values and is
  lock-free (CONFIRMED). But `state.app()` exposes `Services`, whose `Metrics` (`std::sync::Mutex`)
  and `BroadcastPublisher` (`Mutex<HashMap<ThreadId, Sender>>`) are shared by every request. A panic
  while either guard is held poisons it process-wide. Two holders already recover deliberately:
  `Metrics::add/render` drop the increment, and `BroadcastPublisher::channels` uses `into_inner()`
  (CONFIRMED). Any *other* holder that uses `.lock().unwrap()/.expect()` would turn one panic into a
  permanent outage of that subsystem. GUESS: the adapter mutexes were not exhaustively audited.
- FM-2 Silent recovery hides real corruption: `Metrics` drops counts and the publisher keeps a
  possibly-half-mutated map with no warning, so an operator cannot tell telemetry is degraded
  (CONFIRMED for the code path; impact GUESS).
- FM-3 `authenticate()` is `async`; a future mutable field (e.g. a session cache) guarded by a
  `std::sync::Mutex` held across `.await` would both poison on cancellation and violate the
  AGENTS.md "no lock held across .await" rule. The current method holds none (CONFIRMED).
- FM-4 `Debug` is safe today; adding a field whose `Debug` acquires a poisonable lock would make
  routine ` tracing`/panic formatting panic inside a panic — re-entrancy hazard (GUESS).

### 2. Connection-pool exhaustion (every authenticated request needs a connection)
- FM-5 The pool behind `state.app()` is built with `connect_lazy`, `max_connections` 1..=100, and a
  non-zero `acquire_timeout` (CONFIRMED in `db.rs`/`config.rs`). `authenticate()` runs
  `resolve_session` + `resolve_actor` before any handler work, so saturation turns *all* endpoints
  into 503 (`AppError::Unavailable -> ApiError::unavailable()`), not just DB-heavy ones (CONFIRMED
  mapping in `error.rs`).
- FM-6 `/ready` shares that same pool (`SELECT 1`) with no reservation. Long-lived agent/SSE work
  can starve the probe, producing a false 503 and an orchestrator killing a healthy process —
  the exact failure readiness is meant to avoid (REQ-053). No priority or separate pool exists
  (GUESS: no reserving code found).
- FM-7 No bulkhead in `AppState`: unbounded concurrent handlers all wait on `acquire_timeout`, so a
  burst exhausts the pool and stacks timeouts rather than failing fast with a bounded 503 (GUESS).
- FM-8 Lazy pool means the first requests concurrently open connections; no warm-up or
  `min_connections`, so a cold start amplifies latency exactly when traffic arrives (CONFIRMED lazy;
  impact GUESS).

### 3. Init races / startup invariants
- FM-9 In local-first mode `authenticate()` calls `ensure_local_owner(app).await` on the *first*
  request (REQ-066, CONFIRMED in `auth.rs`). Concurrent first requests can race to create the local
  owner; idempotency of that usecase is unverified (GUESS — needs the usecase/DB constraint check).
- FM-10 `AppState::new` is infallible and validates nothing: empty `Readiness`, inconsistent
  `AuthCapabilities.local_first`, or an unvalidated web root can reach runtime. Correctness rests
  entirely on `serve`'s construction order (`computer.provision().await` -> sync `build_app` ->
  `new`) (CONFIRMED). A new caller/test can bypass every bootstrap guard.
- FM-11 `Capabilities`/`AuthCapabilities` are startup snapshots inside `Arc`; immutable by design,
  so no data race today. If runtime config reload is ever added, these would silently go stale
  (GUESS, latent).
- FM-12 `invitation_ttl_seconds()` ignores `self` and returns a constant, so a deployment-level TTL
  setting would be silently ignored (CONFIRMED); a config-coupling bug that hides misconfiguration.
- FM-13 Migrations are a separate `boma migrate` step; `serve` does not run them (CONFIRMED). With a
  lazy pool, first traffic can hit a pre-migration schema if operators skip that step.

## Concrete hardening for v0.4.0 (GUESS)
1. **Uniform poison policy:** audit every lock reachable from `AppState` and recover from poison the
   same way `Metrics`/`BroadcastPublisher` do; emit a warn + counter on recovery instead of
   dropping silently. *Risk:* recovery can keep a half-mutated map; a `parking_lot::Mutex` would
   remove poisoning entirely but adds a dependency (ADR) and a workspace-wide change.
2. **Keep the state lock-free and lock-across-await-free:** document the invariant in the module
   header, add a compile-time `Send` assertion on the `authenticate()` future, and keep `Debug`
   reading only immutable fields. *Risk:* low; future fields must follow the rule.
3. **Protect readiness and auth from exhaustion:** give `/ready` a reserved connection (small
   separate pool or a semaphore slot) and add a bounded concurrent-DB-request semaphore in
   `AppState` that returns 503 fast instead of stacking `acquire_timeout` waits. *Risk:* a second
   pool consumes more server connections (must stay under the DB max); bounds need a load test;
   any `max_connections` change is a documented DEVIATION.
4. **Warm the pool on boot** (`min_connections` or a startup `SELECT 1` with retry) so cold traffic
   does not stampede connection setup. *Risk:* conflicts with "DB down -> 503, not crash loop"
   (REQ-053); must retry/continue rather than fail boot.
5. **Make local-owner init deterministic:** make `ensure_local_owner` idempotent (upsert on a stable
   key / unique constraint) or run it once before binding when `local_first`. *Risk:* boot-time DB
   write changes the lazy-start contract; prefer idempotent usecase over boot gating.
6. **Shift invariants into `AppState`:** keep `new` infallible but add `debug_assert!`s (non-empty
   readiness, `local_first` implies loopback), with full validation in `serve`; optionally make
   `invitation_ttl_seconds()` read a TTL on `HttpConfig`, or delete it if unused. *Risk:* TTL change
   is externally visible (DEVIATION); turning `new` into `Result` is a wide test change — avoid.
7. **Observability:** add counters for pool-acquire timeouts, poison recoveries and bulkhead
   rejections, rendered on `/metrics` via `state.app().services.metrics` (no addresses or secrets in
   labels). *Risk:* label cardinality; keep labels fixed.

## Verification (test-first; REQ id in test names)
- `req_066_concurrent_first_requests_create_one_local_owner`.
- `req_053_readiness_probe_succeeds_while_the_main_pool_is_saturated`.
- `req_053_auth_fails_fast_when_the_bulkhead_is_full`.
- `req_054_app_state_debug_is_lock_free_and_secret_free`.
- `req_053_a_poisoned_metrics_lock_does_not_take_down_authenticate`.

## Open decisions (GUESS where unresolved)
- Idempotency of `ensure_local_owner` and whether a reserved readiness pool is allowed (ADR).
- Whether `invitation_ttl_seconds()` is live config or dead code: check callers before H6.
- Confirm each claim with a test/command before marking CONFIRMED.
