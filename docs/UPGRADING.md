# Upgrading Boma Bot

For operators self-hosting Boma Bot. `0.1.0` is the first release; there is nothing to upgrade
*from* unless you run a pre-release build. Both paths are covered below.

## What ships in a release

- one static web build (the whole UI; the Rust binary serves it — no Node at runtime);
- one Rust binary `boma` (`serve`, `migrate`, `worker`, `probe`, `export-spec`-ish tooling);
- additive-only migrations (`migrations/0001…0053` today). Every migration is numbered,
  one-way-in-place code is never edited; a fix ships as a *new* migration.
- an Electron desktop pack (see §Desktop).

## Before any update

1. **Back up the database**:
   `pg_dump --no-owner --format=custom -d boma > boma-backup.dump`
2. **Back up the artifact store** if you use one: the directory you set in `BOMA_ARTIFACT_DIR`.
3. **Confirm `BOMA_MASTER_KEY` is set and kept somewhere safe.** Without it, every sealed secret
   (bot secrets, MCP credentials, Slack tokens) is lost with the next restart. The key is never
   in the repo and never in a release artifact; it is an environment variable you own.
4. Keep the previous image tag / binary beside the new one — rollback is a swap, not a build.

## Path A — Docker Compose (recommended)

The reference stack boots `db` → `migrate` → `app` with the same image; jobs run in-process
(`BOMA_JOBS=postgres`), realtime fan-out is Postgres (`BOMA_REALTIME=postgres`).

1. `git pull` (or set `IMAGE=…:new-tag` and `docker compose pull`).
2. `docker compose run --rm migrate` — applies the migrations; it is idempotent.
3. `docker compose up -d app` (or `up -d`) — the new binary serves; `migrate` failing blocks it.
4. Verify: `/healthz`, `/ready` and `/readyz` on the published port (default 3000), e.g.
   `curl -fsS 127.0.0.1:3000/ready | jq .` — expect `{"ok": true, …}`.
5. **`serve` never drains the queue on its own.** Run the `worker` alongside it — Compose now
   carries a `worker` service (same image, `boma worker`). If you run Compose yourself, add it;
   without it, `runs/list` shows everything stuck at `queued` and the application silently never
   executes a run.
5. Watch the logs on the first minutes: structured JSON on stdout, one line per decision.

If step 3's migration fails, the new `app` never serves stale schema: Compose waits on it. Fix the
database (restore the backup if needed) and re-run `migrate` — never apply migrations by hand
against a live system that served traffic, and never edit an applied migration file.

## Path B — plain binary

1. Install the new binary where your service manager expects it (`/usr/local/bin/boma`).
2. `BOMA_DATABASE_URL=postgres://… boma migrate`.
3. `BOMA_DATABASE_URL=postgres://… boma serve` (or your systemd unit; `boma probe --addr
   127.0.0.1:3000 --path /ready` is the health-check the packaged images use).
4. The UI is already inside the release's web directory; point your reverse proxy at the binary
   (`serve` listens on loopback and serves the static export — ADR-013) or at the web directory
   you dropped beside it. No Node process runs in production.
5. **Run `boma worker` next to `boma serve`** — the API serves traffic, the worker drains the
   queue. Two processes, two systemd units; without the second, runs stay `queued` forever.
   Check: `boma worker --help` for flags; its drain is observable in the JSON logs
   (`run.execution_started` / `run.finished`).

## Recovering

- **App unhealthy after upgrade**: stop it, restore
  `pg_restore --no-owner --clean --dbname=boma boma-backup.dump`, run `boma migrate` to
  re-assert the expected schema, then start the *previous* binary/image. Migrations are additive;
  a pre-0.1.0 binary cannot read a post-0.1.0 database — restore first, then downgrade.
- **Lost `BOMA_MASTER_KEY`**: sealed secrets are unrecoverable by design. Re-issue every secret
  (bot secrets, MCP server credentials, webhook secrets) and rotate anything sealed with the
  old key; password hashes (Argon2id) are unaffected.
- **Desktop update failed**: reinstall the previous installer; user data lives outside the app
  directory and is untouched. `electron-builder` does not delete data directories.

## Desktop (Electron)

The update feed is static and inert (ADR-016): there is no silent auto-update in 0.1.0. Updates
are manual — download the new pack from the release page, install over the previous one; the
configuration and the local cache live in the OS app directory and survive reinstalls. Verify
the pack's signature before installing (see the release notes for the publisher and checksums).

## What changed with every release

Each release's notes say exactly what shipped, which REQ ids are covered and what remains
deferred (REQ-049 voice and REQ-056 mobile stay out by record; see `docs/PLAN.md`). Traceability
(`tools/trace_check.py`) is the gate: it only goes down, never up.
