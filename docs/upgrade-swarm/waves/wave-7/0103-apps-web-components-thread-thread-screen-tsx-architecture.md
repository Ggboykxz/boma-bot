# 0103 — ThreadScreen architecture proposal (v0.4.0)

Module: `apps/web/components/thread/thread-screen.tsx` (614 lines, `"use client"`)
Worker #103 · lens: architecture · READ-ONLY review of `before-m5` (v0.3.0-rc era).
Proposals only; no code changed. Line numbers cite the before-m5 worktree.
Spec anchor: §8.3 (REQ-001, REQ-005, REQ-020) plus REQ-008/009/017; brief 011 flagged this
file as the largest single component in `apps/web`.

Status: "Current structure" and "Architecture gaps" items marked CONFIRMED were verified by
reading the file and `lib/api/rpc.ts` this session; upgrades and risks are GUESS (design
inference, not implemented or tested).

## Current structure (CONFIRMED)

- One component owns ~15 states, 2 refs, 4 callbacks and 3 effects. Transport/domain state
  (`bot`, `messages`, `answeredCards`, `phase`, `live`, `runStatus`, `readSeq`, `unreadCount`,
  `snoozed`) is interleaved with ephemeral view state (`draft`, `sending`, `sendFailed`,
  `replyTo`, `pickerFor`, `reloadToken`). The doc comment claims "three pieces of state";
  the real surface is much wider, with no seam between use case and pixels.
- Load effect (101-146): `botsGet` -> `registerBot` -> `Promise.all(threadsMessages,
  threadsHead, botsList)` -> reverse, compute `lastSeq`, mark read. It resolves the id as
  `botIdFromPath() ?? botId`; `send()` (264-302) posts to the raw `botId` prop.
- SSE effect (150-222): `threadsSubscribe` at `afterSeq: lastSeq.current`, id-based merge via
  `applyMessage` (sorts by seq), full re-fetch fallback for unparseable events, bounded
  reconnect (`RECONNECT_ATTEMPTS = 3`, linear 1s/2s/3s), `offline` banner on close.
- Derived logic lives in the render body: unread divider (306-308), run label/activity
  (309-312), approval gate (315-322). `TranscriptSkeleton` and `EmptyThread` are already
  extracted subcomponents — the extraction pattern to follow.
- `SubscribeHandlers` (`rpc.ts:614-619`) exposes only `onFrame`/`onClose`; the stream is a
  POST-backed `fetch` reader, not `EventSource`, so retry policy lives in this component.

## Architecture gaps

- `live === "live"` is never assigned anywhere: the union has a dead member and the UI can
  only show "connecting" or "offline". Reconnect `attempts` is never reset after a healthy
  frame, so three closes across a long session permanently disable reconnect. (CONFIRMED)
- No React Context exists in `components/thread`; sibling panels (`ExternalAgentPanel`,
  `NotificationsPanel`, `ComputerPanel`) can only get room state by prop-drilling from this
  parent. One monolithic context would also make every SSE frame re-render all consumers —
  hence the deliberate data/action split below. (CONFIRMED code, GUESS consequence)
- `onFrame` fallback replaces the whole transcript, which can drop an optimistic just-sent
  message and races with a second frame-triggered re-fetch. Handlers are untestable without
  DOM + route interception. (CONFIRMED code, GUESS consequence)
- `reload()` does not clear `messages`/`runStatus`/`readSeq`, so the skeleton renders beside
  a stale transcript: "loading" is not an exclusive state. (CONFIRMED)
- `send()` and load disagree on which bot id is authoritative (prop vs path). (CONFIRMED)

## Target structure for v0.4.0 (GUESS)

```
thread-screen.tsx                     // thin shell: hook + context providers + layout
hooks/use-thread-room.ts              // load, SSE, mutations, derived values (no JSX)
components/thread/thread-room-context.tsx // split data/action contexts + typed selectors
lib/thread-room-state.ts             // pure: mergeMessage, unreadDivider, runLabel, gate
```

