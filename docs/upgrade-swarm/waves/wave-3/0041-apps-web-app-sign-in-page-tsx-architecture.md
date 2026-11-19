# 0041 — apps/web/app/sign-in/page.tsx — architecture lens — v0.4.0 proposal

Module under review: `apps/web/app/sign-in/page.tsx` (11 lines; a `Suspense` wrapper around
`components/auth/sign-in-screen.tsx`). All real logic lives in the screen, so this proposal
covers the pair. READ-ONLY review at `before-m5`.

## Current structure

- `page.tsx` is a static-export friendly thin shell: `<Suspense fallback={null}>` + `SignInScreen`.
  The `Suspense` is required because the screen calls `useSearchParams()`.
- `SignInScreen` (client component) phases: `loading → ready | error`. It fetches
  `/api/auth/capabilities` on mount (AbortController, `reloadToken` retry), then renders only the
  methods the deployment reports:
  - password form (when `passwordAuth`) with `email` / `password`, `autoComplete` set, busy state
    on `aria-busy`, uniform "that address and password did not match" error for every failure;
  - a passkey *notice* when `passkeyRpId !== null || nativePasskeys` ("passkeys land with the
    WebAuthn slice") — there is no passkey button;
  - an SSO anchor (full-page navigation to `/api/auth/<name>`, correct for OAuth redirects);
  - a forgot-password link (hardcoded `/forgot-password`, ignores `capabilities.resetUrl`) and a
    create-account link that forwards a sanitized `next`.
- `next` sanitization is correct: same-origin absolute paths only, `//` rejected, default `/app/`.
- Local-first (REQ-066): capabilities `localFirst === true` → `router.replace("/app/")` with no
  sign-in round trip. Good.
- `AuthShell` provides the one public frame: centered column, mark + wordmark, no nav chrome.
  All styling uses BoardUI semantic tokens (`bg-background-primary-default`, `text-text-primary`,
  `text-body-regular`, `border-border-button-default`, `boma-focus`); no raw hex, no `dark:` forks.

## Session / cookie handling — as presented to the UI

- Session is an opaque token in an `httpOnly; SameSite=Lax; Path=/; Max-Age=<ttl>` cookie,
  `Secure` when the origin is secure (`crates/api/src/auth.rs`); only the hash is stored
  server-side. The web tier never reads the token: every call uses `credentials: "include"` and
  no JS-visible session state exists anywhere in `apps/web` (no `useAuth`, no whoami fetch).
- Consequences, visible on this screen:
  1. After `authSignIn` resolves, the screen does `router.replace(next)` and *trusts* the cookie
     landed; if it did not, the first RPC on the target route 401s and bounces the user back to
     `/sign-in` (see `tests/auth.spec.ts` REQ-041). There is one full broken round trip before
     the failure surfaces, and no client-side confirmation.
  2. `/sign-in` is not aware of an existing session: a signed-in member who opens it sees the
     form again (redirect logic lives in `/`, not here).
  3. The cookie is invisible to presentation by design — nothing to render, but also nothing to
     debug; support has no "session present/absent" indicator.
  4. `capabilities` doubles as the pre-auth probe and must stay anonymous and cacheable; mixing
     session state into it would poison both uses.

## Concrete upgrades (v0.4.0)

1. **Split server shell / client form; real Suspense fallback.** Make `page.tsx` a server
   component that renders `<Suspense fallback={<SignInSkeleton/>}>` where the skeleton reuses
   `AuthShell` and token-matched placeholder bars (as today) but is announced with
   `role="status"`/`aria-busy` instead of `aria-hidden`. Risk: static export + `useSearchParams`
   constraints mean the skeleton must stay CSS/token-only; mis-sized placeholders cause CLS.
2. **Distinguish submit failure classes without leaking identity.** Map 401/403/422 → the
   existing uniform credentials message; map network/5xx → "sign-in could not be completed, try
   again" with the retry path; map 429 → rate-limit copy. Never branch on which side was wrong.
   Risk: over-differentiated errors become an enumeration oracle; mitigated by keeping all
   credential-shaped failures on one message and one code path. i18n strings ×3 locales.
3. **Use the wire, not literals.** Render forgot-password from `capabilities.resetUrl` (fall back
   to `/forgot-password`); render the passkey CTA only when the WebAuthn slice actually lands
   (gate on a new capability flag, e.g. `passkeyEnabled`), retiring the permanent
   "passkeys-unavailable" notice; implement the `emailCode` flow the wire already advertises
   (request code → verify) behind `capabilities.emailCode`. Risk: capabilities/UI drift — each
   new flag needs spec/openapi.yaml + test fixture (`tests/capabilities.ts`) + locale parity, or
   the screen ships dead affordances.
4. **Signed-in fast path.** Add `signedIn: boolean` (or a dedicated `GET /api/auth/session` probe)
   and have the screen `replace(next)` before painting the form when already authenticated.
   Risk: capabilities becomes session-dependent — must be served `no-store` for signed-in
   responses while staying cacheable for anonymous ones, or every visitor pays an auth lookup;
   also risk of redirect loop if `next` itself 401s (keep the existing sanitizer as the guard).
5. **Post-submit confirmation without exposing the cookie.** After `authSignIn` resolves,
   `router.replace(next)` + `router.refresh()` so server components on the target see the fresh
   session immediately; keep the 401-bounce as the safety net. Risk: double-fetch on mount of
   `/app`; negligible vs. the current full extra navigation cycle.

## Notes / non-goals

- No token in JS, ever; keep `credentials: "include"` and httpOnly. Any Bearer-in-localStorage
  "upgrade" would be a regression.
- Keep the `next` sanitizer byte-for-byte; it is the open-redirect fix.
- Passkey work stays out of scope until the WebAuthn slice; today's notice is honest but its
  doc-comment ("passkey when the deployment reports one") overstates reality — fix the comment.
- All new UI must stay on BoardUI semantic tokens (`background-*`, `text-text-*`, composite
  `text-*` type utilities, `border-separator-border`); no raw palette, no per-screen dark mode.

## Suggested order

3 (small, kills drift) → 2 (safety) → 1 (structure) → 5 → 4 (touches backend contract).
Each step lands with the existing Playwright auth spec extended, not a new harness.
