# 0119 — compose.yaml — architecture (v0.4.0)

- Target: `before-m5` (v0.3.0-rc5), module `compose.yaml` (99 lines).
- Read-only review; no deployment or product file edited. Read alongside `Dockerfile`, `.env.example`,
  `crates/bin/src/{cli,commands}.rs`, `docs/adr/018-deployment-ci.md`, ADR-001/009/010/021/030, and
  `deploy/helm/boma-bot/templates/`. English per repo disk convention. Spec/ADR/charter stay
  authoritative; nothing below weakens a gate. Version target v0.4.0.

## 1. Stack structure as it stands (CONFIRMED)

Four services behind one `x-app-environment: &app-environment` anchor, one `db-data` volume:

- `db` — `postgres:17.11-alpine`, server-side `statement_timeout=5000` and
  `idle_in_transaction_session_timeout=30000`, `pg_isready` healthcheck, published `127.0.0.1:5432`.
- `migrate` — `build: .`, `command: migrate`, `restart: no`, waits on `db: service_healthy`.
- `app` — `build: .`, `command: serve`, `restart` unset, published `127.0.0.1:3000`, waits on
  `db` healthy *and* `migrate: service_completed_successfully`, `boma probe --path /ready` healthcheck.
- `worker` — `build: .`, `command: worker`, `restart: unless-stopped`, same migrate gate, no healthcheck.

`boma` CLI dispatch (`cli.rs:29-46,86-88`) confirms the three subcommands are first-class entry
points on one binary: `serve` (Axum), `worker` (claim loop, `commands.rs:338-362`, 250 ms poll,
lease keyed by pid at `:368`), `migrate`, plus `probe`. The worker exists because the v0.1.0 audit
found that without it the API serves traffic but never drains the queue (compose comment :86-87).
Keep the four-role separation: it is the correct shape for "one binary, many entry points" and it
mirrors the Helm chart's `migrate-job` / `app-deployment` / `worker-deployment` split. Do not merge
roles back into a supervisor-in-one-container (that would undo the audit fix).

## 2. Architecture debt worth fixing in v0.4.0

1. **Three independent `build: .` keys for one image.** `migrate`, `app`, `worker` each declare the
   same build context. Compose dedupes identical contexts today (verify with `docker compose config`
   — GUESS until run), but there are three places to drift (a build arg or target added to one
   service silently forks the "one binary" into two). Anchors are used for env and not for build.
2. **ADR-018 describes a different topology.** Its decision text (ADR-018 :35) says "db, app (which
   runs migrate then serve), and an optional supervisor profile"; the file has separate `migrate` +
   `worker` and no `supervisor` profile, and its own header comment still claims "one migration
   step, one server (ADR-018)". This is the same silent ADR/reality drift the CI worker flagged for
   the missing contract gate. The topology in the file is the audited one; the ADR is stale.
3. **No explicit lifecycle contract.** Restart policies are inconsistent by accident (`migrate: no`,
   `worker: unless-stopped`, `app`/`db` unset). No `stop_grace_period`, so the 10 s default can cut a
   worker holding a pid-keyed run lease or an `app` draining in-flight HTTP, both of which AGENTS.md
   requires to finish gracefully. No `init: true`, though these processes spawn children (ACP CLI
   agents ADR-030, stdio MCP servers ADR-021, computer backends ADR-009) that need PID-1 reaping.
4. **Config is half-parameterised and quietly divergent.** `SANDBOX_PROVIDER`, `BOMA_BIND_ADDR`,
   `BOMA_MASTER_KEY`, `BOMA_LOCAL_FIRST` use `${...:-default}`, but `BOMA_LOG_FORMAT: json`,
   `BOMA_JOBS: postgres`, `BOMA_REALTIME: postgres`, `BOMA_RUNTIME: sandboxed` are hardcoded — no
   operator override is possible without editing the file. `BOMA_JOBS=postgres` disagrees with
   `.env.example`'s `inline` (intentional: the queue must be shared for the worker to drain it).
   `BOMA_VERSION` defaults to `0.3.0-rc` while the tree is `0.3.0-rc5` (same rot the CI worker found).
5. **Single stateful service is right, but the backup story is implicit.** `db-data` is the only
   volume and ADR-018 requires a documented backup step before a migration; nothing in this file or
   its comments points to one. Publishing 5432 on loopback is needed for local dev/differential
   tests but is easy to forget when hardening a deployment.

## 3. Concrete upgrades for v0.4.0 (with risks)

