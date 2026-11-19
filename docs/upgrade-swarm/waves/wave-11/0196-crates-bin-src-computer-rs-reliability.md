# v0.4.0 reliability proposal — `crates/bin/src/computer.rs` (computer selection / config)

Worker #196 · LENS=reliability · target v0.4.0 · READ-ONLY analysis (no code changed).

Status of claims: every "current state" statement below is CONFIRMED by a direct read of
`computer.rs` (all 432 lines), its consumer `bin/src/config.rs:318-352` and `bin/src/commands.rs:46-50`,
the runtime it selects `infra/src/computer.rs:1-598`, the port contract `app/src/ports.rs:1544-1691`,
and `app/src/usecases.rs:5578-5583`, in this worktree. Forward-looking items are proposals (GUESS
until implemented and tested). No tests were run for this proposal.

## Current structure (CONFIRMED)

- `from_pairs` (154-250) validates env input into `ComputerConfig` and fails closed, naming the
  variable: unknown/unsupported provider (165-183), non-absolute `DOCKER_HOST` for `docker`
  (187-197), CDP port (203-213), idle 1..=1_440 (215-221), memory 64..=4096 MB (223-229),
  CPU 0.25..=4.0 (231-237). Good: no provider = valid `none` deployment (166); unused socket is
  ignored for `none` (185-197); bounds are unit-tested (356-368).
- `provision` (94-126) builds `DockerComputerRuntime`, applies `command` via `split_whitespace`
  (105), publishes CDP on a random host port bound to 127.0.0.1, and pings with a bounded 3×
  retry + 500 ms backoff (112-122). `None` and unreachable Docker return `Unavailable`, never
  fake state.
- `Display` (129-146) logs the full config including `docker_host`, `image`, `frame_command` and
  `command`. The runtime (`infra/src/computer.rs`) runs operator `command` as container `cmd`,
  `frame_command` and tool `exec` via `sh -c`, and sets `memory`/`nano_cpus`/`shm_size` directly.

## Failure modes

1. **Dead limits — sandbox pressure by misconfiguration (reliability-critical).** The
   `BOMA_COMPUTER_MEMORY_LIMIT_MB`, `BOMA_COMPUTER_CPU_LIMIT` and `BOMA_COMPUTER_IDLE_MINUTES`
   values validated here (215-237) are **never consumed** by `provision`: it forwards only socket,
   image, command, frame command and CDP port (98-109). Container caps actually come from the
   per-computer DB row via `ComputerSpec` (`usecases.rs:5578-5583`, `infra/computer.rs:265-268`),
   and idle expiry from the store query (`stores/computer.rs:215`). An operator who sets these
   three variables believes the limits apply; they do not. Result: containers sized by whatever
   the row says — possible host OOM/CPU starvation and idle containers never reaped. CONFIRMED.
2. **Unbounded command / ping waits — hangs that never fail closed.** `provision`'s retry loop
   pings with no per-call deadline (114): a wedged daemon stalls each `ping()` (bollard's 60 s
   client timeout at `infra/computer.rs:39`) up to 3× plus backoff, so startup can hang for minutes
   instead of refusing fast. Worse, the runtime's `exec` and frame-command `sh -c` streams
   (`infra/computer.rs:327-389`, `391-453`) have **no timeout**: a tool call such as `sleep 1000`
   or a blocking network command holds the request indefinitely, and a hung `create_exec`/`start_exec`
   never yields an `Unavailable`. Only the CDP path has deadlines (`cdp.rs:101-120, 267`). CONFIRMED.
3. **Sandbox escape / isolation gaps selected by this config.** `HostConfig`
   (`infra/computer.rs:263-272`) sets memory/CPU/shm and one bind, but omits `cap_drop`, a seccomp
   or AppArmor profile, `read_only_rootfs`, `pids_limit`, `ulimits`, a non-root `user`, and
   `network_mode` — so tool-run `sh -c` runs as root in a default-bridge container with full
   egress (SSRF to the host network). `DOCKER_HOST` validation (187-197) accepts any absolute
   unix path, so a mis-set socket hands the daemon to the process. The image is a mutable tag
   (`alpine:3.21`, default 199), not a digest pin. The example browser command (377-389) uses
   `--no-sandbox --remote-debugging-address=0.0.0.0`: the Chromium sandbox is off and CDP has no
   auth token, so any local process that reaches the published port can drive the browser. CONFIRMED.