Keep `thread-screen.tsx` as the public export — imported by `app/app/[botId]/page.tsx`,
`app/app/g/[groupId]/page.tsx` and `components/shell/app-entry.tsx`; no route changes.

## Concrete upgrades (in dependency order)

1. **Extract `useThreadRoom(botId)` (GUESS).** Move load/reload, `applyMessage`, send,
   reaction/snooze toggles and run/unread/approval derivations into one typed hook; move pure
   helpers (id-based merge, newest-at-bottom sort, divider, composer gate) into
   `lib/thread-room-state.ts` for DOM-free unit tests.
   *Risk:* drift on the optimistic-send sequence and the WCAG two-`<ul>` separator; move
   first, redesign never, keep i18n keys/attributes fixed; replay REQ-001/005/008/009/017
   Playwright and goldens before merging.

2. **Split context into data vs actions (GUESS).** Provide `ThreadRoomDataContext` (`bot`,
   `messages`, `phase`, `live`, `runStatus`, divider/approval flags) and
   `ThreadRoomActionsContext` (`send`, `toggleReaction`, `toggleSnooze`, `replyTo`, picker
   setters) from the shell; descendants use `useThreadRoomData()` / `useThreadRoomActions()`.
   Memoize each provider value; never put the whole hook return in one context.
   *Risk:* indirection with few consumers today — scope to panels that truly need room
   state, keep the transcript props path for `MessageBlocks`, and profile that SSE frames do
   not re-render the composer.

3. **Harden the SSE loop (GUESS).** Set `live = "live"` on the first valid frame and reset
   `attempts` there; replace lifetime retry counting with a bounded window, exponential
   backoff and jitter; resume from the last *message* seq only (do not advance the cursor on
   `run` frames); ignore duplicate/out-of-order seq; cap and coalesce the fallback re-fetch
   so it cannot overwrite newer local state; abort on unmount/thread change.
   *Risk:* resume semantics depend on the Rust `threads.subscribe` contract — confirm seq is
   shared across event types in `spec/openapi.yaml`/wire before changing the cursor; else
   file a STATE question and keep the current cursor with dedupe only.

4. **Make load an exclusive state machine (GUESS).** Replace `Phase` + scattered flags with
   one union (`idle | loading | ready | error`) and one `reload()` that clears transcript,
   run and read state, using a single id resolution (`botIdFromPath() ?? botId`) everywhere
   including send.
   *Risk:* loading goldens may assert the stale transcript; update tests explicitly, never
   weaken them; keep the composer usable while offline per §8.3.

5. **Test seams and perceivable status (GUESS).** Unit-test pure helpers with vitest; feed
   fake frames to `onFrame`/`onClose` to test reconnect, backoff and dedupe; expose `live`
   transitions through an `aria-live` status, not visuals alone.
   *Risk:* new test deps need a DEPENDENCIES.md entry; any new user-visible copy needs i18n
   keys, not literals.

## Execution order & gate
U1 -> U2 (one structural PR) -> U3 (contract-checked) -> U4 -> U5.
Every step: REQ-001/005/008/009/017 Playwright green, axe AA clean, goldens replayed,
BoardUI semantic tokens only, `cx()` merges, `@remixicon/react` refs, all copy via
`useTranslation`, `role="log"` + `aria-live="polite"` and the two-list WCAG shape intact.

## What not to touch
The public export/path; per-attempt `clientNonce` generation; `thread.*` i18n keys;
`boma-focus`/`boma-target` rings; `MessageBlocks` card-answer flow (REQ-017); snooze duration
and roster refresh (REQ-009); role/seq ordering asserted by goldens.

CONFIRMED by reading thread-screen.tsx and rpc.ts:608-677; risk notes GUESS until U1-U5 land
and REQ tests replay.
