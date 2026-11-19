# 0172 — crates/app/src/health.rs — reliability (v0.4.0)

Worker #172 · `crates/app/src/health.rs` · lens: reliability · READ-ONLY review of `before-m5` at
v0.3.0-rc5. No product code changed. Code facts are CONFIRMED by reading files; inferred claims GUESS.

## What the file already does right (keep, don't re-propose)

- Liveness/readiness split is explicit (health.rs 1-8): `/health` is memory-only, `/ready` probes
  dependencies — a DB blip cannot kill a healthy process.
- `ReadinessCheck::check()` returns a static reason, never a driver error (health.rs 16-25); the DB
  probe logs real errors only at `debug` (db.rs 125-140). `/ready` is unauthenticated, so no secret
  leaks (REQ-054). Keep the shape: golden `ready.json` allows adding fields, not removing them.
- Per-probe timeout (health.rs 59) and config guard (config.rs 119-121) bound a single probe;
  `Readiness` is immutable after construction (`Vec<Arc<dyn …>>`), safe to share via `AppState`.

## Failure modes → concrete hardening

### F1. Dependency checks are sequential: a per-probe deadline is not a request deadline

CONFIRMED: `report()` awaits each probe in a `for` loop (health.rs 56-77). N probes cost up to
`N × timeout` and a slow early probe head-of-line blocks the rest. The config guard compares only one
deadline to the request deadline (config.rs 119-121). Spec 4.4 (04-nonfunctional.md 69-71) expects
`/ready` to report sandbox, integrations, messaging, email, billing, jobs, realtime; if those become
real probes, `N × timeout` can exceed the outer request timeout and the orchestrator sees a dropped
connection instead of a clean 503.

P1. Run probes concurrently (`futures::future::join_all` or `JoinSet`), each still under its own
    `tokio::time::timeout`, so total latency is bounded by one `timeout` plus scheduling.
P2. Add one total budget around the fan-out (`timeout(request_timeout(), join_all(...))`), treating
    expiry as fail-closed; keep the per-probe timeout as the finer signal and document the relation.
P3. Preserve declaration order in `failing` (health.rs 92-96) by zipping results with names after
    `join_all`, so logs/golden consumers see a stable list.

### F2. False healthy: empty set, shallow probe, no severity, no panic/blocking containment

CONFIRMED: `Readiness::default()` / `new(vec![])` give `is_ready() == true` (health.rs 28-50, 85-90).
`is_empty()` is documented as "the binary refuses to start" (health.rs 46) but a repo-wide search
found no caller — nothing enforces it. `serve()` always registers one probe today (bin/commands.rs
43-45), so this is latent.

CONFIRMED: the only probe is `SELECT 1` (db.rs 125-140) on a lazy pool — connectivity, not schema.
Spec 4.1 (04-nonfunctional.md 17-18) requires an unmigrated boot to fail readiness.

CONFIRMED: every check is mandatory (any `Err` appends to `failing`, health.rs 61-72); `Capabilities.
degraded` (health.rs 102-128) is orthogonal. An optional subsystem wired in as a probe (e.g. Redis,
which spec 4.1 calls optional) could 503 the whole deployment.

P4. Make the empty state unrepresentable for serving: a typed constructor that rejects `vec![]` (or a
    `NonEmpty` wrapper) plus a real `is_empty()` call at boot. Test `req_053_empty_readiness_is_not_healthy`.
P5. Strengthen the DB probe to assert schema/migration presence (lookup on `_sqlx_migrations`), so an
    unmigrated boot reports 503. Cross-module on `crates/infra/src/db.rs`; keep it cheap and bounded.
P6. Add `required: bool` (or `severity`) to `ReadinessCheck`; only required failures set a 503,
    optional ones populate a degraded-reason list. Spec 4.4 gates only the DB, so parity is kept.
P7. Wrap each probe in `AssertUnwindSafe(...).catch_unwind()` so a panicking impl becomes a bounded
    failure, not an unwind through the Axum handler (GUESS: `timeout` does not catch panics — reasoned
    from Tokio semantics, untested here). No-op under `panic=abort`; policy keeps unwinding.
