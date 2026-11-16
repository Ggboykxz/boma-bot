# M35 — Self-contained desktop, the OpenBot way (design)

**Intent.** Make the desktop a double-click application that works on a machine with no Docker,
no system Postgres, no Node, no account and no setup — the owner's standing requirement
(2026-11-16: "docker ne doit pas etre obligatoire, l'app doit fonctionner comme
https://github.com/nightly-labs/openbot"). Today the Windows/Linux desktop is a window onto an
origin that does not exist until someone runs `docker compose up -d`; on an installed machine
the compose file does not even exist, so v0.2.3's first-launch path is unusable end to end. M35
replaces it: the installer carries everything the app needs.

## The decision, in one line

Embed a real PostgreSQL in the desktop (zonkyio binaries, PG 17.12.0, Apache-2.0), ship the
`boma` server binary for the same OS inside the package, and boot the whole stack as child
processes of the desktop on first launch. Like OpenBot (Electron + local storage), but Postgres
because the spec mandates it (RLS, migrations, REQ-043).

## Goals

1. **Double-click works.** First launch initialises a data directory in the app's user-data
   folder, starts embedded Postgres on a loopback port, applies migrations, starts
   `boma serve` and `boma worker`, and opens the app. No installer prompts, no downloads, no
   Docker, no account (REQ-066 local-first posture: loopback only, `BOMA_LOCAL_FIRST=true`).
2. **The package is the product.** One `.exe` / one Linux directory pack carries: Electron,
   the web export, the per-OS `boma` binary, and the per-OS Postgres binaries (~15 MB compressed
   per OS from zonky, ~30 MB for the desktop's own assets).
3. **BOMA_DESKTOP_URL keeps working for self-hosted deployments.** When the env var is set, the
   desktop is a window onto that origin and the embedded stack is not started — the "install the
   server bundle yourself" path is unchanged and stays documented.
4. **Data lives with the app.** `userData/boma/` — `pgdata/` (the embedded cluster), `master-key`
   (generated once, 64 hex, per-install), `artifacts/`. Deleting it resets the deployment; the
   first-launch failure dialogs name the folder and that option.

## Non-goals / explicitly out of scope

- macOS builds in M35 (owner's machine is Windows; Linux desktop already ships). macOS is a
  follow-up with the same recipe; the release matrix simply gains a job later.
- Code signing (Windows stays unsigned by owner decision, ADR-016; the Sigstore attestation
  flow is unchanged).
- Any change to the wire, the web, the server logic, or the spec. M35 is distribution.
- PGlite (Postgres in WASM) was considered and rejected: it is single-connection /
  single-user, which breaks the Rust backend's connection pool and the serve+worker split.
- Keeping the Docker path in the desktop (it cannot work on an installed machine); the compose
  file remains for self-hosters who build from source.

## Approaches considered

| Approach | Verdict |
| --- | --- |
| Embedded zonkyio Postgres binaries in the desktop | **Chosen.** Real Postgres (pool, RLS, migrations unchanged), all target OSes, Apache-2.0, ~15 MB per OS, no user setup. |
| PGlite (WASM Postgres) in the Electron process | Rejected: single connection breaks `sqlx` pooling and the serve/worker split; would force a rewrite we don't want. |
| Require Docker but bundle the compose file | Rejected: not the bar; Docker Desktop is a prerequisite the owner refuses. |
| Require a system Postgres | Rejected: same reason; breaks the double-click contract. |
| SQLite instead of Postgres | Rejected: the spec mandates Postgres and RLS. |

## How it works

1. **Bundle layout (per OS, packed by electron-builder into `resources/`):**
   - `postgres/` — the zonkyio binaries for the target OS (pinned `17.12.0`, fetched at
     package time by a script with SHA256 verification, never at user install time);
   - `boma/` — the `boma` server binary for the target OS (built by the release workflow);
   - `web/` — the static export (BOTH OSes gain it in M35; neither ships it today).
2. **First launch (main process):**
   - `BOMA_DESKTOP_URL` set → window onto that origin (unchanged behavior, no embedded stack).
   - Otherwise: ensure `userData/boma/master-key` exists (generate 64 hex once),
     `BOMA_WEB_ROOT=resources/web` is set for the children (the export is packed into the
     bundle — without it every page is the API's JSON 404);
     ensure `pgdata/` exists (else `initdb -U boma --auth=trust --no-locale -E UTF8`);
     the **HTTP port is persisted** in `userData/boma/port` (last good port tried first,
     else a free loopback port is picked and persisted — the origin stays stable so
     cookies/localStorage/Web Push survive relaunches; a genuinely occupied port triggers
     one re-pick); the **PostgreSQL port is picked fresh** on every boot (only the HTTP
     origin needs stability);
     start `postgres` (pg_ctl), wait for readiness (`pg_isready`);
     ensure the `boma` database exists (idempotent `CREATE DATABASE boma` — initdb does not
     create it);
     run `boma migrate` (migrations are embedded in the binary via `sqlx::migrate!`);
     spawn `boma serve` and `boma worker` as supervised child processes
     (`BOMA_LOCAL_FIRST=true`, `BOMA_DATABASE_URL=postgres://boma@127.0.0.1:PGPORT/boma`,
     `BOMA_BIND_ADDR=127.0.0.1:PORT`, `BOMA_MASTER_KEY=<the key>`, `BOMA_ARTIFACT_DIR`,
     `SANDBOX_PROVIDER=none`, `BOMA_LOG_FORMAT=pretty`);
     probe `/health` until ready, then `loadURL(http://127.0.0.1:PORT/)`.
   - **Failures are loud and named**: initdb fails / postgres fails / migration fails / the
     server never becomes healthy → a dialog naming the step, the log lines, and the data
     folder to delete for a reset. Never an anonymous spinner.
3. **Shutdown**: `before-quit` / window close kills worker → serve → `pg_ctl stop`.
   Orphaned children are reaped on the next launch (pid files in `userData/boma/`).
   **Runtime supervision**: if `serve` dies, the app quits with the failure dialog (nothing
   to work against); if `worker` dies, it is restarted once with backoff — a second crash
   inside the boot window quits the app with the dialog (something is systemically broken;
   hiding it is lying).
4. **Data**: everything under `userData/boma/` — nothing in the install directory.
   `userData/boma/port` persists the HTTP port; `userData/boma/pgdata/`, `master-key`,
   `artifacts/` and `port` are the whole deployment state — deleting the folder resets it.

## Release pipeline (release.yml)

New per-OS binary jobs feed the desktop pack:

- `boma-linux` (ubuntu-24.04, default gnu target) and `boma-windows` (windows-latest,
  msvc) build `cargo build --release --locked` and upload the binaries;
- the desktop jobs (ubuntu for the Linux pack, windows-latest for NSIS/zip) download them,
  fetch the pinned zonky binaries (SHA256-checked) into the pack, and build as today;
- assets keep their names; Sigstore attestation unchanged; Linux remains `dir` + tar.gz,
  Windows remains NSIS + portable zip;
- Windows installer size grows from ~90 MB to ~120 MB — accepted by-design (OpenBot is
  in the same range).

## Fetching Postgres (build time only)

`tools/fetch-embedded-postgres.sh` (re-runnable, pinned): downloads
`embedded-postgres-binaries-{linux,windows}-amd64-17.12.0.jar` from Maven Central, verifies
published SHA-256, extracts the jar (it is a zip of the portable PG tree) into
`apps/desktop/vendor/postgres/<os>/`, keeps the licence file (Apache-2.0) and prints a receipt
for `docs/DEPENDENCIES.md`. CI caches the extraction; local runs stay reproducible.

## Verification

- This machine (Linux): the full first-launch flow can be rehearsed — extract binaries, initdb,
  migrate, serve, worker, loadURL-equivalent (HTTP probe), reset, and the failure paths
  (corrupt pgdata, port already in use). Playwright opens the resulting window with the local
  stack and an axe sweep runs on it, as the v0.2.3 verification did.
- Windows: CI builds the NSIS and the binary; the owner smoke-tests the installed app (his
  machine is Windows; v0.2.x verification already follows that shape).
- Repo gates, CI green per push, including a new "desktop boots" job on ubuntu: the pack starts
  the embedded stack and 200-answers `/health` within a timeout — a regression guard for the
  whole path.

## Risks and honest limits

- **Windows verification**: I cannot drive a Windows UI from this machine; the pipeline can
  prove build + pack + signature, and the first-launch dialogs are code-reviewed paths, but the
  real click-through is the owner's machine (as for every Windows release so far).
- **Linux x86-64 only**: the signed-graphic Linux pack stays out; ARM64 Linux/macOS are out of
  scope here.
- **Size**: the Linux directory pack gets ~40 MB heavier (Postgres uncompressed).
- **Portability of zonky binaries**: they are built for reasonably recent glibc; the AppImage
  alternative remains available if a field machine is too old.
- **Scope honesty**: this is a milestone, not an afternoon — desktop main-process work, a
  packaging step, two CI binaries, a verification sweep on Linux, and the release. Small files,
  one PR-sized slice at a time.

## Slices (each CI-green, each on `dev`)

1. `tools/fetch-embedded-postgres.sh` + receipt (slice 1, commit `3e127d1`): pinned fetch, SHA-256 verified, the extracted tree is gitignored and re-fetched at package time.
2. Desktop main-process rewrite: first-launch lifecycle (initdb → serve → worker → load),
   supervision, failure dialogs, BOMA_DESKTOP_URL override preserved; Linux rehearsal harness
   `tools/desktop-stack-smoke.sh` (initdb → migrate → probe → stop) with axe run on the result.
3. electron-builder config + `boma-windows`/`boma-linux` jobs in release.yml; desktop jobs pack
   the binaries and Postgres; release notes, SHA256SUMS extended; attestation unchanged.
4. Cut **v0.3.0** from dev when green (desktop pack = the self-contained app).
