# 0067 — GroupThreadScreen architecture proposal (v0.4.0)

Module: `apps/web/components/group/group-thread-screen.tsx`
Lens: architecture
Spec anchor: REQ-010 (group chat), `spec/08-ui-ux.md` §8.3.

Status of claims: items under "Current structure" are CONFIRMED by reading the file this session;
proposals and risks are GUESS (design inference, not yet implemented or tested).

## Current structure (CONFIRMED)

`GroupThreadScreen` is a single client component that co-locates five concerns:

- Data loading: one `useEffect` on `groupId` fetches `groupsGet`, derives an `attribution`
  map from `memberBots`, then `groupsMessages` and reverses into `messages`.
- Mutations: `reassign(toBotId)` and `send()` each re-fetch the whole transcript inline.
- View state: `phase`, `sending`, `reassigning`, `reassignFailed`, `text`.
- Rendering: header, active-run/reassign bar, `<ol role="log">` transcript, composer form.
- Dead surface: `logRef` (`useRef<HTMLOListElement>`) is declared but never read; `groupsList`,
  `GroupsListReply`, `MessageView` are imported but unused. `Phase` includes `"error"` which is
  never set; `"unavailable"` is terminal with no retry path.

All colour/type classes already use BoardUI semantic tokens and composite type utilities
(`text-text-*`, `bg-background-secondary-default`, `border-border-button-default`,
`text-title-3-medium`, `text-caption-1-semibold`, `rounded-3xl`), plus `boma-focus` /
`boma-target`. This must be preserved verbatim.

Sibling reference (CONFIRMED): `components/thread/thread-screen.tsx` already solves several of
these concerns for the single-bot case (SSE subscribe with bounded reconnect, `reloadToken` +
retry, `sendFailed` feedback, extracted `MessageBlocks`, `TranscriptSkeleton`, `ThreadComposer`).

## Target structure for v0.4.0

Split into a data hook + thin view, mirroring the thread screen without copying its bot-only logic:

```
group-thread-screen.tsx        // route shell: calls hook, renders states, owns group header
hooks/use-group-thread.ts      // load, live stream, mutations, derived attribution, retry
group-message-list.tsx         // <ol role="log"> rows, scroll management, ScrollFade
group-reassign-bar.tsx         // active-run controls + error
```
Keep the existing file as the public export so the route imports do not change.

## Concrete upgrades (3-5)

1. **Extract `useGroupThread(groupId)` and one `reloadMessages()` (GUESS).** The identical
   `setMessages([...page.messages].reverse())` appears three times; centralise it in one
   `useCallback` that takes an `AbortSignal` and is reused by load, reassign and send. Run the
   initial `groupsGet` + `groupsMessages` in `Promise.all`-style parallel where the spec allows.
   *Risk:* changing fetch ordering could alter which message ids/goldens assert first; replay all
   `spec/golden/ui/*.json` before merging. Keep the newest-at-bottom order identical.

2. **Add live group delivery via `threadsSubscribe` (GUESS).** A group chat whose transcript only
   refreshes on local actions cannot show another member's bot reply — the defining REQ-010
   behaviour. Subscribe to the group thread with `afterSeq`, dedupe/merge by message id, and use a
   bounded reconnect (`RECONNECT_ATTEMPTS`, linear backoff) exactly as the thread screen does.
   *Risk:* needs a thread-level subscribe the Rust backend must expose; confirm the endpoint in
   `spec/openapi.yaml` first. If absent, record a DEVIATION/STATE question and ship a manual
   refresh affordance instead of inventing a wire contract.

3. **Replace the silent send with observable outcomes (GUESS).** Today a failed `threadsSend`
   is swallowed and text stays in the composer with no cue; `memberBots[0]` is chosen arbitrarily
   while the comment claims mention routing happens server-side. Add a `sendFailed` state + retry
   (mirroring the thread screen), generate `clientNonce` once per attempt, and stop posting through
   an arbitrary member if the spec defines a group sender. *Risk:* visible error copy is a new
   user-facing string — route it through `useTranslation` and add keys; do not hardcode.

4. **Make transcript rendering shared and complete (GUESS).** The row currently renders only
   `block.type === "text"`, dropping card/non-text blocks that the thread screen shows via
   `MessageBlocks`; reuse `MessageBlocks` (or extract a common `MessageRow`) so group messages do
   not silently lose content. Wire the now-dead `logRef` to auto-scroll on new messages, and apply
   the `ScrollFade` recipe to the transcript wrapper. *Risk:* `MessageBlocks` may assume bot-only
   fields; guard for `GroupMessageView` shape and verify axe `list` rules with `role="log"` kept on
   a real list element.

5. **Add a real retry/error phase (GUESS).** Use the `reloadToken` pattern so a transient load
   failure reaches `phase === "error"` with a retry button instead of the dead-end `unavailable`
   screen; keep `unavailable` only for a confirmed missing group. *Risk:* extra state transition
   changes loading snapshots — update Playwright/golden assertions with REQ-010 test names.

## Constraints

- Keep every BoardUI semantic token and composite type utility; no raw palette/hex, no `dark:`
  colour overrides, merge classes with `cx()` from `@/utils/cx`.
- Keep all user-visible text behind `useTranslation`; add keys rather than literals.
- Preserve `role="log"` + `aria-live="polite"` and logical-property/RTL readiness.
- No new design system; reuse installed `components/base` and `components/application` items.
