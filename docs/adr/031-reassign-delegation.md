# ADR-031 — Reassigning a delegation inside a group room

## Context

REQ-068 requires an operator to move a working delegation to another member of the same room.
The existing delegate tool (`delegate.bot`) is model-initiated and one-hop; there is no
operator-facing seam, and the web room has no control for it. Issue #3's first slice delivered
pause/resume; this ADR covers the reassign slice.

## Alternatives

1. **Server-side `groups.reassign` usecase (chosen).** Explicit RPC; validate the group and both
   member bots; cancel the old run through the existing boundary-aware path; create the new run
   with the same `source_message_id`, context and `parent_run_id`; append a `System` message in the
   room. Fully meets the acceptance criteria and keeps authorisation server-side.
2. UI cancel + `@bot` resend. Cheaper, but the old run stops immediately (not at a tool boundary)
   and the room sees a new user message instead of a hand-off — a visible deviation from the spec.
3. Ask the model to delegate again. No operator control; a refusal or a loop is unpredictable.

## Decision

Adopt (1). No new run trigger: `BotMessage` with `parent_run_id` is already the delegation shape.
The group reply exposes the active run so the room never guesses which run to reassign.

## Consequences

- New RPC + wire type + usecase tests, two Playwright tests, spec contract and export hash updates.
- Old run is cancelled before the new one is created; a failed creation leaves the old run stopped
  with a legible error — acceptable, since a half-reassigned task is worse.
- The hand-off message is a `System` block in the room transcript, so it is visible to every member
  and survives reload (no ephemeral toast).
