# 0172 — crates/app/src/health.rs — reliability (v0.4.0)

Worker #172 · module `crates/app/src/health.rs` (app layer) · lens: reliability · READ-ONLY review of
`rebuild-workspace/.worktrees/before-m5` at v0.3.0-rc5. Proposals only; no product code changed.
Code facts below are CONFIRMED by reading the named files; reasoned/inferred claims are marked GUESS.

## What the file already does right (keep, don't re-propose)

- Liveness/readiness split is explicit and correct (health.rs 1-8): `/health` is memory-only, `/ready`
  probes dependencies; this prevents a DB blip from killing an otherwise healthy process.
- `ReadinessCheck::check()` returns a static reason, never a driver error (health.rs 16-25), and the
  DB probe logs the real error only at `debug` (db.rs 125-140). `/ready` is unauthenticated, so no
  secret leaks (REQ-054). Keep this shape — golden `ready.json` allows adding fields, not removing them.
- `tokio::time::timeout` per probe (health.rs 59) and a config guard `readiness_timeout <=
  request_timeout` (api/src/config.rs 119-121) give a bounded single probe.
- `Readiness` is immutable after construction (`Vec<Arc<dyn …>>`), safe to share via `AppState`.

## Failure modes → concrete hardening

### F1. Dependency checks run sequentially: the per-probe deadline is not a request deadline

CONFIRMED: `report()` awaits each probe in a `for` loop (health.rs 56-77). One probe's timeout is
`timeout`, so N probes cost up to `N × timeout` and a slow earlier probe head-of-line blocks the rest.
The config guard only compares one deadline to the request deadline (config.rs 119-121). Spec 4.4
(04-nonfunctional.md 69-71) expects `/ready` to report sandbox, integrations, messaging, email,
billing, jobs, realtime — if those ever become real `ReadinessCheck`s, `N × timeout` can exceed the
outer request timeout and the orchestrator sees a dropped connection, not a clean 503.

P1. Run probes concurrently with `futures::future::join_all` (or `tokio::task::JoinSet`), still each
    under `tokio::time::timeout`, so total latency is bounded by one `timeout` plus scheduling.
P2. Add a single *total* budget around the whole fan-out (e.g. `timeout(config.request_timeout(),
    join_all(...))`) and treat budget expiry as a fail-closed report. Keep the per-probe timeout as
    the finer signal; document the relationship in the config guard.
P3. Preserve declaration order in `failing` (health.rs 92-96) by zipping results with checker names
    after `join_all`, so golden/log consumers see a stable list.

### F2. False healthy: empty set, shallow probe, no severity, no panic/blocking containment

CONFIRMED: `Readiness::default()` and `Readiness::new(vec![])` yield `report()` → `failing: []` →
`is_ready() == true` (health.rs 28-50, 85-90). `is_empty()` is *documented* as "the binary refuses to
start" (health.rs 46) but a repo-wide search found no caller — nothing enforces it. Today `serve()`
always registers one check (bin/commands.rs 43-45), so this is latent, not live.

CONFIRMED: the only registered probe is `SELECT 1` (db.rs 125-140) against a `connect_lazy` pool.
It proves connectivity, not that migrations are applied. Spec 4.1 (04-nonfunctional.md 17-18)
requires a fresh unmigrated boot to *fail* readiness.

CONFIRMED: every check is mandatory — any `Err` appends to `failing` (health.rs 61-72) with no
 notion of required vs optional. `Capabilities.degraded` (health.rs 102-128) is orthogonal, so an
 optional subsystem wired in as a probe (e.g. Redis, which spec 4.1 calls optional) could 503 the
 whole deployment.

P4. Make the empty state unrepresentable for serving: have `Readiness::new` reject an empty vector
    (a typed constructor returning `Result`, or a `NonEmpty` wrapper), and actually call `is_empty()`
    at boot. Add a test `req_053_empty_readiness_is_not_healthy`.
P5. Strengthen the DB probe to also assert schema presence (e.g. a lookup on `_sqlx_migrations` with
    the expected latest version) so an unmigrated boot reports 503. Cross-module requirement on
    `crates/infra/src/db.rs`; keep it a cheap, bounded query.
P6. Add a `required: bool` (or `severity` enum) to `ReadinessCheck`; only required failures set a
    503, optional ones populate a `degraded` reason list. Spec 4.4 mentions only the DB as gating,
    so this keeps parity while removing the optional-subsystem false-unhealthy trap.
P7. Wrap each probe in `AssertUnwindSafe(...).catch_unwind()` so a panicking implementation becomes a
    bounded failure instead of unwinding the Axum handler (GUESS: `timeout` does not catch panics —
    reasoned from Tokio semantics, not tested here). Note: a no-op under `panic=abort`; current policy
    keeps unwinding, so no ADR needed.
