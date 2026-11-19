# Worker 0224 — `crates/domain/src/computer.rs` reliability proposal (v0.4.0)

Module: `crates/domain/src/computer.rs` (189 lines). Lens: reliability. Read-only review;
only this file was written. Evidence: source read in this worktree on 2026-11-19.
CONFIRMED = read from source; GUESS = inferred. Anchors: REQ-026..031, data-model
"Computer" (`spec/02-data-model.md:100-113`), `AGENTS.md` (make invalid states
unrepresentable, typed errors, no floating-point where exactness matters).

## 1. Structure today (CONFIRMED)

- `ComputerStatus` is a six-value vocabulary (`queued/running/interrupted/completed/failed/
  stopped`) with `ALL`, `as_str`, `Display`, `FromStr`. Unlike `RunStatus` (run.rs:98-134) it
  has **no** transition table and no `can_transition_to`/`transition_to`, so the domain
  refuses nothing.
- `Computer` (computer.rs:91-131) is a bag of 18 **public** fields. Every invariant that matters
  (status vs. timestamps vs. generation vs. takeover) is enforced by ad-hoc `if`s in
  `crates/app/src/usecases.rs` — boot (5586-5594), stop (5889-5898), sweep (5946-5953),
  takeover (6018-6030), release (6058-6066), capability check (6328-6332). A new caller can
  bypass all of them and persist a nonsensical row.
- Three bounds validators (151-189) are exported (`lib.rs:64-66`): memory 64..4096 MB,
  CPU 0.25..4.0 finite f64, idle 1..1440 min. They all return the generic
  `DomainError::InvalidName`; `FromStr` likewise returns `InvalidName` instead of the
  `DomainError::UnknownStatus` used by `RunStatus`. Callers cannot distinguish a bad limit
  from an unknown status without parsing the message.
- **No unit tests at all** in this module (no `#[cfg(test)]`), while `run.rs:394-531` has an
  exhaustive 9x9 table test and wire round-trip tests. This is the thinnest-tested domain type.
- Timestamps are free-form `Option<String>`; `provider` is a free-form `String` with `"none"`
  as a magic sentinel checked only in the app layer (e.g. usecases.rs:5936).
- `screen_generation: u64` (106) is persisted as `i32` (`computer_row.rs:53`, `screen.rs:41`):
  `u64::try_from(i32)` saturates to `u64::MAX` on negative, and the write side clamps with
  `unwrap_or(i32::MAX)` (`stores/computer.rs:142`). Domain and store disagree on width.

## 2. Failure modes

### 2.1 Invalid actions (CONFIRMED)

- No transition guard: boot overwrites `status = Running` from any state (usecases.rs:5586)
  and stop sets `Stopped` from any state (5893). A double boot, a boot of a `Failed`
  computer, or a stop during `Queued` are all accepted; only `saturating_add(1)` (5589)
  keeps generation from wrapping, which can silently reuse a generation and revive stale
  screen capabilities.
- Validators reject correctly (finite CPU, inclusive bounds) but report `InvalidName`, so the
  API maps them to `bad_request` (rpc.rs:1428-1436) with no structured reason; an operator
  chasing a config error gets a name-shaped message for a numeric fault.
- Empty external ids are tolerated: `external_id`/`home_id` are `Option<String>` with no
  non-empty check, yet the sweeper skips a running computer with no `external_id`
  (usecases.rs:5943), leaving it to leak capacity forever.

### 2.2 Permission escalation (CONFIRMED / GUESS where noted)

- `takeover_by_user`, `takeover_at`, `private`, `external_id` are plain public fields. The
  "only the holder may release", "bot input refused while held" and "private home invisible
  to other bots" rules live only in the app layer (REQ-029/031). Any path that sets the field
  without the app gate escalates control or crosses the trust boundary.
- The screen capability check (REQ-030) is an `if` in `usecases.rs:6328`; there is no domain
  method expressing "this capability is current for this computer". GUESS: a second streaming
  path could omit the generation comparison and serve a stale frame.
- `provider: String` is never validated in the domain; a typo (`"None"`, `"docker "`) misses
  the `!= "none"` guard (usecases.rs:5936) and fails open into a provider lookup (REQ-027).
- `confine_home_path` (REQ-028, usecases.rs:5972-5991) is also app-layer, so the central path
  escape defense is not part of the pure module that owns the Computer aggregate.

### 2.3 State drift (CONFIRMED)

