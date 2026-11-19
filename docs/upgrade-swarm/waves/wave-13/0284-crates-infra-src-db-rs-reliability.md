# Worker #284 — `crates/infra/src/db.rs` — Reliability (v0.4.0)

Scope: reliability hardening of the `Postgres` adapter (`connect`, `connect_lazy`,
`migrate`, `readiness`) and its interaction with `DatabaseSettings` and the binary's
`serve`/`migrate` commands. Evidence is static code inspection of `db.rs`, `config.rs`,
`commands.rs`, `crates/infra/tests/database.rs` and the `migrations/` layout (CONFIRMED
unless marked GUESS); no test or runtime probe was run by this worker. Any externally
visible change needs a `docs/PARITY.md` DEVIATION.

## Failure mode 1 — Pool exhaustion and backpressure

Evidence (CONFIRMED): `pool_options()` (l.86-90) sets only `max_connections` (validated
`1..=100` in config.rs l.67) and `acquire_timeout`. There is no `min_connections`,
`idle_timeout`, `max_lifetime`, `test_before_acquire` or connection/queue metric.
Under load, callers queue until `acquire_timeout` then fail with a raw `sqlx::Error`.
Worse, `PostgresReadiness::check()` (l.125-139) runs `SELECT 1` **through the same
pool**: when the pool is exhausted, `/ready` fails too, so an orchestrator sees an
otherwise-healthy but busy process as down and restarts it, amplifying the incident.

Hardening:
- Add `idle_timeout`/`max_lifetime` (recycle stale or leaked connections) and a
  conservative `min_connections` for steady-state latency; keep values in typed config.
- Emit pool metrics (`size`, `num_idle`, acquire wait time, timeouts) via `tracing`/metrics
  so exhaustion is observable before it becomes a restart. Give the probe a **dedicated,
  small connection budget** (separate pool or reserved permit) so a saturated request
  pool cannot flip `/ready` to failing. Classify `PoolTimedOut` distinctly in logs
  (currently all probe errors are `debug` only, l.132) and alert on sustained timeouts.

Risks: `test_before_acquire` adds a round trip per checkout — measure before enabling.
A reserved readiness connection consumes one slot of a small (`max=1..2`) pool and could
change failure behavior → DEVIATION; size budgets from a measured baseline, not a guess.

## Failure mode 2 — Migration ordering and dirty state

Evidence (CONFIRMED): `MIGRATOR` is embedded via `sqlx::migrate!` (l.20); `migrate()`
(l.72-74) delegates to `MIGRATOR.run`, which serializes replicas with an advisory lock
and records rows in `_sqlx_migrations` (asserted in tests/database.rs l.51-56). Forward
ordering is by version prefix across 59 files in one flat directory (`0001`…`0059`,
with gaps). `boma migrate` (commands.rs l.413-420) is a distinct deploy step; `serve`
(l.42-44) uses `connect_lazy` and **never calls `migrate()`**. A migration that fails
midway leaves `success = false`; subsequent runs refuse to proceed until repaired. No
code checks that the applied schema version matches the binary's expectations.

Hardening:
- Add an explicit preflight that reports pending/applied/divergent migrations
  (`MIGRATOR.validate_applied` / applied-vs-available set) and fail fast on a dirty
  `_sqlx_migrations` row with an operator-facing remediation message.
- Make `migrate()` idempotent-and-loud (log applied/skipped counts; keep the existing
  double-apply no-op test, tests/database.rs l.44-49). Enforce gap-free version numbers
  with a CI check; reject duplicate versions; keep migrations forward-only/transactional.
- Optionally gate `serve` startup on the expected latest version (configurable) or
  document `boma migrate` as a hard deploy precondition in runbooks.

Risks: a startup version gate would change boot behavior on a lagging DB → DEVIATION
and needs an explicit opt-in flag. `validate_applied` flags legacy dirty rows that
"worked" before; require a documented repair path (`sqlx migrate repair`) first.
Migrations touching multiple replicas during a rolling deploy can expose old-code/new-
schema mismatch — sequence the deploy, do not run migrate from every replica blindly.

