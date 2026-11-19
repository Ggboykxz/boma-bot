# 0033 — apps/web/app/onboarding/[step]/page.tsx — architecture (v0.4.0)

Worker #033 · module `apps/web/app/onboarding/[step]/page.tsx` · lens: architecture · READ-ONLY
review of `rebuild-workspace/.worktrees/before-m5` at v0.3.0-era source. Proposals only; no code.

## What the file is — and where the real mass lives

`page.tsx` is ~35 lines and correct for ADR-013 (Next.js static export, `trailingSlash`, one
carrier per pre-rendered step): a server component that exports `generateStaticParams()` for
`1 | 2 | 3`, awaits the `params` promise (Next 15 style), coerces via `parseStep()`, and renders
`<OnboardingScreen step={...} />`. `/onboarding` itself (a sibling `page.tsx`) redirects to
`/onboarding/1/`. The comment block cites spec/08-ui-ux.md §8.2, REQ-042, REQ-052.

The actual mass lives one level down in `components/onboarding/onboarding-screen.tsx` (~350
lines, `"use client"`), which currently owns four jobs in one component:

- **Routing** — `router.push('/onboarding/<n>/')` for skip/back/forward, `router.push('/app/…')`
  on finish. No other path-aware logic (deep links land directly on the requested step).
- **Persistence** — a hand-rolled `localStorage` bookmark (`boma-onboarding`, `{version: 1,
  spaceId?, botId?}`), with tolerant parse/self-heal (`loadProgress`/`saveProgress`) and a
  `setSpaceId()` side effect so step 3 talks to the server under the chosen space.
- **Data** — `modelsCatalog` + `modelsCredentials` in `useEffect` with `AbortController`,
  `Promise.allSettled`, manual `useState` orchestration (no cache, no dedup, no retry policy;
  see also the repo-wide absence of a query layer noted in wave-2 reviews).
- **Markup** — header with progress dots (`aria-current="step"`), three inline `<section>`s,
  raw `<input>/<select>`s styled with BoardUI tokens (`text-title-3-semibold`,
  `bg-background-primary-default`, `border-border-button-default`, `boma-focus`,
  `text-foreground-icon-primary`, `text-text-*`, `bg-background-accent-default`), and
  `@remixicon/react` icons. `error` is shared across steps.

## Structural findings (CONFIRMED by reading source)

1. **Silent step clamping.** `parseStep` maps `"2"→2`, `"3"→3`, everything else→1. In a static
   export only `1/2/3/index.html` exist, so the default branch is dead in prod and live only in
   dev/tests — a config or link bug silently renders step 1 instead of failing. The page also
   does not declare `dynamicParams = false`, so the contract "only these three steps exist" is
   enforced by the filesystem convention, not by code.
2. **Duplicated step model.** `STEPS: readonly Step[]` (screen) and `STEPS` + `StepParam`
   (page) are two hand-maintained copies of the same enum, plus a third `1 | 2 | 3` return
   type from `parseStep`. Drift risk when a step 4 (e.g. "invite teammates", which REQ-042
   invites naturally extend to) appears.
