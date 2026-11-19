# Worker 0195 — `crates/bin/src/computer.rs` architecture proposal (v0.4.0)

Lens: architecture. Read-only review of the 432-line module; no code changed, no tests run.
Findings are **CONFIRMED by source inspection** of this file plus `crates/domain/src/computer.rs`,
`crates/infra/src/computer.rs`, `crates/bin/src/config.rs`, `ADR-009` and `spec/01` REQ-027/028.
Forward-looking items are marked RISK; assumptions needing verification are marked GUESS.

## Structure today (as read)

This is a startup-only config + provisioning module, not a runtime tool surface:

- `ComputerProvider { None, Docker }` (15–31) with `name()` feeding `Capabilities.sandbox`
  (`config.rs:327`, a wire field in `/ready`).
- `ComputerConfigError { Missing, Invalid, Unsupported }` (34–58); `Unsupported` text is asserted
  byte-for-byte by tests (290–296).
- `ComputerConfig` (62–86): provider, docker_host, image, frame_command, command (whitespace-split
  argv), cdp_port, idle_minutes, memory_limit_mb, cpu_limit. `Display` (129–146) renders all of them.
- `provision()` (94–127): builds a concrete `DockerComputerRuntime`, then a hard-coded 3×500 ms
  `ping()` retry loop; fails closed with `AppError::Unavailable`. `None` returns `NoneComputerRuntime`.
- `from_pairs()` (154–250): the single validated `BTreeMap` boundary; the `novita` arm (168–177)
  checks `NOVITA_API_KEY` then returns `Unsupported` — a deliberate cloud-provider stub.
- 11 tests, two (407–431) requiring a live Docker daemon (the known sandbox failure).

## Target structure for v0.4.0

Keep this module as the one trusted boundary for "which backend runs computers", but stop owning
provider construction policy and duplicated domain rules:

```
ComputerProvider   // canonical vocabulary shared with domain/app/infra
ComputerConfig     // validated values only; capabilities derived, not stored twice
RuntimeFactory     // provider -> Arc<dyn ComputerRuntime>; registry, not a growing match
retry_provision<F> // pure, injectable retry/backoff policy
from_pairs()       // unchanged entry point; delegates bounds to domain validators
```

`ComputerConfig` is binary-internal; no wire type changes. Preserve every `BOMA_*` name and every
`req_027_*`/`req_028_*` test name (trace gate).

## Concrete upgrades

### 1. Replace the `match` in `provision()` with a provider registry / factory seam

CONFIRMED: `provision()` hard-codes Docker and the `novita` branch is a stub (REQ-027 cites 14
providers in the reference); adding a cloud backend means editing this binary module again. Change:
give `ComputerProvider` a `build(&ComputerConfig) -> Result<Arc<dyn ComputerRuntime>, AppError>` hook,
or hold a `BTreeMap<ComputerProvider, RuntimeFactory>`; `None` and `Docker` are registered factories,
`novita` stays registered but returns `Unsupported` until its adapter lands. `provision()` becomes
lookup + fail-closed error. RISKS: (a) keep exact `Unsupported`/`Missing` text — tests assert it and
REQ-027 requires the missing-variable name; (b) a registered provider must never fall back to `None`
— fail-closed is the invariant; (c) a future cloud factory needs an API key, so introduce a
`secrecy::SecretString` boundary now (secrets never in `Debug`/`Display`/logs); (d) additive only.

### 2. Single-source the limit vocabulary in `boma_domain`

CONFIRMED: `from_pairs` hard-codes `64..=4096`, `0.25..=4.0`, `1..=1440` (215–237), matching
`MIN/MAX_*` and `validate_memory_limit/cpu_limit/idle_minutes` in `crates/domain/src/computer.rs`
(133–189). Three copies (parser, domain validators, DB check) can drift. Change: call the domain
validators from `from_pairs`, mapping `DomainError` to `ComputerConfigError::Invalid { name, value }`
so operator-visible text stays identical. RISKS: (a) domain messages differ — never surface them;
(b) tests 356–368 pin boundary values, keep them; (c) no new dependency; (d) if the DB check is a
fourth copy, note it in `docs/STATE.md`, do not edit migrations in this slice.

