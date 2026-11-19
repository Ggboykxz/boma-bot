# 0026 — apps/web/app/app/page.tsx — reliability (v0.4.0)

Worker #026 · `apps/web/app/app/page.tsx` · reliability · READ-ONLY review of `before-m5`.
The module is a 16-line server component (`AppShell` + `AppEntry`). Its failure surface lives in
`components/shell/app-entry.tsx`, `components/shell/threads-home.tsx`, `components/shell/bot-rail.tsx`,
`lib/bot-directory.ts`, `lib/roster.ts`, `lib/api/rpc.ts` (`call()`) and the Rust static handler
(`crates/api/src/router.rs`). No code ran; file facts CONFIRMED by reading, test status GUESS.

Spec anchors: §8.1 (routes; "The interface must survive latency … a blank panel is a bug"), §8.2
(onboarding lands on `/app/:botId` with the intro streaming), §8.3, §4.8 (on realtime loss:
poll/refetch on focus + a visible "not live" indication, never silent staleness), REQ-057 (a11y).

## What is already right (keep, don't re-propose)

- `ThreadScreen` has distinct phases and a retryable error panel (`thread.error.load` + `common.retry`).
- `refreshRoster` keeps the last good roster on failure; hydration uses `useSyncExternalStore` with
  separate server/client snapshots (no mismatch crash).
- ThemeToggle wraps `localStorage` in try/catch; rail and thread have skeletons and `aria-live` logs.
- `call()` calls `unauthorizedHandler` on 401 — a real path, not a swallow.

## Failure modes seen → concrete hardening proposals

### F1. Malformed deep link crashes the whole `/app` subtree
`botIdFromPath` (`lib/bot-directory.ts:66`) calls `decodeURIComponent(match[1])` unguarded.
`/app/%zz/` (or `/app/%/`) matches the regex, `decodeURIComponent` throws `URIError` **inside
`getSnapshot`**, so React throws during render. There is **no `error.tsx` / `global-error.tsx`
anywhere in `apps/web`** (find: zero hits) → the entire shell unmounts to a blank document: no
header, no rail, no retry, no message.
P1. `try { decodeURIComponent } catch { return null }` in `botIdFromPath` (malformed id ⇒ home,
    not a crash). Add `app/app/error.tsx` as the safety net ("Something went wrong" + reload
    button, generic copy only); never render error internals.

### F2. Group deep links silently render the threads home
The Rust prefix fallback (`router.rs` `PREFIX_FALLBACKS`, `("/app/", "app/index.html")`) serves
**this page** for `/app/g/<id>/` — arbitrary ids have no pre-rendered shell. `AppEntry`'s regex
(`^/app/([^/]+)/?$`) never matches a two-segment path → `null` → `ThreadsHome`. No error, no
redirect: the group the user was sent to (a fresh group created via `createGroup` →
`router.push('/app/g/<id>/')`, an invite, a notification) is unreachable through this entry, even
though `app/app/g/[groupId]/page.tsx` exists. Same for `/app/foo/bar/`.
P2. Extend the dispatch: `/app/g/<id>/` → `GroupThreadScreen` (mirror the `[groupId]` route), or —
    if that belongs to another wave — render an explicit "this link is not supported yet" /
    not-found state instead of a silent home. Verify how `router.push` behaves in the static
    export first (GUESS: missing RSC payload ⇒ full document load ⇒ same fallback path).

