# 0046 · apps/web/components/activity/activity-screen.tsx · LENS=reliability · v0.4.0 proposal

## Scope note

The module is one client component: a single `runs.list` fetch keyed on `reloadToken`, one local
`RunView[]` cache, one global `phase` (`loading | ready | error`), and three mutations (`runsPause`,
`runsResume`, `runsRetry`) whose only persistence is a `setRuns` write-back matched by run id.
REQ-011/REQ-012 require every row to name its state in words and only `failed` to be retryable;
`run-status.ts` supplies the vocabulary. This reviews the client cache only: the server's lease and
fence semantics are out of scope, but every race below can make *correct* server state look wrong.
CONFIRMED as read from this module, `lib/api/rpc.ts` and `lib/run-status.ts`; runtime impact is GUESS
until the tests in "Success criteria" exist.

## Current behavior (as read)

- `useEffect([reloadToken])` makes an `AbortController`, calls `runsList({ limit: 50 }, signal)`,
  sets `runs`/`phase`, and aborts on cleanup; older responses are guarded by `signal.aborted`.
- `reload()` sets `phase = "loading"` and bumps the token. It never clears `runs`.
- The only reload control is the **error** banner's Retry button: no refresh in the ready state, no
  polling, and no `runs.subscribe` in `rpc.ts` (only `threads.subscribe` exists).
- `control(id, action)` is declared *inside* the row map; on success it merges `reply.status` into
  the matching row, on failure it swallows. `busyRun` and `retrying` are two separate global strings.
- `retry(runId)` awaits `runsRetry`, ignores its `{ ok }`, never touches `runs` or `phase`, and
  swallows every error. `rpc.ts` documents the procedure as M1 `not found` until M2 leases land.

## Failure modes

1. **Stale pause/resume write-back.** `control` merges `reply.status` by id with no generation or
   version check. A pause response delayed past a newer server transition (worker leases the run,
   it completes, or `groupsReassign` / REQ-068 moves it) writes `paused` back over `completed`, so the
   row re-grows a Resume button for a finished run. The same happens when a reload lands between the
   request and its response: the mutation and the fetch race with no defined ordering (last write
   wins). This is the core reassign race — a stale client beats fresh server truth.
2. **Double-submit window.** `disabled={busyRun === run.id}` is driven by async `setBusyRun`; two
   clicks in one React batch (or Enter + click) can both pass the guard and fire two `pause`/`resume`
   calls. Same for `retrying`. The server may be idempotent, but the second call can also return
   `CONFLICT` and the UI shows only the first response's status.
3. **No optimistic transition, no rollback path.** Pause/resume waits for the round trip with the
   button merely disabled; retry performs no state change at all. On a slow or hung link the row is
   frozen: nothing tells the operator the command was accepted. Conversely, there is no snapshot to
   roll back to, so a late failure simply leaves the operator guessing (and a late *success* is
   indistinguishable from no-op). Introducing optimism without a snapshot + restore-on-failure
   would turn mode 1 into a worse, user-authored version of the same bug.
4. **Retry is a silent no-op.** Because `runsRetry` is M1 `not found` and the `catch` is empty, every
   click produces a spinner, then the same `failed` row. `403`, `CONFLICT`, `TOO_MANY_REQUESTS` and
   `INTERNAL` are equally invisible (only `401` is handled globally via `call`). No rollback, no
   message, no refetch, no recovery cue.
5. **Live-update gap.** Statuses are fetched once per mount; `queued → leased → running →
   completed/failed` transitions are never observed. There is no ready-state refresh, no polling,
   no stream, and no `visibilitychange`/focus handling. `limit: 50` discards `nextCursor`, so runs
   beyond 50 are permanently invisible. A user parked on Activity sees a stale snapshot indefinitely.
6. **Stale rows presented as current.** `reload()` sets `loading` but leaves `runs` populated, and
   the error phase renders the old `<ul>` below the error banner. A failed reload therefore shows
   old statuses with no staleness marker — the most dangerous state, because it looks authoritative.
7. **Unmount and timeout leaks.** `runsRetry`/`runsPause`/`runsResume` are called without a signal
   even though all three accept one; nothing aborts in-flight mutations on unmount, so late
   `setRuns`/`setBusyRun`/`setRetrying` fire against a dead tree. `rpc.ts` has no client timeout, so
   a hung request pins `busyRun`/`retrying` forever and the row can never be acted on again.
