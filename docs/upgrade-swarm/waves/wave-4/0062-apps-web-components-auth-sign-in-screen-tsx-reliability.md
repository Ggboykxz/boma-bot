# 0062 — `components/auth/sign-in-screen.tsx` — reliability lens — v0.4.0 proposal

Target: `apps/web/components/auth/sign-in-screen.tsx` (181 lines, client). Wire in `lib/api/rpc.ts`
(`authCapabilities`, `authSignIn`, `RpcError`, `throwErr`), shell in `components/auth/auth-shell.tsx`,
tests in `apps/web/tests/auth.spec.ts`. Spec: §8.2 REQ-040/041, REQ-066; ADR-013 (static export).
READ-ONLY review at `before-m5`; no code changed. CONFIRMED = read in tree; GUESS = runtime-only.

## Current behavior (read, not run)

- `phase: loading | ready | error`, `status: idle | submitting | failed`. Capabilities are fetched on
  mount with an `AbortController` + `reloadToken` retry; `localFirst === true` replaces to `/app/`.
- The password form renders only when `capabilities.passwordAuth`; inputs are `disabled` while busy,
  the form carries `aria-busy`, the inline error carries `role="alert"`.
- `submit` calls `authSignIn({ email: email.trim(), password })` with **no signal**, then
  `router.replace(next)`. The `catch` ignores the error object entirely and always shows
  `signin.error.credentials` ("That address and password did not match.") — 401, 429, 5xx, offline
  and aborted all collapse into one message (CONFIRMED).
- `next` is sanitized to same-origin absolute paths (`/` accepted, `//` rejected, fallback `/app/`).
- Server (context, out of scope here): sign-in is rate-limited to **10 attempts / 5 minutes per IP**
  and the check runs *before* any account or password lookup (`authroutes.rs`, D-01); bad credentials
  answer `401 UNAUTHORIZED`, over-limit answers `429 TOO_MANY_REQUESTS` (`error.rs`). A 429 is
  therefore safe to tell apart from a 401 without leaking account existence (CONFIRMED).

## Failure modes

1. **Bad credentials.** `401 UNAUTHORIZED` is shown correctly, but so is everything else. Correct by
   accident, brittle by design: any future per-code copy added carelessly turns the screen into an
   enumeration oracle. Server wording ("Those credentials do not match.") is already identity-neutral;
   the screen must keep one message and one code path for 401/403/422/400.
2. **Rate limit.** 429 is indistinguishable from a wrong password. People behind a shared NAT or
   office egress get "wrong password" for somebody else's attempts and keep retrying, burning the
   same 10/5-minute bucket. No `Retry-After` is consumed; no double-submit ref guard exists, so two
   synchronous submit events can POST twice before React re-renders `disabled` (latch is state-only).
3. **Network down / hung request.** `authSignIn` accepts an `AbortSignal` but none is passed: on a
   hung connection the form sits at "Signing in…" forever with disabled inputs. A `fetch` rejection
   (TypeError, offline/DNS) is not an `RpcError`, so it is mislabeled "wrong password". Worse, if the
   response was lost *after* the server set the cookie, the person is signed in but told they are
   not; a retry mints a second session and spends another rate-limit token.
4. **Capabilities fetch.** Same missing timeout: an unreachable `/api/auth/capabilities` leaves the
   skeleton up indefinitely with only a manual Retry button; the catch discards the error, so 429,
   5xx and offline all become one `signin.error.unavailable` (CONFIRMED).
5. **Session state.** After success the screen trusts the cookie and `router.replace(next)`s. If the
   cookie was refused (cookie blocking, `Secure` over plain HTTP), the target route 401s and the root
   `UnauthorizedRedirect` bounces back — one full broken round trip before the failure surfaces
   (CONFIRMED; the loop itself is already guarded by the auth-path check in `app-providers.tsx`). A
   member who is already signed in still sees the form here. On `localFirst === true` the replace
   happens while `phase` is still `loading`; a failed navigation strands the skeleton with no retry.
6. **`next` path.** The `startsWith("/") && !startsWith("//")` guard rejects the obvious open
   redirect but accepts `/\evil.com`, which browsers may normalize to `//evil.com`. Flagged, not
   changed here: proposal 0041 (architecture lens) pins this sanitizer byte-for-byte — align with
   that worker before touching it (GUESS: browser normalization, unverified this session).

## Proposed hardening (v0.4.0), with risks

- **H1. Branch on `RpcError.code`/`status`, stay identity-neutral.** 401/403/422/400 → existing
  `signin.error.credentials`; 429 → new `signin.error.rate-limited`; network/5xx → new
  `signin.error.network` ("Sign-in could not be completed; try again."); anything else → credentials
  message. *Risk:* 429/5xx copy reveals infrastructure state, never account state; keep all
  credential-shaped failures on one branch. New i18n keys ×3 locales (en/fr/he).
- **H2. In-flight latch + timeout.** `useRef<boolean>` guard set before `authSignIn` and cleared in
  `finally`, plus `AbortSignal.timeout(15_000)` (AbortController + timer fallback) passed to both
  `authSignIn` and `authCapabilities`. *Risk:* abort after the server set the cookie shows a false
  failure — document retry as legitimate; never auto-retry (would burn the 10/5-minute budget).
- **H3. 429 cooldown.** On `TOO_MANY_REQUESTS`, disable submit for 60 s (or `Retry-After` if a later
  envelope carries it) with a visible countdown. *Risk:* client-side only, trivially bypassed — UX,
  not enforcement; shared-egress users may be penalized for others, so copy must not blame them.
- **H4. Input handling.** Keep `email` on failure for fast retry; clear only the password on success;
  never log either value. *Risk:* none. Never add "email not found" hints — the server cannot make
  that distinction, so the UI must not invent it.
- **H5. Confirm the session without reading the cookie.** After `authSignIn` resolves, follow
  `router.replace(next)` with `router.refresh()` so the target's server components see the fresh
  session; keep the 401-bounce as the safety net. Optionally add a `signedIn` capability flag so a
  member opening `/sign-in` is redirected before the form paints. *Risk:* `signedIn` makes
  capabilities session-dependent — `no-store` for authenticated responses, cacheable for anonymous
  ones, plus an ADR and spec/openapi.yaml fixture; coordinate with 0041 item 4.
- **H6. Actionable capability failure.** Log the error code to a dev-only console warning, keep the
  Retry button, and on a failed `localFirst` navigation fall back to the error phase so Retry exists.
  *Risk:* logging must not include email or cookie; no user-facing detail changes.
- **H7. Focus on failure.** Keep focus on the form; move it to the email field for credential-shaped
  errors (fast correction), to the submit button for network/429; keep `role="alert"`. *Risk:* none;
  verify with the existing axe/Playwright auth spec.
- **H8. Tests.** Extend `tests/auth.spec.ts` (REQ-041): route-mock 401/429/500/offline/abort, assert
  distinct copy per H1, that one double-click produces exactly one POST (H2), that the 429 button
  disables (H3), and that no server error body reaches the DOM. *Risk:* test churn only.

**Explicitly not proposed:** any UI distinction between unknown and known addresses; client-side
rate limiting as a security control; auto-resubmission; reading or storing the session token in JS;
changing the `next` sanitizer without 0041's sign-off.

## Bottom line

The happy path is correct, but every failure — 401, 429, 500, offline, hung socket — is reported as
"wrong password"; there is no timeout or double-submit guard, and success is assumed rather than
confirmed. H1–H4 and H6–H8 are local and reversible; H5 touches the auth contract and belongs in the
same ADR as 0041.
