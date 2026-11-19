# 0066 · apps/web/components/board/board-screen.tsx · LENS=reliability · v0.4.0 proposal

## Scope note

`BoardScreen` is the whole `/app/board` surface (REQ-038, spec §8.6): a `loading | ready | absent`
phase machine, one board fetch, three mutations (`ticketsCreate`, `ticketsMove`, `ticketsComment`),
each followed by a full `load()` refetch. RPC goes through `apps/web/lib/api/rpc.ts::call`, which
accepts an `AbortSignal` but sets no client timeout. This module is client-only; fixes here cannot
repair backend ordering, so cross-boundary items are flagged. Note: **there is no drag-and-drop in
this component** — moves happen via column buttons in the dialog. The "drag race" analogue below is
concurrent `ticketsMove` calls plus out-of-order `load()` responses; true DnD is deferred.

## Current behavior (CONFIRMED, as read)

- Two separate fetch paths: the mount `useEffect` (lines 81-101) passes an `AbortController.signal`
  and guards on `signal.aborted`; `load()` (58-71) passes no signal and has no abort guard, yet every
  mutation awaits it. The two paths already differ (failure clears tickets only in `load()`).
- Non-404 `boardsGet` rejection → `phase="ready"` with an empty ticket list; `botsList` rejection is
  silently dropped. A network failure is indistinguishable from "no tickets".
- `selected` is a `TicketView` snapshot. `moveSelected` replaces it with `moved.ticket`, but the
  subsequent `load()` never re-derives it; `commentSelected` never refreshes `selected.commentCount`.
- One `busy` boolean gates create + move; `commenting` is separate; one `actionError` is rendered in
  both the create form and the comment form. Errors are generic i18n strings, one per action.

## Failure modes

1. **Stale board (CONFIRMED).** No refresh while mounted. In a product where bots/other users move
   tickets, the board only updates after the local user's own mutation, and even then the whole list
   is refetched with no version/etag. `load()` after a mutation has no signal: on unmount (or route
   change) it still calls `setTickets`/`setPhase`, and a slow response can overwrite newer state.
   Fetch failure paints an empty board with no retry affordance. No timeout exists at the RPC layer,
   so a hung request leaves the skeleton (`aria-busy`) indefinitely.
2. **Optimistic drift (CONFIRMED).** The dialog renders a stale snapshot, not an item from `tickets`.
   After a move, `selected.state` comes from the mutation response while the list comes from `load()`;
   if anything else changed the ticket in between, dialog and column disagree. After a comment, the
   dialog keeps the old count. There is no sequence token or last-write-wins rule, so the response
   and the refetch can land in either order.
3. **Create-form errors (CONFIRMED).** `actionError` is shared: a failed move writes "move failed"
   into the create form (if open) and the comment form; a create error lingers in the comment form.
   `moveSelected`/`commentSelected` never clear it at start, so an old error survives a new action.
   `busy` disables the dialog's column buttons during an unrelated create. Title is only trimmed
   client-side; server validation detail is collapsed into `board.error.create`; fields correctly
   remain on failure but the form is never told why.
4. **Concurrent move / refetch races (CONFIRMED analogue).** `busy` is React state, not a synchronous
   ref: two rapid clicks or Enter+click can dispatch two `ticketsMove` before re-render. Both call
   `load()`; the later response can be from the earlier request, leaving the board behind the
   dialog. `Escape`/overlay click closes the dialog mid-flight (GUESS: server still applies the move,
   UI shows no result). Focus effect uses `[Boolean(selected)]` with an eslint-disable, so switching
   tickets does not re-focus the dialog and close does not restore focus to the trigger.
5. **Assignee drift (GUESS, depends on backend).** A bot removed from the roster can stay selected in
   the create form; assignee names resolve from a `bots` snapshot that is never refreshed.

## Proposed hardening (v0.4.0)

- **A. One cancellable fetch path.** Replace `load()` and the mount effect with a single function
  taking a signal; keep one `AbortController` per screen lifetime, pass its signal to every
  `boardsGet`/`botsList`/mutation call, and guard state writes on `!signal.aborted`. Add a monotonically
  increasing request id so only the newest response commits. Add an explicit `boardError` state +
  retry button and stop mapping transient failures to "absent"/empty. *Risk:* phase machine grows;
  keep `absent` strictly for 404 and never invent a new user-visible string outside i18n.
- **B. Freshness without a feature.** Keep visibility/focus-driven refetch (cheap, no new surface)
  and, if the backend can serve an etag/version, a conditional poll (~30 s) instead of a blind full
  refetch after every mutation. *Risk:* polling adds load and can fight an open dialog — pause while
  `selected !== null`; backend support needed for the conditional variant (GUESS).
- **C. Derive the dialog from the list.** Store `selectedId`; compute
  `selected = tickets.find(t => t.id === selectedId) ?? null`. Merge each mutation response into the
  `tickets` array (authoritative ticket) instead of relying solely on `load()`. Close the dialog if
  its ticket vanishes from a refresh. *Risk:* brief flash if response and refetch disagree — make the
  response authoritative within one request id, never a stale refetch.
- **D. Per-action state and synchronous guards.** Replace `busy`/`actionError` with keyed state
  (`create | move | comment | board`); clear the key at action start; add `useRef` in-flight flags set
  before each `await` so rapid clicks cannot duplicate a move; disable only the relevant controls.
  *Risk:* more state to reset in every `finally`; verify the form cannot wedge on a thrown error.
- **E. Create-form hygiene.** Align a client title max-length with backend validation, keep entered
  values on failure, and surface field-specific errors where the i18n layer allows. Do not close the
  form until both create and the follow-up refresh succeed. *Risk:* needs the real backend limit.
- **F. Dialog lifecycle.** Restore focus to the invoking ticket button on close; re-focus on `selectedId`
  change (drop the eslint-disable); optionally block close while a move is in flight. *Risk:* focus
  restore needs a ref to the trigger; keep Escape working for accessibility (WCAG 2.2 AA gate).
- **G. Playwright coverage (REQ-038).** Board refresh after a bot-driven move; two quick column clicks
  issue one `ticketsMove`; a move error never appears in the create form; dialog and column agree after
  move + comment; unmount mid-fetch produces no state update. *Risk:* route-mock timing brittleness;
  reuse existing fixture style.

## Explicitly rejected / deferred

- Real drag-and-drop: absent from this component and from §8.6. If added later it needs its own
  race tests and a DnD dependency (ADR), not a silent addition here.
- Distinguishing transient failure from "board absent" in copy beyond a retry action — keep spec wording.

## Risks & rollout

Client-only items C, D, F, G ship first; A partially (abort/guard, request id) ships with them, while
etag/visibility/B and the validation limit in E need backend confirmation (GUESS until then). Highest
regression risk is C (dialog flash/close) and D (guard clearing); cover both with G before release.

## Success criteria (targets, not yet run)

- No state write after unmount; newest fetch always wins; board failure is visible and retryable.
- Dialog state equals list state after any move/comment; one POST per user action.
- An error appears only on the surface that caused it; focus returns to the trigger on dialog close.
