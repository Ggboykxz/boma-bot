# ADR-033 — A question card must not silently approve on an empty answer

## Context

`CardBlock` renders `question` cards as a free-text field whose submit handler currently calls
`threads.answer` with `draft.trim() || "approve"`. An empty field is therefore recorded as an
approval, without the member choosing one. Swarm brief 0029 (OpenBot-inspired, data only) flagged
this as a REQ-017 gap; issue #21 tracks it. Approvals are the only place an external effect is
authorised, so an implicit one is a safety defect.

## Alternatives

1. Keep the silent default (rejected): backwards compatible but an operator can authorise an
   effect without deciding; contradict REQ-017's explicit approval gate.
2. **Reject an empty answer in the client (chosen)**: disable submit while the field is empty and
   show a catalogue hint; no `threads.answer` call is made. The Approve button on `approval` and
   `purchase` cards stays the explicit way to approve.
3. Add a separate "approve anyway" confirm (rejected for this slice): more copy, more UI, and a new
   interaction to design; it can be added later if operators ask for a one-click question answer.

## Decision

Adopt (2). No wire or schema change: `threads.answer` still takes `{cardId, answer}`; the client
simply never sends an empty answer. The change is UI-level and REQ-017 named tests pin it.

## Consequences

- A question card with no text cannot be answered; the operator sees an inline reason and keeps
  focus on the field. This is a visible behaviour change, recorded in `docs/STATE.md`.
- The seeded REQ-007 pending-card golden was updated: the disabled "Send answer" control replaces
  the enabled "Approve" affordance, with an added assertion that no "Approve" button exists on a
  question card. A new `REQ-017` Playwright test pins the no-call rule.
- Approval/purchase cards are unchanged: their explicit Approve/Deny path is the only one.
