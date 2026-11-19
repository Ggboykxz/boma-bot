# 0068 · apps/web/components/group/group-thread-screen.tsx · LENS=reliability · v0.4.0 proposal

Worker #068 · module `apps/web/components/group/group-thread-screen.tsx` · lens: reliability ·
READ-ONLY review of `rebuild-workspace/.worktrees/before-m5` (v0.3.0-rc5). Proposal only; no code
changed here. Spec anchors: REQ-010 (group thread / mention routing), REQ-068 (reassign),
`spec/08-ui-ux.md §8.3`. Complements 0024 (route/screen overview) by going deep on three modes only.
Evidence read this session: the component (260 lines) and `lib/api/rpc.ts §M11.3` (`groups*`,
`threadsSend`, `threadsSubscribe`; all fetchers accept an optional `AbortSignal`, but `call()` sets
no timeout) and the `MessagesReply`/`GroupMessagesReply` shapes carrying `nextCursor`. Items marked
CONFIRMED were read directly; runtime behavior and server semantics are GUESS until exercised.

## Failure modes

1. **Fetch-once staleness** (CONFIRMED lines 49-78). One `groupsGet`→`groupsMessages` chain keyed on
   `[groupId]`, then never revalidated: no poll, no focus refresh, and no `threadsSubscribe` despite
   the SSE helper existing (`rpc.ts:621-677`). Consequences: bot replies, `activeRun` changes made by
   another member, and member rename/removal never appear; stale attribution falls back to rendering
   the raw internal `botId` (lines 181, 219); the send target is frozen at `memberBots[0]` (line 111),
   so a departed bot misroutes or fails. The catch lumps 404/403/5xx/network into one `unavailable`
   phase with only a back link — no Retry.

2. **Sending latch leak** (CONFIRMED lines 43, 80-130). `sending`/`reassigning` are single booleans;
   no `AbortSignal` is passed to `threadsSend`, `groupsMessages` or `groupsReassign`, and `call()` has
   no timeout. A hung fetch never reaches `finally`, so `sending` stays `true` and the composer is
   permanently disabled (`disabled={sending}`) — the latch leak. Same-tick double-submit can also
   read `sending === false` before the DOM `disabled` commits, issuing two RPCs; a fresh `clientNonce`
   is minted per call (line 118), so server dedup cannot collapse them (double message, double run).
   The `!memberBot` early return (112-114) keeps the draft with no feedback, and the empty catch
   (124-126) hides send refusals *and* post-send reload failures. In-flight work is not cancelled on
   unmount or `groupId` change, so a stale response can write another group's messages/draft.

3. **Whole-history reload** (CONFIRMED lines 62-69, 92-95, 121-123). Three sites call
   `groupsMessages({ threadId })` with no cursor/limit and replace state wholesale with
   `[...page.messages].reverse()`. `nextCursor` is ignored; the server answers newest-first in pages
   (`rpc.ts:1418-1422`), so if a page is capped, every send or reassign silently drops older rows and
   the transcript shrinks. Overlapping reloads (send + reassign, or two reassigns) resolve out of
   order: last *response* wins, not last *action*, so the just-sent row can vanish. Full-array replace
   re-renders the whole list, `logRef` is never used to restore scroll, so the reader's position jumps
   and `aria-live` announces rows they cannot see.

## Concrete hardening (v0.4.0)

- **A. Bounded freshness.** For the live path open `threadsSubscribe({ threadId, afterSeq: lastSeq })`;
  reconnect with jitter + max backoff on `onClose` while visible, pause when hidden. Fallback where
  SSE is unavailable: poll `groupsMessages` every 5-10s, paused on hidden, backed off on error. Merge
  rows by id/seq; never blind-replace. Keep the mount effect for initial load only. Risk: SSE POST
  support is CONFIRMED in `rpc.ts`, but `afterSeq` replay semantics are GUESS — verify in
  `spec/03-api-contract.md`; coordinate with the thread-screen proposal so one thread has one
  subscriber, else reconnection storms.
- **B. Per-action abort + version guard.** Give each effect/send/reassign its own `AbortController`,
  pass its signal to every RPC, abort on unmount and on `groupId` change, and add a timeout
  (`AbortSignal.timeout` with a timer fallback) so no latch can hang. Apply state only if still
  mounted and current (monotonic request epoch). Replace booleans with an operation id/`Set` so
  double-submit is impossible and `finally` clears only the matching op. Risk: `AbortSignal.timeout`
  feature support (detect); ensure `finally` still runs on abort.
- **C. Incremental merge, no full reload.** Keep `messages` as a seq-sorted array; after a send merge
  the row named by `SendReply.messageId` (or fetch only the tail) instead of refetching all history;
  dedup by id, sort by `seq` (present on `MessageView`, CONFIRMED), clamp length. Use `logRef` to
  snapshot `scrollTop`/distance-from-bottom before the state update and restore only when the reader
  was already at the bottom. If deeper history is needed, load older pages via `nextCursor` on
  scroll-up. Risk: an optimistic row must be reconciled with the server echo or removed on failure
  to avoid a ghost message.
- **D. Failure legibility.** Classify `RpcError`: 404/403 → `unavailable`; 5xx/network → `error`
  phase with a Retry token. Surface `sendError` as `role="alert"`, keep the draft, and reuse ONE
  `clientNonce` per draft (regen on edit/clear) so a retry dedups server-side. Empty `memberBots` →
  disabled composer plus copy, never a silent return; rebuild `attribution` on every `groupsGet`.
  Reuse 0024's H4 i18n keys. Risk: new strings/visible behavior → `docs/PARITY.md` and golden updates.
- **E. Hygiene.** Remove the unused `groupsList` import; put `aria-busy` on the form while sending;
  clear `reassignFailed` on the next attempt.

## Risks / non-goals

- **R1** Live streaming changes the traffic profile; measure p95 and battery, ship behind a flag.
- **R2** Seq-merge could resurrect a deleted row → server-authoritative replace on reassign,
  append-only for new messages.
- **R3** Nonce reuse depends on `threads.send` dedup being keyed per (botId, nonce) — GUESS until the
  contract is checked (also flagged in 0024 R3).
- **R4** No wire/spec/dependency changes; route/export and error-boundary fixes stay in 0024, shared
  streaming in the thread-screen proposals.

## Success criteria / tests (REQ-named, route-mocked Playwright)

- `REQ-010 bot reply from another session appears within one frame/poll with no local action`.
- `REQ-010 hung threadsSend times out, sending clears, composer stays usable, alert is visible`.
- `REQ-010 rapid double send issues one RPC with a reused nonce; draft survives a failed send`.
- `REQ-010 sending into a 50+ message thread keeps the oldest rows and preserves scroll position`.
- `REQ-068 reassign racing a send yields last-action-wins state; no writes after unmount/switch`.

CONFIRMED = read this session; all runtime outcomes and server-side semantics above are GUESS until
the tests run.
