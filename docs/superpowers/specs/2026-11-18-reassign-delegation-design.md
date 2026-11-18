# Design — reassign a delegated task in a group room (REQ-068, 2026-11-18)

## Intent

The owner approved issue #3's second slice: an operator must be able to move a working delegation
from one member bot to another inside the same group room, with the room showing the hand-off.

## Goals / non-goals

- Goal: spec REQ-068 acceptance criteria — same room, member bots only, old delegate stops at its
  boundary, new delegate receives the same context, room attributes the move.
- Non-goals: cross-room reassignment; reassigning to a non-member; a new run trigger variant;
  a model-mediated re-delegation; any hosted service.

## Approaches

1. **Server-side `groups.reassign` usecase (chosen).** Validates membership and room scope, cancels
   the old run through the existing boundary-aware cancel, creates the new run with `BotMessage`
   trigger and `parent_run_id` pointing at the old run (the same shape delegation already uses),
   and appends a `System` message in the room attributing the move. Full spec compliance; touches
   app/api/domain-contract/web with tests at each seam.
2. UI-only cancel + `@bot` resend (rejected): loses boundary semantics and the hand-off message.
3. Ask the model to re-delegate (rejected): not operator-controlled.

## Interfaces

- `reassign_delegation(app, context, group_id, run_id, to_bot_id) -> ReassignOutcome { run, message }`
  in `crates/app/src/usecases.rs`.
- `groups.reassign` RPC: `{groupId, runId, toBotId}` (strict, deny unknown fields) →
  `{runId, messageId, status}` in `wire.rs`.
- `groupsGet` reply gains `activeRun: {id, botId, status, promptSnippet} | null` so the room can
  offer the action against the run actually working there, without inventing a thread filter.
- Web: `groupsReassign(...)` wrapper; a BoardUI `Select` of the other member bots plus a Reassign
  button in the group room.

## Constraints

- Approval gates and tenant scoping unchanged; unknown group / non-member / cross-thread run are
  refused with the stable error envelope (not found / rule).
- New REQ-named tests (`req_068_*` in Rust, `REQ-068 …` in Playwright).
- `spec/03-api-contract.md` and `spec/EXPORT.md` refreshed with the new procedure and hash.
- BoardUI semantic tokens only; copy through the three catalogues.
