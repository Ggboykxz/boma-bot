# 0104 · apps/web/components/thread/thread-screen.tsx · LENS=reliability · v0.4.0 proposal

## Scope note

One client component owns three states (bot, transcript, live stream) plus a dozen UI flags. Two
effects fetch (`botsGet`/`threadsMessages`/`threadsHead`/`botsList`, then `threadsSubscribe`), and
five mutations (`threadsSend`, `threadsReact`, `threadsSnooze`/`Unsnooze`, `threadsAnswer` via
`MessageBlocks`) write back into local state. This reviews the client only: server lease/replay
semantics are out of scope, but every race below can make correct server state *look* wrong.
CONFIRMED as read from this module, `lib/api/rpc.ts`, `lib/run-status.ts`, `message-blocks.tsx`;
runtime impact is GUESS until the tests in "Success criteria" exist. Focus: SSE gap detection,
stale runs, optimistic approval, scroll drift.

## Current behavior (as read)

- Load effect sets `lastSeq.current` from the fetched page and fires `threadsMarkRead` fire-and-forget,
  ignoring its reply. Stream effect starts only at `phase === "ready"`, passes `afterSeq: lastSeq.current`.
- `onFrame` advances `lastSeq = Math.max(...)` for every frame, applies messages by id, and on a
  payload it cannot normalize replaces the *entire* list with `threadsMessages`. `run` events are
  cast to `{ status }` and overwrite one global `runStatus` string.
- `onClose` sets `offline`, retries `1s/2s/3s` up to `RECONNECT_ATTEMPTS = 3`, then stops forever.
- `answeredCards` is a local `Set` updated from `MessageBlocks.onCardAnswered`; `reload()` resets
  neither it nor `messages`, `runStatus`, `readSeq`, `replyTo`.
- The `role="log"` transcript has no scroll ref, no auto-scroll, no overflow owner of its own.

## Failure modes

1. **No SSE gap detection (silent holes).** `onFrame` accepts any `event.seq` and advances the
   watermark with `Math.max` (L170). A jump from seq 5 to 10 is treated as normal: events 6–9 are
   never flagged, and the watermark now hides them from any later resync. Whether `subscribe`
   replays from `afterSeq` is unspecified here (GUESS); if it is live-only, events lost during a
   disconnect are gone permanently and the transcript is quietly incomplete.
2. **Reconnect exhaustion.** After three failed reconnects (~6 s of linear backoff) `live` stays
   `offline` with no further retry, no focus/visibility resync, and no manual Reconnect. The screen
   still looks usable (composer sends) while the transcript and run state freeze indefinitely — the
   most dangerous state because it looks authoritative. Late/non-JSON frames are also dropped
   silently in `parseEventStream`.
3. **Fallback refetch clobbers concurrent frames + watermark.** When `normalizeMessage` fails, the
   `.then` does `setMessages(ordered)` (L177–183), replacing the whole list. A frame applied by
   `applyMessage` while that fetch was in flight is wiped, and the refetch never reconciles
   `lastSeq`, so the watermark and the visible list can disagree — compounding mode 1.
4. **Stale runs (no runId correlation).** `send` sets `runStatus = reply.status` (L288) and any
   `run` event overwrites it with a bare status string, ignoring any `runId` in the payload. With
   overlapping runs or batch sends (`SendReply.runIds`, REQ-010), a late `completed` for run A
   clobbers `running` for run B. Terminal states are never explicitly cleared, and if the final
   event is lost while offline the `AgentThinking` wave spins forever on a run that already ended.
5. **Optimistic approval with no server truth.** `awaitingApproval` is derived solely from card blocks
   minus the local `answeredCards` set. `CardBlock` unlocks on success and on `CONFLICT`, but
   `MessageView` card blocks carry only `{ cardId, kind }` — no answered flag — and `reload()` never
   resets the set. So a refetch or a lagging `message-update` can re-lock a card the server already
   committed (composer blocked forever), while an answer on another device never unlocks here.
   The gate is local optimism with no reconcile path in either direction (REQ-017).
