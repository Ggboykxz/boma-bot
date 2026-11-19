# 0034 · apps/web/app/onboarding/[step]/page.tsx · LENS=reliability · v0.4.0 proposal

## Scope note

A 33-line async server component: `generateStaticParams` for `"1"|"2"|"3"`, a total `parseStep`,
`<OnboardingScreen step={…}/>`. It owns no state but is the only gate on which step HTML exists
and what an out-of-set param renders. Failures live here (param handling, prerender contract)
and one hop later in `onboarding-screen.tsx`; boundary items are marked [wizard] and
cross-reference wave-3/0036 (owner of the entry).

## Current behavior (as read; CONFIRMED by reading, nothing executed)

- Static export (`output: "export"`, ADR-013) pre-renders exactly `/onboarding/{1,2,3}/`.
  `parseStep` coerces **any** other string (`"7"`, `"abc"`, `"01"`, `""`, `" 1"`) to step 1.
- ADR-013: the Rust static handler maps unknown paths under the app to `index.html`, never a
  listing. A bogus `/onboarding/9/` deep link is served *something*; `parseStep` then renders
  step 1 rather than failing. Reachable, not theoretical.
- The wizard reads `localStorage["boma-onboarding"]` (`{version:1, spaceId?, botId?}`) only in
  `useEffect`; server/client first render match — no hydration mismatch today.
- Step 3 gates Finish on `canFinish = connected > 0 || noProvidersReachable`; both fetches run
  via `Promise.allSettled` with an AbortController, and both may reject silently.

## Failure modes

1. **Bad step param masquerades as step 1.** `parseStep`'s default branch hides broken links,
   mistyped URLs and handler fallbacks. Analytics and tests see "step 1" for a non-step-1 URL.
   No `dynamicParams = false`: "only 1/2/3 exist" is implied, enforced nowhere here.
2. **Partial completion state is invisible at the page boundary.** The page passes only `step`.
   Deep-linking to `/onboarding/3/` with empty or stale `spaceId` (space deleted server-side) is
   accepted silently; the failure surfaces later as a cryptic `modelsConnect` error, or never.
   `/onboarding/2/` with no space bookmark likewise "works" while creating an orphaned bot.
   Steps are spec-skippable, so gating is wrong — but *validating* is not. [wizard]
3. **Lost progress, two kinds.** (a) `saveProgress`/`loadProgress` failures are swallowed; a
   storage-full or private-mode user gets zero resume signal, silently, despite the
   "reloading resumes" contract (spec §8.2). (b) Form drafts (`spaceName`, `botName`) live in
   component state only; browser back/forward or an accidental refresh discards typed-but-unsubmitted
   input with no warning. Neither is a crash; both are silent data loss.
4. **SSR / prerender fragility.** Sound today (storage read in effect only), but fragile: any
   future "resume-aware" edit reading storage during render or in a `useState` initializer
   reintroduces hydration mismatch on a static page with no server to patch it. `autoFocus` inputs
   exist on all three pre-rendered steps; focus lands only after hydration regardless.
5. **Step-3 dead end with no error and no retry.** If `modelsCatalog`/`modelsCredentials` both
   fail or hang (no timeout beyond unmount-abort), `catalog` stays `null` →
   `noProvidersReachable` false; `credentials` stays `null` → `connected` 0 → `canFinish` false →
   **Finish permanently disabled, zero feedback**. The user is stranded on the one non-skippable
   step. `allSettled` was chosen for resilience; without a rejection path it is a silent trap.
6. **Double-fire before `busy` flips.** Enter handlers call `createSpace`/`createBot` without the
   `busy` check the buttons have; rapid Enter/Enter or Enter+click can issue duplicate
   `spacesCreate`/`botsCreate` (cf. 0036). Also, post-connect `modelsCredentials()` sits in the
   same `try`, so its failure reports as a *connect* failure — connect actually succeeded.
7. **`finish()` routes to a possibly-dead `botId`.** If the bookmarked bot was deleted server-side,
   `/app/<botId>/` is a dead route. Page can't see this; wizard can probe cheaply. [wizard]
8. **No error boundary for the segment.** A render throw inside `OnboardingScreen` yields Next's
   default (or a blank) with no recovery path; a segment `error.tsx` with `reset()` + link to
   `/onboarding/1/` is the cheap reliability net.
9. **Silent self-heal without persistence.** Healed version-mismatch is never written back, so a
   corrupt record is re-parsed forever; `spaceId`/`botId` are not shape-checked (`""`, numbers pass
   to `setSpaceId`).

