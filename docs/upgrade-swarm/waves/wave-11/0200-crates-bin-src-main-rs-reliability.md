# Worker #200 — `crates/bin/src/main.rs` — reliability lens — v0.4.0 proposal

Scope note. `main.rs` is 20 lines: `#![deny(missing_docs)]`, seven `mod` declarations and
`#[tokio::main] async fn main() -> anyhow::Result<()> { cli::run().await }`. It owns no business
logic by design, so most failure modes live one layer down in `cli.rs`, `commands.rs` and
`telemetry.rs`; they are listed here because `main` is the only process-wide boundary that can
own them. CONFIRMED by reading `main.rs`, `cli.rs` (run/load_config L74–97), `config.rs`
(`from_pairs`/`ConfigError`), `commands.rs` (serve L42–98, worker L285–363, shutdown_signal
L431–457), `telemetry.rs`, `probe.rs` and `spec/01-requirements.md` REQ-013/053/054/058.
Runtime impact is GUESS until the tests at the end exist. Focus, as requested: startup panic,
config errors, graceful shutdown.

## Role of this module

Composition root and process edge: the one place allowed to use `anyhow`, the place that builds the
Tokio runtime, the place whose return value becomes the OS exit status. It should own the three
process-level concerns it currently delegates or omits: runtime construction, panic/error
reporting, and signal/drain policy. Everything else stays in `cli`/`commands`.

## Failure modes

### A. Startup panic

- **FM-1 (CONFIRMED macro, GUESS impact).** `#[tokio::main]` (L17) builds a multi-thread runtime
  before the function body; a runtime-build failure panics with no structured event and no cleanup.
  The default worker count is the CPU count, which is pathological on low-end hosts and in
  containers with a high CPU limit but small quota (AGENTS.md criterion 4).
- **FM-2 (CONFIRMED).** No panic hook or top-level boundary. A panic anywhere in `cli::run`
  (provisioning, `build_app`, a dependency's `unwrap`) unwinds out of `main`: plain stderr panic
  text, exit 101, no JSON log, no drain. AGENTS.md forbids *our* panics, not dependency panics.
  In-flight runs are then orphaned until lease expiry (REQ-013/REQ-014).
- **FM-3 (CONFIRMED).** `Cli::parse()` (cli.rs:75) is clap's `parse`, which calls `process::exit`
  for `--help`/`--version`/parse errors. That bypasses `Drop` and every process-level hook, and
  clap writes its own unstructured output. Fine for a CLI, but there is no single error path.

### B. Config errors

- **FM-4 (CONFIRMED).** Returning `anyhow::Result` from `main` uses Rust's `Termination` impl: one
  `Error: {err:?}` line on stderr and exit code **1 for every failure class** — bad config, DB down,
  bind failure, failed migration. An orchestrator cannot distinguish "operator misconfiguration,
  stop restarting" from "dependency not up yet, retry" (contrast the lazy-pool intent at
  commands.rs:35–37).
- **FM-5 (CONFIRMED).** `telemetry::init` runs inside `load_config()` *after* `Config::from_env`
  (cli.rs:93–96), so config errors are emitted before any subscriber exists. Worse, the `probe` and
  `automation` commands never call `load_config()`, so they run with no subscriber at all —
  `tracing::info!("probe succeeded")` at probe.rs:47 is silently dropped.
