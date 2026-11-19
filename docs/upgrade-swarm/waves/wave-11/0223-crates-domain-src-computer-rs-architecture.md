# Upgrade proposal — `crates/domain/src/computer.rs` (lens: architecture)

- Worker: 223
- Target version: v0.4.0
- Evidence basis: full read of the 189-line file plus targeted reads/greps of `spec/01-requirements.md`
  (REQ-026..031), `spec/02-data-model.md` (Computer), both computer goldens, `run.rs`, `error.rs`,
  `lib.rs` and sibling wave-11 proposals. Code facts are **CONFIRMED** by inspection; inferences from
  the spec are marked **GUESS**. Read-only assignment: no code changed, no tests run.

## Current structure (as read)

- One opaque `ComputerId`; one `ComputerStatus` enum (6 variants: the 5 provisioning values plus
  domain-only `stopped`) with hand-kept `ALL`, `as_str`, `Display`, `FromStr`. No transition rules.
- One all-pub `Computer` struct (~18 fields) mixing four concerns: identity/ownership
  (`id`, `space_id`, `bot_id`), provider/runtime (`provider: String`, `external_id`, `status`,
  `screen_generation`, `home_id`), control (`takeover_by_user: Option<UserId>`, `takeover_at`,
  `private`), and capacity/lifecycle (`memory_limit_mb`, `cpu_limit`, `idle_minutes`, four
  `Option<String>` timestamps). No validating constructor, no tests in the file.
- Three free validators (`validate_memory_limit`, `validate_cpu_limit`, `validate_idle_minutes`) with
  six bound constants, all returning `DomainError::InvalidName`.

**Architecture gap:** the spec's Computer data model names six entities — `Computer`, `ComputerRun`
(one active holder), `ComputerExecutionLease` (fence/owner), `ComputerUpdate`, `BrowserProfile`,
`AgentHome`/`HomeDeletionNotice` — but this module models only `Computer`. The three composite
invariants that matter most (exclusive holder, lease fencing, generation-based screen capability)
exist only as loose pub fields with no domain type to anchor them. `run.rs` already shows the house
pattern (`can_transition_to`/`transition_to`, `lease_fence`, `holds_lease`) that this module lacks.

## Target structure for v0.4.0

Keep it a pure, IO-free module; reorganise into cohesive value types around one `Computer` aggregate:

```
ComputerProvider        // typed: docker | none | desktop | <cloud...>, fail-closed FromStr
ComputerStatus          // + can_transition_to / transition_to (run.rs pattern)
ResourceLimits          // MemoryLimitMb, CpuLimit, IdleMinutes newtypes + bounds
Takeover                // Idle | Held { by: UserId, since: Timestamp }
ScreenCapability        // (bot, computer, generation); verify is generation-equality
ComputerLease           // owner + fence (companion to the run lease)
```

Timestamps move to a shared domain `Timestamp` newtype (same idea already proposed for `account.rs`
wave-11). Keep every `lib.rs` re-export and every `req_026..031` test name stable for traceability.

## Concrete upgrades (with risks)

### 1. Replace `provider: String` with a typed `ComputerProvider` (REQ-027, fail-closed)
**CONFIRMED:** `provider` is a free-form `String` and the only "validation" is a doc comment.
REQ-027 requires an invalid provider to refuse startup, and DEC-08 fixes the first-release set
(Docker + desktop host). Change: a `#[serde(rename_all = "snake_case")]` enum with a fail-closed
`FromStr` (unknown → `DomainError::UnknownStatus`-style typed variant), plus `none` mapped to a
clear "computers unavailable" answer. Reuse the provider vocabulary in `bin/config.rs` so the two
do not drift. **Risks:** the reference has 14 providers; stored rows may carry names outside the
first-release set — audit `infra` before narrowing. Changing the serde shape is a DEVIATION to log
in `docs/PARITY.md`. **GUESS:** canonical wire name is the lower-case provider key.

### 2. Add a status transition table and a validating constructor (REQ-026)
**CONFIRMED:** all 6 statuses are convertible to/from strings with no legality check, so e.g.
`stopped -> interrupted` is representable. Change: `can_transition_to`/`transition_to` mirroring
`run.rs`, and `Computer::new(...)` used by infra on every write. Define the legal edges explicitly
(`queued -> running -> stopped`, `running -> interrupted -> stopped`, any -> `failed`,
`stopped -> queued` on lazy restart with a generation bump). **Risks:** `stopped` is domain-only
(not in the provider vocabulary), so the table must not be derived from `ALL`; infra likely writes
some transitions directly — audit call sites before enforcing, or the change breaks provisioning.

### 3. Model takeover and screen capability as domain types (REQ-029, REQ-030)
**CONFIRMED:** control is two pub fields (`takeover_by_user`, `takeover_at`) with no invariant, and
the "signed capability names bot, computer and generation" rule has no representation here. Change:
a `Takeover` sum type (exclusive by construction) and a `ScreenCapability` value carrying bot,
computer and generation, with a domain `is_current(&Computer)` check. **Risks:** the 409 conflict
message names the holder and the constant-time compare live in `api`/`infra`; the domain type must
carry enough to render that message without leaking private ids. Changing the struct fields touches
infra DTOs and the two goldens — keep the wire shape unchanged.

### 4. Fold the three validators into validated newtypes (correctness; make invalid states unrepresentable)
**CONFIRMED:** `cpu_limit: f64` is pub and the `NaN` guard exists only inside the free function;
callers can bypass all three validators. Change: `MemoryLimitMb`, `CpuLimit`, `IdleMinutes` with
private fields, `TryFrom` constructors reusing the existing constants/bounds, and keep the three
free functions as thin compatibility wrappers. **Risks:** serde output for the newtypes must stay the
bare number (use `#[serde(transparent)]`) or it becomes a wire DEVIATION; `f64` equality/finite checks
must also hold after deserialization.

### 5. Add minimal companion types for holder lease and home identity (REQ-026, REQ-031)
**GUESS:** the spec lists `ComputerRun`, `ComputerExecutionLease`, `BrowserProfile`, `AgentHome`,
`HomeDeletionNotice`, but none exist in the crate. Add lean record types (fields from the data-model
table) so the one-active-holder and private-home invariants have a place to live, with a
`holds_lease(owner, fence)` helper like `run.rs`. **Risks:** REQ-031 is SHOULD and DEC-08 is not
finalised — this is the most speculative item; keep it to field carriers plus one helper, gate any
behavior behind an ADR, and do not add features outside the spec (`AGENTS.md`).

## Risks & ordering

- #4 is behavior-preserving (wrappers kept) — do first; then #2, #1, #3, #5.
- Any serde/wire change (#1, #4) is a DEVIATION: log it in `docs/PARITY.md` and wait for owner
  approval before shipping. Do not narrow provider acceptance until infra call sites are audited.
- Keep `req_0xx` ids in test names (trace gate) and re-export any new public types from `lib.rs`.
- Re-run `cargo fmt --check`, `clippy -D warnings` and tests in the implementing session; nothing
  here was executed by this read-only worker.

## Tests to add (REQ ids in names)

- `req_027_unknown_provider_is_refused` / `req_027_none_provider_has_clear_answer`.
- `req_026_illegal_computer_status_transition_is_an_error`.
- `req_029_takeover_is_exclusive_by_construction` / `req_030_capability_invalid_on_generation_bump`.
- `req_027_limit_newtypes_reject_out_of_bounds_and_nan`.

Line budget: ~110. Status: proposal only; no code changed (READ-ONLY worker).
