# Worker 0257 — `crates/domain/src/routine.rs` — architecture proposal (v0.4.0)

- Lens: architecture. Requirement: REQ-072 (local automation API and CLI) / REQ-023 (routine model).
- Status: PROPOSAL only — no code changed; read-only except this file.

## What I inspected (CONFIRMED)
- Full `routine.rs` (210 lines), `lib.rs:96-100` re-exports, `Cargo.toml`,
  `usecases.rs:4930-4991,5271-5294`, `ports.rs:1340-1408`, `api/wire.rs:480-504`,
  `infra/stores/routines.rs`, `error.rs`, `ids.rs:60`, `spec/01-requirements.md:189-194,579-585`.
- Grep across `crates/` for `RoutineTrigger`/`RoutineTriggers`: 8 hits, **all inside routine.rs** —
  the parsed types are public in the module but not re-exported and unused by any caller.
- Three callers re-derive `@once` by raw string compare (`usecases.rs:5277`, `wire.rs:493`,
  `infra/stores/routines.rs:209`), duplicating the parser's job.

## Current shape (CONFIRMED)
- Bag of free functions over stringly inputs: `validate_routine_name/prompt/timezone/triggers`,
  `parse_trigger`, `next_run_at(&[String], &str, &str) -> Result<String, _>`; helpers
  `parse_instant`/`format_utc`. Constants `ONCE`, `MAX_ROUTINE_NAME_LENGTH`,
  `MAX_ROUTINE_PROMPT_LENGTH`, `MAX_CRONS`.
- `RoutineTrigger::{Cron(Box<Cron>), Once}` and `RoutineTriggers { triggers: Vec<_> }` exist but
  are rebuilt and discarded inside `next_run_at` — no aggregate `Routine` type in the domain;
  the aggregate lives in `app/ports.rs` with raw `String` crons/timezone.
- Every failure maps to `DomainError::InvalidName`, including values that echo raw input
  (`format!("`{timezone}`...")`, `format!("`{entry}`...")`, `format!("`{instant}`...")`).
- `@once` short-circuits **before** `validate_timezone` (lines 132-140 vs 141) and hardcodes a
  5-minute delay at line 138. Multiple triggers with one `Once` are accepted, then crons ignored.
- `find_next_occurrence(&now, false)` (line 152); output formatted by hand as
  `%Y-%m-%dT%H:%M:%SZ` (line 173), dropping sub-second precision.
- Four REQ-023-named unit tests; no proptest/fuzz. Pure crate, `deny(missing_docs)` satisfied —
  both worth preserving.

## Guiding principles
Stay pure (no IO/clock/async); make invalid states unrepresentable; keep DB/wire text byte-stable.
REQ-072's automation "run a routine" consumes this model through the app layer, so any input
acceptance change here is potentially externally visible: DEVIATION → `docs/PARITY.md` + approval.

## Upgrades (v0.4.0), each with a risk

### U1 — Expose a parsed `RoutineSchedule` aggregate; retire raw-`@once` scanning
- Re-export `RoutineTrigger`/`RoutineTriggers` and add `RoutineSchedule::parse(crons, timezone)`
  built once, with `is_once()`, `next_after(&UtcInstant)`. Reject `Once` mixed with `Cron` at
  construction instead of silently ignoring crons in `next_run_at`. Replace the three caller-side
  string scans with `is_once()`.
- Why: the parsed type is dead code and `@once` detection is duplicated (CONFIRMED) — a drift risk.
- Risk: touches domain public API plus usecases/wire/store. Mitigate with signature-compatible
  shims and unchanged wire/DB text. Rejecting `once + crons` changes accepted inputs → DEVIATION
  if any real caller relies on it; check logs/tests before enforcing (GUESS).

### U2 — Typed routine errors instead of the `InvalidName` catch-all
- Add `DomainError::InvalidRoutine { field, reason }` (or per-field variants) for name, prompt,
  timezone, cron, instant and no-next-run; stop embedding raw user input in `reason` (mirror the
  `req_040` privacy stance in `error.rs`). Map to the existing API error shape at the boundary.
- Why: automation callers cannot distinguish a bad timezone from a bad cron today (CONFIRMED).
- Risk: error text/DTO mapping is a parity surface. Existing tests assert `is_err()` only
  (CONFIRMED), but the API's `InvalidName → 422` mapping site must also handle the new variant
  (location GUESS — verify before editing).

### U3 — Typed instants and timezone; one parse/format authority
- Add `UtcInstant` and `TzName` newtypes (`try_new`, `as_str`) following `ids.rs` conventions;
  `next_run_at` takes/returns `UtcInstant`, with thin `&str` wrappers for current callers. Keep the
  exact `Z`-suffixed formatter for the wire boundary; document that sub-seconds are intentionally
  dropped, and use chrono's RFC3339 parser internally.
- Why: stringly boundaries force repeated parse/format and a hand-rolled formatter (CONFIRMED).
- Risk: ripples into `app/ports.rs` (`next_run_at: Option<String>`), the injected clock and store
  bindings — stage with shims; any format change breaks stored/wire timestamps (parity gate).

### U4 — Make `@once` policy explicit and validate before the short-circuit
- Named/injected delay constant in place of the literal 5 minutes; validate timezone and schedule
  invariants before the once branch so a garbage timezone cannot hide behind `@once`. With U1,
  persist the resolved due instant at creation so dispatch (`usecases.rs:5277`) stops recomputing.
- Why: `next_run_at` skips timezone validation for `@once` (CONFIRMED); "5 minutes" is policy baked
  into a pure fn with no injection point (GUESS whether spec-derived or original behavior).
- Risk: changing the firing delay is externally visible — keep the 5-minute default and ask the
  owner before any change; record the finding in `docs/STATE.md`.

### U5 — Property/fuzz tests and DST edge cases for the external parsers
- proptest: arbitrary strings never panic `parse_trigger`; `next_run_at` returns `next > now`;
  format/parse round-trip. Explicit spring-forward/fall-back cases for `Europe/Paris` and
  `America/New_York` (croner + chrono-tz), plus exact-boundary determinism.
- Why: AGENTS.md mandates property/fuzz coverage for every external-format parser; module has
  four unit tests (CONFIRMED).
- Risk: new dev-dependency and CI time; "never fires" crons legitimately error — model as an
  expected outcome. A DST discrepancy may expose a real defect → `docs/PARITY.md`, not a silent fix.

## Suggested order (small steps, test-first)
Write the failing REQ-named test before each step: U1 + U4 (model invariants) → U2 (errors) →
U3 (types) → U5 (tests). After each: `cargo fmt --check`, `clippy -D warnings`, `cargo test`.

## Verification for v0.4.0
- `fn req_023_once_cannot_be_mixed_with_crons()`, `fn req_023_parsed_schedule_drives_dispatch()`.
- `fn req_072_automation_reports_bad_timezone_distinctly_from_bad_cron()`.
- `fn req_023_next_run_is_after_now_and_stable_across_dst()` (proptest + DST cases).
- Confirm generated OpenAPI/goldens unchanged for accepted inputs — otherwise file a DEVIATION.

## Open questions (GUESS / decisions needed)
- Is the 5-minute `@once` delay spec-derived or inherited original behavior? Check original.
- Is any routine error string part of the wire contract, or free to change with U2?
- Should REQ-072's "run a routine" pass a parsed schedule rather than raw crons through the app?
