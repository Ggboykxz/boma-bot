# 0413 — `deploy/helm/boma-bot/Chart.yaml` — architecture (v0.4.0)

- Worker #413 · Lens: architecture · Target: v0.4.0 · READ-ONLY review; no chart file was changed.
- Module: `deploy/helm/boma-bot/Chart.yaml` (6 lines) at `before-m5`: `apiVersion: v2`,
  `name: boma-bot`, `type: application`, `version: 0.3.0-rc`, `appVersion: "0.3.0-rc"`.
- Read for context (CONFIRMED by direct reads this session): `values.yaml`, all 8 files under
  `templates/` (`_helpers.tpl`, `app-deployment.yaml`, `worker-deployment.yaml`, `migrate-job.yaml`,
  `configmap.yaml`, `secret.yaml`, `postgres.yaml`, `ingress.yaml` — ingress not opened),
  `docs/UPGRADING.md` §Path C (86–115), `AGENTS.md` (DEVIATION / ADR / docs rules), and the peer
  architecture proposal `docs/upgrade-swarm/waves/wave-14/0309-crates-infra-src-plugin-rs-architecture.md`
  for the house format.

## Current structure (CONFIRMED)

`Chart.yaml` is pure identity: no `home`, `sources`, `maintainers`, `keywords`, `icon`,
`annotations`, `kubeVersion`, `dependencies`, `deprecated`, or `Chart.lock`. All runtime posture is
pushed into `values.yaml` and templates, which is the right split, but three consequences follow:

1. **Version truth is duplicated.** `appVersion: "0.3.0-rc"` feeds `BOMA_VERSION`
   (`configmap.yaml:12`) and the `app.kubernetes.io/version` label (`_helpers.tpl:23`), while
   `values.yaml:7` independently hardcodes `image.tag: "0.3.0-rc"` and `Chart.yaml:5` carries a third
   copy as `version`. Nothing checks they agree.
2. **The chart claims an override it does not implement.** `_helpers.tpl:9-10` says releases can
   override the name via `nameOverride`, but `boma-bot.fullname` (`:12-14`) is just
   `.Chart.Name | trunc 63 | trimSuffix "-"`, ignoring both `nameOverride` and `.Release.Name`.
3. **Database provisioning is a hidden fork.** `postgres.yaml` renders an in-cluster Postgres,
   PVC and `*-db` Secret when `.Values.postgresql.enabled`; otherwise a different `*-db` Secret is
   rendered from `externalPostgres.dsn` (`:101-111`). If *both* are set, the in-cluster branch wins
   silently and the user's DSN is ignored. `Chart.yaml` declares no dependency for either path.

`docs/UPGRADING.md:86-90` is honest that Path C is a laptop-only posture (no HA, no TLS, throwaway
PVC); that honesty is a feature and must survive v0.4.0.

## Guiding principle for v0.4.0

Keep `Chart.yaml` a small, declarative identity contract; put tunables in `values.yaml` and behavior
in templates. Make the three version copies mechanically consistent, make the metadata complete
enough to be installable/auditable, and make the two database paths mutually exclusive. Any change
that renames rendered resources or changes defaults is externally visible: record it in
`docs/PARITY.md` and take an ADR before shipping (AGENTS.md). No new language or service is needed.

## Concrete upgrades (5), each with a risk

1. **Complete the metadata block and add a `kubeVersion` guard.** Add `home`, `sources` and
   `maintainers` pointing at the Boma-owned repository, a few `keywords`, and optional
   `annotations: artifacthub.io/*`; keep `type: application` and set `kubeVersion: ">=1.26.0-0"`
   to match the APIs used (batch/v1, networking.k8s.io/v1 implied by `ingress.yaml`).
   *Risk:* URLs/owners can go stale, and an over-tight `kubeVersion` blocks clusters that work today.
   *Mitigation:* only self-owned links and a conservative floor; verify with `helm lint` plus
   `helm template --kube-version` on the oldest supported minor.

