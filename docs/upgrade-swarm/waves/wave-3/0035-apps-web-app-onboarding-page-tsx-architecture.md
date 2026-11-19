# 0035 — app/onboarding/page.tsx — architecture (v0.4.0)

Worker #035 · module `apps/web/app/onboarding/page.tsx` · lens: architecture · READ-ONLY (proposals only)

## What the file is

Six lines: a server component whose entire job is `redirect("/onboarding/1/")` (spec §8.2,
REQ-042, REQ-052). It is the entry screen for the first-run wizard and nothing else — no
markup, no data, no auth. The real mass lives one level down:

- `app/onboarding/[step]/page.tsx` — `generateStaticParams()` for steps `1..3`, an async
  server component that awaits `params`, coerces the raw segment via `parseStep` (anything
  unrecognised silently becomes `1`) and renders `<OnboardingScreen step={…} />`.
- `components/onboarding/onboarding-screen.tsx` (~334 lines, `"use client"`) — the wizard:
  a `localStorage` bookmark (`boma-onboarding`, `{version:1, spaceId?, botId?}`), space
  creation, bot creation, model-provider connect, visible progress dots, finish →
  `/app/:botId/`.
- `next.config.ts` — `output: "export"`, `trailingSlash: true` (ADR-013): every route is a
  pre-rendered directory with an `index.html`, served by the Rust static handler.

## Structural findings (CONFIRMED by reading source / build output)

1. **The entry redirect is a client-only navigation with no no-JS fallback.** In a static export,
   `redirect()` in a server component emits a document whose body carries a `NEXT_REDIRECT`
   payload executed by the client router (documented Next.js static-export behaviour; the
   `out/` directory is not present in this worktree, so the exact bytes were NOT re-verified
   here — treat the literal `out/onboarding/index.html` inspection as GUESS). There is no
   `<meta http-equiv="refresh">` and no static anchor; a visitor without JavaScript, or before
   hydration, sees an empty document. It is also not an HTTP redirect — the Rust static host
   serves a 200 HTML stub.
2. **Resume semantics are broken at the entry.** Spec §8.2 promises reloading resumes on the
   same step. The wizard honours this only when the user lands directly on
   `/onboarding/<step>/`; the bare `/onboarding` entry always lands on step 1, ignoring the
   `boma-onboarding` bookmark (spaceId → step 2, botId → step 3). Sign-up defaults
   `next` to `/onboarding/1/` with the same hardcoded step.
3. **No access gate at the entry.** The route table (spec §8.1) grants
   `/onboarding/:step?` to "signed in, no space yet". The only guard in the web app is the
   root-level `UnauthorizedRedirect` (401 → `/sign-in?next=…`) in `app-providers.tsx`; nothing
   sends a signed-in user who already has a space away from the wizard, nor does anything
   distinguish "anonymous" from "returning operator" at this route.
4. **`parseStep` swallows invalid segments.** In a static export, `/onboarding/9/` does not
   exist and the host 404s — so the `default: return 1` branch is unreachable in production
   yet actively misleading in review. Step parsing is duplicated knowledge with the entry
   redirect (both hardcode the step space) with no shared module.
5. **The entry is invisible to a11y tooling.** A zero-markup redirect document has no
   landmark, no `<title>` beyond the default, no busy state — fine for a human with JS,
   untracked by `a11y.spec.ts`-style checks.

## Proposed upgrades for v0.4.0 (in dependency order)

### U1. Make the entry resume-aware and client-driven
Replace the server `redirect()` with a thin server wrapper (same pattern as `[step]/page.tsx`)
rendering a client `<OnboardingEntry />` in `components/onboarding/`. It reads the
`boma-onboarding` bookmark and `router.replace`s to the furthest unfinished step
(no bookmark → `/onboarding/1/`; `spaceId` only → `/2/`; `botId` → `/3/`), sharing a pure
`resolveResumeStep(saved): Step` helper that is unit-testable without a DOM.
- Risk: a client-only hop means one extra frame of nothing. Mitigate with a minimal branded
  waiting state (`bg-background-full`, `RiSparkling2Line`, `aria-busy`, composite type like
  `text-title-3-semibold`), `replace` not `push` (no history-loop trap), and a Playwright
  test asserting resume-at-entry for the three bookmark shapes. `sign-up-screen.tsx`'s
  hardcoded `/onboarding/1/` default should consult the same helper (cross-module note).

