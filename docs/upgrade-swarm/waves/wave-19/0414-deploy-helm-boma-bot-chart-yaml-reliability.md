# 0414 — deploy/helm/boma-bot/Chart.yaml — Reliability lens (v0.4.0 proposal)

## Scope

`Chart.yaml` is six lines, but it is the reliability keystone of this chart: its
`version`/`appVersion` fields are consumed by the labels, the ConfigMap, and every
rendered image tag, and it declares the hooks whose behaviour lives in `templates/`.
This proposal treats the Chart + its values/hook templates as one unit. Chart is
explicitly local k3d bring-up only (values.yaml:1-3); hardening must not pretend HA.

Evidence below is CONFIRMED by reading files this session unless marked GUESS.

## Current state (CONFIRMED)

- `version: 0.3.0-rc`, `appVersion: "0.3.0-rc"`.
- `values.yaml` `image.tag: "0.3.0-rc"`; `Cargo.toml` version `0.3.0-rc5`;
  `compose.yaml` `BOMA_VERSION` default `0.3.0-rc`.
- `configmap.yaml` hardcodes runtime env (bind addr, jobs, realtime, sandbox)
  and derives `BOMA_VERSION` from `.Chart.AppVersion`.
- `migrate-job.yaml`: `helm.sh/hook: pre-install,pre-upgrade`,
  `hook-delete-policy: hook-succeeded`, fixed name `{{ fullname }}-migrate`,
  `backoffLimit: 5`, no `hook-weight`.
- `secret.yaml`: `fail`s when `boma.masterKey` empty.
- `_helpers.tpl`: `boma-bot.fullname` uses `.Chart.Name` only, ignores `.Release.Name`.
- No liveness probe on app/worker (readiness only), `replicas: 1`, floating image
  tag with `IfNotPresent`.

## Failure modes

### 1. Version / config drift across deploy surfaces
- **Four independent version fields** must change in lockstep for v0.4.0
  (Chart version, Chart appVersion, values.image.tag, Cargo/compose version).
  Missing one yields a pod advertising `0.3.0-rc` while running `0.4.0`, or an
  image tag that never moves. CONFIRMED.
- **Compose vs Helm env divergence**: compose sets `BOMA_REVISION`,
  `BOMA_DEGRADED`, `BOMA_LOCAL_FIRST`, and parameterises `BOMA_BIND_ADDR` and
  `BOMA_LOG_FILTER`; the Helm ConfigMap omits the first three and hardcodes bind
  addr and log filter. A parity bug that works in compose can silently differ in
  k8s. CONFIRMED.
- `BOMA_VERSION` source differs: compose from env override, Helm from
  `.Chart.AppVersion`. CONFIRMED.

### 2. Upgrade-hook hazards
- `hook-delete-policy: hook-succeeded` with a fixed job name: a **failed** migrate
  job is retained; the next `helm upgrade` tries to recreate the same name and
  wedges with `already exists`. No `before-hook-creation` cleanup. CONFIRMED.
- No `helm.sh/hook-weight`: ordering among hook resources is implicit. CONFIRMED.
- `pre-upgrade` migration runs against the **still-running old pods** (not drained),
  so a destructive/forward-only migration can break the old binary mid-flight.
  GUESS (depends on migration semantics; no compatibility guard found).
- Migration idempotency is assumed, not asserted. GUESS.
- No `helm upgrade --wait`/atomic guidance in chart metadata; a bad rollout can
  leave a half-upgraded release. GUESS.

### 3. Default-value traps
- `boma.masterKey` empty + `fail`: Helm does **not** remember `--set` values, so an
  operator who ran `install --set boma.masterKey=…` and then plain `helm upgrade`
  sees the upgrade fail. Worse, losing the key loses all sealed secrets (REQ-046).
  CONFIRMED (fail) / GUESS (operator behaviour).
- Secret material (`masterKey`, postgres password, external DSN) lives in release
  values/`stringData` plaintext. CONFIRMED.
- Insecure defaults `postgresql.password: boma`, floating `image.tag` with
  `IfNotPresent` (stale image on a node), `fullname` ignoring release name (two
  releases collide in one namespace), `replicas: 1`. CONFIRMED.

## Concrete hardening for v0.4.0

1. **Single version source.** Bump the triple to `0.4.0` (Chart version,
   appVersion, values.image.tag) and align Cargo/compose `BOMA_VERSION`. Add a CI
   check that fails when these four differ. Prefer a digest-pinned tag over a
   floating tag.
2. **Close compose drift.** Add `BOMA_REVISION`, `BOMA_DEGRADED`, `BOMA_LOCAL_FIRST`
   to the ConfigMap and expose bind addr/log filter as values with compose-matching
   defaults, so one env contract serves both surfaces.
3. **Make the migrate hook retry-safe:** `helm.sh/hook-delete-policy:
   before-hook-creation,hook-succeeded`, add explicit `helm.sh/hook-weight`, and a
   documented `--wait --atomic` upgrade command. Consider an init-container
   migration gated by old-pod scale-down if migrations are non-reversible.
4. **Stop the masterKey re-supply trap:** prefer `existingSecret` for key/DSN; keep
   the fail-fast but document that upgrades must re-pass it or migrate to a Secret.
5. **Safer defaults:** namespace-scope names via `.Release.Name` in `fullname`
   (with a stable override to avoid PVC orphans), and warn loudly on `boma/boma`
   credentials. Keep `replicas: 1`/no HA — documented as intentional for local use.

## Risks / trade-offs
- Changing `fullname` **renames resources and can orphan the Postgres PVC** on
  upgrade — gate behind a deliberate migration note, not a silent v0.4.0 bump.
- Adding env vars to the ConfigMap changes pod spec on upgrade → rollout restart;
  harmless locally but must be sequenced with migrations.
- `existingSecret` is a behavioural/API change (DEVIATION per AGENTS.md); needs
  sign-off before shipping.
- `hook-delete-policy` tweaks alter hook lifecycle; verify no in-flight migration
  is deleted by `before-hook-creation` on a rapid retry.

## Verification
- `helm template`, `helm lint`, `helm install --dry-run`; simulate a failed migrate
  hook then re-upgrade to prove no name collision.
- Diff rendered ConfigMap env against `compose.yaml` env keys.
- Assert version fields equal across Chart/values/Cargo/compose.
