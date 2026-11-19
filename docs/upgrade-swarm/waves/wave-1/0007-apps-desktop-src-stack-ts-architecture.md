# 0007 — apps/desktop/src/stack.ts · v0.4.0 architecture proposal

- Brief: id 7, module `apps/desktop/src/stack.ts`, lens `architecture` (`docs/upgrade-swarm/briefs-upgrade.jsonl`).
- Status: **PROPOSAL only.** No code, spec, or release edited. Markers: **CONFIRMED** = read this session; **GUESS** = needs owner decision.
- Anchors: `apps/desktop/src/stack.ts` (read at `before-m5`), sibling 0005/0006 (main.ts) for boundary context.

## 1. What the module is today (CONFIRMED)

~450 lines, one module, one job: bring `postgres` + `boma migrate` + `boma serve` + `boma worker` up under `<userData>/boma/`, supervise, tear down in reverse. Key mechanics as shipped (M35):

- **State layout**: `pgdata/`, `master-key` (32 B hex, 0600), `port`, `artifacts/`, `serve.pid`/`worker.pid`, `postgres.log`, `boma.log`. Deleting the folder resets the deployment.
- **Named bring-up steps** (`StackStep`: master-key, initdb, postgres, migrate, serve, worker, healthy) and **`StackError`** carrying step + ≤12/20-line log tail; `persistFailure` appends `fatal: <step>: <detail>` to `boma.log` so support/CI can read what the dialog showed.
- **Postgres 17.12 embedded**: initdb only when `pgdata/PG_VERSION` is absent; a non-empty partial `pgdata` without it is refused with a reset-the-deployment message. Trust auth, UTF8, no locale. **Port persisted** only after health passes (`port` file, 0600); reused next launch iff loopback-free. PG's own port is re-picked per boot. `pg_ctl -w` gates start readiness.
- **Ordered shutdown** (`stopStackAt`): worker → serve (SIGTERM, 3 s grace, SIGKILL) → `pg_ctl -m fast -w stop`; pid files unlinked after.
- **Orphan reaping** (`reapOrphans`): on launch, SIGTERM/SIGKILL whatever `worker.pid`/`serve.pid` name from the previous unclean exit. serve/worker pids written post-spawn; serve exit → `fatal`; worker gets **one** silent retry, second exit → `fatal`.
- **Failure UX**: every failed step throws `StackError` → dialog with step name + log tail; migrate failure calls `stopStackAt` first (no half-up stack left); health gate = 90 × 1 s against `/health` with a 2 s abort.

## 2. Structural verdict

The bones are right: single ownership of the stack lifecycle, honest `PG_VERSION` cluster marker with partial-dir refusal, teardown in reverse dependency order, persist-port-only-on-success, best-effort failure persistence that never masks the original error, and clear M35 distribution-only posture (no network, no IPC in this module). Keep all of that.

Weaknesses, architecture class (CONFIRMED by reading):

1. **Supervision asymmetry / unhandled spawn errors.** `serve`, `worker` and the retry worker have **no `error` listener**. In Node, an unhandled `ChildProcess` `error` event (missing DLL/binary) throws — the desktop shell dies with an opaque exception instead of `StackError("serve"|"worker")`. `run()` handles `error` only for short-lived children.
2. **Pid-file reaping trusts stale pids.** `terminatePid` does `kill(pid, 0)` then SIGTERM without checking process identity. A recycled pid can SIGTERM an innocent process; and `reapOrphans` never reaps **Postgres itself** — a hard-killed app leaves a postmaster running on `pgdata`, and the next `pg_ctl start` fails with a confusing "lock file exists / another server might be running" instead of a clean recover.
3. **Port selection is TOCTOU.** `pickFreePort` closes the probe socket, then Postgres/`serve` bind later — a race that surfaces as a misleading step failure. The persisted-HTTP-port path re-checks but cannot reserve.
4. **Failure taxonomy is inconsistent.** master-key write/read, `mkdirSync` and `initdb` partial-dir throw raw exceptions, not `StackError`; the UI's named-step dialog contract silently degrades on exactly the steps most likely to fail on day one.
5. **No budgets.** `initdb` and `migrate` via `run()` have **no timeout** — a hung binary wedges boot forever with no dialog. Only `pg_ctl -w` and the health loop are bounded.
6. Unused `pgPort`/`_httpPort` params in `stopStackAt`; `workerCrashes` declared after its handler (hoisting smell); empty catches hide reap failures; single worker retry with no backoff and no `fatal` until the second crash.

