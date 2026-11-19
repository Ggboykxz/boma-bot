# Worker #276 — `crates/infra/src/computer.rs` — Reliability (v0.4.0)

Scope: reliability hardening of `DockerComputerRuntime` (REQ-027). Evidence is static
code inspection of the module only (CONFIRMED unless marked GUESS); no runtime test
was run by this worker. Behaviour changes need a DEVIATION entry in `docs/PARITY.md`.

## Failure mode 1 — Sandbox escape / weak isolation

Evidence: `ensure()` builds `HostConfig` (l.263-272) with only memory, `nano_cpus`,
one bind mount and `shm_size`. There is **no** `cap_drop`, `security_opt`
(`no-new-privileges`), `privileged: false`, `read_only_rootfs`, `user`, `userns_mode`,
`network_mode`, or seccomp/AppArmor profile. The container therefore inherits Docker
defaults: root user, full default capabilities, bridge networking with access to
other containers and (typically) the internet. `exec()`/`screenshot()` run caller
strings via `sh -c` (l.339, 407-411); that is by design, but it makes the missing
kernel-level confinement the *only* barrier between a command and the host.

Hardening:
- Set `cap_drop: Some(vec!["ALL"])`, add back only what the image truly needs; add
  `security_opt: Some(vec!["no-new-privileges:true"])` and explicit `privileged: false`.
- Set `network_mode: Some("none")` unless CDP publication requires bridge; keep CDP
  bound to `127.0.0.1` (already correct, l.248). If bridge is required, add a dedicated
  internal network per deployment.
- Run as a non-root `user` where the image supports it; set `read_only_rootfs: true`
  and keep `/home/bot` (the named volume) as the only writable path; add `tmpfs` for
  `/tmp` if needed.
- Validate `docker_host` and `image` at startup (typed config, fail fast); the current
  code only trims `unix://` (l.37). Refuse sockets outside an allow-list.

Risks: capability/network drops can silently break images that bind a port or need
`CAP_NET_RAW`; roll out behind a per-image capability allow-list and a boot smoke test.
`read_only_rootfs` can break browsers that cache to disk — pair with a `tmpfs` and test.
These are externally visible for custom images → likely DEVIATION.

## Failure mode 2 — Resource limits

Evidence:
- **Integer overflow (CONFIRMED code pattern):** `memory: ...unwrap_or(i64::MAX)
  * 1024 * 1024` (l.265). The `i64::MAX` fallback and any large `memory_limit_mb`
  overflow i64 (wrap in release, panic in debug — `panic` is denied by workspace lints).
  Unlike `cpu_limit`, no bound on `memory_limit_mb` is documented here.
- `nano_cpus` relies on a comment "bounded to [0.25, 4]" (l.266-268); there is no
  local validation, so a bad spec silently produces a wrong/negative value.
- No `pids_limit` → a fork bomb inside `exec`/`frame_command` can exhaust host PIDs.
- No `memory_swap`/`memory_reservation` → the container can swap beyond its memory cap.
- No disk/quota limit on the `boma-home-*` volume → a computer can fill the host disk.
- `screenshot()` frame path appends stdout to `bytes` with **no cap** (l.442-451),
  unlike `exec()` which honours `output_cap` (l.382-384). A runaway frame command
  OOMs the supervisor.
- No timeout on `start_exec` / output streaming (l.356-387, 423-451): a command that
  never exits holds the call open indefinitely. No healthcheck on the container;
  `ping()` (l.184-189) only checks the daemon.

Hardening:
- Replace the multiply with `saturating_mul(1024 * 1024)` and validate
  `memory_limit_mb`/`cpu_limit` in typed config at startup; never rely on a caller
  comment. Add `pids_limit: Some(256)`, `memory_swap` mirroring `memory`, and a
  documented disk budget (tmpfs quota or LVM/volume driver option).
- Give `screenshot()` an `output_cap` (reuse the `exec` cap constant), and wrap every
  docker/exec/CDP call in `tokio::time::timeout` with a central config value.
- Add a container healthcheck and treat "started but unhealthy" as `Unavailable` in
  `ensure()`; keep `ping()` as daemon liveness only.

Risks: strict pids/swap limits may kill legitimate multi-process browsers (Chrome
spawns many processes) — size the limit from a measured baseline, not a guess. A
screenshot cap changes behaviour for high-resolution frames → DEVIATION; choose a
cap above the largest golden frame. Timeouts need tuning per machine speed to avoid
flaky failures on slow CI.

## Failure mode 3 — Orphan containers and volumes

Evidence:
- `stop()` (l.455-477) only stops (10s, SIGTERM); it never removes the container, and
  there is no volume reclamation path anywhere. Volumes are "never recycled"
  (l.223), so disk use grows monotonically with every deleted deployment.
- If `start()` fails after `create_container()` succeeds (l.286-288), the error is
  returned while the created container and volume remain; retry relies on name reuse.
- The 409 branch re-inspects (l.294-319); if the container vanished in between,
  `inspect_container` 404s and the whole `ensure()` fails instead of retrying create.
- `ensure()` treats *any* inspect error as "not found" (l.198-205) and proceeds to
  create; a transient daemon error can cascade into a 409 then a second inspect.
- `start()` ignores `_spec` (l.516-528): a reused stopped container keeps the old
  memory/cpu limits, so a spec change is never applied.
- A 10s stop that times out returns `Unavailable` with no SIGKILL fallback → the
  container may stay running, untracked.

Hardening:
- Add explicit lifecycle: `stop()` stops then removes the *container* (keep the home
  volume); add a separate `reclaim(computer_id)` / retention job that removes volumes
  for deleted deployments, gated on the `boma.computer` label already present.
- Make `ensure()` idempotent under races: on inspect 404 during the 409 branch, retry
  create once; distinguish transient errors (retry with backoff) from genuine 404.
- Recreate the container (preserving the volume) when the spec's limits differ from
  the inspected container's `HostConfig`; stop with SIGTERM then SIGKILL fallback.
- Add a startup sweep that finds labelled containers/volumes with no live deployment
  and logs or prunes them (opt-in flag).

Risks: removing containers changes observable state (live CDP sessions die on stop)
→ DEVIATION, and volume GC is irreversible — require an explicit retention window and
a dry-run mode first. Recreating on spec drift can lose in-memory state; document it.

## Priority for v0.4.0
1. Fix the overflow and cap screenshot output (small, no interface change).
2. Add cap/security-opt/timeouts/pids limits behind configuration.
3. Container removal + opt-in volume reclamation with dry-run.

Each item needs REQ-tagged tests (module-level `req_027_*` names) and, for any
externally visible change, a `docs/PARITY.md` DEVIATION awaiting approval.
