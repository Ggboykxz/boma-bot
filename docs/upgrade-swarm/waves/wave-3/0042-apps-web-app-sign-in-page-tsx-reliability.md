# 0042 · apps/web/app/sign-in/page.tsx · LENS=reliability · v0.4.0 proposal

## Scope note (CONFIRMED — read at `before-m5`)

`page.tsx` is an 11-line static-export shell: `<Suspense fallback={null}>` around `SignInScreen`.
It holds no state, so every reliability failure mode lives one hop away in
`components/auth/sign-in-screen.tsx` (phases, form, error mapping) and `lib/api/rpc.ts`
(`authCapabilities`, `authSignIn`). Index-only edits here would be theater; this proposal hardens
the flow across that boundary while keeping `page.tsx` trivial. Complements 0041 (architecture lens);
no duplication of its structural items.

## Current behavior (CONFIRMED from source)

- Mount: `authCapabilities(signal)` → `ready` or `error`; `localFirst === true` → `replace("/app/")`.
- `submit`: `setStatus("submitting")` → `await authSignIn` → `replace(next)`; **any** rejection maps
  to one message, `signin.error.credentials`, and `status = "failed"`.
- `RpcError` already carries `code` and `status` (`rpc.ts:169`); `throwErr` preserves HTTP status.
  The screen ignores both. No `AbortSignal`/timeout on submit; `authSignIn` fetch has no timeout.
- `capabilities` error is generic (`signin.error.unavailable`) with a manual `Retry`.
- Session is an httpOnly cookie; no token in JS; all calls use `credentials: "include"`.

## Failure modes

1. **Bad-credentials UX is a single black hole.** The catch in `submit` collapses *four* distinct
   classes — wrong password (401/403/422), rate limit (429), server fault (5xx), and network/offline
   (`TypeError`) — into "address and password did not match." Consequences: a user with the correct
   password whose request failed on the wire is told they are wrong and retries; a 429 looks like a
   bad password so the user keeps submitting; there is no recovery hint and the password is never
   cleared. `status` is not reset to `idle` on success (navigation masks it), and if `router.replace`
   is aborted the form stays `busy` with no message. (CONFIRMED from `sign-in-screen.tsx:72-87`.)

2. **Lockout / rate-limiting feedback loop.** No client-side throttle, backoff, or cooldown: Enter
   or repeated clicks can fire `authSignIn` in a tight loop. The server's anonymous counter is the
   known in-memory defect D-01 (not shared across replicas, spoofable via D-04). Because 429 is
   rendered as "wrong credentials," an honest user hammers the endpoint and deepens their own
   lockout, while an attacker gets a clean enumeration-free oracle. There is no "temporarily locked,
   retry in N s" state. (CONFIRMED for client; D-01/D-04 CONFIRMED from `05-known-defects.md`.)

3. **Session fixation / trust-on-cookie.** The screen cannot rotate a token itself; after
   `authSignIn` resolves it does `router.replace(next)` and *assumes* the new cookie landed. If
   `Set-Cookie` is dropped (Secure/SameSite mismatch behind a proxy, cookie blocking) the user reaches
   `/app/` on a stale or attacker-planted pre-auth session and only discovers it on the first 401
   bounce. On failed sign-in any pre-existing cookie is left untouched. D-08 records that auth config
   (rotation on login, cookie attributes, lifetime) is currently asserted nowhere. (CONFIRMED code;
   fixation risk GUESS — depends on backend library behavior not read here.)

4. **Network down / hung socket.** `authCapabilities` and `authSignIn` have no timeout: a black-holed
   connection leaves the page on the loading skeleton forever (mount) or the button on `submitting`
   forever (submit) with no escape. The mount error is a generic "unavailable" with manual retry only —
   no `navigator.onLine` check, no `online` event auto-recovery, no backoff, so each click hammers the
   endpoint. The `localFirst === true` branch is also gated on the capabilities fetch succeeding, so
   a local-first boot can be blocked by a network it should not need. (CONFIRMED for missing timeout/
   listener; `localFirst` networking dependency GUESS.)

