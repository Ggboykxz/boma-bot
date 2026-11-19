# 0061 — apps/web/components/auth/sign-in-screen.tsx — architecture lens — v0.4.0 proposal

- Target: `before-m5` (v0.3.0-rc5), module `apps/web/components/auth/sign-in-screen.tsx` (181 lines, one `"use client"` component). READ-ONLY review; no product code touched.
- Scope note: sibling brief 0041 (wave 3) covers `app/sign-in/page.tsx` + the screen's session/cookie story. This brief stays on the **component's internal structure** — decomposition, form wiring, state machine, capability-to-UI mapping. Spec anchors: `spec/08-ui-ux.md §8.2`, REQ-040/REQ-041/REQ-066, BoardUI rules (components first, semantic tokens, composite type, `cx`).

## Structure today (CONFIRMED, read this session)

One `SignInScreen` owns everything: `next` sanitization, a `Phase = loading|ready|error` machine for capability fetch, a separate `Status = idle|submitting|failed` machine for submit, six `useState`s, an `AbortController` + `reloadToken` retry for `authCapabilities`, and all JSX (skeleton / unavailable / password form / secondary methods). `AuthShell` supplies the shared public frame. Capabilities drive visibility: password form on `passwordAuth`, permanent passkey *notice* on `passkeyRpId !== null || nativePasskeys`, SSO anchor on `sso`, forgot link on `passwordReset`, create-account always. `localFirst === true` short-circuits to `/app/` (REQ-066).

Architecture observations:

- **Monolith**: fetch orchestration, two state machines, sanitization and four render branches live in one component; nothing is reusable by `sign-up-screen.tsx` / `forgot-password-screen.tsx`, which repeat the `AuthShell` + phase pattern (GUESS — verify by reading those files before extracting shared code).
- **No BoardUI form components**: two raw `<label>/<input>` pairs and a raw `<p role="alert">`. The `Input` component in `components/base/input` (react-aria) is the house pattern and gives label/`aria-invalid`/`FieldError` association, `inputDir="ltr"` auto-default for `type="email`, disabled styling and RTL handling for free. Hardcoded `placeholder="person@example.com"` bypasses i18n (REQ-057).
- **Submit has no cancellation**: `submit()` has no `AbortController`, unlike the capability fetch; an unmount mid-request leaves a `setState` on an unmounted component. `status` reaches `failed` and only resets on the next submit, so a corrected field still renders the stale credential error. `load()` sets phase `loading` *and* bumps `reloadToken` (double state update for one retry).
- **Wire drift**: forgot link hardcodes `/forgot-password` and ignores `capabilities.resetUrl`; the passkey notice is permanent and names no real affordance (the doc-comment overstates "passkey when reported"); `emailCode` is advertised in `AuthCapabilities` but has no UI. These are capability/UI drift risks, not just cosmetic.
- **Sanitizer inline**: the open-redirect guard (`startsWith("/") && !startsWith("//")` → `/app/`) is correct but embedded; it should be a pure, unit-testable helper shared with sign-up (which forwards `next` too).
- **Tokens are honest throughout**: `bg-background-secondary-default`, `bg-background-primary-default`, `text-text-primary/-secondary/-tertiary/-placeholder/-error-primary`, `text-foreground-icon-primary`, `text-body-regular`/`text-body-2-medium`, `border-border-button-default`/`border-separator-border`, `boma-focus`, `boma-target`. Keep all of these; no raw palette, no `dark:` fork, no hex.

## Concrete upgrades (v0.4.0)

1. **Extract capability data + phases into `useAuthCapabilities()`.** Own `phase`, `capabilities`, `reloadToken`, the `AbortController`, and `localFirst` redirect; return one object `{ phase, capabilities, reload }`. Keep `reload()` a single state transition (drop one of the two `setPhase("loading")`/token-bump writes). Risk: behavior regression on abort — verify the effect's cleanup still aborts before `router.replace`, and that retry after a network error paints the skeleton exactly once. Byte-identical output; cover with the existing REQ-041 Playwright test.

2. **Decompose the render tree into focusing subcomponents.** `SignInSkeleton`, `UnavailableState`, `PasswordForm`, `SecondaryMethods` as siblings under `components/auth/`; `SignInScreen` becomes orchestration only. Risk: prop-drilling creep — pass the hook result as one object; keep `AuthShell` in the top-level component so the shared frame stays single-sourced. No visual change; re-run goldens intentionally rather than by accident.

3. **Migrate the form to BoardUI `Input`.** Replace raw email/password `<label>/<input>` with `components/base/input` (`Input label=… type=… isRequired isDisabled`); move `fieldError` into its `errorMessage` slot so React Aria wires `aria-describedby`/`aria-invalid`. Put `person@example.com` in the i18n catalogue (English fallback allowed per REQ-057). Risk: the `Input` shell uses its own `ring`/height classes, so golden class snapshots and focus-order axe results change — re-record deliberately, preserve `boma-focus` ring behavior and the uniform credentials copy. Email keeps LTR via the component's `type="email"` default.

4. **Capability-driven wiring, not literals.** Render the forgot link from `capabilities.resetUrl` with `/forgot-password` fallback; replace the permanent passkey notice with either a real CTA gated on a new `passkeyEnabled` flag or a documented DEVIATION in `docs/PARITY.md`; decide `emailCode` the same way (implement request/verify or record it as deferred). Risk: contract drift — each new flag needs `spec/openapi.yaml` + `crates/api/src/wire.rs` + capability fixture + fr/he locale parity, or the screen ships dead affordances. Do not add flows outside the spec without maintainer sign-off.

5. **Harden the submit lifecycle with typed errors.** Give `submit` its own `AbortController` (abort on unmount), branch on `RpcError.code` (`rpc.ts` already exports the typed shape): credential-shaped 401/403/422 → the existing uniform message, 429 → rate-limit copy, network/5xx → retry copy. Reset `status` from `failed` on field change so a correction clears the stale alert. Risk: over-differentiated errors become an address-enumeration oracle — keep every credential failure on one code path and one i18n string; three new strings need locale parity.

## Notes / non-goals

- Keep the `next` sanitizer semantics byte-for-byte; only extract it to a pure helper (unit-test `//evil`, `/app/`, relative and encoded inputs). It is the open-redirect fix.
- No JS-visible token, no Bearer-in-localStorage; `credentials: "include"` and httpOnly stay.
- No new design system, no BoardUI Pro, no passkey/WebAuthn implementation here (out of scope until that slice lands).
- Suggested order: 1 → 2 → 3 → 5 → 4 (4 touches the wire contract). Each step: `lint`, `typecheck`, axe, goldens and the REQ-040/041 Playwright specs.