## 3. What to keep (v0.4.0)

Named steps + log-tail `StackError`; `persistFailure` dual-write; `PG_VERSION` marker + partial-dir refusal; reverse-order teardown; persist-port-after-health; reap-before-start; 2 s `/health` abort; env-driven paths (`BOMA_STACK_RESOURCES`) for CI rehearsals; no Docker/no system PG; distribution-only (no telemetry).

## 4. Upgrade proposals (3–5) with risks

**P1 — One supervision policy, all children.** Add `error` listeners to serve/worker/retry and route them into the same `fatal` path with the pid file unlinked; give `run()` a per-step timeout budget (initdb 120 s, migrate 120 s, pg_ctl default) and convert master-key/mkdir failures to `StackError`. Result: every bring-up failure is a named step with a persisted tail — the dialog contract holds everywhere. *Risk:* masked timing regressions on slow disks; mitigate with `tools/desktop-stack-smoke.sh` on Linux+Windows and unchanged step order in `boot.log`.

**P2 — Postgres-aware orphan reaping with identity checks.** Extend `reapOrphans` to read `pgdata/postmaster.pid` (pid + start time) and `pg_ctl status`; reap a live postmaster via `pg_ctl stop -m fast` (never raw SIGTERM to a guessed pid). For serve/worker, verify identity before SIGTERM (command line contains the boma binary path / `pgdata`) before killing; skip with a log line otherwise. *Risk:* platform-specific identity checks (proc vs. tasklist) and killing a legit reused pid — keep `kill(pid,0)` + identity match, never kill on pid alone. **GUESS:** owner accepts `pg_ctl stop` against a foreign-owned cluster as safe (it is scoped to our `pgdata`).

**P3 — Authenticated loopback instead of trust auth.** Generate a random DB password at first boot (stored as `db-password`, 0600, next to `master-key`), switch initdb to `scram-sha-256`, thread the password into `BOMA_DATABASE_URL`. Loopback-only is not a sufficient boundary on a shared machine. *Risk:* breaks existing deployments' `pgdata` (trust→scram change needs re-init or `ALTER ROLE`); mitigate by gating on a `pgdata/.auth-scram` marker and migrating in a controlled step, falling back to trust + a loud log line for legacy clusters until v0.5.0. Requires owner sign-off on the wire-visible connection string change.

**P4 — Port allocation without the race.** For PG, keep the close-then-bind pattern but retry `pg_ctl start` once with a fresh port when the log shows a port conflict (distinct step error otherwise). For HTTP, keep persist-after-health; add a `BOMA_DESKTOP_PORT`/`--port` escape hatch for hosts with tight port ranges. *Risk:* a retry can mask a real postgres wedge; mitigate by retrying only on the port-conflict signal and never past one retry; document in UPGRADING.

**P5 — Structured last-boot report.** Write `last-boot.json` (step, exit code, ISO time, 4 KB capped tail of the relevant log, redacted) on every bring-up failure next to `boma.log`, in addition to the existing text append. Support/CI read one stable JSON instead of scraping prose; zero new deps (plain `fs`). *Risk:* dual sources of truth diverging — keep JSON a strict superset, generated from the same code path as `persistFailure`.

## 5. Suggested sequencing & gates

P1 → P2 (supervision PR), then P4 (small), then P5; P3 alone with owner sign-off. Each slice CI-green (typecheck/lint + Linux and Windows `desktop-boot` legs); no spec/REQ ids; M35 non-goals (macOS packing, code signing) untouched. Success = every bring-up failure surfaces as a named step with a persisted tail, no orphan postmaster after hard kill, Windows boot gate green.
