# Worker 218 — `crates/domain/src/card.rs` — reliability proposal (v0.4.0)

Scope: approval/question cards as a **decision state machine**, not a bag of public fields.
Focus: invalid transitions, double approval, empty-answer implicit approval.

## Evidence (all CONFIRMED unless marked GUESS)

- `Card` exposes every field as `pub` and has **no transition method**; the only tests set
  `status`, `answer`, `answered_at` by direct assignment (`card.rs:47-67`, `208-217`).
- `DomainError::InvalidTransition` exists but carries `RunStatus` only — a card transition
  cannot be expressed as a typed error (`error.rs:11-17`, `132-137`).
- `CardStatus::Declined` exists (`card.rs:43-44`) but the answer path **never produces it**:
  both stores always write `'answered'` (`stores/cards.rs:134`, `testkit/memory.rs:3015`).
  The row mapper does accept `"declined"` (`stores/rows.rs:422-427`).
- `answer_card` does `find` → check `Pending` → `answer()` in separate statements
  (`usecases.rs:7670-7680`). The Postgres `UPDATE ... WHERE status = 'pending' RETURNING`
  (`stores/cards.rs:129-153`) does give a single row-level winner — the remaining gap is
  the wider decision transaction, not the status flip itself.
- Effect gate is the exact string `answer == "approve"`; **any other string** still closes the
  card and re-queues the run as a non-approval (`usecases.rs:7688-7690`, `7737-7741`).
- `effect_key = "approve:<card_id>"` is recorded **only for `CardKind::Approval`**
  (`usecases.rs:7691-7692`). Purchase and Takeover, though effect-gating kinds, do not get
  an idempotency record on this path. GUESS: a purchase store may record payments elsewhere —
  verify before adding a second key (see H-6).
- Card UPDATE, effect INSERT, parked-call resolution and run re-queue run in **separate store
  calls, each with its own tenant transaction** — no shared transaction or outbox.
- `ThreadAnswerRequest.answer` is a bare `String` with no non-empty/length validation
  (`api/src/rpc.rs:968-974`). The web sends an always-enabled Approve for questions
  (STATE.md, M34) — GUESS whether it can send `""` for an approval today.

## Failure modes

- **FM-1 Invalid transitions (silent).** Nothing prevents `Answered → Answered`,
  `Declined → Answered`, `answered_at` set while status is `Pending`, or an answer on a
  `Pending` card with `run_id = None`. Enforcement lives in the store SQL and the use case,
  not the type; an in-memory or future adapter can diverge. Decline is unrepresentable in
  the answer flow, so intent is lost at the domain boundary.
- **FM-2 Double approval (partial atomicity).** A crash after `cards.answer()` but before
  `effects.record()` closes the card without an effect record; a retry then fails at the card
  check with `Conflict`, so the effect is never recorded and the run resumes without it. The
  reverse ordering is safe (effect key wins), but recovery is one-sided. The find-then-act
  window also duplicates work on concurrent calls before the row-lock winner is known.
- **FM-3 Empty-answer implicit approval.** `""`, whitespace, `"APPROVE "` or arbitrary text
  are accepted as a decision: the card closes, the run resumes, no effect is recorded. For a
  gating card this is a **silent decline mislabelled as `Answered`** — the member believes a
  choice was made, audit shows none, and the parked tool call is applied as
  `answer == "approve"` → false (usecases.rs:7741). Exact-match string parsing is the
  approval authority; that is brittle and easy to get wrong in a client.
- **FM-4 Coverage gap.** Purchase/Takeover approvals are gated by the same UI but bypass the
  idempotency record (GUESS: compensated elsewhere). `CardStatus::Declined` being dead means
  the "second answer shows the first result" acceptance criterion (REQ-007) cannot show a
  decline distinctly.

## Concrete hardening (v0.4.0)