- `ComputerStatusReply::for_computer` treats **every** non-`Stopped` status as running
  (`wire.rs:390`), so `Failed`/`Interrupted`/`Queued` computers are reported to the UI as
  "running" with a generation and timestamps. Domain state and user-visible state diverge.
- Nothing ties `status` to `started_at`/`stopped_at`/`takeover_at`: a row can be `Stopped` with
  a takeover holder, `Running` with `stopped_at`, or have `takeover_by_user` set while
  `takeover_at` is `None`. No `validate()` exists to catch a corrupt or hand-built row.
- The u64/i32 generation split means a saturated `u64::MAX` can flow back as `i32::MAX`,
  breaking the "generation increments on every restart" contract (`data-model.md:112-113`).

## 3. Concrete hardening (each with a risk)

H1 (low). Port the `run.rs` pattern: add `ComputerStatus::can_transition_to`/`transition_to`
plus an exhaustive 6x6 test, using the provisioning vocabulary from `data-model.md:112`.
Risk: the exact computer transition table is **not written in the spec** (GUESS) — derive it
from the reference plus the REQ-026/029 flows, freeze today's accepted sequences in a golden
test first, and treat any newly rejected sequence as a DEVIATION pending owner approval.

H2 (medium). Add `Computer::validate() -> Result<(), DomainError>` enforcing the invariants
in 2.3, called at the store boundary (`computer_row.rs::try_from`) and after every mutation in
`usecases.rs`. Risk: testkit/conformance fixtures build `Computer` literals (e.g.
`golden_world.rs:668-675`, `testkit/src/memory.rs:4881-4884`) that may be inconsistent; stage
adoption as deny-new-writes first, and record any fixture correction in `docs/PARITY.md`.

H3 (low). Dedicated errors: `DomainError::InvalidLimit { kind, value }` and
`DomainError::UnknownComputerStatus(String)`; keep `Display` phrasing byte-identical for the
API. Risk: error text is externally visible — lock current strings in a test before changing
variants; keep helper-style predicates source-compatible.

H4 (medium). Make critical values unrepresentable: a `Provider` enum (`Docker`, `None`,
`Cloud(String)`) and a `HomePath` newtype that owns `confine_home_path`, so an escaping path
cannot be stored in a domain value. Risk: `provider` and path strings are on the wire; keep
`as_str`/round-trip parity and the `AppError::Rule` mapping, and add proptests for the path
grammar before moving the logic out of the app crate.

H5 (medium). Add `ScreenCapability { bot_id, computer_id, generation }` and
`Computer::is_current(&capability)`; have both `usecases.rs:6328` and `api/src/screen.rs`
consult it. Risk: signing/HMAC stays in the API — do not move secrets into `domain`; only the
freshness comparison moves.

H6 (higher). Align generation width: migrate the store to `BIGINT`/`i64` (or cap the domain
type to `u32`) and add `Computer::next_generation()` using `checked_add`, refusing overflow
instead of saturating. Risk: a schema migration plus golden fixtures asserting generation
`1`/`2`; needs an ADR and a parity check before shipping.

H7 (low, test-only). Add `#[cfg(test)]` tests named `req_026_*`..`req_031_*`: bound edges,
status round-trip, invariant rejection, and `HomePath` confinement; use `proptest` for the
validators. Risk: none to runtime behavior; traceability needs REQ ids in declaration lines.

## 4. Risks and gates

- Externally visible changes (H1 rejected sequences, H3 error text, H4 wire strings, H6
  generation width) are DEVIATIONs: record each in `docs/PARITY.md` and wait for owner approval.
- Pure `domain` crate: no IO, no async, no new dependencies; `unsafe_code = forbid`, deny
  `unwrap`/`expect`/`panic` in non-test code, `clippy -D warnings`.
- Do not tighten a gate to make a new test pass; a test that passes because it was weakened
  is a lie. Mark the unknown transition table (H1) and any fixture inconsistency (H2) as
  GUESS in `docs/STATE.md` under "Decisions needed".

## 5. Verification for this wave

1. `cargo fmt --check` and `cargo clippy --workspace --all-targets -- -D warnings`.
2. `cargo test -p boma-domain` (new REQ-026..031 tests) plus the existing
   `crates/infra/tests/computers.rs`, `computer_conformance.rs` and `crates/testkit` flows.
3. Differential against the running original for boot/stop/takeover/screen; record only
   CONFIRMED outputs in `docs/PARITY.md`.
