# 0023 — `/app/g/[groupId]/page.tsx` (group room) — architecture lens

Target: `apps/web/app/app/g/[groupId]/page.tsx` (25 lines, static-export shell) + the client
it mounts, `components/group/group-thread-screen.tsx` (260 lines; all architecture lives there).
Spec: spec/08-ui-ux.md §8.3 ("Centre — the thread", group variant of REQ-010), REQ-010, ADR-013
(static export). Claims: CONFIRMED = read in this tree; GUESS = inferred, needs verify.

## Current structure (read, not run)

- Page: async server component, `generateStaticParams()` returns only `group_golden`, renders
  `<AppShell><GroupThreadScreen groupId={groupId} /></AppShell>`. No `error.tsx` / `not-found.tsx`
  siblings (CONFIRMED).
- Client screen: `useState` phase machine (`loading | ready | unavailable | error`), fetches
  `groupsGet` → then `groupsMessages({ threadId })`, stores the whole page reversed into state;
  sends via `threadsSend({ botId: memberBots[0] ... })`; reassign flow re-fetches get + messages
  after `groupsReassign` (REQ-068). No subscription, no cursor use, no key on the screen.
- BoardUI usage is token-clean (semantic surfaces/borders/type utilities, `rounded-3xl` cards,
  `@remixicon/react`, `cx()` not needed — no class merging); it does NOT reuse the app's thread
  primitives (`ThreadComposer`, `MessageBlocks`, `AgentThinking`, `ComposerLoader`) (CONFIRMED).

## Architecture findings

1. **Static-export id mismatch.** The shell is fed `groupId` from server `params`, but ADR-013
   pre-renders only `group_golden`; `ThreadScreen` solves the same problem by reading the id
   from `window.location` (`botIdFromPath`, `lib/bot-directory.ts`). The group screen has no
   equivalent `groupIdFromPath`, so on a direct load of any non-golden group (or one added later)
   the client mounts with the pre-rendered id, never re-reads the URL, and shows the wrong room
   or an empty one (CONFIRMED for the code path; severity depends on the export fallback, GUESS).
2. **No live sync.** The bot thread subscribes via `threadsSubscribe` with a reconnect budget
   (`RECONNECT_ATTEMPTS = 3`) and a seq-ordered `applyMessage` merge; the group room is
   fetch-once plus manual reload after send/reassign. Bot replies and other members' messages
   never appear without a reload, `activeRun` (the reassign banner) goes stale while a run is
   live, and there is no offline/reconnect reconciliation. Two copies of "load transcript" logic
   now diverge (GUESS: no `groups.subscribe` RPC exists as of this read —
   `lib/api/rpc.ts` shows only `groupsGet/List/Reassign/Messages`).
3. **Whole-history reload, O(n) every action.** `groupsMessages` accepts `cursor`/`limit` but
   the screen ignores both; every send/reassign re-fetches the full page and `[...page].reverse()`s
   it. No dedupe by id, no seq ordering, scroll anchoring is lost (scroll sits at top after
   reload; `logRef` is held but never used to scroll). Spec §8.3 wants a virtualised transcript
   with stable anchoring when older pages load.
4. **Send path hack + stuck state.** Messages post through `memberBots[0]` and rely on the
   server's mention fan-out (REQ-010 routing) — works, but the choice of "first bot" is
   arbitrary and the `if (!memberBot) { return; }` sits AFTER `setSending(true)` with no
   `setSending(false)`, leaving the composer permanently disabled when the member list is empty
   (CONFIRMED). The `catch` swallows send refusals silently (no `role="alert"`, no retry, unlike
   the reassign banner's error path). `clientNonce` uses `Math.random()` (fine for uniqueness,
   not for idempotency semantics).
5. **Duplication instead of composition.** GroupThreadScreen re-implements skeleton, unavailable
   screen, message list, attribution map and composer (raw `<input>`/`<button>`), all of which
   exist in the thread feature. It therefore also misses spec §8.3 composer features — mention
   `@bot` affordance, reply-to chip, stop-while-running, send-only-when-empty rule — and the
   transcript quality rules the bot thread has (scroll fade at the top edge, `aria-live` scoped
   to new rows rather than the whole `<ol>`, which `docs/ui-proposals/073-diplomacy.md` already
   flags as noisy for screen readers).