- **FM-6 (CONFIRMED, also noted by worker #132).** `?` in `from_pairs` stops at the first bad
  variable. With ~30 `BOMA_*` keys the operator fixes one, restarts, and finds the next.
- **FM-7 (CONFIRMED).** A telemetry-init failure (bad `BOMA_LOG_FILTER`, double init) is itself
  reported pre-subscriber; `admin check-config` (commands.rs:426) also only logs, so it prints
  nothing when init failed. No stable, machine-readable config error format.

### C. Graceful shutdown

- **FM-8 (CONFIRMED).** Only `serve` wires `.with_graceful_shutdown(shutdown_signal())`
  (commands.rs:95). The `worker` loop (commands.rs:338–362) has no signal handling: SIGTERM kills
  it mid-`step()`, abandoning a leased run. REQ-013 requires handing it to another process and
  REQ-014 requires recovery with attempt history intact.
- **FM-9 (CONFIRMED).** axum's graceful shutdown has no drain deadline: one hung in-flight request
  blocks `serve()` forever until the orchestrator sends SIGKILL — the exact outcome graceful
  shutdown exists to prevent.
- **FM-10 (CONFIRMED).** In `shutdown_signal()` (commands.rs:431–457) the `ctrl_c` block logs a
  registration error and then *returns*, resolving that `select!` branch: a failure to register
  the handler is mistaken for a signal and shuts the service down (fail-open). On non-unix,
  `terminate` is `pending` — no SIGTERM at all. After the first signal there is no force-exit path,
  so a stuck drain can only be killed by SIGKILL.
- **FM-11 (GUESS).** `/ready` is not flipped during drain, so the orchestrator can keep routing new
  work to a process that is about to exit, widening the loss window (REQ-053).

## Concrete hardening for v0.4.0 (each with a risk)

- **H1 — Explicit runtime.** Replace the macro with `tokio::runtime::Builder::new_multi_thread()`,
  `enable_all()`, `thread_name("boma-rt")`, worker count from validated config, `.build()?`, then
  `block_on`. *Risk:* changes runtime topology and adds a config key; needs an ADR; keep the unset
  default identical to today's behavior.
- **H2 — Panic boundary.** Install a `set_hook` before any work: emit one structured ERROR
  (version/revision, panic text as *data*, never parsed/executed), then delegate to the default
  hook and keep unwinding. *Risk:* global and idempotent-required; dependency panic strings are
  untrusted, so never interpolate them into format strings or logs as trusted fields.
- **H3 — Exit-code taxonomy.** `main` maps typed errors to stable codes (0 ok, 2 usage, 3 config,
  4 dependency, 5 migration/data, 70 internal) and emits one final structured event.
  *Risk:* process exit codes are externally visible → DEVIATION, record in `docs/PARITY.md` and get
  owner sign-off before shipping.
- **H4 — Bootstrap telemetry first.** Init one subscriber for *every* job from a shared
  `TelemetrySettings::from_pairs` (no full `Config` needed), then load config. Fixes the dropped
  `probe`/`automation` logs and gives config errors a structured channel.
  *Risk:* duplicated defaults unless shared with worker #132's `HttpConfig::from_pairs` proposal —
  make one helper the owner.
- **H5 — Aggregate config errors.** Collect `Vec<ConfigError>` and report all bad variable names
  (never values) at once, exiting 3. *Risk:* touches binary `ConfigError` and the API error types;
  coordinate with #132; if any golden asserts stderr, record the DEVIATION.
- **H6 — One shutdown primitive.** Move `shutdown_signal()` into a shared module used by both
  `serve` and `worker`; on signal the worker stops claiming and publishes its in-flight run
  (REQ-013) before exit, and an `AtomicBool draining` makes `/ready` answer 503 (REQ-053).
  *Risk:* the hand-off path must exist in the app layer; if not, at minimum stop claiming and rely
  on lease expiry + REQ-014, and document the gap rather than claiming parity.
- **H7 — Bounded drain + force quit.** Race the graceful-shutdown future against
  `BOMA_DRAIN_TIMEOUT` (default below the orchestrator grace period, never shorter than the
  lease-renew period), WARN on timeout, and let a second signal force immediate exit.
  *Risk:* too short a deadline drops work; tune against REQ-013/014 lease timings and keep it
  configurable.
- **H8 — Fail-closed signal registration.** If one signal source fails to register, disable only
  that source and emit an error; if none can be registered, exit with a distinct error instead of
  running with no shutdown path. *Risk:* a new fail-fast condition needs an operator-visible
  message and a testable signal-source abstraction.

## Explicitly rejected / deferred

- `panic = "abort"` — skips all cleanup and contradicts AGENTS.md's "keep unwinding".
- `catch_unwind` around `cli::run` to continue after a panic — an unknown corrupted state is unsafe
  to resume; prefer clean exit + log + orchestrator restart.
- Signal handling for short-lived commands (`probe`, `migrate`, `check-config`) — unnecessary.

## Success criteria (test-first; REQ id in the name)

- `req_013_worker_hands_off_inflight_run_on_sigterm` — SIGTERM a worker mid-run; assert the run is
  republished with attempt history and the process exits 0 within the drain deadline.
- `req_053_ready_turns_503_while_draining` — contract/golden test of the draining flag.
- `req_054_panic_hook_emits_one_structured_event` — unit-test the hook formatter (inject it; do not
  panic in production code); assert no secret field is present.
- `req_058_config_error_exit_code_is_stable` — spawn with a bad env; assert exit 3, the variable
  name present, its value absent.
- `req_053_probe_logs_are_visible` — spawn `boma probe` against a test listener; assert the JSON
  "probe succeeded" line is captured.
- `req_053_signal_registration_failure_keeps_serving` — fake failing signal source; assert `serve`
  continues until the other source fires.
- `req_053_drain_timeout_forces_exit` — hang one handler, SIGTERM, assert exit within the configured
  deadline plus slack and a WARN event.

## Risks & rollout

Phase 1 (H2, H4, H8): observability and signal correctness, no contract change. Phase 2 (H6, H7):
shutdown semantics, coordinated with the app-layer REQ-013 hand-off and measured against lease
timing. Phase 3 (H1, H3, H5): runtime topology (ADR) and exit codes / error format (externally
visible — `docs/PARITY.md` plus owner approval, per AGENTS.md). Do not change observable behavior
silently; mark every claim above CONFIRMED only where a command or test in this session proved it.
