# 0036 · apps/web/app/onboarding/page.tsx · LENS=reliability · v0.4.0 proposal

## Scope note

The module is the index only: `redirect("/onboarding/1/")`. It holds no state, so the reliability
failure modes live one hop later, in `[step]/page.tsx` (`parseStep`) and
`components/onboarding/onboarding-screen.tsx` (the wizard). This reviews the index as flow entry and
hardens across that boundary; index-only changes would be theater.

## Current behavior (as read)

- `/onboarding` unconditionally 307/308-redirects to `/onboarding/1/`.
- `[step]/page.tsx` exports `generateStaticParams` for "1" | "2" | "3" and coerces any unknown `step`
  param to `1` via `parseStep`'s default branch.
- The client wizard persists `{ version: 1, spaceId?, botId? }` under `localStorage["boma-onboarding"]`.
  Resume is keyed off `step` in the URL, not off saved progress. `finish()` removes the key and routes
  to `/app/<botId>/` or `/app/`.
- Steps 1 (space) and 2 (bot) are skippable; step 3 (model) gates finish
  (`canFinish = connected > 0 || noProvidersReachable`).

## Failure modes

1. **Resume vs restart ambiguity (the big one).** `/onboarding` always sends the user to step 1 even
   when `localStorage` holds a half-finished `{ spaceId, botId }`. A returning user who completed
   steps 1–2, closed the tab, and taps "Onboarding" (or follows a stale link) lands on an empty
   step-1 form with committed server-side artifacts behind them. They may re-create a duplicate
   space and duplicate bot. The wizard never reads `saved` to pick a resume step; the URL is the
   only source of truth and the index throws that away.
2. **Partial state / orphaned artifacts.** `spacesCreate` succeeds → `saveProgress` writes
   `spaceId` → `goTo(2)`. If the user then closes the tab, the server has a space with no bot;
   `boma-onboarding` says `{ spaceId }`. On return via `/onboarding/1/` (normal link), nothing
   stops step 1 from creating a *second* space. Worse: `createBot` spreads `saved` and overwrites
   `botId`, but a failure between `botsCreate` success and `saveProgress` (or between RPC success
   and navigation) strands a bot the client has no bookmark for — permanent partial state, invisible
   to the wizard, and `finish()` then routes to `/app/` instead of the created bot.
3. **Back-button loops and dead ends.** Step 3 "back" goes to step 2; finish routes to `/app/`.
   Browser back from `/finish`-onward can re-enter step 3. After `finish()` removes the storage key,
   revisiting `/onboarding/1/` restarts the wizard from scratch against an already-provisioned
   account. There is no "onboarding already complete" guard, so back/refresh can resurrect a
   completed wizard and invite duplicate creation.
4. **Silent param coercion.** `parseStep` maps `/onboarding/7/` or `/onboarding/abc/` to step 1
   rather than 404/redirect-to-canonical, masking bad links and confusing analytics and tests.
   (In scope-adjacent `[step]/page.tsx`, same static export — worth fixing with the index.)
5. **localStorage as single point of truth, unversioned in practice.** `loadProgress` treats any
   parse failure as `{ version: 1 }` (fine), but `saveProgress` failures are swallowed, so a
   storage-full/private-mode user gets zero resume signal while the code comments claim resume
   works. The comment on the component ("Reloading resumes on the same step") is only true for
   URL-driven reloads, not for the `/onboarding` entry, and not at all when storage is unavailable.
6. **Step-3 fetch race on resume.** The step-3 effect fires `modelsCatalog`/`modelsCredentials`
   with an AbortController, but `setSpaceId(loaded.spaceId)` runs in a separate `Promise.resolve()`
   tick from the fetch wiring; on a slow first render the catalog can resolve before the space
   context is set, so the catalog/credentials the user sees may belong to the wrong (default) space.