## Proposed upgrades for v0.4.0

1. **URL-first group id (parity with the bot room).** Add `groupIdFromPath()` mirroring
   `botIdFromPath()`; the client resolves `groupId` from `window.location` (with the pre-rendered
   param as SSR fallback), matching ADR-013. Keep `generateStaticParams` as-is plus the export's
   existing fallback; add `key={groupId}` at the page so route changes reset state.
2. **Shared live-sync layer.** Extract `useThreadLiveSync({ threadId, applyMessage, onResync })`
   from `thread-screen.tsx` (subscribe + 3-attempt reconnect + seq-ordered merge + resync on
   reconnect) and consume it in both screens; for groups, use `threadsSubscribe` on the backing
   thread if the server fans group traffic onto it, else a 2–5 s backoff poll of
   `groupsMessages`/`groupsGet` only while `activeRun` is set (CONFIRMED RPC lacks a group
   stream; polling is the safe default). Reassign banner and transcript both ride this path.
3. **Keyset pagination + incremental append.** Honour `cursor`/`limit`: initial page N, load
   older pages on scroll-up with scroll anchoring, dedupe by id, sort by `seq`, stop reversing
   the array. Replace `<ol>` re-render with the same list primitive the bot thread uses once
   virtualization lands; keep `role="log"` but scope `aria-live="polite"` to appended rows, and
   add the `scroll-fade` foundation for the top edge (BoardUI registry item).
4. **Composer parity + send hardening.** Reuse `ThreadComposer` with an `@bot` mention picker
   fed by `group.memberBots`; wire the stop control to the `activeRun` (REQ-068 adjacent); show
   send refusals via `role="alert"` with retry; fix the `setSending(true)` → early-return leak
   by moving the member-bot guard before the state flip (or into a `finally`); use
   `crypto.randomUUID()` for `clientNonce`.
5. **Failure states + route boundary.** Map RPC errors to `not-found | offline | server`:
   unknown group renders a dedicated empty state with a rail link; add `error.tsx` /
   `not-found.tsx` siblings to this route; add a `retryNonce` retry button; keep the skeleton
   but make it row-shaped like messages and keep the composer active per spec §8.3 "Loading".

## Risks / trade-offs

- **Live path backend gap.** `groups.subscribe` does not exist yet; the shared hook must
  feature-detect or poll. Polling adds load and can double-fetch after send — mitigate by
  invalidating (not re-fetching) on local send and deduping by id/seq.
- **DEVIATION risk.** URL-first id and polling fallback are behavior-visible; record in
  docs/PARITY.md (or STATE.md decisions) before shipping. Golden `group_golden` must be
  replayed unchanged; REQ-010 Playwright tests (`apps/web/tests/group.spec.ts`) gate the work —
  update them only via the spec process, never by weakening assertions.
- **Scope creep / regression in the single-bot room.** Extracting the shared hook touches
  `thread-screen.tsx` (REQ-001, REQ-005). Keep the extraction behavior-preserving and behind the
  existing tests; if it slips, ship fix #1 and #4 standalone — they are independent.
- **Scroll anchoring + virtualization** change what goldens see at mount (newest at bottom).
  Verify against spec §8.3 "newest at the bottom, stable scroll anchoring when older pages
  load" and the accessibility gate (keyboard scroll, no focus theft on resync).
- **BoardUI discipline.** Every new surface must stay on semantic tokens
  (`bg-background-primary-default`, `border-border-button-default`, `text-text-*`,
  `text-foreground-icon-*`), composite type utilities, `cx()` merges, and `@remixicon/react`;
  the hand-built `<input>`/`<button>` and reverse-chronology list must not be re-skinned into
  raw palette classes. No new design-system components — reuse BoardUI + thread primitives.

Files touched (planned): `app/g/[groupId]/page.tsx` (key, siblings), `group-thread-screen.tsx`
(rewrite around shared layers), `lib/bot-directory.ts` or new `lib/group-directory.ts`,
`lib/use-thread-live-sync.ts` (extracted), `thread-screen.tsx` (consume hook), plus tests.