2. **Establish one version source of truth for the v0.4.0 cut.** Set `version: 0.4.0` and
   `appVersion: "0.4.0"` together, change `values.yaml` to `image.tag: ""`, and default it in
   templates to `.Chart.AppVersion` (`default .Chart.AppVersion .Values.image.tag`). Add a short ADR
   defining chart semver (chart changes) vs `appVersion` (binary release) and the bump rule.
   *Risk:* `appVersion` is already wired into `BOMA_VERSION` and the `app.kubernetes.io/version`
   label, so a bump silently changes pod config and labels; an empty default `tag` could render
   `repo:` if a template misses the fallback. *Mitigation:* add a CI assertion that
   `version`, `appVersion` and the rendered image tag agree, grep all three template call sites, and
   template-render before/after to diff the manifest.

3. **Add structural validation: `values.schema.json` + a CI gate.** JSON Schema over `image`,
   `boma.masterKey`, `postgresql`, `externalPostgres`, `resources`, `service` and `ingress`, checked
   by `helm lint --strict` and `helm template` in CI.
   *Risk:* a strict schema retroactively rejects existing `--set` overrides (notably the documented
   `helm install` command in `UPGRADING.md:98-102`), and new required fields become breaking changes.
   *Mitigation:* ship additive-only schema v1 with `additionalProperties: true`, keep `masterKey`
   `required` (already fail-fast at `secret.yaml:2-4`), and lint the exact documented install command
   as a fixture.

4. **Make the database choice explicit at the chart boundary.** In v0.4.0 keep the vendored
   `postgres.yaml` (no new dependency) but add a template-level `fail` when
   `postgresql.enabled` and `externalPostgres.dsn` are both set, and document the fork in
   `Chart.yaml` annotations plus `UPGRADING.md`. Write an ADR comparing this with declaring a real
   `dependencies:` entry (maintained Postgres subchart + pinned `Chart.lock` + `condition`).
   *Risk:* a real subchart brings network fetches, supply-chain/license review and a values mapping
   from `postgresql.user/password/database`; adding the `fail` breaks any (undocumented) config that
   relied on the silent override. *Mitigation:* land the mutual-exclusion guard first with a clear
   message, defer the dependency to a follow-up ADR, and assert both branches render valid YAML.

5. **Honour the name-override contract without breaking upgrades.** Implement `nameOverride` and
   `fullnameOverride` in `_helpers.tpl`, add the missing `NOTES.txt` and a `.helmignore`, and update
   the misleading comment at `_helpers.tpl:9-10`.
   *Risk:* changing `boma-bot.fullname` renames every resource, including the Postgres PVC
   (`postgres.yaml:18`) — an in-place `helm upgrade` would orphan data. *Mitigation:* defaults keep
   today's names byte-for-byte, overrides are opt-in and ADR-gated, and the upgrade doc warns that
   changing them requires a data migration.

## Cross-cutting risks / sequencing

- Sequence 2 → 3 → 4 → 5 → 1: fix version truth first (smallest, highest value), then validation
  and the DB guard, then naming, then metadata polish.
- Freeze rendered resource names, the `BOMA_*` keys in `configmap.yaml` and the fail-fast master-key
  message; `secret.yaml`, templates and `UPGRADING.md` are shared surfaces with other workers —
  coordinate rather than duplicate.
- Gate each step with `helm lint --strict`, `helm template` manifest diff, and the documented
  Path C install in a k3d cluster. Mark every verification CONFIRMED only after running it.

## Out of scope for this module

Liveness probes, `securityContext`, a ServiceAccount/NetworkPolicy, replica/HPA/PDB settings and
image digest pinning live in the Deployment templates (`app-deployment.yaml`,
`worker-deployment.yaml`, `migrate-job.yaml`, `postgres.yaml`) and belong to other workers' modules.
This proposal only covers the chart's identity, versioning and structural contract.
