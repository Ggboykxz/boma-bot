# Worker #072 — Reliability proposal for `onboarding-screen.tsx` (v0.4.0)

Target: `apps/web/components/onboarding/onboarding-screen.tsx` (334 lines).
Lens: reliability. Status: proposal only — no code changed. Confidence legend: **[C]** = verified by
reading this file / `rpc.ts` / `spec/08-ui-ux.md` this session; **[G]** = inferred, needs a test to confirm.

## 1. Failure modes

### 1.1 Lost progress
- **[C] Hydration race / stale closure.** `saved` initializes to `{version:1}` and is filled only in a
  `useEffect` via `Promise.resolve().then(() => setSaved(loaded))` (l.74-82). `goTo` (l.104-109) pushes a new
  URL, so Next remounts the component and the effect re-runs on every step. `createBot` builds its bookmark
  from `const progress = { ...saved, botId }` (l.155) using the render-time closure. If a fast user, autofill,
  or slow hydration lets `createBot` run before the microtask flush, `saved` is still empty and
  `saveProgress` (l.132) overwrites localStorage **without the `spaceId`** set in step 1 — the space pointer
  is lost permanently. Same pattern for `createSpace` (l.131).
- **[C] Silent storage failure.** `saveProgress` swallows every error (l.44-50: quota, disabled storage,
  Safari private mode). Progress vanishes on reload with no signal; only `loadProgress` self-heals (l.30-42).
- **[C] Bookmark cleared before navigation commits.** `finish` calls `localStorage.removeItem` *then*
  `router.push` (l.111-118). A failed push or tab close leaves server artifacts but no bookmark; retrying
  onboarding starts from zero.
- **[C] No bookmark migration.** Version mismatch or malformed JSON silently resets to `{version:1}` (l.38-41),
  dropping any ids; ids are never shape-checked or validated against the server (stale/deleted space id is
  trusted by the step-3 `setSpaceId` on l.77-79).
- **[G] Draft loss on remount.** `spaceName`, `botName`, `botPurpose` (l.64-66) live in component state and
  are lost on any route change (Back included), forcing retyping.

### 1.2 Forward-only navigation
- **[C] One-way doors.** Steps 1 and 2 have only Skip (l.240, l.273); the sole Back is step 3 → step 2
  (l.327). Once the user skips creating the space (or the bot) there is no path back to correct it. Spec
  (`08-ui-ux.md` §8.2) says steps are independently skippable — a skip must be reversible in-session.
- **[C] No precondition guard.** `step` comes straight from the route prop (l.59); a deep link to
  `/onboarding/3` fetches catalog/credentials (l.83-99) with no saved space/bot. `setSpaceId` runs only when
  `saved.spaceId` exists (l.77-79), so step-3 server calls may run with no intended space context.
- **[C] History stacking.** `goTo` uses `router.push` for skips (l.104-109), so browser Back/skip builds a
  long stack of onboarding URLs that can strand the user between skipped steps and bypass step invariants.

### 1.3 Partial completion
- **[C] Dead end on fetch failure.** `canFinish = connected > 0 || noProvidersReachable` (l.190-192), and
  `noProvidersReachable` requires `catalog !== null`. Step-3 loads use `Promise.allSettled` (l.85-97) which
  swallows rejections: if catalog and credentials both fail, both stay `null`, Finish is disabled and **no
  error is shown** — the user cannot finish, retry, or see why.
- **[C] Connect/refresh split is non-atomic.** `connectProvider` (l.166-188) clears `apiKey` (l.175) *before*
  `modelsCredentials()` refresh (l.176). If the refresh throws, the connection persisted server-side but
  `credentials` stays stale: UI shows "nothing connected" (l.320), Finish is disabled, the key is gone, and
  a retry may duplicate. No `AbortSignal`/timeout on this call (unlike the catalog fetch).
- **[C] Double-submit.** `busy` is React state (l.67, set l.126/148/171); the Enter handlers on the inputs
  (l.233, l.266, l.308) call the action without checking `busy`. Two rapid Enter events before re-render can
  double-create a space/bot/connection; no ref-based in-flight lock or idempotency key.
