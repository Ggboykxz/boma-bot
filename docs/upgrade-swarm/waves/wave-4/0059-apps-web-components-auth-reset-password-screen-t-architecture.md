# v0.4.0 Architecture Proposal — `reset-password-screen.tsx`

- Worker: 059 · Module: `apps/web/components/auth/reset-password-screen.tsx`
- Lens: architecture · Status: PROPOSAL (CONFIRMED by file read; no code changed)
- Related: REQ-040 (spec/08-ui-ux.md §8.2), Playwright `tests/auth.spec.ts` (mismatch + submit paths)

## Current structure (CONFIRMED)

Single client component (`"use client"`) with four concerns in one function:

1. **Route concern** — `useSearchParams()` reads the opaque `token`; missing token short-circuits to a standalone `AuthShell` branch with a link to `/forgot-password`.
2. **Validation concern** — `localValidation()` re-checks `MIN_PASSWORD_LENGTH = 10` and `password === confirm` locally, mirroring the server.
3. **Mutation concern** — `submit()` calls `authResetPassword({ token, password })`, then `router.replace("/app/")`; every rejection is folded into one generic `t("reset.error.invalid")`.
4. **Presentation concern** — two hand-written `<label>/<input>` pairs, a `role="alert"` error line, a primary `Button`, and a back link, all inside `AuthShell`.

State is a flat quartet: `password`, `confirm`, `status` (`idle | submitting | succeeded | failed`), `error`. `status === "succeeded"` is set but never rendered — it is reset by navigation, so the value is effectively dead.

Observed architectural smells: an unused `RiErrorWarningLine` import (line 4); duplicated input `className` strings; `submit()` does not guard against re-entry while `status === "submitting"`; and failure classification is lost because `catch {}` discards the error.

## Target structure

Keep it a single screen component (the route is thin and parity-bound), but split concerns into pure, testable units and reuse installed BoardUI form primitives:

- `validateReset(password, confirm, t)` — pure function returning a translated message or `null`; unit-testable, no React.
- `classifyResetError(err, t)` — maps an API error to a stable key; keeps the generic message as the safe default so no token/oracle detail leaks.
- `PasswordField` — a small local wrapper over the installed `@/components/base/input/input` (`TextField`/`Input` on `react-aria-components`), reused for both fields.
- The component then composes: read token → early missing-token branch → render form with the two fields + error + submit + back link.

BoardUI tokens stay exactly as today: `bg-background-full`, `bg-background-primary-default`, `border-border-button-default`, `text-text-primary/-secondary/-error-primary`, composite `text-title-3-semibold` / `text-body-regular` / `text-body-2-medium`, `rounded-3xl` shell card and `rounded-md` fields. No raw palette or hex. `cx()` for any merged classes; focus via `boma-focus` / `focus-visible:ring-border-focus-ring`.

## Concrete upgrades (with risks)

1. **Adopt BoardUI `Input`/`TextField` for both password fields.** Replaces raw `<input>` (and the duplicated class strings) with the react-aria component, gaining `aria-labelledby`/`aria-invalid`, RTL and autofill handling, and a single styled field definition.
   - *Risk:* React Aria may change label/field DOM, breaking `getByLabel(..., { exact: true })` selectors in `auth.spec.ts` and axe assertions. *Mitigation:* keep the exact `t("reset.password")` / `t("reset.confirm")` labels and run the Playwright auth spec + a 360 px light/dark axe pass.

2. **Extract pure `validateReset` and `classifyResetError`.** Removes validation/error logic from the render closure and makes the local/server rule drift visible in one place.
   - *Risk:* changing message selection could alter the tested mismatch/short strings or leak server detail. *Mitigation:* return the same `reset.error.*` keys; keep `reset.error.invalid` as the catch-all default.

3. **Make the submit path re-entry-safe and the status model honest.** Add an early `if (status === "submitting") return;` guard and drop the unused `succeeded` value (navigate immediately on success), or render a brief success state.
   - *Risk:* a visible success state changes UI golden timing and could conflict with the immediate `router.replace`. *Mitigation:* prefer the guard + remove dead state; any success UI is a separate DEVIATION noted in `docs/PARITY.md`.

4. **Honor a safe `next` redirect target (parity with sign-in).** Sign-in accepts a same-origin `next`; reset always hardcodes `/app/`. Adopt the same origin-checked helper so a reset can return to the requested page.
   - *Risk:* an unvalidated `next` would enable open redirect. *Mitigation:* reuse the exact `startsWith("/") && !startsWith("//")` rule from `sign-in-screen.tsx`; keep `/app/` as fallback.

5. **Remove the unused `RiErrorWarningLine` import and align the missing-token link label.** Use a reset-specific "request a new link" string instead of reusing `t("forgot.submit")`, and drop the dead import shared with `forgot-password-screen.tsx`.
   - *Risk:* a new i18n key must be added in all three locales (`en`/`fr`/`he`) or the screen shows a raw key. *Mitigation:* add the key to `lib/i18n.ts` for every locale together with the refactor.

## Sequence and verification

- Steps 5 → 1 → 2 → 3 → 4 (cheapest/lowest-risk first); commit per step with a Conventional Message mentioning REQ-040.
- Verify: `cargo fmt --check` / clippy are N/A here; run the web typecheck/lint, `tests/auth.spec.ts`, UI golden replay, and a 360 px RTL light/dark axe pass before marking done.

## Decisions needed

- Is a brief success confirmation desired, or is immediate `router.replace("/app/")` a fixed parity requirement? Default (GUESS): no new success state; keep current navigation.
