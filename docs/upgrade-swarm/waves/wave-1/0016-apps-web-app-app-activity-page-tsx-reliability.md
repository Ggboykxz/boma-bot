# 0014 — apps/web/app/app/activity/page.tsx — reliability (v0.4.0)

Worker #014 · `apps/web/app/app/activity/page.tsx` · reliability · READ-ONLY review of `before-m5`.
The page is a 15-line wrapper; the failure surface lives in `components/activity/activity-screen.tsx`
(cited), `lib/api/rpc.ts` (`call()`), `tests/run-controls.spec.ts`. No code ran; file facts CONFIRMED
by reading, test status GUESS (STATE.md: two REQ-067 tests green).

Spec anchors: §8.4 (working runs "update in place"), §4.8 (on realtime loss: poll/refetch on focus +
visible 'not live' indication, never silent staleness), REQ-011/012/067.

## What is already right (keep, don't re-propose)

- List fetch: `AbortController` + aborted-check; `reload` aborts the previous fetch — no last-writer-wins.
- Retry limited to `failed` (`isRetryable`), matching REQ-012's only legal retry transition.
- Unknown wire status/trigger → raw string, no crash; skeletons `aria-hidden`; reduced-motion in
  `globals.css`; statuses in words, never colour alone; distinct phases; empty state only when ready.

## Failure modes seen → concrete hardening proposals

### F1. Statuses go stale forever (the biggest one)
No poll, no refetch-on-focus, no live indication: a finished run shows `running` until manual reload —
violating §4.8 and defeating §8.4's "update in place". `activity.loading` exists in the catalogue but
is never rendered — screen-reader users hear nothing during load.
P1. Refetch on focus/`visibilitychange` + jittered polling (≥5s) **only while a non-terminal row is
    visible**; stop when all terminal; skip hidden tabs; back off on persistent 5xx.
P2. Always render a last-updated line ("Updated 14:02:11" / "Not live — retrying") from the last good
    fetch; one polite `role="status"`. This is the §4.8 fallback if SSE lands later.

### F2. The list fetch can hang forever
`call()` (`lib/api/rpc.ts:245-268`) has no timeout. A hung `runs.list` leaves the skeleton up forever;
the `error` phase never renders.
P3. Give list/pause/resume/retry an explicit timeout (`AbortSignal.timeout(10_000)`); on timeout
    show the error panel, never an endless skeleton. Surface `RpcError.code`, not a flat catch-all.

### F3. Pause/resume busy-state race (`busyRun` is a single global slot)
Clicking pause on A then B **re-enables A's button while A is in flight** (`disabled={busyRun === run.id}`)
→ double dispatch. Also: the clicked button becomes disabled while focused — focus can drop to `<body>`.
P4. Track pending ids in a `Set<string>`; a row stays busy until *its* request settles; other rows
    are never displaced.
P5. Prefer `aria-disabled` + busy label ("Pausing…") over `disabled` (keeps focus, announced); or move
    focus to the row heading on settle. Verify in Playwright.

### F4. The reply is applied wholesale — and can regress a newer state
`control()` maps `reply.status` over the row unconditionally (110-113). If a focus-refetch (P1) lands
`running` before the pause reply, the late reply rewrites the row to `paused` — a state already superseded.
P6. Guard the apply: patch only if the row still holds the pre-action status (compare-and-swap),
    else trust the newer refetch. Simpler acceptable variant: apply the reply **and** refetch
    immediately. Never let a mutation reply win blindly.

### F5. Pause/resume/retry failures are swallowed silently
`catch {}` with no UI in `control()` and `retry()` (61-65, 113-117). REQ-067 lets `pause` on a run that
transitioned `queued→leased→running` fail with a conflict error — the operator gets *no feedback*; the button
re-enables. Nothing else reports itself despite the comment (line 62) claiming it does.
P7. Per-row inline error (`role="alert"`, `text-text-error-primary`): "Couldn't pause — the run is no
    longer queued. The list was refreshed." On `CONFLICT`, auto-refetch once; `TOO_MANY_REQUESTS` shows
    retry-after if carried; `401` keeps the existing `unauthorizedHandler` path.
P8. Log client-side via the telemetry hook if one exists; never log the run prompt.

### F6. Retry: no refresh after success, repeat-click storm
`retry()` clears `retrying` in `finally` with no reload; the row keeps showing `failed` after re-queue, and
the button re-enables — repeat clicks the server rejects as not-`failed`, silently.
P9. On success, refetch (or optimistically set `queued` + reconcile); retry stays disabled until the
    refetch settles; on error, inline message per P7. Closes F4/F6 together.

