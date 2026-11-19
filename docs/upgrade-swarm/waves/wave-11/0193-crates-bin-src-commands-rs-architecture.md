# v0.4.0 architecture proposal — `crates/bin/src/commands.rs` (CLI command dispatch)

Worker #193 · LENS=architecture · target v0.4.0 · READ-ONLY analysis (no product code changed).

Status of claims: every "current state" statement below is CONFIRMED by direct reads in this
worktree of `crates/bin/src/commands.rs`, `cli.rs`, `main.rs`, `automation.rs`, `probe.rs`,
`docs/adr/017-cli.md` and `spec/01-requirements.md` (REQ-053/058/072). Forward-looking items are
proposals (GUESS until implemented, tested and approved).

## Current dispatch structure (CONFIRMED)

- `main.rs:17-20` is a thin composition root: `#[tokio::main]` calls `cli::run()`. The binary owns
  no business rule; `anyhow` is confined to this crate (AGENTS.md), which is the right boundary.
- `cli.rs:16-66` declares a flat clap surface: `Cli { command: Command }`, with `Command` =
  `Serve | Worker | Migrate | Rekey | Admin{AdminCommand} | Probe | Automation{AutomationCommand}`.
  Nesting is two levels only (`admin`, `automation`) — deliberately simple, not over-abstracted.
- `cli.rs:74-91` is the single dispatch `match`. Each arm is a free-function call; handler
  signatures are inconsistent: `serve/worker/migrate/rekey(Config)` own the config,
  `check_config(&Config)` borrows it, `probe(addr, path)` takes neither, `automation(command)` reads
  its own env. Config + telemetry are built lazily by `load_config()` (`:93-97`) only for arms that
  need them, which correctly keeps `probe` dependency-free.
- Telemetry is initialized in **three** places with different inputs: `cli.rs:load_config()` uses
  `config.telemetry`; `automation::init_output()` (`automation.rs:130-142`) re-parses
  `BOMA_LOG_FILTER`/`BOMA_LOG_FORMAT` with its own defaults; `probe` never initializes a subscriber.
- `commands.rs` holds the jobs plus their assembly. `build_app` (`:118-275`) takes **20 positional
  args** — the whole `Config` unpacked twice, verbatim, in `serve` (`:53-73`) and `worker`
  (`:295-315`) — and needs three clippy exemptions (`too_many_arguments`, `too_many_lines`,
  `fn_params_excessive_bools`, `:115-117`). The two argument lists can drift silently.
- Composition is duplicated further: `secrets_vault()` is called five times inside `build_app`
  (`:190,193,195,201,210`), while `PostgresNotificationDestinationStore` gets a separate
  `SecretBox::ephemeral()` (`:220-223`) with no comment saying why.
- `migrate` (`:413-420`) and `rekey` (`:382-406`) take a full `Config` but use only `config.database`
  (plus `BOMA_*` env read directly in `rekey`). `probe` and `automation` bypass `Config` entirely.
- `cli.rs` tests (`:105-154`) assert parsing, defaults and refusal of unknown jobs (REQ-058), but
  never assert dispatch correctness, match exhaustiveness, exit codes, or `--help` coverage.
- Contract gap vs ADR-017: it promises an admin local-TTY / `--allow-remote` gate and audit-logged
  admin actions, and lists bootstrap-operator / export-audit / inspect-run; the code implements only
  `AdminCommand::CheckConfig` (`:62-66`) with no gate and no audit write.

## Guiding principle for v0.4.0

Keep one binary and one hand-written dispatch `match` (simplicity wins — no trait-object registry).
Make each command declare the narrow config and one-time services it actually needs, initialize
telemetry once, and grow the operator surface behind a single guard. Any wire-visible CLI change is
a DEVIATION (AGENTS.md): record it in `docs/PARITY.md` and wait for approval.

## Upgrades (5 concrete items)

1. **One initialization path for config + telemetry.** Introduce a single `bootstrap()` used by every
   dispatch arm; it parses `Config`, validates at startup (fail fast) and installs the subscriber
   once. Fold `automation::init_output()` into it, giving `BOMA_LOG_FILTER/FORMAT` one documented
   precedence; let `probe` explicitly opt out (it must stay dependency-free for REQ-053).
   *Risk:* automation's current defaults differ from `config.telemetry`, so unifying can change
   operator-visible log lines — a DEVIATION. *Mitigation:* merge both sources in one function with a
   written precedence rule; add tests pinning automation log format and `probe` independence.

