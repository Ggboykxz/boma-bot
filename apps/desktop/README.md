# `apps/desktop` — the Boma Bot desktop shell (REQ-055)

An Electron shell for the web app: the browser session in a partition only this app owns,
native WebAuthn via the loopback origin, a menu affordance to sign out (clearing the partition),
a consent-gated "run a local stack" action wired to the Compose file, and a signed-update path
that stays inert until the owner signs.

## What ships / what does not

- **Shipped (this milestone):** the shell itself — window, partition, native menu, local-stack
  runner, update-feed wiring (inert by default), packaged `electron-builder --dir` in CI.
- **Not wired:** the signing identity and the update feed. `BOMA_DESKTOP_SIGNED=true` turns
  checking on; without a signed feed it stays off (ADR-016). No certificate is embedded anywhere
  in this tree, and none should be: signing keys live with the owner.
- **Not in this shell:** no Node in the renderer, no API proxying, no cookies shared with any
  other window or profile. The deployment origin is a same-origin loopback by default
  (`BOMA_DESKTOP_URL`, port 3000), overridable at package time for a hosted deployment.

## Build and run

```sh
npm ci
npm run build        # tsc -> main/main.js
npm start            # build + electron
npm run dist         # electron-builder --dir (unsigned pack)
```

CI carries a `desktop` job: installs, typechecks, builds, runs the smoke, builds the pack on
ubuntu-latest (it has the system GTK libraries). On this machine there is no GTK, so runtime
is verified by CI only.

## The self-contained build (M35)

The desktop is fully self-contained for the default case: the pack carries the web export,
the matching `boma` binary and an embedded PostgreSQL 17.12.0 (zonkyio, Apache-2.0). First
launch runs `initdb` into the app's user-data `boma/` folder, starts Postgres on a fresh
loopback port, applies the migrations (they are compiled into the binary), starts
`boma serve` and `boma worker` as supervised children, and opens the app — no Docker, no
system Postgres, no account, no setup.

- The HTTP port persists in `userData/boma/port` across launches.
- `BOMA_DESKTOP_URL` still pins the shell to an external origin (a self-hosted server); the
  embedded stack is not started in that case.
- `userData/boma/` holds the cluster, master key, artifacts and the persisted port; deleting
  it resets the deployment.
- Local rehearsals without a GUI: `tools/stage-desktop-vendor.sh linux` (stages the pack
  ingredients) and `tools/desktop-stack-smoke.sh` (initdb → migrate → serve/worker → RPC).
- The Windows installer is unsigned by decision (ADR-016); verify with `gh attestation
  verify`, never by running the unsigned binary.
