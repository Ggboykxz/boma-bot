# v0.4.0 Architecture Proposal — `forgot-password-screen.tsx`

- Worker: 057 · Module: `apps/web/components/auth/forgot-password-screen.tsx` (77 lines)
- Lens: architecture · Status: PROPOSAL (CONFIRMED by file read; no code changed)
- Related: REQ-040 (spec/08-ui-ux.md §8.2), `tests/auth.spec.ts`, companions 0055 (AuthShell),
  0058 (reliability lens, same module), 0059 (reset-password architecture).

## Current structure (CONFIRMED)

One `"use client"` screen composed of four concerns inside a single function:

1. **Frame** — `<AuthShell title={t("forgot.title")} subtitle={t("forgot.subtitle")}>` supplies the
   centered page ground, brand header, `h1` title block and the `rounded-3xl` card.
2. **Form branch** (everything except `status === "sent"`) — one `<label>/<input>` pair, a
   `role="alert"` error line, a primary `Button`, `aria-busy` on the `<form>`, and a trailing
   `<Link>` back to `/sign-in` rendered *outside* the form.
3. **Success branch** — a `t("forgot.sent")` confirmation paragraph plus a *second* back affordance
   (a secondary `Button` to `/sign-in`), while the trailing `<Link>` from (2) is still rendered.
   Two identical `t("forgot.back")` actions therefore coexist in the success state.
4. **Mutation/state** — flat quartet `email`, `status: "idle"|"submitting"|"sent"|"failed"`,
   `error`. Rendering branches only on `sent`; `failed` changes no structure, only the error line.
   `submit` is a `useCallback([email, t])` calling `authForgotPassword({email: email.trim()})`.

Observed smells: an unused `RiErrorWarningLine` import (line 4); a hardcoded placeholder
`"person@example.com"`; a raw `<input>` whose class string is duplicated in sign-in/reset-password;
no semantic status container for the confirmation; and no `dir`/`<bdi>` handling for the address in
RTL. Structural hardening (latch, timeout, error codes) belongs to 0058 and is not repeated here.

## Token & copy surface (CONFIRMED)

- **Copy is already centralized** in `lib/i18n.ts` for all three locales (`en` 358–364, `fr`
  711–717, `he` 1063–1069): `forgot.title`, `forgot.subtitle`, `forgot.submit`,
  `forgot.submitting`, `forgot.sent`, `forgot.back`, `forgot.error.generic`, plus the borrowed
  `signin.email`. `useTranslation()` derives the catalogue from `useLocale()`, so locale and
  direction agree (REQ-057). No per-locale branch exists in this screen.
- **The only hardcoded user-visible text** is the email placeholder. Everything else goes through
  `t()`. `forgot.sent` deliberately keeps the conditional "if an account exists" phrasing — this is
  the enumeration-honesty contract and must survive any copy edit.
- **Semantic tokens in use (keep all):** `bg-background-full`, `bg-background-primary-default`,
  `text-text-primary`, `text-text-secondary`, `text-text-placeholder`, `text-text-error-primary`,
  `border-border-button-default`; composites `text-title-3-semibold`, `text-body-regular`,
  `text-body-2-medium`; helpers `boma-focus`, `boma-target`, `rounded-3xl`/`rounded-md`.
  No raw palette or hex today — preserve that.
- **RTL surface:** `he` is an active locale, so the address field must stay LTR inside the RTL page
  and the two back controls must mirror. The current raw input has no explicit `dir`.

## Target structure

Keep a single route-bound screen as the composition root, but make the states explicit and reuse
BoardUI form primitives:

- A discriminated `ForgotState` (`{kind:"form", error?:string} | {kind:"busy"} |
  {kind:"sent"}`) instead of a four-value `status` plus a separate `error`, so an error cannot
  coexist with `sent` and the view is a pure `switch` on `state.kind`.
- A `ForgotEmailField` local wrapper over installed `@/components/base/input` (`TextField`,
  `Input`, `Label`, `HintText`), shared in spirit with sign-in/reset-password, carrying the
  field tokens, `aria-invalid`, `type="email"`/`autoComplete="email"` and LTR isolation in one place.
- The confirmation becomes a single `role="status"` region with one canonical back action; the
  error line and alert semantics stay unchanged.

## Concrete upgrades (with risks)

1. **Make invalid states unrepresentable.** Replace `status` + `error` with the discriminated
   `ForgotState` above and drive the two branches from `state.kind`. *Risk:* purely internal, but
   any regression in which branch renders breaks the REQ-040 "same words" assertion. *Mitigation:*
   keep identical markup/copy per branch and replay `tests/auth.spec.ts` + UI goldens.

2. **Centralize the field on BoardUI `Input`/`TextField`.** Removes the duplicated raw-input class
   string and gains `aria-invalid`/label wiring and a single RTL/LTR rule. *Risk:* the installed
   `Input` uses `rounded-2lg`, `bg-background-tertiary-default` and inset focus rings, not this
   screen's `h-10 rounded-md border-border-button-default` surface — swapping changes pixels and
   the react-aria label DOM, drifting goldens and `getByLabel` selectors. *Mitigation:* either keep
   the current token classes via `fieldClassName`/`inputClassName` overrides, or record the visual
   change as a DEVIATION in `docs/PARITY.md` and re-run 360 px light/dark axe.

3. **Collapse the duplicate back affordance in `sent`.** Drop the secondary back `Button` (or the
   trailing `<Link>` in that branch) so the success state exposes one action, and give the
   confirmation a `role="status"` container. *Risk:* changes DOM and golden snapshots, and a live
   region can add screen-reader noise. *Mitigation:* coordinate with 0055's shell-level
   `aria-live` proposal so ownership is defined once; keep `forgot.back` wording unchanged.

4. **Close the copy surface.** Add a `forgot.email-placeholder` key (en/fr/he) to remove the
   hardcoded placeholder, and state the recovery token's single-use/~30-minute lifetime inside
   `forgot.sent` while preserving the conditional "if an account exists" prefix (complements 0058
   S5). *Risk:* new keys must land in all three locales or the screen shows a raw key; a lifetime
   sentence reworded without the conditional prefix becomes an enumeration oracle. *Mitigation:*
   add keys atomically in `lib/i18n.ts` and keep fr/he tone under review.

5. **Restore client-boundary and import discipline.** Remove the unused `RiErrorWarningLine`
   import, or adopt one shared error presentation (icon + `role="alert"`) across all four auth
   screens instead of importing it inconsistently. *Risk:* none for behavior; a shared error
   component touches sign-in/reset-password too, so land it as a separate, golden-verified change.

## Non-negotiables

Keep every BoardUI semantic token listed above, the `cx()` merge rule, and direction-agnostic
(physical) layout so RTL keeps working. No raw palette/hex, no hand-built second design system.
Do not add any known-vs-unknown distinction in copy or DOM — that is REQ-040 parity, not a
refactor target.

## Verification / decisions needed

CONFIRMED by source read only; no tests, builds or browser runs were executed for this review.
Any implementation must run the web typecheck/lint, `tests/auth.spec.ts`, the UI golden replay and a
360 px RTL light/dark axe pass before being marked done.

- GUESS (default): keep the current field appearance; treat BoardUI `Input` adoption (upgrade 2)
  as opt-in with a recorded DEVIATION rather than an automatic visual swap.
- Decide with 0055 whether the confirmation's `role="status"` lives in the shell or the screen.
