# 0071 — components/onboarding/onboarding-screen.tsx — architecture (v0.4.0)

Worker #071 · module `apps/web/components/onboarding/onboarding-screen.tsx` · lens: architecture · READ-ONLY review of `rebuild-workspace/.worktrees/before-m5`. Proposals only; no code in this change.

## What the file is

The ~334-line `"use client"` wizard rendered by the `[step]` route (spec §8.2, REQ-042, REQ-052). It is the whole flow in one component and owns four jobs:

- **Step routing** — `goTo()` (`router.push('/onboarding/<n>/')` for skip/back/forward) and `finish()` (clear bookmark, then `push('/app/<botId>/')` or `/app/`).
- **Persistence** — hand-rolled `boma-onboarding` localStorage bookmark (`loadProgress`/`saveProgress`, `{version:1, spaceId?, botId?}`) plus the module-global `setSpaceId()` side effect that scopes later RPCs to the chosen space.
- **Data + mutations** — `spacesCreate`, `botsCreate`, `modelsConnect`, and a `useEffect` that fans out `modelsCatalog` + `modelsCredentials` via `AbortController`/`Promise.allSettled`.
- **Markup** — progress `<ol>` with `aria-current="step"`, three inline `<section>`s, raw `<input>`/`<select>` fields styled with BoardUI tokens, and a shared `error`/`busy` state.

Sibling workers own `app/onboarding/page.tsx` (#0035: resume-aware entry) and `[step]/page.tsx` (#0033: unknown-step handling, shared step enum). This proposal stays inside the screen.

## Structural findings (CONFIRMED by reading source)

1. **No component seams.** Fetch, mutations, progress header, and all three forms share one state bag (10 `useState` hooks). Nothing is testable without a DOM; the wizard cannot be reasoned about step by step.
2. **State leaks across transitions.** `error`, `busy`, and every field value survive `goTo()`. Going back from step 3 to step 2 re-renders step 2 with the step-3 message still in its `role="alert"` and blank fields — the URL changed, but the view is a mix of two steps.
3. **Step 3 has a silent dead end.** `canFinish = connected > 0 || noProvidersReachable`, and `noProvidersReachable` is true only when the catalog *resolved empty*. If `modelsCatalog` rejects, `catalog` stays `null`, Finish stays disabled forever, and no message explains why (`allSettled` ignores rejections). Loading / error / empty / connected are not modelled.
4. **Raw fields bypass BoardUI form primitives.** `<input>/<select>` + `<label><span>` + one `role="alert"` replace the installed `Input`, `Select`, `Label` and `FieldError`, losing label↔input association, `aria-invalid` and RTL-safe behaviour. `placeholder="<provider key>"` is hardcoded user-visible text (i18n policy).
5. **Rules drift in the header.** Progress-dot classes use template-string concatenation instead of `cx()`; the completed-step check is `aria-hidden` with no visually hidden "completed" text, so state is conveyed by icon/colour alone.

## Proposed upgrades for v0.4.0 (BoardUI tokens preserved verbatim)

### U1. Shell + per-step bodies + a pure step model
Split into `onboarding-screen.tsx` (shell: `bg-background-full` page, header, progress `<ol>`) and `step-space.tsx`, `step-bot.tsx`, `step-model.tsx`. Extract a DOM-free `wizardReducer` (per-step draft + phase `idle | busy | error | saved`) and import the shared `Step` enum from the sibling #0033 `lib/onboarding/steps.ts` work.
- Risk: focus / Enter-to-submit regressions. Mitigate by preserving `autoFocus` and `onKeyDown` Enter semantics on each step's primary field, gated on `tests/onboarding.spec.ts` plus a keyboard-path smoke test before/after.

### U2. Scope phase and errors to the current step
Dispatch `STEP_ENTERED` in the `useEffect` keyed on `step` so drafts/errors are per-step; never render a foreign step's error. Switch progress-dot classes to `cx()` and add a visually hidden "Step N of 3" / "completed" via `text-text-tertiary`.
- Risk: bookmark writes must stay synchronous before `router.push` (as `saveProgress` does today); keep them in the shell, not in child unmount effects, or a fast Back loses the resume point.

### U3. Adopt BoardUI form components
Replace raw fields with `Input`, `Select`, `Label`, `FieldError` from `components/base/`; move the `<provider key>` placeholder behind an `onboarding.step3.api-key-placeholder` i18n key; let React Aria own `aria-invalid`/`aria-describedby`. Keep classes exactly (`boma-focus`, `bg-background-primary-default`, `border-border-button-default`, `text-text-primary`, `text-text-placeholder`, `text-text-error-primary`, `text-body-2-medium`).
- Risk: BoardUI `Select` renders a button trigger + popover, changing the golden control shape and the `getByPlaceholder` selectors in `tests/onboarding.spec.ts`. Land with updated goldens/tests; do not swap the native `<select>` silently.

### U4. Model step-3 async state explicitly
Replace the two nullables with a small state (`loading | ready | empty | error | connected`): `error` renders a message plus a BoardUI secondary `Button` retry, and Finish stays disabled; `empty` (catalog genuinely reachable but empty) remains the sole legitimate bypass, per §8.2's unfulfillable "connect a model" case. Never enable Finish on an unresolved `null`.
- Risk: a retry path adds controls and moves the goldens. Do not let a fetch error become "continue anyway" — that would break §8.2's no-model prohibition; document the empty-catalog bypass in docs/STATE.md.

### U5. Make navigation, focus and finish explicit
Use `router.replace` for skips (no history trap), set a per-step document title through the i18n layer, move focus to the step `<h2>` on transition, and clear the bookmark only after `router.push` resolves. Cross-module: the bot-less `finish()` fallback to `/app/` still conflicts with §8.2's `/app/:botId` promise — keep the confirmation-sheet idea from #0033 here rather than inventing a route.
- Risk: `replace` alters Back semantics asserted by the goldens — confirm before changing. Clearing after navigation risks a stale bookmark on a fast reload; prefer clear-then-push with a one-shot guard.

## Constraints / verification

- Static export (ADR-013): all `window`/localStorage access stays in `useEffect`/handlers; retain the existing `Promise.resolve().then(...)` hydration pattern.
- Tokens unchanged: `bg-background-full/-primary-default/-secondary-default/-accent-default`, `text-text-on-accent`, `text-text-*`, `border-border-button-default`, `border-separator-border`, `text-foreground-icon-primary`, composites `text-title-3-semibold`/`text-headline-medium`/`text-body-*` and `boma-focus`; dark mode rides the tokens.
- i18n: every new string (API-key placeholder, hidden progress text) must exist for every shipped locale or the key-parity check fails.
- Acceptance bar: REQ-042/REQ-052 goldens and `apps/web/tests/onboarding.spec.ts` stay green; U3/U4/U5 ride with updated goldens.
- Status: findings 1–5 CONFIRMED against this file, `tests/onboarding.spec.ts`, `components/base/{input,select}`, `lib/api/rpc.ts:1191-1204` and spec §8.2 / REQ-042 / REQ-052. Upgrades are proposals, not verified implementations.
