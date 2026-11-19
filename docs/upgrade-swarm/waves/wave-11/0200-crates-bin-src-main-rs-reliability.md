# Worker #200 — `crates/bin/src/main.rs` — reliability lens — v0.4.0 proposal

Scope (CONFIRMED by reading `main.rs`, `cli.rs` L74–97, `config.rs`, `commands.rs` L42–98/285–363/
431–457, `telemetry.rs`, `probe.rs`, spec REQ-013/053/054/058): `main.rs` is 20 lines —
`#[tokio::main] async fn main() -> anyhow::Result<()> { cli::run().await }`. It owns no business
logic, but most failure modes sit one layer down; they are listed here because `main` is the only
process-wide boundary able to own them. Impact is GUESS until the tests below exist.

## Role of this module

Composition root and process edge: it builds the runtime, uses `anyhow`, and its return value is the
OS exit status. It should own runtime construction, panic/error reporting and signal/drain policy;
everything else stays in `cli`/`commands`.

## Failure modes

### A. Startup panic

- **FM-1 (macro CONFIRMED, impact GUESS).** `#[tokio::main]` (L17) builds a multi-thread runtime
  before the body; a build failure panics with no structured event. Default worker count = CPU count,
  pathological on low-end hosts and containers with high CPU limits but small quotas.
