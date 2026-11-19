# v0.4.0 architecture proposal — `crates/infra/src/db.rs`

Lens: architecture. Scope: DB pool + migration structure. Worker #283.

## Current structure (CONFIRMED by reading this tree)

- `Postgres` is a thin `#[derive(Clone)]` wrapper around one `sqlx::PgPool`. It exposes two
  constructors (`connect` eager / `connect_lazy`), `pool()` for the ~45 store adapters,
  `migrate()` and `readiness()`.
- Migrations are embedded at compile time with `sqlx::migrate!("../../migrations")` (59 SQL files)
  and applied through `Migrator::run(&pool)` — i.e. on the *same* pool that serves requests
  (CONFIRMED: `migrate()` line 73).
- The only configurable pool knobs are `max_connections` and `acquire_timeout` (`pool_options`).
  `connect_options` pins `search_path=public` via `PgConnectOptions::options`, so pool and
  migrations share one schema — a sound defense against `$user` schema shadowing.
- Readiness is a bounded `SELECT 1` (`tokio::time::timeout`); driver detail stays at `debug`, only
  a probe name reaches the client (REQ-054, CONFIRMED).
- Wiring: `serve`/`check` use `connect_lazy` (`bin/commands.rs:44,287`); `migrate`/`rekey` and all
  integration tests use `connect` + `migrate()` (`:395,414`, `tests/database.rs`).
- All queries are runtime `sqlx::query(...)`; there is no `.sqlx` offline metadata and no `query!`
  macro anywhere (CONFIRMED by search). This diverges from the AGENTS.md SQLx default of
  compile-time-checked queries with committed offline metadata.

## Proposed upgrades for v0.4.0

### 1. Make readiness assert schema currency, not just connectivity
Spec `04-nonfunctional.md:17-18` requires a fresh boot without migrations to *fail* readiness;
today `SELECT 1` passes on an empty, unmigrated database (CONFIRMED — gap is real). Add a
migration-status check alongside the probe: compare pending migrations / `_sqlx_migrations`
checksums against `MIGRATOR`, and fail closed when the schema is behind. Cache the result in a
one-shot `OnceLock`/atomic set at first successful check (not a query per `/ready` hit).
*Risk:* if `boma migrate` and `boma serve` remain separate deploy steps, an intentional gap window
would 503 the whole fleet — sequence the deploy gate and keep the check behind a flag until the
rollout order is proven. GUESS: exact spec intent for the gap window.

### 2. Give migrations their own connection path and a status API
Running `Migrator::run(&pool)` on the request pool can pin a connection for a long migration and
starve readiness under load. Build a dedicated single-connection migrator using the *same*
`connect_options` (search_path pin), expose `migration_status() -> {applied, pending}` for
observability, and add a `boma migrate --check` dry-run used by the deploy gate.
*Risk:* two connection paths must stay byte-identical in their `search_path` option or tables land
in different schemas — pin both from one shared helper. Extra deploy-time connection; keep the
migration advisory lock so two replicas cannot race.

### 3. Adopt compile-time-checked queries with committed offline metadata
Introduce `.sqlx/` metadata + `query!`/`query_as!` for hot store queries so schema drift is a build
failure, matching the AGENTS.md default. Keep runtime `sqlx::query` only where a query is truly
dynamic (builder-style filters).
*Risk:* mechanical but wide — touches ~40 store files; offline metadata must be regenerated on
every migration change and CI must run with `SQLX_OFFLINE=1 --locked` or builds break. Sequence
this after upgrade 2 stabilizes the migration set.

### 4. Centralize connection policy and broaden validated pool settings
Fold `pool_options` + `connect_options` into one builder owned by `DatabaseSettings`, and add the
missing knobs via validated settings: TLS mode (rustls), `min_connections`, idle/max lifetime,
`test_before_acquire`, and a server-side `statement_timeout` startup option (the readiness probe
currently relies only on the client-side `tokio::time::timeout`, which can leave a stuck connection
occupying a slot). This also removes the `connect`/`connect_lazy` duplication.
*Risk:* new settings need an ADR, `.env.example`, golden config and `check_config` updates;
changing pool defaults alters behavior under load — record externally visible changes in
`docs/PARITY.md` as DEVIATIONs if they surface. TLS mode change must not silently downgrade.

### 5. Introduce a typed library error boundary
`connect`/`connect_lazy`/`migrate` currently leak raw `sqlx::Error` / `MigrateError` past the infra
crate, contrary to the AGENTS.md "thiserror in libraries, anyhow only at binary edges" rule. Wrap
them in a `DbError` enum (`Connect`, `Configure`, `Migrate { source }`) with redaction-safe
messages, keeping driver detail at `debug`.
*Risk:* cross-crate mechanical change — `bin/commands.rs` and tests call `.context()` on the raw
types and must be updated; low behavioral risk, no client-visible format change (the API already
returns generic names only).

## Ordering
1 → 2 → 5 are small and self-contained; 3 is a broad follow-on once migrations are stable; 4
pairs with the ADR process. Items 1 and 2 are the priority: they close a confirmed parity gap in
REQ-053 / spec §4.1.
