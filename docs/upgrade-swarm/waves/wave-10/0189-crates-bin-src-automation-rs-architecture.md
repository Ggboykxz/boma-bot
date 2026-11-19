# Worker 0189 — `crates/bin/src/automation.rs` — architecture proposal (v0.4.0)

- Lens: architecture. Requirement: REQ-072 (local automation API and CLI).
- Status: PROPOSAL only. Read-only except this file. No code changed.

## What I inspected (CONFIRMED)
- Full `crates/bin/src/automation.rs` (218 lines): `AutomationCommand` (6 variants),
  `endpoint()/token()/client_timeout_seconds()`, `request()`, `init_output()`, `run()`.
- `crates/bin/src/cli.rs:54-59,85` — the enum is parsed in `cli.rs`, executed in `automation.rs`.
- `crates/bin/src/config.rs:202-214` — the *server-side* `AutomationSettings` is a typed struct;
  the CLI has a second, ad-hoc env surface with no struct (CONFIRMED).
- `.env.example:97-106` documents the server vars but not `BOMA_AUTOMATION_ENDPOINT` or
  `BOMA_AUTOMATION_CLIENT_TIMEOUT_SECONDS` (CONFIRMED).
- `spec/01-requirements.md:579-585` — REQ-072 also requires "start a fresh chat" and that the
  answer "returns to the script"; the CLI has no chat command and no answer field (CONFIRMED).
- `crates/bin/src/telemetry.rs:16-32` / `cli.rs:93-97` — two places assemble
  `TelemetrySettings` and call `telemetry::init` (CONFIRMED).

## Current architecture (CONFIRMED)
- Config is read lazily at call time from three independent `std::env::var` readers; nothing is
  validated at startup; the bearer is a plain `String`, not `secrecy`.
- `run()` is one `match` that builds `serde_json::json!` bodies inline and interpolates raw ids
  (`bot`, `card`, `routine`, `run`) into URL paths — no segment encoding, no id newtypes.
- `request()` is transport + auth + timeout + status handling in one function; no pure mapping, so
  command→request behaviour cannot be unit-tested without a live deployment.
- `answer: String` accepts any value although the doc says `approve|deny`; `timeout_seconds` is
  unbounded; `RunStatus` says "poll until it leaves running" but performs a single GET.
- Output rides the tracing subscriber (`BOMA_LOG_FORMAT=json` ⇒ a JSON *event* with a `body`
  field); `print_stdout` is denied workspace-wide, so there is no clean machine-readable channel.
- Two extra timeout authorities (client env, per-call flag) plus the server's own cap; no
  cancellation on Ctrl-C; every non-2xx is a generic `bail!` with the body only in logs.

## Gaps against the architecture bar (CONFIRMED / GUESS)
1. Two config sources for one feature; client-only vars undocumented; no fail-fast validation.
2. No typed ids / enum for `answer`; invalid states representable (AGENTS.md rule).
3. Raw path interpolation and no pure request builder — correctness, injectable paths, untestable.
4. Output contract is a log envelope, not a value; scripts are told to bypass the CLI (GUESS:
   acceptable today, but it defeats the "ergonomic shorthand" purpose).
5. REQ-072's fresh-chat + answer-return clauses are absent from the CLI (CONFIRMED).

## Upgrades (v0.4.0)

### U1 — One typed `AutomationClient`, one config struct
Introduce `pub struct AutomationClient { endpoint: Url, token: SecretString, timeout: Duration }`
built once from a `ClientSettings` struct; move the env readers there, validate at construction,
and share one `TelemetrySettings` builder with `cli.rs` instead of assembling it twice.
- Why: fail fast, one source of truth, bearer wrapped per AGENTS.md, no repeated env reads.
- Risk: env names/defaults must stay byte-identical (DEVIATION guard); `SecretString` must never
  appear in `Debug` or `tracing` fields — add a test asserting the token is absent from logs.

### U2 — Pure command→request mapping + transport port
Extract `fn build_request(&ClientSettings, &AutomationCommand) -> Result<RequestSpec>` returning a
typed method, an encoded `Url`, and an `Option<Value>` body; keep `request()` as the only IO. Add
a thin `trait AutomationTransport` (or wiremock in tests) so mapping tests need no deployment.
- Why: the six mappings become unit-testable and REQ-named; ids can no longer inject paths.
- Risk: percent-encoding changes URLs for ids containing `/`, `?`, spaces — needed for correctness
  but a wire-level DEVIATION; record in `docs/PARITY.md` and golden-test normal ids.

### U3 — Make CLI states unrepresentable
`answer` becomes a clap `ValueEnum` (`approve|deny`); ids (`bot/card/routine/run`) become newtypes;
`timeout_seconds` gets a validated range; add a `chat` subcommand for REQ-072 "start a fresh chat".
- Why: REQ-072 invalid states rejected at parse time, not by the server.
- Risk: rejecting free-text answers breaks tolerant callers (DEVIATION — ask before shipping);
  `chat` depends on the API-side fresh-chat route (worker 0127 U4) — sequence after it lands.

### U4 — Machine-readable output contract
Add a typed formatter: JSON mode emits the response body as a JSON *value* on one line (not a
tracing envelope), errors get stable exit codes plus a structured error line; fix the `RunStatus`
doc to match its single GET (or implement the documented poll).
- Why: deliver the "answer returns to the script" clause without forcing scripts to use HTTP.
- Risk: `print_stdout` denied workspace-wide — keep the log sink with an explicit ADR/exemption,
  or change output shape behind a default-off flag to avoid breaking consumers (DEVIATION).

### U5 — Single timeout authority + cancellation
Compute one effective deadline `min(client_cap, per-call flag, server_wait_cap)`, validate that the
client cap is not below the server cap at startup, and `select!` the long poll against Ctrl-C /
shutdown so a waiting operator can abort cleanly.
- Why: two/three authorities can silently cut off valid waits; aligns with API worker U5.
- Risk: changing caps alters observed latency — DEVIATION candidate; signal handling adds
  complexity; prefer the API-side deadline as the authority of record.

## Suggested order (small steps, test-first)
U1 → U2 (parity-locked refactors) → U3 → U5 → U4 (behavior; operator sign-off). Each step:
`cargo fmt --check`, `clippy -D warnings`, REQ-072-named tests.

## Verification for v0.4.0 (REQ-named)
- `fn req_072_cli_config_fails_fast_on_missing_or_short_token()`
- `fn req_072_automation_command_maps_to_the_documented_request()` (table-driven, 6 variants)
- `fn req_072_answer_rejects_anything_but_approve_or_deny()`
- `fn req_072_wait_result_returns_the_terminal_answer_to_the_script()`
- `fn req_072_token_never_appears_in_log_output()`
- Contract check: `boma automation --help` lists chat/send/approvals/...; `.env.example` names every
  `BOMA_AUTOMATION_*` var the CLI reads.

## Provenance
CONFIRMED = read from the named files this session. GUESS = design judgment / parity uncertainty.
