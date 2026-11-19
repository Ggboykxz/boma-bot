# 0120 — compose.yaml — reliability (v0.4.0)

Worker #120 · module `compose.yaml` · lens: reliability · READ-ONLY review of
`rebuild-workspace/.worktrees/before-m5` at v0.3.0-rc5. Proposals only; no product code changed.
"probe 56" is the 56 compose-job reruns reported in this month's swarm brief (same count as brief
0002); no CI log was re-derived here. Code facts below are CONFIRMED by reading the named files;
anything inferred is marked GUESS.

## What the file already does right (keep, don't re-propose)

- Single DB, one `migrate` step, one server + one worker (ADR-018); loopback publishing only.
- `db` healthcheck via `pg_isready`, plus server-side `statement_timeout=5000` and
  `idle_in_transaction_session_timeout=30000` (compose.yaml 36-52): slow queries die inside Postgres,
  not only at the client deadline.
- `migrate` gated on `db: service_healthy`; `app` and `worker` gated on `migrate:
  service_completed_successfully` and `db: service_healthy` (62-65, 73-77, 91-95) — no "serve before
  schema" race in the normal path.
- `app` healthcheck uses `boma probe --path /ready` (78-83), the same readiness contract CI probes;
  `boma probe` is dependency-free with its own 10s timeout (crates/bin/src/probe.rs 16-49).
- `worker` has `restart: unless-stopped` (96), `migrate` is `restart: "no"` (65); CI tears down with
  `docker compose down -v` under `if: always()` (ci.yml 300-302).

## Failure modes seen in CI → concrete hardening

### F1. "probe 56": readiness is a single 200-or-fail gate, with two opposite tolerances

CONFIRMED: `boma probe` fails on anything but HTTP 200 (probe.rs 42-46); `/ready` returns 503 when a
dependency fails (crates/api/src/health.rs 114-128), and its DB check is `SELECT 1` under a timeout
(crates/infra/src/db.rs 125-140). The app healthcheck allows only 3 retries at 10s intervals after a
10s start_period (compose.yaml 78-83), while CI's curl probe retries 30× / 2s (ci.yml 271-279). A
transient DB probe timeout under `statement_timeout=5000` therefore marks the container unhealthy
before the slower CI probe would have recovered — two readiness definitions, opposite budgets.

P1. Align them: raise app healthcheck `retries` to 6-10 and `start_period` to 30s, or probe `/health`
    (liveness, memory-only) in the container and keep `/ready` for the CI gate. Preferred: `/health`
    in-container, so a DB blip does not restart an otherwise healthy process.
P2. Give `app` an explicit `restart: unless-stopped` (worker already has it); today an unhealthy app is
    neither restarted nor replaced, so a transient 503 leaves a dead-but-running stack.
P3. Make the CI probe assert the body, not just status (`"ok":true` in `/ready` JSON) and print
    `docker compose ps` + `docker inspect` health on failure (extends ci.yml 280-283 / brief 0002 P11).
P4. Deduplicate the healthcheck: declared in both Dockerfile 48-49 and compose.yaml 78-83; keep the
    compose one authoritative in a comment. Historical evidence: the desktop health-wait once hammered
    a wrong path (`/health/health` → 404 forever, STATE.md 254-257); one probe source shrinks that class.

### F2. Health races: `pg_isready` is not "database ready to serve"

CONFIRMED: db healthcheck (48-52) is `pg_isready`, which can pass against the temporary bootstrap
server during initdb (GUESS: standard Postgres init race; not reproduced here). `app` uses
`connect_lazy` (db.rs 50-59), so it starts with no connection and lets `/ready` turn 503 → 200 later —
good — but `migrate` uses eager `connect` (41-46) and must succeed on that first health signal.

P5. Strengthen the db healthcheck to a real query as the credential: `pg_isready ... && psql -U boma
    -d boma -c 'SELECT 1'`. `pg_isready` alone can accept a server that has not finished creating the DB.
P6. Add `start_period` (~10s) to the db healthcheck so an initdb restart is not burned as a failure
    before `retries` run out.
P7. Bound a first-connection retry for `migrate` (`restart: on-failure:3` or a wrapper) so losing the
    connection right after `pg_isready` does not fail the stack; cap attempts, never infinite-restart.
P8. Add a `worker` healthcheck on `/internal/health` (health.rs 153-171; CI probes no worker today).
    Without it a stuck claim loop is invisible — "API serves, queue never drains" is exactly the
    v0.1.0 defect this service was added to fix (compose.yaml 85-88).

