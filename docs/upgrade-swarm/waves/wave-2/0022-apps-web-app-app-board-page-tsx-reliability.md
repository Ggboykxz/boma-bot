# 0022 — `apps/web/app/app/board/page.tsx` — Reliability (v0.4.0)

Scope: `app/app/board/page.tsx` (16-line wrapper) — thin; reliability lives in
`components/board/board-screen.tsx` ("use client", REQ-038, spec §8.6) + `lib/api/rpc.ts`.
Read-only; no code changed.

## Today (CONFIRMED by reading)

Mount probes `boardsGet`/`botsList` behind an `AbortController` (unmount-only, no timeout).
Post-mutation `load()` re-fetches **without a signal**, no in-flight dedupe. Moves are **not**
optimistic; **no drag/drop exists today** — column moves are button-driven in the dialog and
wait for `tickets.move` before `setSelected` + `load()`. Refresh is manual-only.

## Failure modes

1. **Server-error ⇒ silent empty board.** Non-404 rejection maps to `phase="ready"` with
   `tickets=[]` (mount: `status === 404 ? "absent" : "ready"`; `load()`: same). A 500/outage
   renders an empty, zero-warning board — indistinguishable from "flag on, no tickets".
2. **Overlapping `load()` races.** Every mutation calls the same signal-less `load()`.
   `commentSelected` (`commenting`) and `moveSelected` (`busy`) run concurrently; their
   `boardsGet` calls resolve in arbitrary order — the older payload can overwrite the newer.
3. **Refresh failure misreported as action failure.** `createTicket`'s catch conflates create
   with the post-create `load()`; refresh-only failure shows `board.error.create` after commit —
   a retry double-creates. Same in `moveSelected` / `commentSelected`.
4. **Optimistic rollback — not implemented (gap, not bug).** v0.4.0 move optimism would have no
   rollback path: no snapshot, no undo, no reconciliation on rejection or a lost response.
5. **Drag/drop races — surface doesn't exist yet.** If v0.4.0 adds DnD, the naive version races
   today's code: drop mid-`tickets.move`; drop during a `load()` repaint; double-drop before
   re-render; drop in a filtered view where the card unmounts mid-gesture; drop post-unmount.
6. **Stale lists.** `boardsGet` runs once at mount; background bot work (the stated purpose —
   "the board shows progress") never repaints. `commentCount`/`state`/`assigneeBotId` go stale
   until a mutation or reload; no "last updated" cue.
7. **Stale `selected`.** A ticket deleted/moved by another member or a background bot stays
   open in the dialog; move/comment then fails with a generic `board.error.*`. `load()` never
   prunes `selected`.
8. **`busy` shared, inconsistently.** Create and move share one `busy`; comment uses
   `commenting`, so comment + move overlap (see 2). Neither flag clears on unmount; resolved
   promises set state after unmount.
9. **Silent state corruption.** `parseColumn` maps any unknown `state` to `backlog` — a server
   bug or schema drift shows the ticket in the wrong column with no diagnostic. Unknown should
   be visible, not misfiled.
10. **Create-form.** No idempotency key on `ticketsCreate` (timeout-but-committed ⇒ duplicate);
    `createTicket` doesn't re-check `busy` — fast double Enter double-submits before re-render;
    the single screen-global `actionError` paints into both form and dialog; empty-title failure
    is indistinguishable from server failure.
11. **Roster staleness / raw-id leak.** `botName()` falls back to the raw bot id when the roster
    is stale or the bot is deleted; spec §8.9 ("ids never reach the screen") is violated, and the
    server error message is discarded in every catch (no correlation id for support).
12. **401 vs 403 vs 500 unmapped.** All non-404s take the same path as success-ready (see 1);
    401 fires the global handler but the screen still lands on an empty ready board.

## Hardening (v0.4.0)

- **Split the failure axes.** Separate `loadPhase` (`loading|ready|error|absent`) from action
  state. Non-404/non-401 → `error` phase with a full-screen `role="alert"` panel + retry
  (`RiRefreshLine`, `variant="secondary"`); 404 keeps `absent`; 403 flips to a forbidden panel;
  401 stays the global handler. Empty board is only "ready + zero tickets".
- **Sequence the refresh.** One `refreshId` ref; `load()` aborts via a composed signal
  (`AbortSignal.any([unmountSignal, AbortSignal.timeout(15_000)])`) and discards responses whose
  id is not the latest. Every mutation then awaits its *own* refresh — fixes 2, 8 (unmount setState).
