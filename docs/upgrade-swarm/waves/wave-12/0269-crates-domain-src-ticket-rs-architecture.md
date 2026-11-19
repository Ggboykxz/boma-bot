# Ticket domain model — architecture proposal (v0.4.0)

Module: `crates/domain/src/ticket.rs` (REQ-038 ticket board, feature-flagged).
Lens: architecture. Scope: proposal only — no code was changed.

## Current shape (CONFIRMED — read this session)
- Three public bounds: `TICKET_TITLE_MAX = 200`, `TICKET_DESCRIPTION_MAX = 4_000`,
  `TICKET_COMMENT_MAX = 10_000`; all re-exported from `lib.rs` and kept byte-stable.
- `TicketState { Backlog, Todo, InProgress, Done }`: `as_str()` (`backlog`, `todo`,
  `in_progress`, `done`), `ALL`, and `parse()` returning `DomainError::InvalidName`.
  No `serde`, `Display`, `Default`, `opaque_id`, or transition rules.
- Two free validators: `validate_ticket_title` trims, rejects empty, counts `chars()` against
  the title bound, and forbids control chars except `\n`/`\t`; `validate_ticket_text(text, max,
  what)` checks the same control-char rule and a caller-supplied `max` but does **not** trim
  and does **not** reject empty.
- The domain holds *no* ticket aggregate. The model lives as stringly-typed structs in
  `app/ports.rs` (`TicketSummary`, `NewTicket`, `TicketComment`, `NewTicketComment`), raw
  `String`s in `infra/stores/tickets.rs` (`state`, `author_kind`, ids, and private `TicketRow`
  tuples), and a private `TicketRow` in `testkit/memory.rs`. Vocabularies are duplicated:
  state strings and `member`/`bot` author kinds are untyped in app/infra.
- Call sites (`app/usecases.rs`): create validates title + description bound, forces
  `state = "backlog"`, trims before storing; move parses the state then passes `as_str()` to
  the store; comment validates the body bound, checks non-empty, then trims.
- No transition policy: `tickets_move` accepts any of the four states from any state (kanban).
- Three in-file unit tests, all named `req_038_...`; no proptest despite the domain standard
  for critical rules and external-format parsers. `#![deny(missing_docs)]` is satisfied.

## Guiding principles
Keep the domain pure (no IO/clock/async). Make invalid states unrepresentable: a `Ticket` that
exists should already satisfy its rules. Keep DB columns, wire field names, `as_str()` values,
error text and public re-exports byte-stable — any change is a DEVIATION needing `docs/PARITY.md`
and approval before shipping (AGENTS.md). Move the model *into* domain without reshaping adapters.

## Upgrade 1 — Give `TicketState` the standard enum treatment
- Current: hand-rolled parse with the generic `InvalidName` variant; unlike `CardStatus` in
  `card.rs` it has no `serde`, `Display`, or `Default`; its parse error is a name error, not a
  vocabulary error (cf. `DomainError::UnknownStatus` / `UnknownRole`).
- Proposal: add `#[derive(Serialize, Deserialize)] #[serde(rename_all = "snake_case")]`,
  `Display`, `Default = Backlog`, and a dedicated `DomainError::UnknownTicketState(String)`;
  keep `as_str()`, `ALL`, `parse()` and the four wire strings exactly as they are.
- Why: one state vocabulary across domain, store, wire and testkit; consistent error taxonomy.
- Risks: changing `parse`'s error variant/text is externally visible through `AppError::Rule`
  and the RPC contract tests — keep the current `InvalidName` message or record a DEVIATION
  first; serde renames must be verified against `api/wire.rs` `TicketView`/`TicketReply` so the
  JSON stays identical. Effort: low.

## Upgrade 2 — Move the ticket aggregate into the domain
- Current: the canonical `Ticket` shape exists only in `app/ports.rs`; infra and testkit carry
  their own copies. The domain cannot enforce any aggregate invariant.
- Proposal: add `TicketId`/`TicketCommentId` via `opaque_id!` (keeping prefixes `tkt`/`tcm`),
  a `CommentAuthorKind { Member, Bot }` enum to replace the `member`/`bot` strings, and typed
  `Ticket`, `NewTicket`, `TicketComment`, `NewTicketComment` structs. Keep the `ports` structs
  and the store/wire shapes unchanged this release; convert at the app boundary.
- Why: one model, testable without Postgres; removes the private duplicate row types.
- Risks: cross-crate refactor touches usecases, both stores and testkit — convert, do not
  rename fields; tenant and archived-bot checks require IO and must stay in app/infra.
  Key `BOT-<n>` allocation and per-space sequence live in SQL and must not move down.

## Upgrade 3 — Typed, validating text fields so invalid states are unrepresentable
- Current: `validate_ticket_text` takes a raw `max` and a human label; a caller can pass the
  wrong bound, and empty description vs empty comment are enforced in two different places
  (description may be empty — tests create with `""`; comment emptiness is checked in app).
- Proposal: expose `validate_ticket_description` and `validate_ticket_comment` that use the
  constants internally, plus newtypes `TicketTitle`, `TicketDescription`, `CommentBody`
  constructed through validating constructors that centralize trim + bounds + control-char
  policy. Keep the existing free functions as thin wrappers for compatibility.
- Why: the bound is attached to the field it protects; rules get one home and unit/proptest
  coverage instead of being scattered across usecases.
- Risks: preserve the exact current semantics — title trims, text validators do not; `chars()`
  counts code points, not grapheme clusters; `\n`/`\t` are allowed today. Changing any of these
  alters what clients may submit (DEVIATION). Confirm against the original before tightening.

## Upgrade 4 — Decide and document transition policy explicitly
- Current: `tickets_move` permits all state pairs; no `can_transition_to` on `TicketState`
  (unlike `RunStatus` in `run.rs`).
- Proposal: either document all-pairs moves as the intended kanban rule with a
  `can_transition_to` that always returns `true`, or — only with approval — add a transition
  table mirroring `RunStatus::can_transition_to` / `transition_to`.
- Why: makes the state machine's contract explicit and testable rather than implicit in SQL.
- Risks: the spec (REQ-038) says nothing about restrictions and the RPC tests move
  `backlog → in_progress`; the safe default is today's all-pairs behavior (GUESS — record in
  `docs/STATE.md`). Any restriction is externally visible and needs `docs/PARITY.md` approval.

## Upgrade 5 — Lock the contract with unit + property tests
- Current: three unit tests cover state round-trip, title and comment bounds only; `parse`
  errors, serde shape, description bound, Unicode/control-char edges and empty-body behavior
  are untested; no proptest.
- Proposal: add `req_038_...`-named tests for every `as_str`/`parse` pair and rejection,
  serde round-trip, constructor boundaries (200/201, 4_000/4_001, 10_000/10_001 chars),
  newline/tab/combining-mark cases, and a `proptest` for the validators and `parse`.
- Why: protects the wire/DB contract while upgrades 1–4 move code between layers.
- Risks: if `chars()` diverges from the original's length rule, treat it as a parity finding
  and run the original as oracle; never "fix" silently.

## Suggested order
1 → 5 → 3 → 2 → 4 (transition policy last — the only behavior-visible change). Each step:
`cargo fmt --check`, clippy with `-D warnings`, domain and contract tests; record every
externally visible difference in `docs/PARITY.md` and wait for approval before shipping.