P8. Document in the trait rustdoc that `check()` must be cancellation-safe and must not block the
    async runtime; `timeout` only polls at await points, so a synchronous blocking probe is not
    interruptible (GUESS, reasoned). If a blocking probe is ever needed, run it via `spawn_blocking`.

### F3. Startup race: the server binds and serves before readiness is established

CONFIRMED: `serve()` builds a lazy pool, constructs `Readiness`, binds the listener and starts Axum
(bin/commands.rs 43-45, 82-94) with no initial readiness gate and no migration call. So all routes are
live while `/ready` may be 503. CONFIRMED: `/internal/health` is hardcoded `ok: true` with no worker
loop yet (api/src/health.rs 164-170), so a stuck worker also reports healthy.

P9. Add a bounded startup warm-up: after bind, await one `report()` (with the config deadline) and
    log an explicit `readiness_established`/`readiness_pending` event; never abort on failure (the
    lazy-pool design deliberately avoids a crash loop). This gives operators a clear boot signal.
P10. Keep serving during warm-up but expose a monotonic `ready_once` latch in `ReadinessReport` so
     an orchestrator can distinguish "never ready" from "temporarily not ready" (additive field —
     golden `ready.json` tolerates additions). Do not delay the bind: that would trade a 503 for a
     CrashLoopBackOff.
P11. Wire the worker heartbeat into `/internal/health` in M1 (already noted in api/src/health.rs
     165-166): return 503 when the heartbeat exceeds its window. Until then, document it as a known
     false-healthy (GUESS: no test currently asserts staleness).

### F4. Flapping and log amplification under a brownout

GUESS (reasoned from code, not reproduced): with no hysteresis, consecutive `/ready` calls can
alternate 200/503 as a dependency wobbles, causing orchestrator churn; each failing probe logs a
`warn!` per request (health.rs 62-70, api health.rs 123-126), so a sustained outage emits a flood.
No per-probe latency is captured, so "down" and "slow" are indistinguishable in logs.

P12. Add a consecutive-failure/success threshold (e.g. 2 samples) inside `report()` via a small
     stateful wrapper, so a single blip does not flip the published state. Keep the instantaneous
     result available for diagnostics.
P13. Record per-probe elapsed time in `ReadinessReport` and log/warn only on state transitions, not
     every request. Add `Cache-Control: no-store` to `/ready` and `/health` (cross-module note on
     api/src/health.rs) so an intermediary cannot serve a stale 200.

## Explicit non-goals / rejected options

- Do not relax the secret-free error contract (REQ-054), the 503-on-DB-failure rule (spec 4.4), or the
  golden `/health` and `/ready` shapes to make proposals easier.
- Do not add caching that could serve a stale healthy result; P13's `no-store` is the opposite.
- Do not move migration execution into request time; P5 is a probe, not a fix for the compose ordering.
- Do not introduce a second readiness framework or a new dependency for P1 — `futures` is already in
  the workspace (verify before coding).

## Risks and trade-offs

- R1. Concurrency (P1/P2) raises simultaneous load on a struggling dependency; cap probe count and
  keep per-probe timeouts short. Confirm `futures` is already a direct dep before using it (GUESS).
- R2. Rejecting empty `Readiness` (P4) is a public API change to `boma_app`; all test fixtures build a
  probe, but verify the conformance/golden crates still compile (GUESS: they do, from grep).
- R3. `_sqlx_migrations` lookup (P5) couples the probe to sqlx's schema; if the expected version is
  hardcoded it drifts on every migration — derive it from the embedded migrator instead.
- R4. Severity (P6) adds a field to the trait, touching every `ReadinessCheck` impl in api/infra tests;
  keep a default `required = true` to avoid churn (behavior-preserving for existing probes).
- R5. Hysteresis (P12) makes `/ready` slower to recover — an instance stays out of rotation a bit
  longer. Choose a small threshold (2) and document it.
- R6. `catch_unwind` (P7) needs `UnwindSafe` handling and is inert under `panic=abort`; it is defence
  in depth, not a substitute for correct probes.

## Suggested rollout for v0.4.0

Land P1/P2/P3/P4/P7/P8 first — they harden the core `report()` path without changing the wire contract
or any dependency's behavior. Then P5/P6/P9/P11 (cross-module: infra probe, trait severity, bin
warm-up, worker heartbeat) as separate PRs with REQ-053 in the test names. Leave P12/P13 as a
follow-up if flapping is not observed in the 30-day window; they add state and complexity. No public
API or golden shape changes except the additive `ready_once` field (P10), which needs a DEVIATION note
in docs/PARITY.md only if an external consumer exists (GUESS: none).