- **H-1 — Encapsulate the state machine (high value, wide diff).** Make `Card` fields private;
  add `Card::new`, `card.answer(decision, at)` and `card.decline(at)` returning a new typed
  `DomainError::CardTransition { from: CardStatus, to: CardStatus }` (extend the existing
  invalid-transition pattern). Invariants: only `Pending → Answered|Declined`; `answer` and
  `answered_at` set together and never on a closed card. Add `CardStatus::{as_str, parse}`
  and `CardKind::{as_str, parse}` for symmetry with `PendingToolCallStatus`/`ToolKind`.
  *Risk:* ~6 construction sites across `api`, `conformance`, `testkit`, `infra` (grep
  `CardStatus::Pending`) must migrate. Stage it: add methods + `#[deprecated]` pub fields first,
  privatise only once callers are clean. Internal-only API change → no wire DEVIATION.
- **H-2 — Canonical decision parsing at the boundary (small, safe).** Introduce
  `enum CardDecision { Approve, Decline, Text(String) }` and one `CardDecision::parse(&str)`
  that trims once and maps exact canonical tokens. Keep `"approve"` as the approval token so
  existing clients keep working; reject empty/whitespace for gating kinds with a typed
  `DomainError::EmptyAnswer` → HTTP 400, never a 200 that silently closes the card.
  *Risk:* rejecting `""` is externally visible for a previously accepted input → record in
  `docs/PARITY.md` and wait for owner approval before shipping (AGENTS.md). Verify the
  `REQ-007-pending-card` golden first so question-card text answers stay valid.
- **H-3 — Make decline first-class (medium).** Route non-approval decisions on gating cards to
  `CardStatus::Declined` and `PendingToolCallStatus::Refused`; keep `Answered` for questions.
  Update the store UPDATE to `WHERE status = 'pending' RETURNING ...` with a computed status
  so both terminal states are written in one atomic step. *Risk:* changes the stored status
  value (data-format DEVIATION); row mapper already accepts it, but the card-answered state is
  still absent from the messages wire (STATE.md M34) — coordinate so a decline is visible on
  reload, else the two statuses look identical to a member.
- **H-4 — One atomic decision transaction (high value, infra change).** Cover card UPDATE +
  effect INSERT + parked-call resolution + run re-queue in a single transaction (or a small
  outbox table). Make `effects.record()` return the existing record on replay with the same
  payload rather than only `Conflict`. *Risk:* stores currently own their own tenant
  transaction; refactor to accept a shared executor, add timeouts and keep lock scope short.
  Requires testcontainers tests and a careful cancellation-safety review.
- **H-5 — Extend idempotency to all gating kinds (medium).** Use a kind-qualified key
  (`approve:<card_id>` retained for existing approvals; e.g. `purchase:<card_id>`,
  `takeover:<card_id>` for the rest) so REQ-018 purchases cannot double-charge. *Risk:* if a
  purchase key already exists elsewhere you get two idempotency domains — verify first; a key
  format change needs a backfill for already-recorded effects.
- **H-6 — Property tests (low risk).** `proptest` over (initial status, decision, replay)
  asserting: at most one effect per card, terminal status is stable, no answer mutates a
  closed card, and empty input never produces `Approve`. Name them with REQ ids, e.g.
  `req_017_a_second_answer_never_records_a_second_effect`,
  `req_007_a_declined_card_is_terminal`, `req_017_an_empty_answer_cannot_approve`.

## Ordering / risk summary

1. H-2 + H-6 first (boundary validation + tests; no change for valid answers).
2. H-3 next, gated on the wire-status fix and a PARITY entry.
3. H-4 + H-5 as one infra-backed change; highest blast radius, needs transaction tests.
4. H-1 last, after construction sites are migrated — the type-level guarantee is worth the
   diff only once the codebase no longer relies on pub fields.

Net: the reliability win is mostly **making the decision explicit and atomic**; the main
risk is not the Rust logic but externally visible status/input changes (PARITY + owner sign-off)
and the transaction refactor across store boundaries.