## Proposed hardening (v0.4.0)

1. **Strict param gate (this file).** `export const dynamicParams = false`; rewrite `parseStep` as
   a type-guard over `STEPS` and call `notFound()` on any out-of-set value. Keep the three
   pre-rendered paths byte-identical. Risk: sloppy inbound links now 404 instead of landing on
   step 1 — verify against `tests/` and any captured analytics first; if any exist, ship
   `redirect("/onboarding/1/")` instead and note the DEVIATION in docs/PARITY.md. Also confirm
   the exported `out/onboarding/<x>/` behavior against the Rust fallback handler, not just the
   build.
2. **Segment `error.tsx` (new file, sibling of page).** Client component, `reset()` +
   `/onboarding/1/` link, strings through `useTranslation` (no hardcoded copy), token styling per
   BoardUI rules. Risk: an extra reachable control set may shift UI golden counts — coordinate
   with the golden fixtures (0036 flags the same class of change).
3. **[wizard] Step-3 failure visibility + timeout.** On `allSettled` rejection, set an inline
   error naming the failed call with a Retry button (re-run the effect body); add a ~8s
   `AbortSignal.timeout` composed with the unmount controller. Only when both calls fail *and*
   zero credentials exist may `canFinish` consider a `catalogFailed` escape hatch — but **do not**
   enable Finish on failure; spec (§8.2) makes the model step non-skippable, so the fix is
   visibility + retry, never bypass. Risk: retry button changes golden control counts; escape
   hatch rejected.
4. **[wizard] Idempotency at the call site.** Funnel Enter handlers through the same `busy` ref
   used by the buttons (`busyRef.current`), and move the post-connect `modelsCredentials()` into
   its own `try` so its failure reports "connected but refresh failed", not a connect error.
   Risk: minimal; keep `finally { setBusy(false) }` semantics.
5. **[wizard] Bookmark validation + honest write-back.** Shape-check `spaceId`/`botId`
   (non-empty string, reasonable charset) on load; persist the healed record so corruption does
   not recur; on `saveProgress` throw, set a visible "progress won't be saved" notice (one line,
   dismissible) instead of silence. Optionally warn on `beforeunload` while `busy`. Risk: the
   beforeunload prompt is aggressive — gate it strictly on in-flight RPC.
6. **SSR discipline (this file + wizard).** Keep all storage/env reads inside effects; if any
   resume logic lands in the page later, render a stable skeleton until mounted. Add a short
   comment in `page.tsx` stating the static-export contract so the next editor does not regress
   it. Risk: none if limited to comments/skeleton.
7. **Tests.** Playwright (REQ-042/REQ-052 suites stay green): `/onboarding/9/` → 404 (or
   documented redirect); step-3 with both RPCs mocked to fail → inline error + Retry restores
   the happy path; storage-disabled context renders + notice, no crash; double-Enter on step 1
   yields exactly one `spacesCreate`. Risk: route-mock brittleness — reuse existing fixtures.

## Explicitly rejected / deferred

- Gating `/onboarding/{2,3}/` on saved progress (steps are spec-skippable; needs spec owner).
- Enabling Finish when step-3 RPCs fail — violates the non-skippable model step (§8.2).
- Replacing localStorage resume with cookies/server state — impossible on static export (ADR-013);
  see 0036 for entry-level resume work.
- Fixing the `finish()` dead-`botId` route here — needs a `bots.get` probe; deferred to wizard,
  tracked there.

## Risks & rollout

Highest risk is #1 (404 vs redirect) — it is externally visible; decide redirect-vs-404 with the
owner before flipping, and log the choice in docs/PARITY.md. #2/#3 add controls and may shift UI
goldens. #4/#5/#6 are internal and low risk. Order: 6 → 1 (behind owner decision) → 4 → 5 → 3 →
2 → 7. No new dependencies; bundle impact negligible.

## Success criteria

- `/onboarding/<out-of-set>/` never renders step 1 (404 or owner-approved redirect, tested).
- Both-RPC-fail on step 3 yields a visible error and a working Retry — never a silently disabled
  Finish.
- Storage-disabled and corrupt-bookmark environments render the wizard with an honest notice and
  valid (post-heal) state.
- Double-Enter cannot issue duplicate creates; Enter and click share one serialization path.
- Existing REQ-042/REQ-052 Playwright tests unchanged and green (not run in this review).