### 3. Unify the three provider vocabularies

CONFIRMED: provider identity exists three ways — bin enum + `name()`, domain `Computer.provider:
String`, infra `DockerComputerRuntime::provider() -> &'static str`; `config.rs` sets the wire field
`capabilities.sandbox` from `bin.name()`. Change: one canonical `ComputerProvider` with `as_str()`/
`FromStr` (domain or app port), have infra return it, convert to the wire string at the
`Capabilities` boundary. RISKS: (a) `/ready`'s `none`/`docker` values must stay byte-stable — a wire
change is a DEVIATION (`docs/PARITY.md` + owner approval), so keep an internal conversion, not a DTO
type change; (b) `FromStr` adds an invalid-name path — reuse `DomainError::InvalidName` as domain
`ComputerStatus::from_str` does (72–88); (c) `Computer.provider` is stored in the DB — GUESS until
checked, so keep it `String` and narrow only at parse time if the migration permits.

### 4. Make `provision()` testable and bounded

CONFIRMED: the 3×500 ms retry (110–123) is untestable without Docker, has no overall timeout, and
constructs the bollard client synchronously inside async code. Change: extract a pure
`retry_provision<F, Fut>(f)` over `FnMut() -> Fut`, inject a stub pinger in tests to prove retry
count, backoff and terminal `Unavailable` without Docker, and wrap the probe in `tokio::time::timeout`
with a bounded budget; move client construction off the runtime if blocking (GUESS — verify first).
RISKS: (a) preserve defaults 3/500 ms/same error or record a DEVIATION; (b) a timeout adds a failure
mode — log it, keep fail-closed semantics; (c) keep the two daemon tests as integration tests;
(d) cancellation safety — a dropped `provision()` must leave no container or client behind.

### 5. Derive typed computer capabilities at startup; validate cross-field coherence

CONFIRMED: `frame_command`, `command`, `cdp_port` are independent options (69–79) with no coherence
check — a `cdp_port` with no browser command yields observe/act `Unavailable` at run time, and
`/ready` reports only the provider name. Change: derive an internal `ComputerCapabilities`
(screen/browser/shell/files) from validated fields and refuse incoherent selects (`cdp_port` without
a browser command, `frame_command` with `None`) fail-closed at boot; reuse it for REQ-028 tool
refusals so an unavailable capability answers "computers unavailable", not a runtime CDP error.
RISKS: (a) new `/ready` fields are observable — keep the struct internal in v0.4.0, add wire fields
only via PARITY + approval; (b) flags must not promise what the image cannot do (GUESS until the
`BOMA_COMPUTER_IMAGE` contract is checked); (c) `split_whitespace()` argv (103–105) cannot express
quoted flags — a shlex-style parser is a follow-up; (d) new error text goes through the i18n layer.

## Sequencing and cross-cutting risks

1. #2 and #3 first (mechanical, no behavior change); run `cargo fmt --check`, `clippy -D warnings`
   and the REQ-027/028 tests after each step.
2. #4 next (testability + bound); keep the two daemon tests as the only live-Docker dependency.
3. #1 then #5 (registry + capabilities): additive; keep `novita` a stub and record any `/ready`
   change in `docs/PARITY.md` before shipping.
4. Preserve every `req_027_*`/`req_028_*` name and `BOMA_*` variable; no spec edit, no new dependency
   without `cargo add` + docs.rs + `docs/DEPENDENCIES.md`.
5. Claims are CONFIRMED by source/grep or marked GUESS/RISK; re-run gates in the implementing
   session before flipping any status to verified-by-test.

## Explicit non-goals

No cloud-provider implementation (Novita/E2B stay stubs), no change to the `ComputerRuntime` port or
REQ-028 tool wire contract, no new design system, no migration edits, no silent DEVIATION —
externally visible changes go through `docs/PARITY.md` and owner approval.