## Concrete hardening (v0.4.0)

1. **Classify errors without leaking identity.** In `submit`, branch on `RpcError.status`/`code`:
   401/403/422 → existing uniform credentials message (never say which side was wrong); 429 →
   dedicated `signin.error.rateLimited` with a countdown; 5xx and network → `signin.error.retryable`
   with a Retry action. Clear the password field on failure, keep the email. Reset `status` to `idle`
   in a `finally` so a thrown `router.replace` cannot strand `busy`.
   *Risk:* over-differentiation becomes an enumeration oracle — keep every credential-shaped failure
   on one code path and one string; add the 3 new strings to all locales (i18n parity gate).

2. **Cooldown after 429.** On 429, disable submit for `Retry-After` seconds (fallback 30 s), render a
   live-region countdown, and block Enter by gating the form's `onSubmit` on the same guard as the
   button (close the current Enter/click race). Wire to the shared server counter (D-01/D-04) —
   client throttling is UX, never the enforcement boundary.
   *Risk:* clock skew / `Retry-After` absent → use a bounded default; a cooldown that outlives the
   server window annoys honest users — keep it shorter than the server TTL.

3. **Bound every request.** Give `authCapabilities` and `authSignIn` an `AbortSignal.timeout`
   (e.g. 8 s / 12 s) combined with the existing mount controller; on timeout map to the retryable
   message, not "unavailable"/"credentials." Add `online`/`offline` listeners so reconnect auto-retries
   the capabilities fetch once, and rate-limit the manual Retry button.
   *Risk:* a slow-but-valid login on a poor link (criterion 4) could be cut off — tune the timeout to
   the measured p95 and keep the error non-destructive (no account state changed).

4. **Verify the session after sign-in.** After `authSignIn` resolves, call `router.refresh()` and a
   cheap `/api/auth/session` probe before `replace(next)`; if absent, show the retryable message and
   stay on the form rather than bouncing through `/app/`. On failure, leave any pre-auth cookie as-is
   (do not attempt client-side clearing).
   *Risk:* an extra round trip on every login; a probe that 401s due to a transient fault could
   falsely deny a real login — treat probe network errors as "proceed with the 401 bounce safety net."

5. **Backstop session-rotation evidence (D-08).** Add a Playwright/API test asserting sign-in returns
   `Set-Cookie` with a *new* token (pre-auth token invalidated), `httpOnly`, `Secure` on HTTPS,
   `SameSite=Lax`, and the configured TTL; assert no token ever reaches `localStorage`.
   *Risk:* couples the web suite to backend auth internals — keep it in the auth config assertion
   test, not the page's UI test; coordinate the contract with the backend slice.

## Test coverage

Extend `apps/web/tests/auth.spec.ts` (REQ-040/041) with: wrong password → credentials message and
password cleared; 429 → countdown + disabled submit + no second RPC; offline submit → retryable, not
credentials; capabilities hung past timeout → retryable state, not permanent skeleton; reconnect →
auto-recovered capabilities; forced cookie-drop → stays on form with retryable message.
`REQ-04x` in each test name. *Risk:* Playwright route-mock timing brittleness — reuse existing
fixtures and `fake timers` for the countdown.

## Explicitly deferred

- Server-side rotation and shared rate-limit counters (0042 cannot fix D-01/D-04/D-08; it only
  surfaces them honestly). Backend work, tracked separately.
- Passkey/email-code flows and `resetUrl` wiring — architecture scope (0041), not reliability.

## Success criteria

- No error class is rendered as "wrong credentials" except credential-shaped failures.
- No request can hang the UI past the timeout; no retry storm from a single user action.
- A dropped session cookie never silently delivers the user to `/app/`.
- New states degrade accessibly: `role="alert"`/`aria-live`, visible focus, no color-only signal.
