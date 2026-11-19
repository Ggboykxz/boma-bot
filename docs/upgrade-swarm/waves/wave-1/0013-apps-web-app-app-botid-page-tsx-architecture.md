# 0011 — app/app/[botId]/page.tsx — architecture (v0.4.0)

Worker #011 · module `apps/web/app/app/[botId]/page.tsx` · lens: architecture · READ-ONLY
review of `rebuild-workspace/.worktrees/before-m5` at v0.3.0-era source. Proposals only;
no code in this change.

## What the file does — and where the real mass lives

`page.tsx` itself is 36 lines and correct for ADR-013 (Next.js static export): a server
component that awaits `params`, renders `<AppShell><ThreadScreen botId={botId} /></AppShell>`,
and emits `generateStaticParams()` for `bot`/`bot_golden` with the `/app/*` static-handler
fallback (ADR-013) for every other bot. The architecture problem is not this file; it is
that the file hands a "room" to a client component that does everything itself.

Observed shape of `components/thread/thread-screen.tsx` (614 lines, `"use client"`):

- **One component, four jobs.** `ThreadScreen` owns (a) initial load (`botsGet` →
  `threadsMessages` + `threadsHead` + `botsList` in parallel, `abort` wiring), (b) the live
  SSE stream (hand-rolled `fetch` POST to `threads.subscribe`, manual `parseEventStream`
  reader in `lib/api/rpc.ts`, `RECONNECT_ATTEMPTS = 3` with fixed 1s·n backoff), (c) all
  interaction mutations (send with optimistic `seq = lastSeq.current + 1`, reactions,
  snooze/unsnooze, reply-quote, unread divider computation, `answeredCards` gating of the
  composer), and (d) all markup (header, skeleton, error, empty state, divider, two `<ul>`s,
  composer). `renderMessageRow` is an inline closure re-created every render.
- **No context anywhere in the room.** `botId` is prop-drilled to `ComputerPanel`,
  `ExternalAgentPanel`, `NotificationsPanel`. Cross-cutting state lives in module-level
  singletons instead (`lib/bot-directory`, `lib/roster`, `lib/run-status`) — a second,
  invisible state layer beside React state/props, with no invalidation semantics.
- **Streaming is resilient but blind.** `applyMessage` upserts by `id` and re-sorts by
  `seq` (idempotent for duplicates), reconnect resumes from `lastSeq.current` — good. But
  there is no seq-gap detection (a frame `seq=9` after `lastSeq=6` silently loses 7–8), no
  backoff jitter/reset, no "catching up" affordance distinct from "offline", and any
  unparseable `message` event triggers a **full transcript refetch** rather than one
  targeted read. There is no token/block-delta streaming at all — the UI only ever shows
  whole messages.
- **Cost profile.** Every frame → `setMessages` → full sort + full re-render of every row;
  no memoized row component, no list windowing; goldens count DOM rows, so the two-`<ul>`
  divider structure is load-bearing.

## Proposed upgrades for v0.4.0 (3–5, in dependency order)

### U1. Extract a `useThreadRoom` hook + reducer; make `ThreadScreen` a composition
Pull fetch/stream/send/react/snooze logic out of the component into one hook with a
`useReducer` state machine (`phase: loading|ready|error`, `live: connecting|live|offline|catching-up`,
`messages`, `readSeq`, `unreadCount`, `runStatus`, `answeredCards`). Split markup into
`ThreadHeader`, `TranscriptLog` (+ memoized `MessageRow`), `UnreadDivider`, `ComposerSlot`.
`page.tsx` stays a thin server wrapper; the room contract becomes explicit and testable.
- Risk: behavior drift against the UI goldens (REQ-001/005/008/009/017/020). Mitigate:
  golden replay + Playwright before/after diff; move first, redesign never, in one PR.

### U2. One scoped `ThreadRoomContext` (split data/dispatch), retire singleton sprawl
Provide `{ bot, threadId, live, runStatus, readSeq, dispatch }` from the hook; consumers
(`ComputerPanel`, `ExternalAgentPanel`, `NotificationsPanel`) read by context instead of
prop-drilling. Keep the context room-scoped (not global), split value vs. dispatch so
dispatch-only consumers never re-render on frame traffic. Fold `bot-directory`/`roster`
writes behind the same boundary; no new module-level mutable state.
- Risk: a single broad context re-renders every panel per SSE frame; and accidental
  global leakage. Mitigate: two contexts (`data`, `dispatch`), `useSyncExternalStore` for
  the roster if it must stay hot; panel-level `React.memo`.

### U3. Streaming client hardening: gap detection + bounded resync, targeted refetch
In `rpc.ts` client (or a thin wrapper): track `expectedSeq = lastSeq + 1`; on a gap, enter
a resync mode — one `threadsMessages` refetch, rebuild `lastSeq`, resume the stream from
there (never replay dropped frames). Replace linear `1s·n` with capped exponential backoff
(+jitter, reset on first good frame), cap reconnects, and surface `catching-up` distinct
from `offline`. Replace the full-page refetch on unparseable message events with a
single-message fetch when the event carries an id; keep full refetch only as fallback.
- Risk: resync loops if the server replays overlapping frames; reorder vs. the optimistic
  `seq = lastSeq+1` user placeholder can still flicker. Mitigate: keep `applyMessage`
  idempotency, assert seq monotonicity in the reducer, goldens + a forced-drop test.

### U4. Block-delta streaming for in-flight assistant turns (spec amendment, ADR required)
Today, a running assistant message appears only at commit time. Propose adding an optional
`message-delta` SSE event (`messageId`, block index, text/JSON delta, `done` flag) that
patches the message block in place, with the existing full-message upsert as the
termination/commit truth. This is an externally visible protocol change: it must land as
an ADR + spec edit with owner approval, and degrade gracefully (clients that ignore the
event type behave exactly as today).
- Risk: spec contract breach if shipped unapproved (DEVIATION in docs/PARITY.md);
  half-rendered markdown churn; duplicate deltas on reconnect. Mitigate: deltas are
  idempotent (block-versioned), reconnect restores last committed snapshot, never raw
  deltas; ship behind a capability flag from `/api/auth/capabilities`.

### U5. Virtualize the transcript (only after U1–U2 land)
Once rows are memoized `MessageRow`s with stable keys, add windowing above ~200 rows.
Keeps the two-`<ul>` divider contract visible to a11y tooling if done as "render window,
keep list semantics" (or drop virtualization if axe/goldens cannot be satisfied).
- Risk: WCAG 2.2 AA breakage (`list`/`listitem` rule), golden row-count assertions, lost
  `role="log"` announcement fidelity. Mitigate: windowing must preserve list roles and the
  `aria-live` region; if it cannot, defer U5 — it is an optimization, not a defect fix.

## Execution order & gate
U1 → U2 → U3 are one v0.4.0 theme (structure, then transport); U4 needs owner sign-off
(spec); U5 is optional tail. Every step gated by: UI goldens green, Playwright REQ suite
green, axe AA clean, bundle budget (docs/PLAN.md) not exceeded, `page.tsx` diff stays ≤ a
thin server wrapper.

## What not to touch
`generateStaticParams` + ADR-013 fallback (works, contractual); the `role="log"` +
`aria-live="polite"` live region; the two-list divider split (goldens depend on it);
mark-read-on-mount background call; clientNonce idempotency on send.

CONFIRMED by reading source; all risk notes GUESS until U1 lands and goldens are replayed.