6. **Scroll position drift.** The `role="log"` div has no scroll handling: new SSE/send messages
   never scroll to bottom, and full-list replacement (mode 3) or divider movement after reload —
   `threadsMarkRead`'s fresh `readSeq` is ignored so the divider jumps on next visit (REQ-009) —
   shifts content with no `scrollTop` restore. During `reload()` the skeleton *and* the old list
   render together (L462 + L476), growing then shrinking the log; the fabricated optimistic seq
   `lastSeq.current + 1` (L291) can reorder a row after the real event, adding another jump.
7. **Unrouted mutations / unmount residue.** `send`, `toggleReaction`, `toggleSnooze` pass no signal
   (their RPCs accept one), so late responses call `setMessages`/`setRunStatus`/`setAnsweredCards`
   on a dead tree and optimistic writes are never reconciled. `botIdFromPath() ?? botId` (L103) can
   disagree with the `botId` prop used by `send`, targeting a different bot on the static shell.

## Proposed hardening (v0.4.0)

- **A. Watermark + gap reconciliation.** In `onFrame`, ignore `seq <= lastSeq`, and if
  `seq > lastSeq + 1` record a gap and trigger one bounded resync that *merges by id* (reusing
  `applyMessage`), then advance `lastSeq` only to the highest contiguously seen seq. Risk: depends
  on server replay; if live-only, resync is the sole recovery — cap resyncs per window to avoid loops.
- **B. Reconnect resilience.** Replace fixed 3 tries with exponential backoff + jitter, reset the
  counter on any frame, gate reconnects on `visibilityState`, resync on focus, and add an explicit
  Reconnect control when `offline`. Risk: background reconnect storms — bound attempts and clear
  timers on unmount (the `stopped` flag already helps).
- **C. Correlate runs.** Stamp `runStatus` with the `runId` from `SendReply` and read `runId` from
  `run` events; let only the newest/active run drive the wave, clear on terminal, and add an offline
  expiry so a wave cannot spin forever. Risk: wire may not carry `runId` on events — flag to backend;
  degrade to single status + "last updated" rather than guessing.
- **D. Reconcile approval from server truth.** Add an answered flag to the card projection (backend)
  or re-derive `answeredCards` from `head`/`message-update` and clear it on `reload()`; treat
  `CONFLICT` as authoritative, never unlock on other failures. Risk: wire change — if blocked, at
  minimum reset the set on reload and surface "answered elsewhere" instead of silently re-locking.
- **E. Scroll anchoring.** Ref the log, auto-scroll only when already at bottom (stick-to-bottom),
  preserve anchor across full-list replacement and divider movement in a layout effect, and render
  skeleton **or** messages on reload (not both). Risk: `aria-live` chatter and CLS — keep `polite`,
  test at 360 px and RTL.
- **F. Reconcile optimistic send + cleanup.** Keep the optimistic message keyed to `reply.messageId`
  as a pending row and let the authoritative SSE event replace it; pass `signal` to all mutations,
  abort on unmount, and reset all stale flags in `reload()`. Risk: aborting a send the server applied
  yields "failed" locally — reconcile on the next resync, never tell the user it did not happen.

## Explicitly rejected / deferred

- Server idempotency keys for answer/subscribe replay — correct long-term fix, wire change; flag to backend.
- `useSWR`/React Query rewrite — larger dependency; local refs/version guards achieve the same reliability.
- Always-scroll-to-bottom — violates reading history; stickiness only (mode 6).

## Risks & rollout

Ship F, E, D (no contract change, lowest blast radius) first, then A/B (stream behavior), then C
(wire-dependent) last behind a flag. Preserve REQ-008/009/017 word-based, non-colour-only states and
the composer-stays-usable guarantee throughout.

## Success criteria

- Playwright: a seq jump (5→10) triggers exactly one bounded resync and fills the hole (REQ-005).
- >3 dropped reconnects recover on focus / manual click; no permanent frozen-offline state.
- A late `completed` for run A never overwrites `running` for run B; offline wave expires (REQ-001/010).
- An answered card stays unlocked after refetch/reload; another device's answer unlocks (REQ-017).
- New message scrolls to bottom only when at bottom; scroll up is preserved across refetch/divider move.
- Reload renders skeleton or list, never both; unmount mid-send produces no state update (REQ-020).
