# v0.4.0 Architecture Proposal — `apps/web/components/auth/auth-shell.tsx`

Lens: architecture. Read-only review; nothing in `apps/web` was changed.

## Current structure (CONFIRMED)

`AuthShell` is a single presentational wrapper with `title: string`, `subtitle?: string`,
`children: ReactNode`. It renders a centered page ground (`bg-background-full`, `min-h-dvh`),
a hardcoded brand header (`<img src="/icon.svg">` + `t("app.name")` linking to `/`), and a
`main > header (h1 title + optional subtitle) > card (rounded-3xl, border-border-button-default,
bg-background-primary-default)` holding `children`. It calls `useTranslation()` (a `"use client"`
hook backed by `useLocale()`) but declares no `"use client"` of its own — it is pulled into the
client graph only because all importers are client components.

Consumers (CONFIRMED): the four auth screens (`sign-in`, `sign-up`, `forgot-password`,
`reset-password`) plus `invite/accept-screen.tsx`, `integrations/integrations-setup.tsx`, and
`artifacts/artifacts-grid.tsx`. Each screen invokes the shell **two to three times** — one for
the loading skeleton, one for the error/unavailable branch, one for the ready branch — and only
the ready branch passes `subtitle`.

## Proposed upgrades for v0.4.0

1. **Name the role honestly / split the generic frame.** The doc comment says "one frame for every
   public auth screen," yet invite, integrations setup and artifacts grid also use it. Extract a
   neutral `CenteredPublicShell` (ground + centered `main` + card) and keep `AuthShell` as a thin
   semantic wrapper that adds the brand header and auth title block.
   *Risk:* a rename touches ~7 import sites; keep `AuthShell` as an exported alias so existing
   imports keep compiling. Do not change rendered DOM, or UI goldens/`@axe` snapshots drift.

2. **Make phase states first-class to kill title/subtitle drift and CLS.** Loading and error
   branches repeatedly re-declare `<AuthShell title=...>`, and most omit `subtitle`; the subtitle
   line appears only when ready, shifting the card vertically (a CLS risk against the PLAN.md
   budget). Add either compound slots (`AuthShell.Skeleton`, `AuthShell.Error`) or a `phase`
   prop so the heading/subtitle are declared once and stay identical across phases.
   *Risk:* slots change children composition in every caller; migrate one screen at a time and
   replay `spec/golden/ui/*.json` after each. Reserved subtitle space must match the current
   `text-body-regular` line box exactly.

3. **Make the brand header configurable, not hardcoded.** Expose optional `headerHref` (default
   `/`) and `showBrand` props, and centralize the logo mark so new guest surfaces do not duplicate
   the raw `<img>` + eslint-disable. This is where invite flows can point "back" at the right
   entry point instead of always `/`.
   *Risk:* extra props widen the public API; keep defaults byte-identical to today's output.
   The static-export `<img>` comment must be preserved if it is replaced with `next/image`.

4. **Tie the card to the heading for accessibility.** Give the `h1` a stable `id` and add
   `aria-labelledby` to the card wrapper; wrap swapped error content in an `aria-live="polite"`
   region so phase changes are announced. (WCAG 2.2 AA is a gate; zero critical/serious axe
   findings.)
   *Risk:* adding live regions and ids changes the DOM/aria snapshot; re-run `@axe-core/playwright`
   and keyboard-only paths. Live regions must stay `aria-hidden` for skeletons to avoid noise.

5. **Restore an explicit client boundary.** Because `useTranslation()` is a client hook, either
   mark `AuthShell` `"use client"` or move translation fully to callers (they already pass
   translated strings) so the shell becomes a server-compatible presentational component and
   shrinks the client bundle.
   *Risk:* removing the hook is safe only if no consumer relies on the shell's internal
   `t("app.name")` for the wordmark; that string must be passed in. Verify the hook's provider
   requirement before demoting it — getting this wrong breaks locale/RTL agreement (REQ-057).

## Non-negotiables

Keep every BoardUI semantic token already in use: `bg-background-full`,
`bg-background-primary-default`, `bg-background-secondary-default`, `text-text-primary`,
`text-text-secondary`, `text-text-tertiary`, `text-text-error-primary`,
`border-border-button-default`, `border-separator-border`, `text-foreground-icon-primary`,
composite type utilities (`text-headline-medium`, `text-title-3-semibold`, `text-body-regular`,
`text-body-2-medium`), `cx()` for class merges, and the `boma-target`/`boma-focus` focus utilities.
No raw palette or hex values, no `dark:` overrides with literals. Preserve direction-agnostic
(logical) layout so RTL keeps working.

## Verification

CONFIRMED by reading the source only; no tests, builds, or browser runs were executed for this
review. Any implementation must re-run `cargo fmt --check` (repo-wide gate), Playwright UI goldens,
and the axe accessibility suite before it is considered done.
