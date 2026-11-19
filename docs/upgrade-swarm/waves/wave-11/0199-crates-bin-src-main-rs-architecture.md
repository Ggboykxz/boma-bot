# 0199 — `crates/bin/src/main.rs` — architecture (v0.4.0)

- Worker: 199 · Lens: architecture · Target: v0.4.0 · Read-only review; no code edited.
- Module: a 20-line composition root — `#![deny(missing_docs)]`, seven `mod` declarations, and
  `#[tokio::main] async fn main() -> anyhow::Result<()> { cli::run().await }`. Spec anchors:
  `AGENTS.md` "Errors" (anyhow only at binary edges), "Async" (no blocking calls, graceful shutdown
  that drains in-flight requests), "Config" (typed config, validated at startup, secrets in `secrecy`),
  "Observability" (structured logs), ADR-017 (one binary, several jobs), REQ-058 (CLI job surface).
- Read here: `crates/bin/src/{main,cli,commands,config,telemetry,probe,automation}.rs`,
  `crates/bin/Cargo.toml`, and `wave-10/0169-crates-app-src-error-rs-architecture.md` (format).
- CONFIRMED = read in this session; GUESS = inferred.

## Structure today (CONFIRMED)

`main.rs` is a clean composition root: it owns no logic, delegates to `cli::run()`, and `anyhow`
is confined to the binary edge — a strength to preserve. The real entrypoint architecture lives one
layer down, and four structural seams show:

1. **Telemetry is initialized as a hidden side effect of config loading**, not at the entrypoint:
   `cli.rs::load_config()` (lines 93–97) calls `telemetry::init`. Only `serve`, `worker`, `migrate`,
   `rekey` and `admin` go through `load_config`; `probe` and `automation` skip it (cli.rs:77, 85).
   So `probe.rs:47` emits `tracing::info!` with **no subscriber installed** — the log is a no-op —
   and `boma automation` produces no structured output despite its doc promising JSON lines.
2. **The runtime is macro-owned and untunable.** `#[tokio::main]` (main.rs:17) fixes a multi-thread
   runtime for every job. One-shot jobs (`probe`, `migrate`, `admin`, `automation`) pay for a
   thread pool; there is no env control of worker/blocking threads, and no explicit error boundary:
   a bootstrap failure returns `anyhow::Result` and is printed by `Termination` (raw `Debug`) rather
   than through the configured log format.
3. **Composition is duplicated and parameter-exploded.** `commands.rs::build_app` takes 18 arguments
   behind three `clippy` allows (lines 115–117) and is called with a near-identical 18-value list by
   `serve` (53–73) and `worker` (295–315). `commands.rs` (457 lines) is a god-module holding serve,
   worker, migrate, rekey, `build_app`, `vapid_config`, `secrets_vault` and `shutdown_signal`.
4. **Lifecycle and secret handling are inconsistent.** Only `serve` uses graceful shutdown
   (`shutdown_signal`, 431–457); `worker()` loops forever (338–362) with no signal handling, so
   SIGTERM can kill a claimed run. Secrets bypass typed `Config` in `vapid_config`, `secrets_vault`
   and `rekey` (which reads `BOMA_NEW_MASTER_KEY` straight from `std::env`), and `secrets_vault()`
   is rebuilt from env up to four times inside `build_app` (190, 193, 195, 201, 210).

## Concrete upgrades for v0.4.0 (each with a risk)

1. **Own the runtime and add an explicit error boundary in `main`.** Replace `#[tokio::main]` with a
   manual `tokio::runtime::Builder` (env-tunable `worker_threads` / `max_blocking_threads`, `enable_all`),
   match on `cli::run()` to log the failure through `tracing` and exit with a stable code, and flush the
   subscriber before exit. Risk: runtime flavor/thread counts change scheduling and latency, and the
   error rendering is externally visible to scripts — freeze the current error output in a golden
   test and treat any change as a DEVIATION in `docs/PARITY.md`.

2. **Initialize telemetry once, unconditionally, at the entrypoint.** Move `telemetry::init` out of
   `cli::load_config()` and call it before dispatch in `cli::run()` so `probe` and `automation` log
   too; make `init` idempotent (or injectable) instead of global `try_init`. Risk: enabling logs changes
   `probe`/`automation` output contracts (scripts may parse stdout) and `try_init` double-call panics are
   currently avoided only by calling it once — gate output changes behind an ADR and keep secrets out.

3. **Introduce a parameter object and one shared bootstrap for serve + worker.** Fold the 18 args into
   a `BootConfig` built once from `Config`, build the `SecretBox` once and share it (`Arc`), and extract
   the duplicated database/computer/`build_app` prologue into one `bootstrap()` used by both jobs.
   Risk: a mechanical field move can silently drop a feature flag; land behind a golden test that
   snapshots the resulting capability/flag matrix, and keep all three `clippy` allows until the last
   call site is migrated.

4. **Give the worker the same graceful shutdown as `serve`.** Extract `shutdown_signal()` into a shared
   module, select it into the worker loop, and make `sleep`/`step` cancellation-safe so SIGTERM drains
   the in-flight run. Risk: cancelling `step()` mid-claim can leave a run leased until expiry (or be
   double-claimed) — add a SIGTERM-during-step integration test and document the lease behavior before
   changing it.

5. **Split `commands.rs` and route all secrets through typed config.** Move serve/worker/migrate/rekey
   into `commands/{serve,worker,migrate,rekey}.rs` with a thin `mod.rs`, and move VAPID, master-key and
   `BOMA_NEW_MASTER_KEY` reads into `config.rs` (redacted `Debug`, `secrecy`) like the other secrets.
   Risk: module moves churn REQ-tagged test names that `tools/trace_check.py` counts — keep every REQ id
   in the test function name and re-run the checker; changing key parsing risks a fail-fast regression,
   so preserve today's error messages verbatim.

## Explicit non-goals

No new jobs outside ADR-017's set, no HTTP types in the binary, no removal of `anyhow` at the binary
edge, and no `BOMA_*` interface change shipped without an ADR / `docs/PARITY.md` entry. Do not merge
upgrades #1/#2 without golden coverage of current output — both touch externally visible behavior.