8. **Closure captures render-time status.** `control` closes over `run.status` from the render that
   created it, deciding pause vs resume at click time. If a poll lands first, the in-flight action
   no longer matches the button the operator pressed.

## Proposed hardening (v0.4.0)

- **A. Generation guard on write-back.** Keep a `dataVersion` ref incremented on every successful
  list fetch; stamp each mutation with the version it started at and discard its `setRuns` if the
  version changed. Cap it with a per-run `updatedAt`/status check so a stale reply can never
  downgrade a newer row. Risk: a rejected legitimate reply is silently dropped — surface a
  "view changed, status refreshed" hint rather than nothing.
- **B. Synchronous in-flight lock.** Replace the two strings with a `useRef<Set<string>>` checked and
  populated before the async call, plus a single `busy` map (`{ [id]: "pause" | "resume" | "retry" }`)
  for rendering. This closes the batch double-click window without relying on React state timing.
  Risk: a crashed request must always release the key in `finally`, and the ref must be cleared on unmount.
- **C. Optimism with explicit rollback.** On pause/resume/retry, apply the expected target status
  immediately, snapshot the prior `RunView`, and restore it on failure *only if* the row has not
  since changed (pair with A). On success apply `reply.status` (or force `queued` for retry). This
  makes the action feel immediate without reintroducing the stale-write bug. Risk: optimistic states
  can flicker on high latency, and rollback can mask a server-side change made by another actor —
  mitigate by merging the snapshot with the latest row, never replacing it wholesale.
- **D. Make retry honest.** On M1 `not found` (or any failure) restore `failed`, show a per-row
  inline error via `RpcError.code` (a translated key, not a raw string), and offer a manual refresh.
  Do not keep a spinner that resolves to nothing. Risk: requires i18n keys and a small error slot per
  row; keep it to one line and reuse existing `activity.*` keys where possible.
- **E. Live updates with backoff and focus awareness.** Poll `runsList` on a modest interval with
  `document.visibilityState` gating, skip when a mutation is in flight for that run, and resync on
  focus. If/when a `runs.subscribe` procedure exists, prefer it over polling. Request `nextCursor`
  pages or page them so run 51+ is reachable. Risk: polling amplifies load and can starve mutations —
  use an AbortController per tick, clear on unmount, exponential backoff on error, and no polling in
  the error phase.
- **F. Honest staleness in loading/error.** Keep rendering stale rows only with an explicit
  "last updated …" / `aria-busy` marker, or clear them on reload. Decide one and test it; today the
  error banner plus a fresh-looking list is contradictory. Risk: clearing rows causes layout jump —
  keep the skeleton already present for `loading` and show the error panel alone for `error`.
- **G. Abort + timeout all mutations.** Store mutation controllers in a ref, pass `signal` to all three
  RPCs, abort them in the unmount cleanup, and add a client-side timeout that releases the lock and
  reports failure. Risk: aborting a request that the server already applied yields "failed" locally
  with a real server change — reconcile on the next poll (E), never claim the command did not happen.
- **H. Accessibility of live changes.** Give the status cell a considered `aria-live` policy so
  polled transitions are announced without flooding; keep status in words (REQ-012). Risk: too
  chatty in a screen reader — use `aria-live="polite"` and throttle announcements to the visible viewport.

## Explicitly rejected / deferred

- Server-side idempotency keys for pause/resume — correct long-term fix but a wire contract change; flag to backend, do not block v0.4.0.
- `useSWR`/React Query rewrite — larger dependency; the ref/version guards achieve the same reliability locally. Revisit if E grows complex.
- Client-side retry success while the server returns `not found` — that would be a lie; wait for M2.

## Risks & rollout

Ship B, D, F, G first (no contract change, low blast radius), then A/C together since they touch the
same write-back path, then E (highest regression risk: load, flicker, screen-reader noise) behind a
flag and measure it. Everything must keep the REQ-011/REQ-012 word-based status rendering intact.

## Success criteria

- Playwright (`REQ-011`/`REQ-012` suite): delayed pause reply + concurrent reassign never re-shows a
  Resume button on a completed row; double-click fires one RPC; failed retry restores `failed` and
  shows a message; unloading mid-mutation produces no state update; polled completion changes the row
  without a manual reload; a run past index 50 is reachable; error phase never shows unmarked old rows.