### U2. Give the static document a no-JS path
Emit `<meta http-equiv="refresh" content="0; url=/onboarding/1/">` plus a visible
`<a href="/onboarding/1/">` fallback inside the entry screen's static HTML (directory-style
URL, consistent with `trailingSlash: true` and the Rust handler). no-JS users then land on
step 1 — correct, since localStorage resume is impossible without JS anyway — and the 307
payload remains the JS fast path.
- Risk: meta refresh + client replace can double-navigate; harmless with `replace`, but the
  goldens (`out/onboarding/index.html`) and any smoke test asserting on the document must be
  updated in the same change. Do not hand-write the meta outside React; a small server
  component wrapper keeps it inside the export.

### U3. Add the missing access gate (or record the decision)
Before the wizard renders — entry and ideally `[step]` — check session + spaces (one
`spacesList`-style RPC, mirroring the `modelsCatalog`/`modelsCredentials` pattern in the
wizard): anonymous → `/sign-in?next=/onboarding/1/`; signed-in with a space → `/app/`. The
route table promises this; today it is unenforced on the client. If the Rust static handler
already gates `/onboarding*` at the edge, verify that instead and write it in docs/STATE.md —
GUESS today.
- Risk: an extra round-trip on every wizard entry; a false "has space" reading could bounce a
  brand-new user to `/app/` with nothing there. Fail open to the wizard, cache the spaces
  answer for the session, and stop on 401 via the existing `setUnauthorizedHandler`. This is a
  behaviour change in the sense of §8.1's access column — flag it in docs/STATE.md under
  decisions needed, not as a silent DEVIATION.

### U4. Single module for the step space
Extract `STEPS`, `Step`, `parseStep`, `resolveResumeStep` into
`lib/onboarding/steps.ts`; `[step]/page.tsx` imports from it, entry imports from it, and
`parseStep` becomes `notFound()`-aligned (or at least explicitly documented as unreachable
defence) instead of silently coercing to 1. Keep `generateStaticParams()` byte-identical —
ADR-013's single-shell contract depends on the `1..3` set staying exactly `{1,2,3}`.
- Risk: low; pure refactor. Goldens and `tests/onboarding.spec.ts` pin the visible behaviour,
  so they must stay green unchanged; any diff is a signal the extraction leaked.

### U5. Make the entry document a first-class screen, not dead air
While U1/U2 are in flight, treat the entry like any other BoardUI surface: semantic tokens
only (`bg-background-full`, `text-text-primary/-secondary`), one landmark, a
`useDocumentTitle` ("Welcome — Boma Bot" via the i18n layer, never hardcoded), focus ring on
the fallback link (`outline-none focus-visible:ring-2 focus-visible:ring-border-focus-ring`),
and `prefers-reduced-motion` respected (no animated shimmer — a static mark + progress is
enough). Logical CSS properties (`ps`/`pe`, `ms`/`me`) so the entry is correct under
`dir="rtl"` from day one (REQ-057).
- Risk: scope creep into visual redesign; keep it to structure and tokens. Any new i18n key
  must land in `lib/i18n.ts` for every shipped locale or the build's key-parity check fails.

## Out of scope / notes

- Step-3 skip gating, the invisible `noProvidersReachable` finish path and `role="alert"`
  placement are `onboarding-screen.tsx` concerns — separate worker's module. The raw
  `<select>`/raw `<input>` usage there bypasses the installed BoardUI form components and
  should ride this wave but is not this file.
- Cross-module: `components/auth/sign-up-screen.tsx` (`next=/onboarding/1/`) and
  `app/page.tsx` (signed-in redirect) should share U1's resume helper.
- Verification status: claims CONFIRMED against source (`page.tsx`, `[step]/page.tsx`,
  `onboarding-screen.tsx`, `next.config.ts`, `sign-up-screen.tsx:30`, spec §8.1/§8.2), except the
  exported `out/onboarding/index.html` bytes (no `out/` in this worktree; GUESS) and the
  edge-level access-gate question (U3, GUESS pending docs/STATE.md). REQ-042/REQ-052 goldens are
  the acceptance bar; Playwright `tests/onboarding.spec.ts` must gain the resume-at-entry cases.
