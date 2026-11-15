# ADR-025 — Group bot-to-bot handoff: one Write-gated `delegate.bot`, one hop, room-routed

Status: accepted (2026-11-15, milestone M11.3b — REQ-010).

## Context

`spec/01-requirements.md` REQ-010 gives group chats two clauses:

1. a member addresses specific bots by mention and only those bots run (kept since M11.2);
2. a bot-to-bot request, when the receiving bot answers, its reply is attributed to that bot in
   the group thread and the requesting run continues with the result.

Until this slice, clause 2 had no mechanism (M11.2 recorded the gap; nothing was inventable
silently). The naive fan-out — "a bot's message mentioning another member bot queues a run" — is
implementable, but it does **not** satisfy the acceptance criteria: the requester has already
finished its turn by the time the mention lands, so there is no "requesting run continues with the
result" and no boundedness story. The spec's wording points at delegation with a resumption,
which the `delegate.helper` machinery (REQ-019, M2) already provides for the single-bot case.

## Decision

1. **`delegate.bot` is a builtin tool with `ToolEffect::Write`** — exactly the approval-card
   convention of `delegate.helper` (REQ-017/REQ-019). A member bot hands a task to **another
   member of its own group room** (addressed by name, case-insensitive, or by id). The call parks
   the requesting run; the human approves it like any other write; the effect then creates the
   child run. No new approval semantics, no trust bypass.
2. **One hop.** A bot that is itself answering for another bot (`is_helper()` or
   `is_bot_delegation()`) cannot delegate further; self-delegation and non-member targets are
   refused via `DomainError::DelegationRefused`. Loops and delegation storms are structurally
   impossible — the deepest call chain in a room is `room member → delegated member`, and it
   stops there.
3. **The receiver runs as a `BotMessage` child** with `parent_run_id` set, on the **shared
   room thread**, attributed to itself. Its visible reply posts as a regular bot message in the
   room (the transcript shows who said what); its structured outcome is relayed to the requester
   as a `role=tool` row on the same thread (hidden from chat, fed to the requester's own history
   exactly like a helper result) and re-queues the requester (`WaitingInput` → `Queued`).
4. **Every thread-resolving write path resolves the room now**, not just the API send: the
   worker claim, the mail inbound, held mail release, webhook wake, and routine wake paths all
   route a grouped bot into its room. Solo bots fall back to their own thread unchanged.
5. **Memory and model preference come from the parent run** — the delegate inherits the
   requester's model selection and the member acts on behalf of the same user, consistent with
   M3's decisions.

## Alternatives considered

- **Mention fan-out on bot messages** (auto-enqueue runs for mentioned member bots, no parent
  relay): covers "bots chat with each other", fails the AC verbatim (the requester cannot
  continue with the result), invites ping-pong between bots without a depth bound, and hides
  attribution behind plain fan-out. Rejected as the *mechanism for clause 2*; the fan-out for
  human mentions (M11.2) keeps working for the social case.
- **A new approval-free effect for handoffs**: breaks the M2 rule that writes park for approval.
  Rejected — if cross-bot asks deserve free execution, that is a spec decision, not a tool's.
- **Arbitrary-depth recursion with a cycle detector**: more machinery (budget, tracing across
  runs) for no spec'd benefit. One hop is the smallest useful and safe bound.
- **Delegate through the existing helper instead of a sibling** (helper acts, room stays quiet):
  REQ-010 clause 2 asks for attribution in the room; the helper writes `role=tool`, not a bot
  message. Rejected as not answering the requirement.

## Consequences

- Cross-bot asks produce an approval card by default: a human sees every handoff. Consistent
  with every other Write-class effect (and with REQ-019), but expect more cards in busy rooms.
- The room is now the bot's conversation everywhere messages land — previously impossible states
  (bot reply in the private thread while the room shows the question) are structurally gone.
- Relay rows carry the child's attribution (`bot_id`, `run_id`); REQ-019's `helperRunId`
  name is kept for wire compatibility.
- Fan-out *delegation* is deliberately shallow. If a future REQ asks for deeper collaboration,
  it is a new ADR with a budget and depth config, not an edit of this one.

## Evidence

`crates/testkit/src/build.rs::group_routing_tests::req_010_a_bot_delegates_to_a_sibling_bot_and_the_parent_continues`
(park → approve → child run attributed → relay → requester re-queued → final answer in the
room), `req_010_delegation_to_a_non_member_is_refused`, `req_010_a_bot_cannot_delegate_to_itself`,
`req_010_nested_delegation_is_refused`; CI run 43750818272 (all jobs) on `522f77f`.
