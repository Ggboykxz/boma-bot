# Upgrading Boma Bot

For operators self-hosting Boma Bot. `0.1.0` is the first release; there is nothing to upgrade
*from* unless you run a pre-release build. Both paths are covered below.

## Desktop, self-contained (v0.3.0 line)

The Windows and Linux desktop builds are self-contained from v0.3.0-rc onward: the pack carries
the web export, the `boma` server and embedded PostgreSQL. Install it and open it — no Docker or
system Postgres install. On first launch it initialises a database in the app's user-data folder
(e.g. `%APPDATA%/Boma Bot/boma/`), starts it on a fresh loopback port, applies migrations and
opens the app. Delete `userData/boma/` to reset the deployment. `BOMA_DESKTOP_URL` still pins an
external origin. The Windows installer remains unsigned by owner decision (ADR-016); verify with
`gh attestation verify` as documented.

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

### Local-first bring-up instead of Path B (REQ-066)

If the machine is yours and the loopback interface is the network you mean to trust, you do not
need an account at all:

```bash
BOMA_LOCAL_FIRST=true BOMA_BIND_ADDR=127.0.0.1:3000 boma serve
```

Requests without a session cookie resolve as the machine owner (an implicit `local-owner` —
an org, one space and a user, seeded on first use); the web bundle boots into the product; no
password exists for this mode by construction. A request carrying a cookie is still strictly
resolved. The mode is off by default and refuses to start on a non-loopback bind, so it cannot
be flipped into a network-open deployment by environment drift alone. Code agents that drive
the binary over loopback (opencode, Claude Code, scripts) use the same path — there is nothing
else to authenticate. Do not set this on a shared or proxied host; it is the brings-up-a-judging-
laptop mode, not a team deployment.

## Path C — local k3d (single-node chart)

For hacking on Boma Bot from a clean checkout on your laptop — not a production posture
(ADR-016). There is no HA, no TLS opinion, and no rolling upgrade; one node runs everything,
and Postgres lives in-cluster with a throwaway PVC.

1. Install k3d and helm (`brew install k3d helm` or the distro equivalent).
2. Create a cluster:
   `k3d cluster create boma -p 3000:3000@loadbalancer`
3. Install the app:
   ```bash
   MASTER_KEY=$(python3 -c 'import secrets; print(secrets.token_hex(32))')
   helm install boma deploy/helm/boma-bot \
     --set boma.masterKey="$MASTER_KEY" \
     --set image.repository=boma-bot \
     --set image.tag=0.1.0
   ```
4. Wait for the Job (`kubectl get jobs`), then port-forward:
   `kubectl port-forward svc/boma-bot-app 3000:3000 &` and open <http://127.0.0.1:3000>.
5. Probe: `/health`, `/ready`, `/healthz`, `/readyz` — expect `{"ok": true}`.

What the chart carries: app (api + worker), an in-cluster Postgres with a throwaway PVC, a
`migrate` Job (helm hook), the service, and one ConfigMap + two Secrets. It requires the
`BOMA_MASTER_KEY` to be set — omitting it fails the chart with a legible message, never a
silent degraded boot.

Delete everything: `helm uninstall boma && k3d cluster delete boma`.

For anything beyond a laptop: point `externalPostgres.dsn` at a real Postgres, leave the
in-cluster one disabled, and put a reverse proxy with TLS in front of the service yourself.
The chart never creates TLS; an ingress is opt-in and references an existing secret you supply
via `ingress.tls`.

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

ADR-016: the update feed is static and inert; there is no silent auto-update. Updates are manual —
download the new pack from the release page and install over the previous one; the configuration
and the local cache live in the OS app directory and survive reinstalls.

**The pack is not signed by an Apple or Microsoft identity.** The release pipeline therefore
covers every published artifact — including the desktop pack and `SHA256SUMS.txt` — with a
**keyless Sigstore attestation** via the release workflow (GitHub Actions OIDC, no private key
is used or stored). You do not need an Apple Developer account to verify that the pack really
came from this repository's release workflow. Verify before installing:

```bash
gh attestation verify boma-desktop-<version>-linux-x64.tar.gz \
  --repo Ggboykxz/boma-bot
```

Expected output: JSON (or `verified` with exit code 0) with a SLSA v1 predicate, the workflow
path (`release.yml`), the ref (`refs/tags/vX.Y.Z`) and the commit hash that ran the release. If
attestation lookup fails (e.g. no GitHub access), compare SHA256 against `SHA256SUMS.txt` from
the same release page; those hashes are themselves attested.

Linux/macOS Gatekeeper note: an unsigned app may still trip a first-launch warning (right-click →
Open on macOS, `Allow` on Linux). This is expected for unsigned bytes; it is not an indication
the release is forged — the Sigstore attestation is the proof of origin.

Signing remains unsigned by owner decision (ADR-016); attestations are the chain of custody in
their place — verify them, don't skip them.

## Desktop, first launch (no server yet)

Since v0.2.3 the desktop app never strands you: on launch it probes
`http://127.0.0.1:3000/health`. If nothing answers, it offers a one-click local stack —
`docker compose up -d` with `BOMA_LOCAL_FIRST=true`, a loopback-only, no-account instance of the
whole product on your own machine — waits for readiness, then opens straight into it (no sign-in:
the machine is the trust anchor, REQ-066). If docker or the compose file is missing, the dialog
says so and names both supported paths: the one-click stack, or the server tarball plus
`BOMA_DESKTOP_URL` pointing at your origin. Nothing leaves the network except pulling images.

## Desktop on Windows (unsigned: what SmartScreen tells you)

The same release publishes two Windows artifacts (built on a real `windows-latest` runner, no
Wine involved):

- `boma-desktop-<version>-win-x64-setup.exe` — NSIS installer, per-user by default (no admin
  required), lets you pick the install directory.
- `boma-desktop-<version>-win-x64-portable.zip` — unpack anywhere, run `boma-desktop.exe`.

The `.exe` is **not code-signed** (ADR-016, no Apple/Microsoft identity exists for this project).
Windows SmartScreen will therefore show **"Windows protected your PC"** on first run of the
installer: click **More info → Run anyway**. That prompt appears because the binary is unsigned,
not because it is harmful — an attacker's forged pack looks identical to Windows, which is why
the provenance check matters:

```bat
gh attestation verify boma-desktop-<version>-win-x64-setup.exe --repo Ggboykxz/boma-bot
gh attestation verify boma-desktop-<version>-win-x64-portable.zip --repo Ggboykxz/boma-bot
gh attestation verify SHA256SUMS-windows.txt --repo Ggboykxz/boma-bot
```

Expected: JSON with a SLSA v1 predicate, workflow path `.github/workflows/release.yml`, ref
`refs/tags/vX.Y.Z`, and exit code 0. If GitHub is unreachable, compare SHA256 against
`SHA256SUMS-windows.txt` from the same release page (the attested file). SmartScreen may also
warn about the unpacked portable build; the same verification covers it.

## What changed with every release

Each release's notes say exactly what shipped, which REQ ids are covered and what remains
deferred (REQ-049 voice and REQ-056 mobile stay out by record; see `docs/PLAN.md`). Traceability
(`tools/trace_check.py`) is the gate: it only goes down, never up.
