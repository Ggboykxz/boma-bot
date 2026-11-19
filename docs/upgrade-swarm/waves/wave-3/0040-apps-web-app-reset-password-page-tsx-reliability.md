# 0040 · apps/web/app/reset-password/page.tsx · LENS=reliability · v0.4.0 proposal

## Scope note

The module is an 11-line index: `<Suspense fallback={null}><ResetPasswordScreen/></Suspense>`
(REQ-040, spec/08-ui-ux.md §8.2). It holds no logic. Every reliability failure mode lives one hop
later in `components/auth/reset-password-screen.tsx` and in the backend
(`crates/app/src/accounts.rs::reset_password`, `crates/api/src/authroutes.rs::reset_password`).
This reviews the entry point and hardens across that boundary; editing only the index would be theater.

## Current behavior (CONFIRMED, as read)

- Page reads `token` from `useSearchParams()`; empty token → "missing token" copy + link to `/forgot-password`.
- Screen validates locally: `password.length >= 10`, `password === confirm`; everything else is one
  catch-all `catch { setError(t("reset.error.invalid")) }` → "link is invalid, expired or already used".
- Server: `validate_password` runs *before* token consume (min 8 chars, max 256 bytes); token is
  consumed atomically with expiry in one statement; all other links for the user are revoked, all
  sessions deleted, audit events written; invalid/used/expired share one refusal string.
- Endpoint is rate-limited ("reset-password" window/max) and parses a strict body.
- `authResetPassword` is called with no `AbortSignal`; success does `router.replace("/app/")`.

## Failure modes

1. **Expired / used token — dead end (CONFIRMED).** Any non-empty token renders the full form; there
   is no early signal. After submit the generic message is correct but the only recovery affordance is
   a link to `/sign-in`, not `/forgot-password`, despite the text saying "Ask for a new one". The token
   remains in the URL, so refresh replays a dead token. (Uniform wording is intentional and must stay.)
2. **Replay / double-submit (CONFIRMED).** The `status === "submitting"` guard is React state, not a
   synchronous ref: Enter+click or two rapid Enter presses can issue two POSTs before commit. Token is
   single-use server-side, so the second fails — but the catch-all can paint "link invalid" *after* the
   first succeeded and `router.replace` was queued. On `"succeeded"` the form falls through to the
   normal render (controls re-enabled), widening the window. No abort on unmount.
3. **Weak-password acceptance & policy drift (CONFIRMED).** Client floor is 10 chars with no upper
   bound; server floor is 8 chars and max 256 **bytes**. A 9-char or >256-byte password passes local
   checks, is rejected server-side as `WeakPassword` (token is *not* consumed — good), and is reported
   to the user as an invalid/expired link. False signal drives needless re-requests. Two independent
   constants with no shared source of truth will drift further.
4. **Enumeration / token leakage (CONFIRMED design, GUESS risk).** Generic refusal avoids account
   enumeration — keep it. But the token rides in a query string: browser history, Referer to any
   third-party asset, proxy logs and screenshots. Adding a naive "check token on mount" call later
   would create a token-validity oracle unless it reuses the same limiter and uniform response.
5. **Error conflation (CONFIRMED).** 400 (weak/overlong password), 429 (rate limit), 5xx and offline
   all map to "link invalid". On 429 the UI tells users to request a new link, feeding the limiter;
   on a transient outage it declares the link dead. The one useful message is applied to every case.
6. **`Suspense fallback={null}` (CONFIRMED).** Blank flash on static-export hydration; cosmetic but
   indistinguishable from a broken page on slow devices.

## Proposed hardening (v0.4.0)

- **A. Typed error mapping.** Have `throwErr` expose a stable code; map token refusal → existing
  generic copy, `weak-password` → inline password-field error, `429` → "too many attempts, retry
  later", 5xx/network → "couldn't reach the server, try again". Never split expired vs used vs absent
  (spec §8.2). *Risk:* needs a backend error-code contract; if absent, at minimum stop claiming
  "expired" on 429/5xx. Coordinate with backend, do not invent client-side classification from text.
- **B. Synchronous submit guard.** `useRef` in-flight flag set in `submit` before the await, reset in
  `finally` on failure only; disable the form for both `"submitting"` and `"succeeded"`; render an
  explicit success state; pass an `AbortSignal` from an unmount effect to `authResetPassword`.
  *Risk:* flag must be cleared on every failure path or the form wedges; abort is harmless post-success.
- **C. One password policy.** Enforce the server's 256-byte max client-side and align/annotate the min
  (client may stay stricter, but as a documented choice, not drift). Ideally publish policy via the
  capabilities endpoint. *Risk:* capabilities change needs backend; smallest safe v0.4.0 step is the
  max-length check plus A so a rejection is never mislabeled as a link failure.
- **D. Recovery CTA on failure.** Always render a `/forgot-password` link beside the inline error.
  *Risk:* none functionally; keeps copy unchanged and avoids the dead end in #1.
- **E. Token hygiene.** On mount capture token, then `history.replaceState` to strip `?token=`, keeping
  the value in component state for the submit; clear on success; rely on `Referrer-Policy: no-referrer`
  for any external asset. *Risk:* stripping breaks refresh-resume unless persisted (sessionStorage) —
  prefer just adding the referrer policy and leaving the URL intact until backend can accept a
  one-time exchange, or document the refresh trade-off explicitly.
- **F. Playwright coverage (REQ-040).** Expired/used token → generic message + new-link CTA; double
  submit issues one POST; overlong password → password error, not link error; 429 → retry message.
  *Risk:* route-mock brittleness; keep the existing fixture style.

## Explicitly rejected / deferred

- Distinguishing expired vs used vs nonexistent tokens in the UI — enumeration oracle, forbidden by §8.2.
- Moving the token to a route param — static export (ADR-013) cannot prerender per-token paths.
- Breached-password checking — out of spec scope; propose to the spec owner, do not add unilaterally.

## Risks & rollout

A and C need a backend error-code/policy contract; B, D, E, F are client-only — ship and verify those
first, then negotiate the contract. Highest regression risk is B (form wedging) and E (refresh resume).

## Success criteria (CONFIRMED targets, not yet run)

- No transient/server error produces an "invalid link" message.
- One POST per user action; success never shows a failure state.
- Password rejections surface at the password field; a failed token always offers a new-link path.