### F7. Reload-over-content renders skeleton AND stale list
`reload()` sets `phase="loading"` with `runs` still populated; skeleton (72) and `<ul>` (100) render at
once; a screen reader gets nothing (`activity.loading` unused, list not `aria-busy`).
P10. When `runs.length > 0`, skip the skeleton; list gets `aria-busy="true"` + "Refreshing…";
     empty list keeps skeletons; clear `aria-busy` on settle.

### F8. Error phase shows stale rows with no staleness marker
With `phase="error"` but `runs.length > 0`, the operator sees old data next to an error box with no way
to tell it is old; an empty-looking failure is indistinguishable from "no runs yet".
P11. Error panel branches on `runs.length`: with runs → "Couldn't refresh — showing the last known
     state (14:02:11)"; without → existing panel. `role="alert"` (today: none, so failures go unannounced);
     keep the list visible underneath either way.

### F9. Accessibility / axe gaps
- `/app/activity/` is **not** in `tests/a11y.spec.ts` ROUTES and never passes through `AxeBuilder`; RTL/Hebrew
  scan does not cover this screen either. `run-controls.spec.ts` asserts visibility only.
- Per-row `role="status"` (130): N live regions burst on every refresh; prefer one polite summary region.
- `disabled` buttons (F3/P5); caption-1-semibold contrast on `bg-background-primary-default` (light+dark);
  skeleton trio has no accessible name.
P12. Add `/app/activity/` to a11y ROUTES (light+dark, `wcag22aa`) + one RTL pass; `role="alert"` on
     the panel; single polite live region; focus kept on settle; axe assertion in `run-controls.spec.ts`.

### F10. Type looseness at the boundary
`RunView.status`/`trigger` are `string`; comparisons rely on literals and unknowns degrade raw (good) but
`run.status === "queued"` is unchecked. Nothing stops a typo'd status string compiling.
P13. Narrow once at the boundary (`isRunStatus`) → `RunStatus | "unknown"`; derive actionability from
     it; keep raw-wire display fallback. No `as` casts.

### F11. Completeness and unmount hygiene
page: `runs.list` returns `nextCursor` — ignored; user sees only the newest 50, with no note.
`control()`/`retry()` set state after unmount (harmless in React 18, unhygienic).
P14. Consume `nextCursor` ("Load more") or state the 50-limit explicitly; gate post-settle setState on a mounted ref.

### F12. `page.tsx` itself
Static wrapper is fine; add route `metadata`/title if missing (check siblings) — cosmetic.

## axe notes
Zero automated axe coverage today (F9). After P12 assert: no critical/serious `wcag22aa` light+dark+RTL;
error panel announced; no colour-only status (already true — keep); visible focus on pause/resume/retry;
`prefers-reduced-motion` respected (CSS does it — keep it out of JS).

## Risks and trade-offs
- R1. Conflict→auto-refetch loop: cap at one per action; further failures stay inline, never loop.
- R2. Polling cost on a self-hosted box: non-terminal rows only, ≥5s+jitter, hidden tabs skipped — §4.8 demands degradation, not a hot loop.
- R3. Compare-and-swap complexity; "apply + immediate refetch" is an acceptable simpler fallback.
- R4. `aria-disabled` must stay keyboard-focusable *and* inert; cover with a keyboard test.
- R5. Auto-refetch after conflict can mask the cause; the inline message must name it ("the run changed state"), never a bare "error".
- R6. Fake-timer polling tests risk flakiness; follow the seeded-RPC-route pattern in `run-controls.spec.ts`.
- R7. Poll re-renders reorder rows mid-click (working runs sort to top): pin row order by `id` during an in-flight action on that row.

## Explicit non-goals
No list redesign (§8.4 day-grouping, bot/status filters, link-back to message, working-sort) — that is a
parity/feature gap, not reliability, and belongs in its own wave. No SSE/WebSocket introduction, no backend
lease/fence work (M2), no changes to `runs.retry` semantics.

## Suggested v0.4.0 rollout
Wave 1: F2, F3/F4, F5/F6, F9 — small, independently revertable. Wave 2: F1, F7/F8/F10/F11.
Acceptance: no bare `catch {}` left on the screen; a11y gate green for `/app/activity/`; soak note shows
stale-status duration < poll interval while runs are non-terminal.

_Scope: line refs against `before-m5`; no code/tests/builds ran; swarm-brief failure counts not re-derived._