## Failure mode 3 — Connection loss and failover

Evidence (CONFIRMED): `connect()` (l.41-46) fails fast on an unreachable DB;
`connect_lazy()` (l.56-59) never connects and relies on `/ready` (REQ-053). With no
`test_before_acquire`, `idle_timeout` or `max_lifetime`, connections that go stale
after a failover or network blip can be handed to callers, producing a burst of errors
that the readiness probe may not see. `connect_options()` pins `search_path=public`
(l.93-95) but sets no `connect_timeout`/TCP keepalive, so a black-holed host can hang
until `acquire_timeout`. Errors are logged at `debug` only (l.132, 136) — no operator
signal for repeated connection loss.

Hardening:
- Add `connect_timeout` to `PgConnectOptions` and bounded TCP keepalives/keepalive
  intervals; set `idle_timeout`/`max_lifetime` so pools heal after failover.
- Enable `test_before_acquire` (or periodic health validation) so dead connections are
  recycled rather than returned to callers.
- Record connection-error and reconnect events at `warn`/`info` with a counter; keep
  credentials out of logs (REQ-054 — the URL is already redacted in config.rs l.106-113).
- Retry transient `sqlx::Error` classes (with jittered backoff, bounded by
  `acquire_timeout`) at the use-site layer, not blindly inside the adapter.

Risks: `test_before_acquire` adds latency and load on every checkout — make it
configurable. Aggressive recycling can thrash a pool during a short outage; tune
`max_lifetime` above the expected blip. Retry logic must stay cancellation-safe and
must not double-apply non-idempotent writes (AGENTS.md async/idempotency rules).

## Failure mode 4 — First launch against an empty database

Evidence (CONFIRMED): `serve` builds a lazy pool and registers a `SELECT 1` probe
(commands.rs l.44-45). On a brand-new database with no migrations, `SELECT 1` succeeds,
so `/ready` reports **ready** while every repository query fails with
`relation ... does not exist` — a green readiness on a nonfunctional service. `connect()`
(used by `boma migrate`, l.414) works against an empty DB because Postgres connects to
the `postgres` maintenance DB by default, so the deploy order must still run `migrate`
before `serve`. The pinned `search_path` (l.27, 94) also means a wrong-schema install
silently produces "table missing" rather than a clear config error.

Hardening:
- Extend readiness with a cheap schema-existence check (e.g. `SELECT 1 FROM
  _sqlx_migrations LIMIT 1` plus one sentinel table) so first-launch reports
  *not ready* until migrations are applied. Keep the reason string generic (REQ-054).
- Add a startup/bootstrap diagnostic that distinguishes "DB unreachable", "DB empty /
  migrations pending", and "schema version mismatch" in operator logs.
- Verify `search_path=public` resolves the expected schema at startup (fail fast if a
  role-level override would shadow it).

Risks: a schema-aware probe couples readiness to migration state and could mark a
running-but-migrating replica unready — acceptable, but document it; keep it behind a
config flag if the spec treats `/ready` as liveness-only → DEVIATION check. Ensuring the
sentinel table is cheap and stable across migrations avoids probe flapping.

## Priority for v0.4.0

1. Observability first: pool metrics, distinct `PoolTimedOut`/connection-error logs,
   and a schema-aware readiness gate (highest value, no public interface change).
2. Pool hygiene: `idle_timeout`, `max_lifetime`, `connect_timeout`, configurable
   `test_before_acquire`; reserve a dedicated readiness connection budget.
3. Migration safety: applied/pending/dirty preflight, gap-free CI check, documented
   `boma migrate`-before-`serve` deploy order and dirty-state repair runbook.

Each item needs REQ-tagged tests (module-level `req_053_*`/`req_054_*` names) — unit
tests for option construction and error classification, integration tests in
`crates/infra/tests/database.rs` against a real database for the probe/failover cases.
Any externally visible change (readiness semantics, startup gate, retry behavior) must
be recorded as a `docs/PARITY.md` DEVIATION awaiting approval before shipping.