### F3. Migration ordering and version skew

CONFIRMED: migrations are embedded (`sqlx::migrate!`, db.rs 20) and applied by a one-shot service;
re-runs are idempotent via `_sqlx_migrations` (GUESS: sqlx's standard behaviour, not re-verified here).
Compose-level risks:
- No deadline on `migrate`: DDL blocked by a stale lock from a crashed prior run can hang `up -d`;
  CI's 30-minute job timeout is the only backstop. `idle_in_transaction_session_timeout` does not
  bound a lock *wait* (GUESS).
- Three independent `build: .` entries (58, 68, 88) can, on a moving base tag, produce three binaries
  with different embedded migration sets; a stale `migrate` against a newer schema is silent.
- `BOMA_VERSION` defaults to `0.3.0-rc` (line 10) and `/ready` reports config version, not applied
  schema version — no compose guard against a v0.4.0 binary on a v0.3.0 DB.
- `BOMA_MASTER_KEY` empty (line 31) means an ephemeral key; a container restart silently rotates it
  and loses every bot secret — a data-integrity failure compose can at least make loud.

P9. Bound `migrate`: wrap as `timeout 60 boma migrate` (or a configurable `BOMA_MIGRATE_TIMEOUT`) and
    keep `restart: "no"`, so a failed migration fails visibly instead of looping or hanging.
P10. Build once: `x-app-build: &app-build { build: . }` (or one `image:` shared by a single build
     service) and reference the anchor from all three services, guaranteeing migrate/app/worker run
     the same bytes. Note it in the ADR that introduced the one-binary-many-jobs layout (ADR-017).
P11. Set `BOMA_VERSION: ${BOMA_VERSION:-0.4.0}` for this line and add a startup assertion that the
     binary's embedded migration checksum matches the DB's latest applied migration. The assertion
     belongs in `boma migrate`/`serve` — filed here as a cross-module requirement, not code in this pass.
P12. Make an empty `BOMA_MASTER_KEY` loud in compose-run deployments: `${BOMA_MASTER_KEY:?set a master
     key}` with a documented local-first override, or a CI compose-job `::warning::` when it is empty.

## Explicit non-goals / rejected options

- Do not relax SHA-256 pinning, `--locked`, or the 200-only `boma probe` contract to green CI; harden
  timing and topology instead (same rule as brief 0002).
- Do not auto-retry the CI *probe* into green (brief 0002 P11 keeps probe semantics intact).
- Do not publish the DB port beyond loopback (53-54) or add a second database — out of ADR-018 scope.
- Do not add backup/restore to v0.4.0 compose (the `db-data` volume stays); flag it separately.

## Risks and trade-offs

- R1. Probing `/health` (P1) stops restart-on-DB-blip but also stops container-level detection of a
  broken DB; CI's `/ready` gate and the new worker probe (P8) must compensate.
- R2. `restart: unless-stopped` on `app` (P2) can mask a crash loop; bound it and keep `docker compose
  ps` / logs in the CI failure path (P3).
- R3. `pg_isready && psql SELECT 1` (P5) needs a client in the db image (expected in
  postgres:17-alpine — GUESS, verify in CI); if absent, check from the app side with `boma probe`.
- R4. `timeout 60` around migrate (P9) may cut a legitimate long migration on a big DB; make the
  limit configurable and document the override.
- R5. Single-build anchor (P10) changes image tagging; ensure CI's `down -v` still removes a stale
  reused tag (follow-up, not a v0.4.0 gate).

## Suggested rollout for v0.4.0

Land P1/P2/P4/P5/P6/P8/P9/P10 first — probe budgets, restart policy, real DB readiness, worker probe,
migrate deadline, single build — no public API change, each independently revertable. Then P3/P7/P11/P12;
P11 needs a commands.rs change, so it stays a cross-module requirement in this read-only pass.
Re-measure the compose rerun rate for 30 days; target ≥70% of the reported 56 gone. Gate semantics,
`.env.example` discipline and the 200-only `boma probe` contract remain unchanged.

_Scope note: line references are against `.worktrees/before-m5/compose.yaml`, `Dockerfile`, `.github/workflows/ci.yml`
and `crates/{bin,infra,api}/src/...` as read here. "probe 56" is the swarm-brief count, not re-derived; GUESS marks inference._