4. **Permission gating lives outside this module.** `frame_command`/`command` here are trusted env
   strings; `Display` logs them verbatim, and `provision` passes them to the runtime unchecked.
   `command` is split on whitespace (105) with no shell quoting, so any flag containing a space
   silently becomes two argv tokens and the container may boot wrong. `exec` in the runtime takes
   a caller-supplied `&str` and runs it with `sh -c` (`infra/computer.rs:339`): the container is the
   only barrier; authorization for that call is the caller's job, not this config's. A wrong
   `SANDBOX_PROVIDER=docker` on a workstation exposes that barrier to untrusted tool input. CONFIRMED.

## Concrete hardening (with risks)

1. **Wire the limits or delete them.** Either thread `idle_minutes`/`memory_limit_mb`/`cpu_limit`
   from `ComputerConfig` into the defaults used to create a computer, or remove them from the config
   and `Display` so the validated values cannot mislead. Prefer threading them as deployment-wide
   defaults that the per-computer row may narrow, never widen.
   *Risk:* changing effective container caps alters run outcomes → a DEVIATION in `docs/PARITY.md`.
   *Mitigation:* keep the current DB-row values authoritative for v0.4.0 and first emit a startup
   warning when a set bin limit differs from the effective one.
2. **Deadline every wait.** Wrap each `ping()` in `provision` in `tokio::time::timeout` (a few
   seconds) so the 3× retry is genuinely bounded; add a configurable exec timeout in the runtime and
   cancel the exec on expiry. Promote `idle_minutes` to an enforceable reaper config.
   *Risk:* a tight exec timeout cuts legitimate long commands. *Mitigation:* default generously,
   make it explicit config, and log the timeout with the external id (never the command/env values).
3. **Harden the container this config selects.** Pin the image by digest, add `cap_drop: ["ALL"]`,
   no-new-privileges, a seccomp profile, `read_only_rootfs` with a tmpfs, `pids_limit`, ulimits,
   a non-root `user`, and `network_mode` with an egress allow-list; require a CDP auth token and
   drop `--no-sandbox` where the image supports it.
   *Risk:* broken browser images are common under a dropped `SYS_ADMIN`/seccomp — differential-test
   `chromium.rs` conformance before enabling; ship as an opt-in flag with an ADR if parity breaks.
4. **Validate the command surface.** Reject `command`/`frame_command` values with embedded shell
   metacharacters or NUL at config time, or replace `split_whitespace` with a real argv parser and
   a small allow-list; document them as operator-trusted, never derived from tool input.
   *Risk:* over-strict rejection can block a legitimate browser flag → record any new refusal as a
   DEVIATION. *Mitigation:* start with logging of suspicious flags and a unit test for whitespace.
5. **Fail-closed socket binding.** For `provider=docker`, resolve/validate the unix socket exists
   and is not world-writable at startup; consider a dedicated boma socket/daemon user rather than
   the host default.
   *Risk:* strict socket checks can break container-in-container deployments; make them a warning
   first and gate to error in a later release.

## Cross-cutting risks / sequencing

- Order 1 → 2 → 3 → 4 → 5: surface the dead-config lie first, then bound hangs, then tighten
  isolation; each isolation step needs the conformance browser tests green.
- Items 1, 3 and 4 can change run outcomes → `docs/PARITY.md` entries + operator approval before
  shipping (AGENTS.md). Verification gate per step: `cargo fmt --check`, `clippy -D warnings`,
  unit + `tests/` integration, new timeout/socket/command tests carrying the REQ id in the name
  (`tools/trace_check.py` counts declarations, not comments).

## Out of scope for this module

Per-computer authorization for `exec`/`navigate`/`evaluate` (app/api layer), the store's idle
reaper, worker retry policy, and the CDP websocket implementation live elsewhere. This proposal
covers config validation, limit wiring, deadline policy and the isolation posture this module
selects.