3. **No routing invariants.** Steps 1 and 2 are skippable by design (spec §8.2), but the page
   does not resume-or-redirect: an explicit deep link to `/onboarding/3/` with an empty bookmark
   is legal and lands on a half-usable step 3; the resume policy ("reloading resumes on the
   same step") is reduced to localStorage persistence, with no defined precedence between
   explicit URLs and the bookmark.
4. **One component, no seams.** Fetch, mutations (`spacesCreate`, `botsCreate`,
   `modelsConnect`), progress dots, and all three sections share local state; nothing is
   unit-testable without a DOM, and the error banner is shared mutable state whose lifetime is
   easy to get wrong on step transitions.
5. **Finish contract is soft.** `finish()` clears the bookmark and pushes `/app/<botId>/` or
   `/app/`; spec §8.2 says finishing lands on `/app/:botId` with the bot's intro message
   streaming — if the user skipped step 2, the product silently degrades to the bot-less home
   instead of an explicit "finish anyway?" state.

## Upgrades for v0.4.0 (keep BoardUI tokens throughout)

1. **Fail loudly on unknown steps.** Export `dynamicParams = false` from `[step]/page.tsx` and
   replace the silent default with `notFound()` (dev/tests) — pairs with the Rust static
   handler, which already 404s unknown `/onboarding/*` paths. Kills the dev-only divergence and
   makes the route contract machine-checked.
   *Risk:* deep links from pre-0.4 bookmarks/emails to non-1/2/3 paths flip from "silently
   step 1" to 404. *Mitigate:* keep a one-release redirect map (`/onboarding/0/` → `1/`) in the
   Rust handler or a client effect, then drop it.
2. **Single source of truth for the step enum.** Extract `STEPS`, `Step`, `parseStep` into
   `components/onboarding/steps.ts` (pure, no React); page and screen import from it. Adding a
   step becomes one edit. *Risk:* low; watch import cycles — the module must not import the
   client screen.
3. **Split the screen into shell + per-step bodies.** `onboarding-screen.tsx` keeps the
   progress header (`aria-current="step"`, add a visually-hidden "Step N of 3" via
   `text-text-tertiary`), and delegates to `step-space.tsx`, `step-bot.tsx`, `step-model.tsx`,
   each with its own error state and form; a tiny `useReducer` models `idle → busy → error |
   saved` instead of shared `error`/`busy` pairs. Retain token classes verbatim (`boma-focus`,
   `bg-background-*`, `border-separator-border`, `text-headline-medium`, …). *Risk:* focus
   management regressions (`autoFocus`, Enter-to-submit) and localStorage bookmark drift — gate
   on the existing UI goldens (they count reachable controls) and add keyboard-path smoke
   tests before/after.
4. **Define resume precedence explicitly.** Policy: explicit `/onboarding/<n>/` wins; a bare
   `/onboarding/` redirects to the **farthest reached** step from the bookmark (not always 1);
   bookmark parse failure still self-heals to step 1. Implement the redirect in
   `app/onboarding/page.tsx` as a client component (server cannot see localStorage), bounded by
   a one-shot guard to avoid loops. *Risk:* redirect loops on corrupt bookmarks and flicker on
   hydration — mitigate with the bounded guard and `Promise.resolve().then(...)` pattern the
   screen already uses; verify with a cleared/corrupt/step-3 bookmark matrix.
5. **Make the finish contract explicit.** If `botId` is absent (steps skipped), show a
   confirmation sheet ("finish without a bot?") with the BoardUI secondary/ghost buttons
   instead of silently routing to `/app/`; when `botId` exists, land on `/app/<botId>/` per
   spec. *Risk:* extra UI surface moves the golden-control count; spec §8.2 conformance must
   be re-checked (finishing is defined to land on `/app/:botId` — a bot-less finish is an edge
   the spec does not bless, so keep it behind the confirmation rather than inventing a route).

## Risks and constraints

- Static export (ADR-013): no dynamic server logic in the page; anything reading localStorage
  or `window` must stay client-side and behind hydration (`useEffect` / `Promise.resolve()`).
- UI goldens count reachable controls: any new buttons/dialogs in upgrades 3 and 5 will move
  `golden-status.json`; land them with updated goldens, not as silent deltas.
- REQ-042/REQ-052 semantics are unchanged: spaces, bot, catalog/credentials are server-authored;
  the web app only orchestrates. Keep all copy behind `useTranslation` keys; do not hardcode
  strings, including the `<provider key>` placeholder.
- Do not introduce a data-fetching library in this module to fix finding #4/#5 — adopting
  TanStack Query is a cross-cutting ADR-scale decision (noted in wave-2); scope it here only if
  v0.4.0 adopts it globally.
- Out of scope: making step 3 non-skippable is already the designed behavior (steps 1–2
  skippable, model connection required via `canFinish`); preserve it.

## Suggested landing order

2 → 1 (trivial, mechanical) → 4 (policy) → 3 (structural) → 5 (product-visible, needs spec
sign-off). Each step is independently shippable; 3 and 5 must ride with updated goldens.