- **FM-2 (CONFIRMED).** No panic hook or top-level boundary: a panic anywhere in `cli::run`
  (provisioning, `build_app`, a dependency's `unwrap`) unwinds out of `main` → stderr text, exit 101,
  no JSON log, no drain; in-flight runs are orphaned until lease expiry (REQ-013/014). AGENTS.md
  forbids *our* panics, not dependency panics.
- **FM-3 (CONFIRMED).** `Cli::parse()` (cli.rs:75) is clap's `parse`, which calls `process::exit` for
  `--help`/`--version`/parse errors — bypassing `Drop` and all process hooks, with unstructured
  output. Fine for a CLI, but there is no single error path.

### B. Config errors

- **FM-4 (CONFIRMED).** `Result<()>` in `main` uses Rust's `Termination` impl: one `Error: {err:?}`
  on stderr, exit code **1 for every class** — bad config, DB down, bind failure, failed migration.
  An orchestrator cannot tell "misconfigured, stop restarting" from "DB not up, retry" (contrast the
  lazy-pool intent at commands.rs:35–37).
- **FM-5 (CONFIRMED).** `telemetry::init` runs inside `load_config()` *after* `Config::from_env`
  (cli.rs:93–96), so config errors precede any subscriber. `probe` and `automation` never call
  `load_config()`, so they have no subscriber at all — `tracing::info!` at probe.rs:47 is dropped.
- **FM-6 (CONFIRMED; also worker #132).** `?` in `from_pairs` stops at the first bad variable among
  ~30 keys: fix one, restart, discover the next.
- **FM-7 (CONFIRMED).** A telemetry-init failure (bad filter, double init) is itself reported
  pre-subscriber, and `admin check-config` (commands.rs:426) only logs — so it prints nothing. No
  stable machine-readable config error format.

### C. Graceful shutdown

- **FM-8 (CONFIRMED).** Only `serve` wires `.with_graceful_shutdown(shutdown_signal())`
  (commands.rs:95). The `worker` loop (L338–362) has no signal handling: SIGTERM kills it mid-`step()`,
  abandoning a leased run. REQ-013 requires handing it on; REQ-014 requires recovery with history.
- **FM-9 (CONFIRMED).** axum's graceful shutdown has no drain deadline: one hung in-flight request
  blocks `serve()` until the orchestrator sends SIGKILL — the outcome graceful shutdown prevents.
- **FM-10 (CONFIRMED).** In `shutdown_signal()` (L431–457) the `ctrl_c` block logs a registration
  error and *returns*, resolving that `select!` branch: registration failure is mistaken for a
  signal (fail-open). Non-unix uses `pending` (no SIGTERM). No second-signal force-exit path.
- **FM-11 (GUESS).** `/ready` is not flipped during drain, so new work can keep arriving at a process
  about to exit (REQ-053).

## Concrete hardening for v0.4.0 (with risks)

- **H1 — Explicit runtime.** Replace the macro with `Builder::new_multi_thread()`, `enable_all()`,
  `thread_name("boma-rt")`, worker count from validated config, `.build()?`, `block_on`. *Risk:*
  changes runtime topology, needs an ADR; keep unset default identical to today.
- **H2 — Panic boundary.** `set_hook` before work: one structured ERROR (version/revision, panic text
  as *data*), then the default hook, keep unwinding. *Risk:* global/idempotent; dependency strings
  are untrusted — never used as format strings or trusted fields.
- **H3 — Exit-code taxonomy.** Map typed errors to stable codes (0 ok, 2 usage, 3 config, 4
  dependency, 5 migration/data, 70 internal) plus one final structured event. *Risk:* externally
  visible → DEVIATION in `docs/PARITY.md`, owner approval.
- **H4 — Bootstrap telemetry first.** Init one subscriber for every job from a shared
  `TelemetrySettings::from_pairs` (no full `Config`), then load config; fixes dropped probe logs.
  *Risk:* duplicated defaults unless shared with #132's `HttpConfig::from_pairs` — one owner.
- **H5 — Aggregate config errors.** Collect `Vec<ConfigError>`, report all bad names (never values)
  at once, exit 3. *Risk:* touches api error types; coordinate with #132; PARITY note if stderr is
  golden-asserted.
- **H6 — One shutdown primitive.** Share `shutdown_signal()` between `serve` and `worker`; on signal
  the worker stops claiming and publishes its in-flight run (REQ-013), and an `AtomicBool draining`
  makes `/ready` answer 503 (REQ-053). *Risk:* the hand-off path must exist in the app layer; if not,
  stop claiming, rely on lease expiry + REQ-014, and document the gap — do not claim parity.
- **H7 — Bounded drain + force quit.** Race graceful shutdown against `BOMA_DRAIN_TIMEOUT` (below
  orchestrator grace, never below lease-renew period), WARN on timeout, second signal = force exit.
  *Risk:* too short drops work — tune against REQ-013/014, keep configurable.
- **H8 — Fail-closed signal registration.** If one source fails, disable only it and emit an error;
  if none register, exit distinctly rather than run without a shutdown path. *Risk:* new fail-fast
  case needs an operator message and a testable signal-source abstraction.

## Explicitly rejected / deferred

- `panic = "abort"` — skips cleanup, contradicts AGENTS.md's "keep unwinding".
- `catch_unwind` around `cli::run` to continue — corrupted state is unsafe to resume; prefer clean
  exit + log + restart.
- Signal handling for short-lived commands (`probe`, `migrate`, `check-config`) — unnecessary.

## Success criteria (test-first; REQ id in the name)

- `req_013_worker_hands_off_inflight_run_on_sigterm` — assert republish with history, exit 0 in time.
- `req_053_ready_turns_503_while_draining` — contract/golden test of the draining flag.
- `req_054_panic_hook_emits_one_structured_event` — inject the formatter; assert no secret field.
- `req_058_config_error_exit_code_is_stable` — bad env → exit 3, name present, value absent.
- `req_053_probe_logs_are_visible` — JSON "probe succeeded" captured from a spawned probe.
- `req_053_signal_registration_failure_keeps_serving` — fake failing source; serve continues.
- `req_053_drain_timeout_forces_exit` — hung handler + SIGTERM → exit within deadline, WARN logged.

## Risks & rollout

Phase 1 (H2/H4/H8): observability and signal correctness, no contract change. Phase 2 (H6/H7):
shutdown semantics, coordinated with app-layer REQ-013 hand-off and lease timing. Phase 3 (H1/H3/H5):
runtime ADR and externally visible exit codes/errors (`docs/PARITY.md` + approval). Keep every claim
marked CONFIRMED only where a command or test in this session proved it.
