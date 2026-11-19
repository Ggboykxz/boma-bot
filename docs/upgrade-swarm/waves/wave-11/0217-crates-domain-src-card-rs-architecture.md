# Worker 0217 — `crates/domain/src/card.rs` architecture proposal (v0.4.0)

Lens: architecture. Scope: read-only review of the 230-line domain module plus targeted reads of
`spec/01-requirements.md` (REQ-017/018), `docs/adr/033-...`, `crates/app/src/usecases.rs`,
`crates/infra/src/stores/{cards,rows}.rs`, `crates/testkit/src/memory.rs` and
`migrations/0004_cards.sql`. No code changed, no tests run. Claims below are **CONFIRMED by source
inspection**; forward-looking items are marked RISK. The module itself is otherwise clean: pure,
`no_std`-friendly data definitions, good rustdoc, serde snake_case, conservative `ToolEffect` default.

## Current shape (as read)

- `Card { id, space_id, thread_id, run_id, kind, status, answer: Option<String>, created_at,
  answered_at }` — an inert data bag; no transition method, no invariant tying `status`, `answer`
  and `answered_at` together. The only test sets all three fields by hand (lines 212–214).
- `CardKind`, `CardStatus`, `ToolKind`, `ToolEffect` and `PendingToolCallStatus` are the vocabulary;
  only the last two have `parse`/`as_str`. Wire parsing of `kind`/`status` is duplicated in
  `infra/stores/rows.rs:410–430` and re-encoded in `infra/stores/cards.rs:78–81`, both mapping
  unknown values to `DomainError::UnknownEventType`.
- `ExternalEffect.effect_key: String` is the idempotency identity; the doc claims "exactly once
  per run" (lines 6–8). The key is *formatted* outside the domain: `approve:<card>` in
  `usecases.rs:7686`, `tool:<run>:<call>` in `usecases.rs:7105`, plus literals in testkit/m1.
- `answer_card` (`usecases.rs:7659–7710`) does the real decision with a string compare:
  `card.kind == Approval && answer == "approve"`. ADR-033 forbids an implicit approval on an empty
  answer but the model happily represents `status = Answered, answer = Some("")`.

## Concrete upgrades for v0.4.0

### 1. Make the decision typed and transitions illegal-by-construction (REQ-017, ADR-033)
Replace the free `Option<String>` decision with an enum carried by the card, e.g.
`CardDecision::{Approved, Declined, Text(String)}`, and expose `Card::approve(&self) -> Result<Card,
DomainError>`, `decline(...)`, `answer_text(...)`, each accepting only `CardStatus::Pending` and
setting `answered_at` itself; keep the stored/wire `answer: String` via `serde` conversion so the
`threads.answer` contract is unchanged. Add a domain test that `CardStatus::Answered` with empty text
is a `Text("")` (not `Approved`) and that a second transition from a terminal status is rejected.
RISK: touches the public `Card` struct; `app`, `infra` (rows), `testkit`, `conformance` and four test
suites construct `Card` literally. Keep field names and serde output byte-stable or record a
DEVIATION in `docs/PARITY.md`. The store still owns the DB write; the domain method must be called
*before* it, not replace it, or the existing `Conflict` on replay is lost.

### 2. Move kind/status parsing into the domain (single vocabulary)
Add `CardKind::parse`/`as_str` and `CardStatus::parse`/`as_str` mirroring `ToolKind`
(`card.rs:156–172`), and have `infra/stores/rows.rs` and `cards.rs` call them. Consider one typed
`UnknownCardValue(String)` error instead of reusing `DomainError::UnknownEventType`, which is
semantically about events. RISK: error text is operator-visible; preserve the exact strings and
the existing `req_0xx` test names (traceability gate). This is a pure refactor — no wire change.

### 3. Let the domain own the effect-key format and its identity (REQ-017)
Add `EffectKey` newtype with `CardId::approve_effect_key()` / `RunId::tool_effect_key(call_id)`
constructors and a `Card::required_effect_key()` returning `Option<EffectKey>`; update the three
call sites to use them. Add a property test that the rendered key is unique for distinct
card/run ids. RISK: keys are persisted — `"approve:<card>"` and `"tool:<run>:<call>"` must remain
byte-identical or stored idempotency records stop matching (silent double-effect). Separately,
**CONFIRMED divergence**: `migrations/0004_cards.sql:30` and Postgres use `PRIMARY KEY (run_id,
effect_key)`, but the in-memory store dedupes on `effect_key` alone (`testkit/memory.rs:3247`);
the same key in two runs conflicts in tests but not in production. Fix the memory store to compare
`(run_id, effect_key)` in the same change and add a two-run regression test.

### 4. Type the timestamps
`created_at`, `answered_at`, `ExternalEffect.applied_at` are raw `String`, so ordering is lexical and
malformed values are representable. Introduce one `Timestamp` newtype (or `chrono::DateTime<Utc>`)
with RFC3339 serde and a `Display` that matches `boma.rfc3339(...)`. RISK: adds a dependency
(record name/version/license in `docs/DEPENDENCIES.md`, docs.rs-check the API per `AGENTS.md`) and
must round-trip existing rows exactly; any format drift is an externally visible DEVIATION. Lower
priority than #1–#3; land only with golden replay green.

### 5. Resolve the dead card kinds against REQ-018
`CardKind::{Purchase, Takeover}` exist and round-trip through storage, but no Rust code creates
them and `answer_card` records an effect only for `Approval` (`usecases.rs:7683`) — so the
REQ-018 "purchase becomes a card and a recorded payment" path is not represented here. Add
`CardKind::gates_effect()` (true for Approval/Purchase/Takeover, false for Question) and make the
usecase consult it instead of the hard-coded equality. RISK: if purchase/takeover cards are ever
created, this changes observable effect recording — confirm against `spec/golden/` and record a
DEVIATION or owner approval before shipping. Alternatively propose removing the unused kinds in
`docs/STATE.md`; do not delete them silently.

## Sequencing / cross-cutting risks

1. Land #2 first (mechanical, no behavior change); run `cargo fmt --check`, `clippy -D warnings`,
   targeted tests after each step.
2. Then #3 (key ownership + memory-store divergence) — keep key strings byte-stable; it underpins
   the REQ-017 exactly-once guarantee.
3. Then #1 (typed decision) — the widest blast radius; update every literal `Card` constructor and
   keep ADR-033's no-implicit-approval rule pinned by name.
4. #4 and #5 last: both can change observable behavior. Gate on golden replay and `docs/PARITY.md`
   entries per `AGENTS.md`.
5. Preserve all `req_017_*` test names; `tools/trace_check.py` counts declarations, not comments.
6. All statuses above are CONFIRMED by read-only inspection only; re-run the verification commands
   in the implementing session before flipping any claim to verified-by-test.
