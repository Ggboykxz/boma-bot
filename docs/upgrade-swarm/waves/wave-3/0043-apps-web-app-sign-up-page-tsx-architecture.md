# 0043 — apps/web/app/sign-up/page.tsx — architecture — v0.4.0 proposal

Worker #043 (retry) · module `apps/web/app/sign-up/page.tsx` · lens: architecture ·
READ-ONLY review of `rebuild-workspace/.worktrees/before-m5`. Proposals only, no code.

## What the file is (CONFIRMED)

- `page.tsx` is 11 lines: a static-export friendly server shell, `<Suspense fallback={null}>`
  around `<SignUpScreen />` (spec §8.2, REQ-040). The `Suspense` is mandatory because the
  screen calls `useSearchParams()` in an `output: "export"` build (ADR-013); the `null`
  fallback leaves pre-hydration dead air.
- All behaviour lives in `components/auth/sign-up-screen.tsx` (client, ~168 lines): phases
  `loading → ready | error`, `authCapabilities()` on mount (AbortController + `reloadToken`
  retry), local-first fast path (`localFirst === true` → `router.replace("/app/")`, REQ-066),
  then a name/email/password form posting `authSignUp` with `credentials: "include"`.
- `next` sanitisation is correct (same-origin paths only, `//` rejected) and defaults to
  `/onboarding/1/`; that is the open-redirect fix and must stay byte-for-byte.
- Styling is entirely BoardUI semantic tokens (`bg-background-full`,
  `bg-background-primary-default`, `text-text-primary/-secondary/-tertiary/-error-primary`,
  `border-separator-border`, `boma-focus`, `boma-target`) and composite type utilities
  (`text-title-3-semibold`, `text-body-regular`, `text-body-2-medium`); no raw palette, no `dark:` forks.

## First-owner bootstrap — as wired today

- `Accounts::sign_up` writes user + org + first space + membership in one transaction
  (`crates/app/src/accounts.rs`): a fresh signup already yields the owner of their own
  org/space; onboarding then adds the first bot and model provider (CONFIRMED).
- The client has **no signal** for "is this the first account?". `AuthCapabilities` carries
  only `sso/passwordAuth/passwordReset/resetUrl/emailCode/billing/passkeyRpId/nativePasskeys/
  localFirst` (`crates/api/src/authcap.rs`, `spec/openapi.yaml`, `tests/capabilities.ts`).
  Therefore `signup.first-account` ("first account becomes its operator; later invites land
  as a member") is rendered to **every** signer, including the thousandth — the opposite of
  §8.2's "say so plainly rather than silently granting power" (CONFIRMED).
- `signup.closed` exists in all three locale catalogues but no component references it
  (grep finds only `signup.first-account`). The screen doc-comment promises a closed
  sign-up state that has no code path. STATE.md (M15.4) records that the old OpenBot-style
  `signup: "closed"` capability was dropped when the wire was corrected to the spec shape,
  so this string is leftover, not a pending feature (CONFIRMED).
- No deployment-level `operator` role grant exists in domain/app code (grep) — GUESS:
  "operator / later invites are members" is aspirational copy for the access-control slice.
  Confirm in docs/STATE.md before treating the wire as defective.

## Concrete upgrades (v0.4.0)

1. **Real Suspense fallback, not `null`.** Have `page.tsx` render
   `<Suspense fallback={<SignUpSkeleton />}>` with a token-matched `AuthShell` skeleton
   (`bg-background-secondary-default` bars) announced via `role="status"` / `aria-busy`,
   mirroring the sign-in proposal (0041-U1). Risk: static export + `useSearchParams` means the
   skeleton must stay CSS/token-only; mis-sized bars cause CLS against the LCP budget.

2. **Wire first-owner truth instead of guessing.** Add explicit capability flags —
   `signupOpen: boolean` and `firstAccountEligible: boolean` — to `AuthCapabilities`
   (Rust struct, `spec/openapi.yaml`, golden `auth-capabilities.json`,
   `tests/capabilities.ts`). Render `signup.first-account` **only** when
   `firstAccountEligible`; render `signup.closed` when `!signupOpen`; delete the misleading
   doc-comment. Risk: five-surface drift (wire + spec + fixture + golden + i18n ×3) lands in
   one change or the screen ships dead affordances again; also decide whether exposing
   "first account" is an enumeration concern (low: pre-auth, deployment-wide, not per-address).

3. **Share the onboarding resume helper for `next`.** Replace the hardcoded `/onboarding/1/`
   default with `resolveResumeStep()` from the onboarding steps module (0035-U1) while
   keeping the sanitizer byte-for-byte. Risk: new cross-feature import boundary; a circular
   import would be a smell — keep the helper pure under `lib/`. Any sanitizer edit reopens the
   open-redirect defect, so pin it with `tests/auth.spec.ts`.

4. **BoardUI form components + real a11y.** Replace the four raw `<input>` elements with the
   installed react-aria-based BoardUI form components; pass the active locale to
   `authSignUp` instead of the hardcoded `"en"` (line 75); add a `useDocumentTitle` via the
   i18n layer. Risk: BoardUI form component API/validation differs from raw inputs (test
   `required`/`type=email`/`autoComplete` parity), and any new i18n key must exist in all
   three catalogues or the key-parity check fails.

5. **Split submit failures without leaking identity.** Today one `catch` maps 409 conflict,
   429 rate-limit, and network/5xx all to `signup.error`. Map 409 → "address may already
   exist" (current copy), 429 → retry-later copy, 5xx/network → generic retry, keeping
   `role="alert"` and the existing retry path. Risk: over-differentiation becomes an address
   enumeration oracle — keep all account-shaped failures on one message, as sign-in does.

## Notes / non-goals

- No token in JS, ever: keep `credentials: "include"` and the httpOnly cookie; a
  Bearer-in-localStorage "upgrade" would be a regression.
- `localFirst` redirect, SSO/passkey/email-code affordances and password policy belong to
  sign-in / backend slices — do not expand this file into those.
- All new UI stays on BoardUI semantic tokens and composite type utilities; no raw palette,
  no per-screen `dark:` handling. Respect `dir="rtl"` and `prefers-reduced-motion`.
- Verification status: structure and copy claims CONFIRMED against source, spec §8.2 and
  STATE.md M15.4; the deployment-level operator role is GUESS pending confirmation.

## Suggested order

3 (small, kills the hardcoded step) → 2 (centerpiece: first-owner truth + dead string) →
1 (structure/a11y) → 5 (safety) → 4 (touches installed components and i18n). Each step extends
`tests/auth.spec.ts` rather than adding a harness; a new REQ-040 case per upgrade.
