# Worker 0192 — `crates/bin/src/cli.rs` reliability proposal (v0.4.0)

Module under review: `crates/bin/src/cli.rs` (155 lines). Lens: reliability.
Evidence: source read in this worktree on 2026-11-19. Claims tagged CONFIRMED (read from source)
or GUESS (inferred). Read-only task; only this file was written.

## 1. What the module does (CONFIRMED)

- `run()` (cli.rs:74-91) parses with `Cli::parse()` and dispatches seven jobs: `serve`, `worker`,
  `migrate`, `rekey`, `admin check-config`, `probe`, `automation ...`.
- `probe` is the only job that skips `load_config()` (cli.rs:77), so it needs no `BOMA_DATABASE_URL`
  by design (module doc, cli.rs:4-6). Every other job calls `load_config()` (cli.rs:93-97), which
  parses the whole environment and installs the global tracing subscriber before dispatch.
- `main.rs:17-19` returns `anyhow::Result<()>` straight from `cli::run()`; the binary edge is the
  one place anyhow is allowed (AGENTS.md).
- Existing tests (cli.rs:99-155) cover happy parsing for every job, probe defaults, admin nesting,
  and rejection of an unknown job (`req_058_an_unknown_job_is_refused`).

## 2. Failure modes

### 2.1 Bad arguments (CONFIRMED)

- Unknown subcommands and a malformed `--addr` are rejected by clap with a useful message
  (`SocketAddr` typing, cli.rs:48-49; test at cli.rs:149-154). Good.
- `probe --path` is an unconstrained `String` (cli.rs:50-52). `probe.rs:30` interpolates it
  verbatim into the HTTP request line. A value containing CR/LF or spaces injects headers or a
  second request; a value not starting with `/` produces a malformed request. No length bound.
  This is the clearest argument-boundary defect in the module.
- No test covers: no subcommand, `admin` with no subcommand, `automation` with no subcommand,
  an unknown flag, or a hostile `--path`.

### 2.2 Missing environment (CONFIRMED)

- Any missing/invalid `BOMA_*` setting is surfaced before dispatch, with the variable named by
  `ConfigError::Missing/Invalid` (config.rs:24-38, 426-449). First error wins: the operator fixes
  one thing, reruns, and discovers the next.
- `rekey` needs `BOMA_MASTER_KEY` and `BOMA_NEW_MASTER_KEY`, but neither is read by `Config`
  (config.rs) nor checked by `admin check-config` (commands.rs:426-428). They are read only in
  `commands::rekey` (commands.rs:382-394), after config + telemetry init. So `check-config` can
  print "configuration is valid" for a deployment where `rekey` fails immediately — a false
  guarantee on the operator's own diagnostic command.
- Conversely, a bad *unrelated* var (e.g. `BOMA_LOG_FORMAT=yaml`, config.rs:307-316) blocks
  `migrate` and `rekey`, the very commands an operator most needs during recovery. A narrow
  command is coupled to the full config surface.

### 2.3 Opaque errors (CONFIRMED unless tagged)

- `load_config()` wraps everything as `.context("the configuration is not usable")` (cli.rs:94).
  The cause chain still names the variable, but the top line and any single-line log aggregator
  lose the "which job / which setting" signal.
- `ConfigError::Invalid` echoes the raw value (config.rs:33-37). Today that is mostly harmless
  (`BOMA_LOG_FORMAT`, IPs), but any future secret routed through `optional_parsed` would be
  printed. Recommend a redaction policy keyed on variable name.
- Errors reach stderr through anyhow's `Debug` rendering, unaffected by `BOMA_LOG_FORMAT=json`
  (telemetry.rs:19-31). JSON-logging deployments get unstructured failures.
- `probe` decides health with `status_line.contains(" 200")` (probe.rs:44): a status line such as
  `HTTP/1.1 2000 ...` would pass. Also `PROBE_TIMEOUT` is documented as the whole-probe bound but
  is applied per operation (probe.rs:16, 25, 31, 37) — worst case about 30 s, too long for a
  liveness check. `read_to_end` buffers the entire body although only the status line is used.
- GUESS: the `Rekey` doc comment (cli.rs:35-38) promises both master-key vars but `Config` has no
  field for them; operators reading only `cli --help` may miss that `check-config` cannot verify
  them.

