# 0089 — apps/web/components/shell/not-found-screen.tsx — architecture (v0.4.0)

Worker #089 · module `apps/web/components/shell/not-found-screen.tsx` · lens: architecture ·
READ-ONLY of `rebuild-workspace/.worktrees/before-m5`. Proposals only; no code in this change.
Collaborators inspected: `app/not-found.tsx`, `app/layout.tsx`,
`components/providers/app-providers.tsx`, `components/shell/landing-screen.tsx`,
`components/foundations/direction/direction.tsx`, `components/base/buttons/button.tsx`,
`lib/use-translation.ts`, `lib/i18n.ts`, `spec/08-ui-ux.md` §8.1/§8.8/§8.9, `apps/web/tests/`.

## What already exists (keep, don't re-propose)

- Correct split: `app/not-found.tsx` (server) is a 6-line wrapper; the screen owns the markup.
  Same thin-route pattern as the activity/board pages. Keep it.
- The screen is mounted inside `AppProviders > DirectionProvider` via the root layout, so
  `useLocale()` (through `useTranslation()`) and `lang`/`dir` agree — REQ-057 holds (CONFIRMED).
- Tokens are already clean: `text-text-primary` / `text-text-secondary` and the composite
  `text-title-2-medium` / `text-body-medium`; `ButtonLink` is a real BoardUI base component,
  `variant="secondary"`. No raw palette classes, no hand-built type (CONFIRMED).
- Security-safe copy: the body says the address matches no screen, never echoes
  `window.location.pathname`, and never surfaces a correlation id — right for a navigation
  result, and consistent with §8.8's uniform unavailable screen (CONFIRMED).
- The `<main>` landmark + single `<h1>` give the page a correct accessibility skeleton.

## Structure and recovery affordances — the architectural seam

Two constraints collide here and should be made explicit before v0.4.0:

1. The source comment promises "one heading, one way back". That is a real invariant: the
   screen is the only chrome-less surface whose *entire* job is recovery, and a second link
   invites drift toward a mini-navigation menu. Treat any added control as a DEVIATION to
   record in `docs/PARITY.md` for owner approval, not a free UI tweak.
2. Recovery is a two-hop chain today: the sole link targets `/`, which mounts
   `LandingScreen`, which probes capabilities and only then replaces to `/app` or `/sign-in`
   (CONFIRMED). A signed-in member whose deep link broke loses their thread context and
   passes through a public landing page. That is the main UX debt, but fixing it by reading
   session state inside the screen would couple the static 404 to auth and break the uniform
   behavior §8.8 requires. The right seam is an injected destination with a safe default.

Two structural risks worth naming: the screen owns `<main>` and must only be mounted at a
boundary that supplies no other main landmark (a nested `not-found.tsx` under `AppShell`
would create two `<main>` elements and fail axe); and `"use client"` exists solely because of
the context hook, so the screen ships JS for a page with zero state — acceptable while the
locale is chosen at runtime from `localStorage`, but it should stay a leaf, not absorb data
fetching.

## Concrete upgrades

U1. **Parameterise the recovery target.** `NotFoundScreen({ recoveryHref = "/" })` passed from
    `app/not-found.tsx`; default keeps today's behavior byte-for-byte. Gives §8.8's uniform
    unavailable screen and any future nested boundary a reuse point without hardcoding a
    route in two places. Risk: low; a prop-only change with no DOM/role drift. Do not let the
    prop accept an attacker-controlled value — only literal, app-owned routes.

U2. **Extract the shared chrome-less shell.** `LandingScreen` repeats the same
    `mx-auto flex min-h-dvh max-w-xl flex-col justify-center gap-4 px-6 py-16` layout. Add a
    `CenteredScreen` primitive under `components/shell/` (slots: heading, body, actions; the
    `<main>` and landmark naming live in one place), consume it here and from `landing-screen`.
    Merge any conditional classes with `cx()` from `@/utils/cx`. Risk: low-medium; pixel drift
    on the landing route — verify both at 360 px, light/dark and RTL before removing the
    duplicated classes.

U3. **Give the document an identity.** Export `metadata` from `app/not-found.tsx`
    (`title: "Page not found · Boma Bot"`, `robots: { index: false, follow: false }`) so the tab
    and screen-reader page announcement differ from every other route and the 404 is not
    indexed. Risk: low but must be verified against `next build`'s emitted `404.html` head —
    static export may drop `not-found` metadata; if so, set it in the boot path instead. The
    title stays English like root metadata (the runtime locale switch cannot retitle static
    HTML) — acceptable, but note it as a known limitation, not a hardcoded *screen* string.

U4. **Tighten a11y without changing the one-link contract.** Add `aria-labelledby` linking
    `<main>` to the `<h1>`, and focus that heading (`tabIndex={-1}`) on client-side arrival so
    a failed `router.push` announces the new page instead of leaving focus on `<body>`. Keep
    the link's name and `role` unchanged. Risk: low-medium; focus stealing on a hard load is
    mildly disruptive and the client-vs-hard-navigation distinction is hard to detect
    (GUESS: Next's route announcer may already cover it — verify with a screen reader and an
    axe run before keeping the effect).

U5. **Optional second recovery affordance, gated.** A plain "Go back" `<button>` calling
    `history.back()`, rendered only when `history.length > 1`, would fix the stale-link case
    without echoing the path. This breaks the stated "one way back" invariant, is
    hydration-dependent (CLS risk), and is an externally visible behavior change: ship only
    as an opt-in prop behind an ADR/DEVIATION approval. If approved, add the key to all 14
    catalogues additively — only `en`, `fr` and `he` currently define `notfound.*`; the rest
    rely on the documented English fallback (CONFIRMED). Risk: medium, mostly governance.

## Non-goals for v0.4.0

No session-aware destination, no path echo or "details" disclosure, no support/report link,
no product mark on the 404, no changes to `LandingScreen` behavior beyond U2's layout move,
and no new Playwright assertions beyond a small 404 contract test pinning the `<h1>` name, the
single link, RTL and the absence of the requested pathname in the text.

## Risks and sequencing

Wave 1: U1, U3, U4 (self-contained, no visual regress). Wave 2: U2 after a screenshot diff of
the landing route. U5 stays a proposal until approved. Overall the module is structurally
healthy — the work is about making implicit seams (destination, shared layout, document
identity, focus) explicit rather than correcting broken behavior.

_Line references are against `.worktrees/before-m5`; this document makes no code, spec-hash or wire changes._
