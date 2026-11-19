# 0087 — apps/web/components/shell/landing-screen.tsx — architecture (v0.4.0)

Worker #087 · module `apps/web/components/shell/landing-screen.tsx` · lens: architecture · READ-ONLY.
Proposals only; no code changed. Line refs against `before-m5`.

## What the file does (CONFIRMED)

A 57-line `"use client"` component mounted only by the thin root server page (`app/page.tsx:7-9`).
On mount it fires one `botsList()` probe (`rpc.ts:423`) with an `AbortController`; a resolved answer
means "signed in" → `router.replace("/app/")`; any rejection — 401, 500, offline, late abort —
sets `probeFailed` and → `router.replace("/sign-in")`. Cleanup aborts on unmount, so the effect
fires once even under React strict mode. Until the answer arrives it renders a static landmark:
centered `main`, `/icon.svg` mark, `app.name`, `landing.title`, `landing.body`, and a `ButtonLink`
whose href is `/app/` until failure then `/sign-in`. Spec §8.1 only requires "/" to redirect to
`/app` when signed in, else to the sign-in screen.

## Boundaries & debt this lens sees

- A roster data procedure (`bots.list` → `BotsListReply`) is abused as an auth oracle: the landing is
  coupled to the bots RPC shape and pays for a roster fetch just to classify a session.
- Every rejection is treated as "not signed in"; a network drop or 5xx sends a signed-in member to
  sign-in and flips the CTA. No branch on `RpcError.code` (`rpc.ts:169`).
- Destination policy is duplicated in three places: sign-in sanitizes a `next` param
  (`sign-in-screen.tsx:31-34`), rpc.ts has `setUnauthorizedHandler` → `/sign-in?next=...`
  (`rpc.ts:913-925`), landing sends a bare `/sign-in` and loses any deep-link context.
- One boolean cannot express probing / signed-in / signed-out / error; the CTA guesses `/app/` before
  the probe resolves — an inaccurate affordance for an anonymous visitor.
- The brand mark + centered public layout is a third copy (cf. `AuthShell` #0055, `NotFoundScreen`),
  each repeating the static-export `<img>` + eslint-disable comment.

## Proposed upgrades (v0.4.0)

### U1. Cheap, typed session probe instead of the roster oracle
Replace `botsList()` with a cheap session-status read. Prefer the existing `GET /api/auth/capabilities`
(already used by sign-in, exposes `localFirst`, REQ-066) over inventing a new endpoint. Extract
`resolveLandingSession(signal)` in `lib/` returning a typed outcome (`authenticated | anonymous |
local-first`); branch on `RpcError.code === "UNAUTHORIZED"` for the anonymous case and keep
transport/5xx as a distinct error state.
- Risk: a new endpoint is a DEVIATION (docs/PARITY.md, owner approval); reusing capabilities avoids
  it. Capabilities currently also answers 200 for anonymous callers, so confirm it distinguishes a
  real session before relying on it (GUESS). Keep the same `AbortController` cleanup.

### U2. One source of truth for auth destinations
Add `lib/auth-navigation.ts` with `appPath()`, `signInPath(next?)`, `safeNext()`; port the sanitizer
from sign-in (accept only same-origin paths starting `/` but not `//`). Landing then sends a
`next`-bearing sign-in path instead of a bare `/sign-in`, and sign-in plus the rpc unauthorized
handler import the same builders. Normalize the `/app` vs `/app/` trailing-slash convention once.
- Risk: changing redirect targets drifts UI goldens and the ADR-013 static fallback; keep default
  outputs byte-identical and port the sanitizer verbatim — never introduce an open redirect
  (REQ-040/041).

### U3. Explicit phase union instead of `probeFailed`
Model the effect result as a discriminated phase (`probing | signed-in | signed-out | error`),
ideally via `useReducer`, and derive both the redirect side-effect and the CTA href from it. Keep a
"probing" render that does not claim `/app/` until resolution, removing the CTA flicker.
- Risk: extra phases mean extra golden snapshots; keep the resolved happy path byte-identical and make
  the probing state visually identical to today's first paint. The effect must still fire once and
  never redirect an aborted/unmounted run.

### U4. Extract a shared public-landing shell / brand mark
Pull the centered `main`, `/icon.svg` mark + `t("app.name")` and the token set into a reusable
`PublicLandingShell` (coordinate with wave-4 #0055 `CenteredPublicShell`) and a single `BrandMark`
that owns the static-export `<img>` + eslint-disable. Landing keeps only `landing.title/body/cta`.
- Risk: a competing abstraction with #0055; align naming before merging. DOM must stay byte-identical
  or UI goldens and `@axe` snapshots drift.

### U5. Shrink the client island
Keep the pure landmark in a server component and isolate the probe/redirect into a small client child
with no markup (`LandingRedirect`), so only the interactive part ships as client JS (bundle budget,
ADR-013 static export).
- Risk: `useTranslation()` depends on `DirectionProvider`'s locale (REQ-057); moving `t()` to the
  server must use the same catalogue/direction or RTL breaks. Verify locale flow before demoting.

## Execution order & gate

U2 first (destination policy), then U1 (probe), U3 (phase), then U4/U5 (structure/split). Each step:
unit tests for session classification and `safeNext`, Playwright landing path (REQ-040/041, sign-in
redirect) green, axe AA (single `main`, 360 px, light/dark, RTL), UI goldens green, and bundle budget
(docs/PLAN.md) unchanged.

## Non-negotiables

Keep the BoardUI tokens already in use: `text-text-primary`, `text-text-secondary`,
`text-body-strong`, `text-body-medium`, `text-title-2-medium`, `ButtonLink` from
`@/components/base/buttons/button`, `RiArrowRightLine` as a component reference, `rounded-3xl`.
No raw palette/hex or `dark:` literal overrides; preserve logical layout, RTL, dark mode via tokens,
and every `landing.*` catalogue string.

CONFIRMED by reading `landing-screen.tsx`, `app/page.tsx`, `sign-in-screen.tsx`, `auth-shell.tsx`,
`button.tsx`, `rpc.ts`, `i18n.ts`, spec §8.1/§8.2. GUESS: capabilities distinguishes anonymous
sessions, golden landmark assertions, the #0055 abstraction name — verify before implementation.
