# v0.4.0 architecture proposal — `crates/bin/src/cli.rs`

Worker: 191 | Lens: architecture | Scope: CLI argument structure, dispatch and config bootstrap
Target: v0.4.0 | Status: proposal (no code changed in this pass)

## 1. Current argument structure (CONFIRMED — read from source)

```
boma <command>
  serve                         -> commands::serve(config)
  worker                        -> commands::worker(config)
  migrate                       -> commands::migrate(config)
  rekey                         -> commands::rekey(config)   # reads BOMA_*_MASTER_KEY from env
  admin check-config            -> commands::check_config(&config)
  probe [--addr SOCK] [--path P]-> probe::probe(addr, path)  # no Config, no DB
  automation <agents|send|approvals|answer|routine-run|run-status>
                                -> automation::run(command)   # owns its own telemetry init
```

Shape today: a single `Cli` struct holding one `#[command(subcommand)] command: Command`;
`run()` calls `Cli::parse()` then a `match`. There are **no global flags** — log format/filter
are env-only (`BOMA_LOG_FORMAT`, `BOMA_LOG_FILTER`). Nested subcommands use two different
patterns: `AdminCommand` is hand-declared in this file, while `AutomationCommand` is imported
from `crates/bin/src/automation.rs`. Config/telemetry bootstrapping is split three ways:
`load_config()` for five jobs, `automation::init_output()` (its own env parsing) for automation,
and nothing for `probe` (by design — the header comment says probe must not need a DB URL).

## 2. Constraints that shape the design

- CLI is a **public interface**: AGENTS.md treats any CLI change as a DEVIATION recorded in
  `docs/PARITY.md` pending maintainer approval. All proposals below are additive; no existing
  command, flag name or default may change.
- `probe` must stay config-free and DB-free so container health checks work from outside the process.
- `print_stdout` is denied; all output rides `tracing`, so any "global verbosity/output" flag must
  feed `telemetry::init`, not bypass it.
- REQ-058/REQ-072 require the operator surface to grow (plugin review, access grants, settings);
  the structure must scale without turning `run()` into a god-match.

## 3. Proposed v0.4.0 structure (additive)

```
boma [GLOBAL] <command> [OPTIONS]
  GLOBAL (flattened, optional):
    --log-format <pretty|json>     # CLI > env > default; env fallback preserved
    --log-filter <FILTER>          # idem
    --dry-run                      # opt-in safety for mutating jobs only
    --config <PATH>                # optional; Config::from_env stays the default source
  serve [--shutdown-grace SECS]
  worker [--once]
  migrate [--dry-run]
  rekey [--dry-run]
  admin <check-config|review-plugin|grant|settings>   # grows over REQ-058
  probe [--addr SOCK] [--path PATH] [--timeout SECS]
  automation <...>                 # unchanged surface
```

## 4. Concrete upgrades, each with its risk

**U1 — Split parsing from execution.** Add `fn parse_from<I, T>(args: I) -> Cli` and
`async fn dispatch(cli: Cli, deps: ...)`, leaving `run()` as `dispatch(Cli::parse(), ...)`.
Enables dispatch tests without spawning the binary and makes config injection explicit.
*Risk:* low. Order of validation errors could change; keep the same `match` arms and add a
test that `run`-path and `parse_from`-path produce identical errors for sample argv.

**U2 — One telemetry/config bootstrap with an explicit opt-out.** Replace ad-hoc
`load_config()` + `automation::init_output()` with a small per-command capability
(`needs_config`, `needs_telemetry`) resolved in `dispatch`. Probe keeps both false; automation
loses its duplicated env parsing but keeps identical defaults (pretty/info).
*Risk:* medium. Two bootstrap paths exist today (CONFIRMED) with subtly separate defaults and
error messages; centralising could surface a different error for a bad `BOMA_LOG_FILTER`.
Pin the old defaults in a regression test before merging.

**U3 — Global flags via `#[command(flatten)] GlobalArgs`.** Feed `--log-format/--log-filter`
into `telemetry::init`; define precedence CLI > env > default and leave env as fallback.
*Risk:* medium / DEVIATION. Changing effective defaults is forbidden; a flag that only mirrors
an env var must be proven byte-identical when absent. `--config <PATH>` risks widening the
config contract (currently env-only) — flag it in `docs/STATE.md` as a proposal, not a silent add.

**U4 — Typed, validated args.** `automation answer --answer` becomes a
`clap::ValueEnum { approve, deny }`; `automation send --timeout-seconds` gets
`.requires("wait")`; `probe --addr/--path` gain `value_parser` validation and a `--timeout`.
*Risk:* medium / DEVIATION. Refusing strings previously accepted (arbitrary `answer` values)
is externally visible; gate it on maintainer approval or restrict only where a spec/defect
justifies it. `--timeout` must stay optional with today's behaviour.

**U5 — Structural traceability test.** Replace the hand-written variant→string map in
`req_058_every_job_has_a_command_line` with a generic check over
`Cli::command().get_subcommands()` (`clap::CommandFactory`), plus a dispatch-arm coverage test.
*Risk:* low. Generated clap names must match the expected strings; keep the explicit list of
expected names so new jobs still fail loudly until a REQ id is added.

## 5. Cross-cutting risks

- `--dry-run` for `rekey`/`migrate` needs new no-write paths in `commands.rs`: cross-crate scope
  creep and a correctness risk if the dry-run path drifts from the real one. Ship it only with a
  test that asserts zero DB writes.
- Flattened globals must not be accepted after the subcommand unless intended; document the
  grammar in `--help` examples.
- Any change to command names, defaults or accepted values is a DEVIATION: log each in
  `docs/PARITY.md` and wait for approval before shipping.

## 6. Evidence

All statements in §1 and §2 are CONFIRMED by direct reads this session of `cli.rs`, `main.rs`,
`automation.rs` (`init_output`, `AutomationCommand`, `run`) and `spec/01-requirements.md`
(REQ-046/058/072). §3–§5 are design proposals (GUESS as to final shape); no build, test or
Clippy run was performed in this read-only pass.
