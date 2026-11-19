# v0.4.0 Architecture Proposal — `crates/bin/src/probe.rs` (worker #201, lens: architecture)

## Evidence and limits
CONFIRMED (read this session): `probe.rs` is 106 lines exposing one function,
`probe(addr: SocketAddr, path: &str) -> anyhow::Result<()>`, a hand-rolled HTTP/1.1 client over one
`TcpStream`. `PROBE_TIMEOUT` (10 s) is applied to **each** of connect, `write_all` and `read_to_end`
separately, so the doc comment "upper bound on the whole probe" is false: worst case is ~30 s.
Status is tested with `status_line.contains(" 200")` on the first lossy-decoded line; the body is
drained with unbounded `read_to_end`. The request is `format!("GET {path} HTTP/1.1\r\n…")` with no
validation of `path`. Three tests carry `req_053_*` names (200, 503, closed port).

CONFIRMED (related files): `cli.rs` declares `Command::Probe { addr, path }` inline with defaults
`127.0.0.1:3000` and `/ready` (REQ-058 parse tests), and `cli::run()` dispatches `Probe` **before**
`load_config()`, so `telemetry::init` is never called in the probe process. `telemetry.rs` is the
only subscriber installer and needs config. `compose.yaml` runs
`boma probe --addr 127.0.0.1:3000 --path /ready` with `timeout: 5s`, `interval: 10s`, `retries: 3`.
`spec/01-requirements.md` REQ-053 makes `/ready` a bounded probe with no secret leakage; ADR-017
keeps one binary with several jobs (AGENTS: `anyhow` only at binary edges, timeout on every outbound
call, DEVIATION for CLI-visible changes, REQ id in test names).

GUESS (read-only scope, not verified): whether CI probes use the same flags, whether any deployment
raises the Docker timeout, and which telemetry filter an operator expects from `boma probe`.

## Health-probe CLI structure today
`cli.rs:45-53` owns the flags; `probe.rs` owns only the transport. This keeps `probe` free of config
(a correct property: it runs against loopback from outside the checked process, no DB URL needed),
but splits the command's interface from its implementation, makes the CLI hard to unit-test in the
module that defines behavior, and leaves the success `tracing::info!` with no subscriber.

Proposed structure: define a `ProbeArgs` struct in `probe.rs` with `#[derive(clap::Args)]` and the
**existing** `--addr` / `--path` names and defaults; `cli.rs` embeds it as `Probe(ProbeArgs)` and
nothing more. Add an optional `--timeout` flag and a `timeout` parameter on `probe`, keeping the old
two-argument call as a thin wrapper so current tests/callers do not move. `cli.rs` retains REQ-058
parse tests; `probe.rs` gains REQ-053 argument-validation tests.

## Concrete upgrades (change → risk)

1. **One deadline for the whole exchange (highest value).**
   Change: wrap connect + write + read in a single `tokio::time::timeout` around an inner async
   block; lower the default to ~2-3 s so it fits the compose `timeout: 5s` with margin, and expose
   `--timeout` for operators. Risk: any default change is externally visible — record it in
   `docs/PARITY.md` and check CI/docker timeouts first (GUESS). A budget shorter than a cold-start
   `/ready` under load can flap; keep `start_period` as the ramp and prefer retries to a longer probe.

2. **Parse the status code strictly; read only the head.**
   Change: match a parsed 3-digit code (e.g. `HTTP/1.1 200 ` prefix) instead of `contains(" 200")`,
   and read up to a bounded head (e.g. 8 KiB) or the first CRLF line rather than `read_to_end`.
   Risk: over-strict parsing could reject a valid reason phrase or a non-1.1 status line — pin the
   accepted grammar in tests; a head cap must never truncate the line before the code is seen.

3. **Validate `path` at the CLI boundary (request-smuggling guard).**
   Change: `ProbeArgs::path` goes through a typed `ProbePath` `TryFrom<String>` that rejects CR, LF
   and other control bytes; keep `/ready` as default. Risk: rejecting a previously accepted path is
   a CLI-visible behavior change (DEVIATION candidate) — confirm no deployment passes exotic paths;
   keep `--addr` a `SocketAddr` so it is already injection-safe.

4. **Give the probe a minimal telemetry path.**
   Change: add a `telemetry::init_probe()` that installs a subscriber without `Config` (fixed
   `info` filter, format from an env var), or accept that probe output stays on stderr and document
   it. Risk: installing a subscriber changes observable container output; avoid parsing full config
   (the probe must stay dependency-free) and avoid a second logging system; a log line carrying the
   raw error must not include secrets (REQ-054).

5. **Make the probe outcome typed and testable.**
   Change: introduce an internal `ProbeResult { status, elapsed }` (private, `thiserror`-free) so
   success/timeout/non-200/bad-status are distinct, with `Debug` redaction; add property tests over
   arbitrary status lines and paths, keeping the three `req_053_*` names. Risk: more types require
   rustdoc and clean pedantic clippy; do not widen the public `Result<()>` signature used by `cli.rs`.

## Cross-cutting risks / acceptance
- Keep flag names, defaults and the `probe(addr, path)` signature unless a DEVIATION is recorded;
  no change to `/ready`'s wire contract or secret redaction (REQ-053/054).
- Preserve the traceability rule: new behavior gets `req_053_*` names, CLI behavior `req_058_*`.
- Verify with `cargo fmt --check`, `clippy -D warnings`, and the unit tests before any merge.
- Keep the probe in `bin`: moving it to `infra` would add layering for a deliberately
  dependency-free client and contradict its own rationale; document the deviation instead.