P8. Document in trait rustdoc that `check()` must be cancellation-safe and must not block the async
    runtime; `timeout` polls only at await points, so a blocking probe is not interruptible (GUESS,
    reasoned). Use `spawn_blocking` if a blocking probe is ever needed.

### F3. Startup race: bind and serve before readiness is established

CONFIRMED: `serve()` builds a lazy pool, constructs `Readiness`, binds and starts Axum
(bin/commands.rs 43-45, 82-94) with no initial readiness gate and no migration call — every route is
live while `/ready` may be 503. CONFIRMED: `/internal/health` is hardcoded `ok: true` with no worker
loop (api/src/health.rs 164-170), so a stuck worker also reports healthy.

P9. Add a bounded startup warm-up: after bind, await one `report()` and log an explicit
    `readiness_established` / `readiness_pending` event; never abort on failure (the lazy pool
    deliberately avoids a crash loop).
P10. Keep serving during warm-up but expose a monotonic `ready_once` latch in `ReadinessReport` so an
     orchestrator can tell "never ready" from "temporarily not ready" (additive field — golden
     `ready.json` tolerates additions). Do not delay bind: that trades a 503 for CrashLoopBackOff.
P11. Wire the worker heartbeat into `/internal/health` in M1 (already noted at api/src/health.rs
     165-166): 503 when the heartbeat exceeds its window. Until then document a known false-healthy
     (GUESS: no test asserts staleness).

### F4. Flapping and log amplification under a brownout

GUESS (reasoned, not reproduced): without hysteresis, `/ready` can alternate 200/503 as a dependency
wobbles, causing orchestrator churn; each failing probe logs a `warn!` per request (health.rs 62-70,
api/src/health.rs 123-126), so an outage floods logs. No per-probe latency is captured, so "down"
and "slow" look identical.

P12. Add a small consecutive-failure/success threshold (e.g. 2) in a stateful wrapper around
     `report()` so one blip cannot flip the published state; keep the instantaneous result for
     diagnostics.
P13. Record per-probe elapsed time in `ReadinessReport`; warn only on state transitions. Add
     `Cache-Control: no-store` to `/ready` and `/health` (cross-module note on api/src/health.rs) so
     an intermediary cannot serve a stale 200.

## Explicit non-goals / rejected options

- Do not relax the secret-free error contract (REQ-054), the 503-on-DB-failure rule (spec 4.4), or the
  golden `/health` and `/ready` shapes to make proposals easier.
- Do not cache a healthy result; P13's `no-store` is the opposite. Do not move migrations into request
  time (P5 is a probe, not a fix for compose ordering). No second readiness framework or new dep for
  P1 — verify `futures` is already in the workspace before coding.

## Risks and trade-offs

- R1. Concurrency (P1/P2) raises simultaneous load on a struggling dependency; cap probe count, keep
  per-probe timeouts short; confirm `futures` is already a direct dep (GUESS).
- R2. Rejecting empty `Readiness` (P4) changes the `boma_app` API; fixtures all build a probe, but
  verify conformance/golden crates still compile (GUESS: they do, from grep).
- R3. The `_sqlx_migrations` lookup (P5) couples the probe to sqlx's schema; derive the expected
  version from the embedded migrator, never hardcode it (it would drift each migration).
- R4. Severity (P6) touches every `ReadinessCheck` impl in api/infra tests; default `required = true`
  to avoid churn (behavior-preserving for existing probes).
- R5. Hysteresis (P12) delays recovery slightly; use a threshold of 2 and document it.
- R6. `catch_unwind` (P7) needs `UnwindSafe` and is inert under `panic=abort`; defence in depth only.

## Suggested rollout for v0.4.0

Land P1/P2/P3/P4/P7/P8 first — hardening the core `report()` path with no wire-contract change.
Then P5/P6/P9/P11 as separate PRs (infra probe, trait severity, bin warm-up, worker heartbeat), with
REQ-053 in the test names. Leave P12/P13 as a follow-up unless flapping appears in the 30-day window.
The only wire change is the additive `ready_once` field (P10); add a docs/PARITY.md DEVIATION note if
an external consumer exists (GUESS: none).
