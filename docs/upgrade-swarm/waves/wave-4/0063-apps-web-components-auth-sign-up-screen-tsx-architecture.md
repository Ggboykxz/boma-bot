# 0063 — apps/web/components/auth/sign-up-screen.tsx — architecture lens — v0.4.0 proposal

Module under review: `apps/web/components/auth/sign-up-screen.tsx` (168 lines). Its page
`apps/web/app/sign-up/page.tsx` is an 11-line `<Suspense fallback={null}>` shell (the `Suspense`
is required by `useSearchParams()`); all real logic lives in the screen, so this proposal covers
the pair. READ-ONLY review at `before-m5`; CONFIRMED by reading the file, the sibling
`sign-in-screen.tsx`, `lib/api/rpc.ts`, `lib/i18n.ts`, `lib/use-translation.ts`, `spec/01-requirements.md`
REQ-040, `spec/08-ui-ux.md` §8.2 and `crates/api/src/authcap.rs`.

## Current structure

- One client component owns everything: two phase enums (`Phase = loading|ready|error`,
  `Status = idle|submitting|failed`), seven `useState` values, one `useEffect` capability fetch,
  one retry callback, one submit callback, and three full `<AuthShell>` render branches.
- Mount effect: `authCapabilities(signal)` with `AbortController` + `reloadToken` retry; on
  `localFirst === true` it `replace("/app/")` (REQ-066); otherwise `capabilities` is stored and
  the screen flips to `ready`. Any rejection → `error`. `capabilities` is used for exactly one
  field (`localFirst`) — the rest of the wire document is ignored.
- `next` sanitizer is correct and load-bearing: same-origin absolute paths only, `//` rejected,
  default `/onboarding/1/`. It is the open-redirect fix; keep it byte-for-byte.
- Submit: `authSignUp({ email, password, name, locale: "en" })`, then `router.replace(next)`;
  every failure collapses to one `signup.error` string. No signal, no unmount guard, no refresh.
- All colour/type is already on BoardUI semantic tokens (`bg-background-secondary-default`,
  `text-text-primary/-secondary/-tertiary/-error-primary`, `border-separator-border`,
  `border-border-button-default`, composite `text-body-regular`/`text-body-2-medium`, `boma-focus`,
  `RiErrorWarningLine`). No raw palette, no hex, no `dark:` forks — preserve this exactly.

## Architecture findings

1. **The screen's contract with REQ-040 is only half implemented.** The doc-comment promises a
   closed `/sign-up` "says so plainly", and `signup.closed` exists in all three catalogues
   (en/fr/he), but `AuthCapabilities` (`crates/api/src/authcap.rs`, mirrored in `rpc.ts`) has no
   sign-up-availability or allow-list field, and the screen never renders `signup.closed`. A
   deployment with sign-ups disabled therefore shows a working-looking form whose submit always
   fails with the generic "address may already exist". The spec says capabilities must report this.
2. **Near-total structural duplication with `sign-in-screen.tsx`.** Same Phase/Status types, same
   `next` sanitizer, same AbortController/reloadToken fetch, same localFirst redirect, same
   skeleton/error branches, same hand-rolled `<input>` class strings. Two copies already drifted
   (sign-in maps more errors; sign-up hardcodes a locale).
3. **Raw `<input>` and string-literal classes.** BoardUI rules require react-aria-components-based
   form fields and `cx()` for class merging; the three fields repeat a 100+ char className with
   plain literals, bypassing both.
4. **Hardcoded `locale: "en"`** on an i18n/RTL product: a Hebrew or French UI creates an English
   account. `useLocale()` is already the source behind `useTranslation`, so this is avoidable.
5. **No submit cancellation and no post-submit `refresh()`** — the twin weaknesses flagged for
   sign-in; the cookie lands, but server components on `next` may render anonymous on first pass.

## Concrete upgrades (v0.4.0)

1. **Add the sign-up capability and gate the screen on it.** Extend the wire document with
   `signUpEnabled` (+ allow-list state if kept) in `spec/openapi.yaml`, `crates/api/src/authcap.rs`
   and `AuthCapabilities` in `rpc.ts`; render `signup.closed` with a link to `/sign-in` instead of
   the form when closed/absent. This fixes a REQ-040 MUST, not a DEVIATION.
   *Risk:* four-layer contract change (spec/Rust/TS/fixtures) plus three locales; the endpoint must
   stay anonymous and cacheable, and the closed message must not leak whether an address is known.

2. **Extract shared auth primitives, keep the screens as composition.** Move the duplicated pieces
   into `components/auth/`: `useAuthCapabilities()` (fetch/abort/retry/localFirst), `useSafeNext()`,
   `AuthSkeleton` (`role="status"`, `aria-busy`, token-matched bars), `AuthErrorState` and
   `AuthTextField`. Risk: over-abstraction couples two screens that will diverge once sign-up gains
   closed/allow-list states — keep primitives narrow and presentational, never merge their copy.

3. **Replace the three raw inputs with the installed BoardUI field component and `cx()`.** Keep
   `autoComplete` values (`name`, `email`, `new-password`), `required`, `aria-busy` and the
   `role="alert"` error line; retain every existing semantic token.
   *Risk:* react-aria validation/focus semantics differ from native `required`; re-run the auth
   Playwright spec and axe at 360 px in light/dark before considering it done.

4. **Stop hardcoding `locale: "en"`.** Call `useLocale()` and pass the active BCP 47 locale to
   `authSignUp`; keep name/email in LTR isolation (`bdi`/`dir="auto"`) under RTL.
   *Risk:* the server must accept exactly the catalogue locales (en/fr/he); if the wire enum is
   narrower, whitelist client-side rather than sending an unsupported tag — confirm before coding.

5. **Harden submit and confirm the session.** Pass an `AbortSignal` to `authSignUp`, skip state
   updates after unmount, and on success do `router.replace(next)` followed by `router.refresh()`;
   add a small error classifier (409 conflict → existing `signup.error`, 429 → rate-limit copy,
   5xx/network → retry copy) that never reveals whether an address exists.
   *Risk:* `router.refresh()` double-fetches `/onboarding/1/` — verify it does not reset wizard
   state; extra error strings ×3 locales; keep the 401-bounce as the safety net.

## Notes / non-goals

- No token in JS, ever: keep `credentials: "include"` and the httpOnly cookie; a Bearer/localStorage
  "upgrade" would be a regression.
- Keep the `next` sanitizer untouched; it is the open-redirect fix (and gates upgrade 5's redirect).
- SSO/passkey/email-code affordances belong to sign-in; do not import them here until REQ-040's
  sign-up slice actually lands.
- Every new element stays on BoardUI semantic tokens and composite type utilities — no raw palette,
  no `dark:` forks, no second design system.

## Suggested order

1 (fixes the spec gap) → 2 (removes duplication while the surface is still small) → 3 → 4 → 5.
Each step lands with the existing auth Playwright spec extended, not a new harness.