### F3. Empty workspace has no first-bot onboarding path on this screen
`ThreadsHome` renders "Threads" + `shell.placeholder` ("Pick a bot in the rail to open its
conversation.") even when the space has zero bots. There is nothing to pick; the only affordance
is a 14 px "+" `IconButton` in the rail. A user who lands on `/app` with no bots (fresh space,
step 3 of `/onboarding` skipped, all bots archived) hits a dead end, contra §8.2's first-bot path.
P3. When the roster resolves to **ready-and-empty** (see F4), render a real empty state here:
    one sentence, a primary Button "Create your first bot" (opens `BotCreateForm`'s flow or links
    to `/onboarding/2`), secondary link to the onboarding route. All copy via the catalogue
    (en/fr/he/ar — four locales, not two). Keep the `<h1>` and landmarks the goldens pin.

### F4. Rail empty-state condition is inverted; failure masquerades as "No bot yet."
`bot-rail.tsx:168`: `roster === null && directory.length === 0` — fires only **while loading or
after failure**, never after a successful empty fetch (`roster === []`). And on first-load failure
it renders `shell.rail.bots.empty` ("No bot yet."), which is a lie: the server may have bots, the
fetch just failed. Same shape for groups: `groupsList(...).catch(() => setGroups([]))`
(`bot-rail.tsx:91-93`) makes "server down" indistinguishable from "no groups". Neither failure
has a retry affordance.
P4. Give the roster store a status (`loading | ready | failed`). Render "No bot yet." only when
    ready-and-empty; while loading, a quiet skeleton/`…`; when failed, an inline
    `role="alert"` "Couldn't load your bots — Retry" calling `refreshRoster()`. Same treatment
    for groups (failed flag + inline retry, never `setGroups([])` on catch). Empty copy only ever
    asserts emptiness you have evidence for.

### F5. No RPC timeout → the rail can stall forever, invisibly
`call()` (`lib/api/rpc.ts`) has no timeout; only the caller's optional signal. A hung
`bots.list`/`groups.list` leaves the rail permanently empty with **no spinner and no error** —
a blank panel, the exact thing §8.1 forbids. `groups.list` failure isn't even rendered.
P5. Default timeout inside `call()` (e.g. `AbortSignal.timeout(10_000)`, combinable with the
    caller's signal); map timeout to a distinct `RpcError` code. F4's retry UI then names it.
    Make the default opt-out per call site, not a behaviour change for streaming procedures.

### F6. Deep links flash the wrong screen; double render on every `/app/<id>/`
Server HTML is `ThreadsHome` (server snapshot `null`, `app-entry.tsx:26`); on hydration the
client swaps to `ThreadScreen`. Every deep link paints the threads home first (wrong landmarks
for one frame, aria-live noise, `ThreadScreen`'s fetch starts only post-hydration).
P6. Render a neutral, quiet shell-body skeleton as the pre-hydration pass instead of
    `ThreadsHome`; keep `ThreadsHome` for the confirmed-`/app/` case. Verify against the UI
    goldens that pin `/app/` landmarks (GUESS they exist) — skeleton must not add tab stops.

### F7. Home screen goes stale forever
`refreshRoster()` fires on mount and on `pathname` change only. On the static shell (no SSE on
this screen) unread badges and the snoozed shelf never update while the tab stays open — silent
staleness, contra §4.8.
P7. Refetch on `visibilitychange` → visible (debounced ≥5 s), skipping hidden tabs. One line plus
    a guard; do **not** add polling yet — focus refetch is enough here.

## axe notes
Home and rail render inside the shell covered by the a11y suite (GUESS: `/app/` not in the route
list — worth checking). After P1/P3/P4: `role="alert"` on the failure inline; single polite live
region for roster updates (never per-row); focus returned to the create button on settle; no new
colour-only states; skeleton `aria-hidden`. New copy must hold 4.5:1 in light **and** dark.

## Risks and trade-offs
- R1. Goldens/REQ-057 pin the shell's landmarks: F6's skeleton and F3's empty state must be
  reviewed against them; a mismatch is a defect or a DEVIATION, never silent.
- R2. F2 spans `AppEntry`, `bot-rail` and the group flow — confirm the `router.push`-in-export
  behaviour (P2) before building; scoping it into the `[botId]`/group waves may be wiser.
- R3. P5 changes every RPC: gate behind tests; 10 s default may cut long-polling procedures —
  opt those out explicitly (list them, don't guess).
- R4. P4 adds a third state to the store; the "last good roster" fallback must stay (never blank
  a populated rail on a transient failure).
- R5. P3 adds a primary CTA: it must not bypass onboarding invariants (bot creation may require
  an answered model step); link to `/onboarding/2` rather than duplicating `bots.create` semantics.
- R6. Error boundary (P1) must be boring: generic copy, one reload button, no stack, no secrets.

## Explicit non-goals
No onboarding flow redesign, no SSE/WebSocket, no roster/snooze semantics changes, no theme-system
rework, no `ThreadScreen` retry redesign (already sound), no group-thread feature work beyond F2's
dispatch or explicit not-found.

## Suggested v0.4.0 rollout
Wave 1 (small, independently revertable): F1 (try/catch + `error.tsx`), F5 (timeout), F4 (roster
status + retry UI). Wave 2: F3 (empty state + CTA), F6 (neutral first paint), F2/F7 after the
navigation question is answered. Tests first: unit tests for `botIdFromPath` (malformed, `%`,
`g/<id>`, trailing slash), Playwright for the three rail states and the empty home, axe on
`/app/` light+dark+RTL.
Acceptance: no bare `catch {}` decides what this screen shows; malformed URLs render a friendly
error, never a blank page; empty workspace offers the first-bot path; a failed roster load is named and retryable; UI goldens green.

_Scope: line refs against `before-m5`; no code/tests/builds ran; navigation behaviour GUESS._