## 3. Concrete hardening for v0.4.0 (ordered by value/risk)

H1 (cli.rs, low risk). Validate `--path` at the boundary: require a leading `/`, reject control
characters (especially CR/LF) and whitespace, cap at 512 bytes. Add
`req_058_probe_path_refuses_crlf` and `req_058_probe_path_requires_leading_slash`. Keeps the
flag; tightens accepted values.

H2 (cli.rs, low risk). Pass the job name into `load_config(job)` and include it in the context:
`configuration for 'rekey' is invalid`. Call sites at cli.rs:79, 86-89. Add a test asserting the
error string names the job. Error text is externally visible — see Risks.

H3 (cli.rs + config.rs, medium). Add a preflight `validate_rekey_env()` that checks presence and
shape of `BOMA_MASTER_KEY`/`BOMA_NEW_MASTER_KEY` before telemetry init, called from `rekey`
dispatch and mirrored by `admin check-config` so the diagnostic cannot lie. Keep values wrapped
in `secrecy`; never log them.

H4 (config.rs, medium). Aggregate missing/invalid variables instead of first-error-wins: collect
`ConfigError`s in `from_pairs` and return them all at once, so one deploy fixes the whole set.
A `load_recovery_config()` validating only the DB URL for `migrate`/`rekey` is a GUESS needing an ADR.

H5 (probe.rs, low risk, adjacent). Parse the status code strictly (split whitespace, `u16 == 200`),
wrap the whole exchange in one `tokio::time::timeout`, and read at most about 8 KiB. Fix the
"whole probe" doc line. Add tests for a `2000` status and for the overall deadline.

H6 (main.rs/cli.rs, higher risk). Distinguish failure classes with exit codes and render errors as
JSON when `BOMA_LOG_FORMAT=json`: switch `main` to `-> ExitCode`, log once via `tracing::error!`,
map args=2 (clap), config=1, dependency=3, unhealthy probe=4. Use `eprintln!` only before init.

H7 (tests, low risk). Table-driven bad-arg tests (no subcommand, empty `admin`/`automation`,
unknown flag, hostile `--path`) and a missing-env test asserting variable name and job label
appear in the rendered chain. Extract a pure `dispatch`/env-injection seam if `run()` cannot be
tested without mutating the process environment — keep it minimal.

## 4. Risks and gates

- Externally visible CLI changes (H2 error text, H1 rejected values, H6 exit codes) are DEVIATIONs
  per AGENTS.md: record each in `docs/PARITY.md` with justification and wait for owner approval
  before shipping. Do not silently change flags or defaults.
- No new dependencies in this wave; use existing clap/anyhow/tokio. If one becomes necessary,
  `cargo add`, read docs.rs, and record name/version/license in `docs/DEPENDENCIES.md`.
- Lints: `clippy -D warnings`, `unsafe_code = forbid`, deny `unwrap_used`/`expect_used`/`panic`/
  `print_stdout` in non-test code; H6's stderr path must not trip `print_stdout`.
- Secret hygiene: never print `BOMA_*` key/token/secret/password values; keep `Config`'s redacted
  `Debug`/`Display` and `secrecy` wrapping intact (REQ-054).
- Telemetry installs a global subscriber once (telemetry.rs:16-31); tests must not call `run()`
  twice in one process. Prefer pure helpers or subprocess assertions.
- Probe changes affect container health checks: verify against the local target with canary-watch
  after deploy; never point probes at third-party hosts.
- Traceability: put REQ-058 / REQ-053 in test names; `tools/trace_check.py` counts declarations,
  not comments. Mark unknowns in `docs/STATE.md` under "Decisions needed" as GUESS.

## 5. Verification for this wave

1. `cargo fmt --check` and `cargo clippy --workspace --all-targets -- -D warnings`.
2. `cargo test -p boma-bin` plus the new REQ-058/REQ-053 tests.
3. Manual differential: `boma probe --help`, `boma` (no args), `boma admin`, `boma probe --path
   '/ready\r\nX: y'`, `env -u BOMA_DATABASE_URL boma migrate`; log output in `docs/PARITY.md`.