- **[C] Non-skippable invariant not enforced.** Spec forbids leaving *a space without a model*, yet with an
  empty catalog (`noProvidersReachable`) Finish succeeds with zero credentials (l.191-192, l.324), and with
  no bot `finish` falls back to `/app/` (l.117). A user can complete with a space/bot but no model. Any change
  here is externally visible → DEVIATION, needs `docs/PARITY.md` + approval.

## 2. Concrete hardening (prioritized)

**P0 — write progress correctly**
1. **[C] Eliminate the closure race:** hydrate `saved` in a lazy `useState` initializer guarded by
   `typeof window !== "undefined"` (avoid SSR `window` access), and drop the microtask deferral. In each
   mutation handler, derive the next bookmark from `loadProgress()` (or a synchronously maintained
   `progressRef`), not from render-time `saved`. *(Risk: hydration mismatch if initializer touches `window`
   unguarded — the guard is mandatory.)*
2. **[C] Make the bookmark write verifiable:** after `setItem`, read back once; on failure set a `storageOk`
   flag and show a neutral, i18n'd "progress may not be saved" notice instead of failing silently. *(Risk:
   extra state + one new translation key; must not block the flow.)*
3. **[C] Reorder `finish`:** navigate first, clear the bookmark on successful commit (or write a `completed`
   marker cleared on next `/app` load). Make finish idempotent. *(Risk: a stale marker briefly persists —
   harmless; ensure no double-redirect.)*

**P1 — make navigation reversible and guarded**
4. **[C] Add Back on steps 2 → 1 (and keep 3 → 2); record a `furthestStep`/visited set in the bookmark so
   completed steps stay reachable.** *(Risk: none for UI; spec-aligned.)*
5. **[C] Use `router.replace` for forward skips and add a small step-transition guard** that validates
   preconditions before honoring a route (deep link / browser Back). For step 3 ensure a space context
   exists (server default via `getSpaceId` fallback) before server calls. *(Risk: changing push→replace alters
   browser-Back behavior; verify it does not violate the "reloading resumes on the same step" contract —
   REQ in `08-ui-ux.md` §8.2.)*
6. **[G] Persist draft text (space/bot name and purpose) across remount, never the API key.** *(Risk: stale
   drafts; clear on finish.)*

**P2 — close partial-completion gaps**
7. **[C] Add an explicit load-error state for the step-3 `Promise.allSettled`:** distinguish "catalog empty"
   from "catalog failed"; on failure show a retry button and stop leaving Finish silently disabled. *(Risk:
   a new i18n key and a retry path that must be covered by tests.)*
8. **[C] Make connect atomic:** do not clear `apiKey` until the refreshed credentials confirm the connection;
   re-fetch credentials on refresh failure; treat connect as idempotent; add `AbortSignal` + timeout. *(Risk:
   holding the key in state longer — keep it out of logs/URL and never persist it.)*
9. **[C] Add a ref-based in-flight lock** and gate the Enter handlers on `busy` in addition to the disabled
   buttons. *(Risk: must preserve keyboard-only flow — a11y gate; verify with axe.)*
10. **[G]/DEVIATION — reconcile the "space without a model" invariant:** either require a connected model
    before Finish when a space/bot exists, or persist an explicit "model deferred" acknowledgement that
    `/app` surfaces. *(Risk: behavior change — log in `docs/PARITY.md`, request operator approval before
    shipping.)*

## 3. Verification (test-first, REQ-tagged Playwright)
- **[C]** `REQ-042`: create space → reload → bookmark still holds `spaceId` (catches P0-1); storage blocked →
  notice shown (P0-2).
- **[C]** `REQ-052`: catalog+credentials rejected → retry affordance visible, no dead end (P2-7); connect then
  refresh failure → key retained, accurate connected state, no duplicate (P2-8).
- **[G]** Onboarding step contract (`08-ui-ux.md` §8.2): skip step 1 then Back to step 1 (P1-4); deep-link
  `/onboarding/3` does not lose space context (P1-5); double-Enter creates exactly one bot (P2-9); Finish
  state matches the model invariant (P2-10). Cover reload-resume and browser Back, light/dark, 360 px.