7. **Double-submit windows.** `busy` guards buttons, but the Enter-key path is not serialized with
   the create RPC; Enter + click can fire two `spacesCreate` calls (client-side twin of #2).

## Proposed hardening (v0.4.0)

For the index module itself, keep it trivial but make it *resume-aware*:

- **A. Resumable entry.** Replace the unconditional redirect with a client (or server-read) entry
  that resolves the intended step: if `localStorage["boma-onboarding"]` holds `botId` → step 3;
  else `spaceId` → step 2; else step 1. Static export means no request-time server read — do the
  decision client-side in a tiny `OnboardingEntry` client component (read storage, `router.replace`,
  render nothing/skeleton). `router.replace`, not `push`, so the back button does not land on a
  redirect stub. Risk: flash of blank/skeleton and an extra client round trip; mitigate with a
  minimal inline fallback and `<noscript>` link to `/onboarding/1/`. Storage-unavailable users fall
  back to step 1 — document that.
- **B. Completion guard (anti-loop).** Persist a `completedAt`/tombstone marker (or rely on
  `finish()` removal + a separate `boma-onboarding:done` flag) so `/onboarding` and step pages can
  detect "already provisioned" and redirect to `/app/` instead of restarting. Risk: tombstone can
  outlive a deliberate "reset onboarding" need; gate it behind an explicit reset affordance or
  version bump, not silent forever.
- **C. Server-truth reconciliation.** On entry (any step), if storage shows `spaceId`, validate it
  with a cheap `spaces.get`/`list` before enabling creates; if the stored id is stale (space
  deleted server-side), clear the bookmark and start at step 1 with a notice. Same for `botId`.
  Risk: extra RPC on every entry; cache with short TTL or only validate on step mount, and treat
  validation failure (network down) as "keep bookmark, proceed optimistically" — never hard-block
  onboarding on a probe.
- **D. Idempotent creates.** Send an idempotency key (client-generated UUID per wizard session,
  stored in the same localStorage record) with `spacesCreate`/`botsCreate`; server dedupes retries
  so double-click/Enter races and post-RPC-crash retries cannot create duplicates. Requires a
  server contract — coordinate with backend; without it, at minimum disable all step-advancing
  controls while `busy` and remove the Enter shortcut path or funnel it through the same guard.
  Risk: key reuse across wizard sessions if not scoped/rotated — rotate on `saveProgress` version.
- **E. Fix the partial-state window.** Write the bookmark *before* the RPC resolves is wrong;
  instead write `{ pendingCreate: "space" }` pre-flight and reconcile post-RPC, or snapshot the
  created id into storage synchronously in the same tick as the RPC resolution and navigate with
  `router.replace`. If `botsCreate` succeeds but storage write throws, surface a non-blocking
  "we created your bot but couldn't save progress — do not recreate" banner and route straight to
  `/app/`. Risk: complicates the happy path; keep the reconciliation helper small and unit-test it.
- **F. Canonical step handling.** In `[step]/page.tsx`, replace silent `parseStep` default with
  `notFound()` (or a redirect to `/onboarding/1/` for known-bad values) so unknown steps don't
  masquerade as step 1. Risk: previously "working" sloppy links now 404 — check analytics/tests for
  reliance before flipping; consider redirect-with-warning instead of 404 if any inbound links
  exist.
- **G. Back-button sanity.** Use `router.replace` for post-create advancement (steps 2→3, 3→finish)
  so the browser back stack doesn't contain duplicate wizard entries; keep `push` only for
  intentional lateral navigation. Consider `beforeunload` when `busy` to prevent tab-close mid-RPC
  (pairs with D). Risk: `replace` strips the ability to go "back" to a step intentionally — acceptable,
  the wizard already exposes explicit Back controls.
- **H. Storage failure honesty.** Track a `storageOk` flag (probe `setItem` once on mount) and when
  storage is dead, show a persistent inline notice ("progress won't be saved on this device")
  instead of the silent-in-memory behavior. Risk: alarming users in private mode for a cosmetic
  concern — keep copy calm, one line, dismissible.
- **I. Test coverage.** Extend `apps/web/tests/onboarding.spec.ts` with: re-entry via `/onboarding/`
  with partial storage lands on the right step; storage-mid-write failure still advances; stale
  `spaceId` self-heals; completed tombstone redirects to `/app/`. Existing REQ-042/REQ-052 tests
  must stay green. Risk: Playwright route-mock brittleness — keep the new specs in the same
  fixture style.

## Explicitly rejected / deferred

- Moving the wizard to a server component with request-time storage read — impossible on static
  export; would force dynamic rendering for a marketing-static page.
- Replacing localStorage with cookies for resume — same static-export constraint for the index;
  revisit only if the app gains dynamic rendering.
- Auto-skipping step 3 when a space already has a default model — that's a product decision, not a
  reliability fix; flag to spec owner.

## Risks & rollout

- Highest-risk items are A/B (entry redirect semantics) and D (server idempotency contract). Ship
  A/B/G/H/F behind the static export with no server dependency first, verify with the Playwright
  suite and a manual private-mode pass, then negotiate D/E with backend.
- Watch for regressions in `finish()` routing (`/app/<botId>/`) when B's tombstone interacts with
  accounts that re-run onboarding intentionally — needs the reset affordance before or with B.
- No new dependencies; all changes are in the onboarding entry, `[step]/page.tsx`, and the wizard
  client component. Bundle impact negligible.

## Success criteria

- No path from `/onboarding` to a duplicate space/bot creation in automated tests.
- Unknown `/onboarding/<x>` values no longer silently render step 1.
- Storage-disabled environments degrade with a visible notice, not silent amnesia.