2. **Narrow config slices; kill the 20-arg unpack.** Build one validated `Deployment`/`AppConfig`
   from `Config` and pass it by `&` to `build_app`, so `serve` and `worker` share a single assembly
   call site instead of two copy-pasted lists. Let `migrate`/`rekey`/`check_config` accept only the
   fields they use (`&DatabaseConfig`, `&Config` for validation).
   *Risk:* wide mechanical edit across `config.rs`, `serve` and `worker`; narrowing *validation*
   would weaken fail-fast. *Mitigation:* keep full `Config::from_env()` validation at startup and
   narrow only what is passed; add a test asserting `serve` and `worker` derive `build_app` args from
   one constructor so the lists cannot drift; remove the three clippy exemptions as the args shrink.

3. **Inject one vault into assembly.** Construct the `SecretBox` once in the composition root and
   pass `Arc<SecretBox>` to the five secret-backed stores; document (or ADR) why the notification
   destination key is ephemeral before deciding to unify it.
   *Risk:* shared ownership changes type signatures in `Stores::new`; the ephemeral key may be
   deliberate isolation. *Mitigation:* keep stable/ephemeral selection identical and prove it with a
   test that checks which stores receive which key; do not merge the two keys without owner sign-off.

4. **Grow `admin` behind one guard, matching ADR-017 / REQ-058.** Add the missing operator jobs
   (bootstrap operator, audit export, inspect run) as `AdminCommand` variants, and apply a single
   `require_local_admin()` gate (local TTY or explicit `--allow-remote`) to the whole `Admin` arm in
   dispatch — not per subcommand. Route admin writes through the service layer so they land in the
   audit log.
   *Risk:* scope expansion — spec/REQ-058 and ADR-017 must be checked first; a hard TTY gate breaks
   containerized/CI operators, which is why ADR-017 already allows `--allow-remote`.
   *Mitigation:* propose the new flags/commands in `docs/STATE.md` and record any behavior change as a
   DEVIATION in `docs/PARITY.md` before shipping; keep `--allow-remote` so automation/CI keeps working.

5. **Dispatch-level tests and a stable CLI contract.** Keep the `match` exhaustive (no `_` arm) so a
   new `Command` variant fails to compile until dispatched; extend the REQ-058 test to assert every
   variant is handled. Add a snapshot of the command/flag list and `--version`, plus error-path tests
   for missing config/token and exit codes, using a dry-run dispatch that never starts a server or DB.
   *Risk:* clap help snapshots are brittle across versions; real exit-code tests could touch the DB.
   *Mitigation:* snapshot only command/flag names (pin clap in `Cargo.lock`), keep REQ ids (058/072)
   and any D-id in test names — `tools/trace_check.py` counts declarations, not comments.

## Cross-cutting risks / sequencing

- Items 3 and 5 are local and low-risk; do them first and keep them continuous. Item 1 is local but
  carries the log-format DEVIATION risk. Item 2 is the widest mechanical change and should follow 1.
  Item 4 is a contract change — owner/spec gate before any code.
- Items 1, 4 and 5 can alter observable CLI behavior (log format, flags, help, exit codes). Per
  AGENTS.md each is a DEVIATION: `docs/PARITY.md` entry + approval before shipping.
- Never read a secret from argv (ADR-017): `rekey` reads `BOMA_NEW_MASTER_KEY` from env (`:383-384`);
  keep that property in every new command.
- Verification gate per step: `cargo fmt --check`, `clippy -D warnings`, unit + `tests/` integration,
  and the cli help/traceability tests.

## Out of scope for this module

The worker loop body and `shutdown_signal()` reliability (worker #194's lens), `config.rs` schema
details, `telemetry.rs` subscriber implementation, `automation.rs` subcommand internals, and all HTTP
routes/lifecycle in `crates/api`. This proposal only restructures the CLI dispatch and assembly in
the binary crate.