- **Honest error attribution.** Wrap the mutation and the refresh separately: refresh failure
  "Couldn't refresh the board — ticket saved. Retry." vs action failure "Couldn't create the
  ticket." Keep `role="alert"`; clear the action error on dialog close and on new submit.
- **Optimistic move, safely (only if DnD/optimism is added).** Snapshot `tickets` before the
  local move; on rejection restore the snapshot and toast the error; on success replace by
  server echo (`ticketsMove` already returns it). Move is naturally idempotent (set-state), so
  a lost response heals on the next `load()` — still snapshot so a *rejection* rolls back. Disable
  all column buttons for that card while in flight (`aria-busy`, per-card pending set), never
  the whole dialog.
- **If drag/drop lands:** HTML5 DnD or BoardUI-compatible pointer DnD with a per-card
  `pendingMove` ref; drop while pending is a no-op; hit-test only visible column bodies;
  after drop, revert the DOM on rejection (snapshot above); never animate a value mid-read;
  honor `prefers-reduced-motion`. Gate behind the same `BOMA_BOARD` flag.
- **Freshness without a firehose.** `visibilitychange`/`focus` revalidation throttled ≥30 s with
  in-flight dedupe + a manual refresh button; `aria-live="polite"` "Updated HH:MM" caption
  (`text-text-tertiary text-caption-1-semibold`). If the backend grows an event stream, prefer it.
- **Prune `selected` on refresh.** If the refreshed list no longer holds `selected.id`, close the
  dialog and show "That ticket is no longer available." (or route the move/comment 404/NOT_FOUND
  code to the same copy).
- **Client invariants.** Re-check `busy` inside `createTicket`; add a client nonce on
  `ticketsCreate` (harmless if the server ignores it); cap title length client-side to the wire
  limit; per-field error keys (`board.error.title` vs `board.error.create`) instead of one banner.
- **Diagnostics.** `parseColumn` unknown → render in a labelled "Unsorted" bucket (or keep in
  place with a warning icon + `console.warn`), never silently in backlog. `botName` fallback →
  "Unknown bot" + id behind a `<details>`-style support affordance; surface the `RpcError.code`
  and correlation id there, per spec §8.9 "details" pattern.
- **i18n/a11y.** New strings through `useTranslation` only; keep `role="alert"`, dialog focus
  trap and Escape; on `error`/`absent` transition move focus once to the panel heading; keep all
  hit targets ≥32 px at 360 px.

## Risks

- `load()` refactor touches every mutation path on a COULD-flagged surface — needs Playwright
  for ready/error/absent × (create, move, comment) before merge; the flag gate means regressions
  ship silently if untested with the flag on.
- Timeout 15 s is a guess for poor networks; too tight ⇒ false errors, too loose ⇒ hangs. Measure.
- Optimistic rollback fights the server-truth model if a background bot moves the same ticket
  concurrently — snapshot-restore can clobber the bot's newer state. Mitigation: reconcile by
  `updatedAt` (restore only if server is older), or keep moves server-confirmed until the
  backend offers field-level versioning.
- Focus/visibility revalidation can double-fetch on alt-tab spam — throttle + dedupe mandatory.
- "Unsorted" bucket changes ticket placement semantics; if the spec fixes four columns only, it
  needs an ADR/DEVIATION note, not a guess.
- Idempotency nonce needs server cooperation; log-and-continue if ignored — never block create.
- Splitting error attribution changes user-visible copy → keep strings minimal, list any new
  keys in the i18n files; visible behavior change ⇒ DEVIATION/PARITY note.
- No drag/drop exists today: proposing it for v0.4.0 is scope; confirm with the milestone owner
  before implementing, or ship the rollback/sequence/timeout fixes alone.

## Verification

Playwright (flag on **and** off): outage on mount → error panel + retry recovers; 500 with flag
on → error (not empty board); slow (>15 s) boards.get → error, not skeleton; create-then-refresh-
500 → ticket appears once, refresh retried, no duplicate; move rejected → card back in origin
column (when optimism ships); comment + move concurrently → final list reflects both; ticket
deleted elsewhere → dialog closes with the gone-copy; keyboard-only create/move/comment; axe
zero critical/serious; focus restored to the originating card on dialog close.

CONFIRMED: wrapper, screen, rpc.ts (no timeout, signal optional, 401 global, envelope `{code,error}`).
GUESS: server idempotency support, wire timeout budget, planned DnD scope, four-column-only spec reading.