**P1 — One image, one build (do first; mechanical).** Add `x-app-build: &app-build` carrying
`build: .` plus an explicit `image: boma:${BOMA_VERSION:-0.4.0}` tag, and have `migrate`/`app`/
`worker` reference the anchor (or a single `image:` plus one builder service) so exactly one build
produces the binary all three run. Risks: (a) if Compose currently builds one shared image, this is
clarity not a size win — confirm before claiming a speedup; (b) a bad anchor merge can yield an
implicit `:latest` or a per-service retag, so validate with `docker compose config` and a clean
`docker compose build` before merging; (c) no wire/API change, no ADR impact.

**P2 — Reconcile ADR-018 with the four-service reality (decision, owner veto).** Submit an
ADR-018 amendment recording `db` / `migrate` / `app` / `worker`, the v0.1.0 queue-drain rationale,
and that "supervisor profile" was not adopted; fix the stale header comment in this file to match.
Do **not** change the topology to satisfy the old text. Risks: (a) the owner may prefer the
supervisor design — that is a deployment-visible change and must be their call; (b) an amendment
that merely rubber-stamps reality is worthless — cite the audit finding as the evidence. No code
change; safe to land early.

**P3 — Define the lifecycle contract explicitly.** Set `restart: unless-stopped` on `app` for
symmetry with `worker` (keep `migrate: no` — one-shot, idempotent; keep `db` policy deliberate), add
`init: true` on `app`/`worker` for child reaping, and a `stop_grace_period` long enough to drain an
in-flight request (suggest ~40 s, above `BOMA_REQUEST_TIMEOUT_MS=30000`) and let a worker release its
lease. Risks: (a) a crash-looping `app` with `unless-stopped` can hide a boot bug — verify with a
smoke `up` after the change and watch `/ready`; (b) longer grace slows `down`, so document it;
(c) a run can outlive any grace period and rely on lease expiry — GUESS the lease TTL from
`crates/app/src/worker.rs` before promising "no abandoned runs"; (d) graceful shutdown itself must
already be wired in `serve`/`worker` (AGENTS.md), else this only adds waiting.

**P4 — Single-source the config surface.** Turn the four hardcoded env entries into
`${BOMA_LOG_FORMAT:-json}`, `${BOMA_JOBS:-postgres}`, `${BOMA_REALTIME:-postgres}`,
`${BOMA_RUNTIME:-sandboxed}` so `.env` can override them, and set the `BOMA_VERSION` default from
one repo version source (or a neutral `0.0.0-dev` as the CI worker suggested for chart tags).
Keep `BOMA_BIND_ADDR` default `0.0.0.0:3000` inside the container with the loopback publish — the
comment explains the local-first desktop case and it is intentional. Risks: (a) making `BOMA_JOBS`
settable invites `inline`, which would starve the worker — keep `postgres` as the documented
deployment invariant and warn if overridden; (b) the version string appears in `/ready`'s capability
list, so changing the default is a value change, not a shape change — confirm no golden asserts it.

**P5 — Make the data boundary explicit.** Add a short comment (or `docs/operate.md` pointer) tying
`db-data` to the ADR-018 backup-before-migrate step, and offer the 5432 publish as an opt-in profile
or `${BOMA_DB_PUBLISH:-127.0.0.1:5432:5432}` so a hardened deployment can drop it. Risks: (a)
removing the port outright breaks local dev, differential tests and direct-DB tooling — make it
configurable, never delete; (b) a profile change is deployment-visible, so note it in `docs/PARITY.md`
if it alters the default `docker compose up` experience. No new service or datastore: keep the
"few moving parts" property.

## 4. Ordering and non-goals

Order: **P2 → P1 → P3 → P4 → P5.** Non-goals, explicitly: no merge of `migrate`/`worker` into `app`,
no supervisor-in-one-container, no second datastore/queue, no removal of the loopback-only publish,
no weakening of healthchecks or the `service_completed_successfully` migration gate, no `.env` with
real secrets, no ADR edit without the owner. Anything that changes the published API, wire or data
format is a DEVIATION for `docs/PARITY.md` and owner approval — none of P1–P5 should need one.

Success for v0.4.0: one build produces one binary for all three app roles, ADR-018 matches the file,
every service has a deliberate restart/init/stop policy, every tunable has one home, and the backup
step is discoverable from the compose file.

## 5. Evidence

CONFIRMED — read this session: `compose.yaml` :1-99; `Dockerfile` :1-52; `.env.example` :1-132;
`cli.rs` :29-46,86-88; `commands.rs` :24-37,330-369; ADR-018 :33-49; the Helm template file list.
GUESS — whether Compose dedupes the three identical `build: .` contexts into one image (verify with
`docker compose config`); the worker run-lease TTL in `crates/app/src/worker.rs` (verify before P3's
"no abandoned runs" claim); whether any golden asserts the `BOMA_VERSION` value (verify before P4).
