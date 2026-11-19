# 0259 — `crates/domain/src/run.rs` — architecture (v0.4.0)

- Worker: 259 · Lens: architecture · Target: v0.4.0 · Read-only review; no code edited.
- Module: 531-line hand-written run domain model: `RunStatus` (9 variants + `ALL`/`as_str`/predicates/
  `can_transition_to`/`transition_to`), `RunTrigger` (8 variants), `Run` (25 all-`pub` fields), retry
  constants + `retry_delay_seconds`, and 7 unit tests. Re-exported at `lib.rs:101`
  (`pub use run::{Run, RunStatus, RunTrigger}`); retry helpers are reached by path
  (`boma_domain::run::MAX_ATTEMPTS`, `retry_delay_seconds`).
- Spec anchors: REQ-012 (exhaustive state machine, single executor, stale fence) and REQ-024
  (backoff, attempt cap, `not_before`) in `spec/01-requirements.md:99-105,196-203`;
  `spec/02-data-model.md:58-77` (Run entity, state-machine diagram, status groups); goldens
  `run-retry.json` (failed→queued, attempt 2) and `ui/REQ-012-run-statuses.json`; open decisions
  DEC-02 (fence semantics) and DEC-03 (queue holding) in `spec/06-decisions-needed.md:25-43`.
- Read this session: `run.rs`, `lib.rs`, `error.rs`, the spec sections and goldens above, plus greps
  of `can_transition_to`/`transition_to`/`set_status`/`.status =` across `crates/{domain,app,infra,api,
  testkit,conformance}`. Also skimmed wave-8 doc 0115 (the UI vocabulary mirror). CONFIRMED = read
  this session; PROPOSAL/GUESS = design inference.

## Structure today (CONFIRMED)

1. **The transition table is not the enforcement point.** `can_transition_to` / `transition_to`
   (run.rs:100-134) appear only at their definitions and inside run.rs's own tests (447, 471, 482,
   494); no production caller exists. Real writes go through `RunStore::set_status`
   (`app/src/ports.rs:1011-1019`, `infra/src/stores/runs.rs:386-429`), whose SQL guards only the fence
   (`$4 IS NULL OR lease_fence = $4`) — never `status = <from>`. So `completed → running` is writable
   from app/infra today. `retry()` (runs.rs:431-477) *does* enforce `status = 'failed'` (line 457),
   but only for that one path. The file header's claim ("source of truth … exhaustive … never a silent
   no-op") is aspirational at the mutation point.
2. **The vocabulary has three hand-maintained mirrors in Rust and more in SQL.** `ALL` (43-53),
   `as_str()` (57-69) and `FromStr` (143-159) enumerate the same 9 statuses independently; triggers
   repeat the pattern (186-195, 199-209, 231-243). `set_status` hardcodes
   `('completed','failed','cancelled')` (runs.rs:403), duplicating `is_terminal()` (run.rs:85-87);
   `retry()` hardcodes `'queued'` plus an 8-column reset (runs.rs:446-457). Adding a status to `ALL`
   compiles in Rust but silently misses the SQL `CASE`. The round-trip test (500-515) catches some
   drift, not the cross-layer split.
3. **`Run` is a mutable bag of 25 public fields, not an aggregate.** Every field in 250-321 is `pub`;
   timestamps are `Option<String>`; `status`, `attempt`, `lease_*`, `completed_at` and `not_before`
   can hold any combination. The only transition method is on the *status value* and returns a bare
   `RunStatus`, never a `Run` with its derived fields updated. `#[serde(default)]` on fence/steering/
   model fields lets a partial payload deserialize into a plausible-looking invalid run. This does
   not meet AGENTS.md "make invalid states unrepresentable".
4. **Retry policy is split across layers.** Domain owns the constants and ladder (325-344), but the
   attempt cap is enforced in `app/src/usecases.rs:5081`, `not_before` is rendered in app (5085-5092),
   and terminal/backoff semantics are encoded again in SQL. `Run` exposes no `can_retry()` or
   `next_retry_delay()`, so any other caller can skip the three-attempt cap.
5. **Status policy is scattered and sits on an open decision.** `holds_queue()` (94-96) =
   `is_working() || WaitingTakeover`; `is_waiting_on_user()` (79-81) has no external caller (grep).
   The comment at 91-92 and DEC-03 say `waiting_input` does not hold the queue, while
   `spec/02-data-model.md:75` says both waiting states "keep the queue" — a spec-vs-code divergence
   that belongs in `docs/STATE.md`, not a silent code fix. `holds_queue`/`is_working` have no domain
   test pinning their matrix (only `is_terminal` is tested). Intent is GUESS; text is CONFIRMED.

## Concrete upgrades for v0.4.0 (each with a risk)

1. **Make the table the enforcement point with a compare-and-swap store contract.** Add a whole-value
   `Run::transition(next, at, fence) -> Result<Run, DomainError>` that updates derived fields
   (`started_at`/`completed_at`/`error`/lease) atomically, and change `set_status` to carry the
   expected `from: RunStatus` so SQL becomes `WHERE id = $1 AND status = $from AND lease_fence = $f`.
   Risk: the port signature change ripples through app/testkit/infra/goldens, and CAS turns today's
   ignored stale writes into errors — DEC-02 (reject vs ignore) must be settled first. Land the
   additive domain API here, then the store change in the owning waves.
2. **Collapse the mirrors into one table-driven vocabulary.** Define `STATUS_TABLE` / `TRIGGER_TABLE`
   once (variant, wire string, flags) and derive `ALL`, `as_str`, `FromStr`, `Display` and predicates
   from them; keep wire strings byte-identical and extend the round-trip test (500-515) with a golden
   of every `as_str()` output. Risk: a derive crate needs `cargo add` + docs.rs review per AGENTS.md;
   a no-dependency const table is safer but more code. Any wire drift is externally visible →
   `docs/PARITY.md` entry.
3. **Narrow `Run`'s surface with constructors and typed substructures.** Add `Run::new(...)`,
   `Run::retry(now)` and getters; group lease fields into a `Lease` newtype and timestamps into a
   typed instant, then reduce field visibility where serde/row mapping allows. Risk: serialized field
   names must stay for DB/API compatibility; large call-site churn in app/testkit/conformance — tighten
   incrementally behind accessors and never rename a serde field.
4. **Move the whole retry rule onto the domain type.** Add `Run::can_retry()` and
   `Run::next_retry_delay()` over the existing constants, have `usecases.rs:5081-5084` call them, and
   add a test asserting the SQL terminal-status literal equals the join of
   `RunStatus::ALL.filter(is_terminal).map(as_str)`. Risk: SQL cannot call Rust at runtime, so the
   duplication is only *checked*, not removed; wire that test into CI or a forgotten new status
   silently escapes the SQL `CASE`.
5. **Give status policy one home and settle DEC-03.** Consolidate `is_working` /
   `is_waiting_on_user` / `holds_queue` into one `StatusPolicy` table and add a test pinning the
   current queue-holding matrix; record the `02-data-model.md:75` vs run.rs:91-96 divergence in
   `docs/STATE.md`. Risk: flipping whether `WaitingInput` holds the queue changes scheduling behavior
   (DEVIATION + owner approval); for v0.4.0 keep behavior and only document/test it.

## Explicit non-goals

No IO/clock/async in the domain crate; no SQL rewrite inside this module (only the checked contract
above); no new statuses or triggers beyond the spec; no wire-string change without an ADR / PARITY
entry; no merge with the UI vocabulary owned by wave-8 doc 0115; no `AppError`/API mapping redesign;
and no behavior change while DEC-02/DEC-03 are unresolved.
