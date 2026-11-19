# 0044 · apps/web/app/sign-up/page.tsx · LENS=reliability · v0.4.0 proposal

## Scope note

The assigned module is the static-export index only: it wraps `<SignUpScreen />` in
`<Suspense fallback={null}>` and holds no state. All reliability failure modes live one hop later in
`components/auth/sign-up-screen.tsx` and the transport in `lib/api/rpc.ts` (`authCapabilities`,
`authSignUp`, `RpcError`/`throwErr`). This reviews the page as flow entry and hardens across that
boundary; an index-only change (e.g. a different fallback) would be theater.

## Current behavior (as read)

- `page.tsx`: `Suspense fallback={null}` → blank screen until the client screen renders; no error
  boundary, so any render throw in `SignUpScreen` blanks the route with no recovery affordance.
- `SignUpScreen` loads capabilities via `authCapabilities(signal)` with an `AbortController` aborted on
  unmount; failure → `phase="error"` with a Retry button. `localFirst === true` → `router.replace("/app/")`.
- `submit()` calls `authSignUp({ email, password, name, locale: "en" })` with **no** signal and **no**
  status re-check, then `router.replace(next)`. Any throw is swallowed into one generic
  `t("signup.error")` and `status="failed"`. The discarded `RpcError` carries `code` (`BAD_REQUEST`,
  `CONFLICT`, `TOO_MANY_REQUESTS`, `INTERNAL`) and an explicitly user-safe `message`.
- `next` is sanitized against absolute/protocol-relative URLs (`startsWith("/") && !startsWith("//")`),
  default `/onboarding/1/`. `busy = status === "submitting"` disables inputs and the button.

## Failure modes

1. **Duplicate owner / duplicate email (409 CONFLICT).** On a fresh deployment the first account may
   become operator (spec §8.2, REQ-040). A second submit for an existing email returns CONFLICT but the
   screen shows the same generic error as a network outage — no "email taken", no pointer to
   `/sign-in`. Worse, two concurrent *first* signups race to claim owner; whichever the server accepts
   first wins, and the loser gets an undifferentiated error with no reconciliation of who owns the
   deployment. The server must enforce a single owner atomically; the client must surface the outcome.
2. **Validation errors (400 BAD_REQUEST).** Password policy, name/email rejection and disabled-signup
   refusals come back with a safe, displayable `message` and `code`, but `submit()` discards them. HTML
   `required` / `type="email"` give only superficial client validation; a weak-password or
   signups-closed user gets the same useless "error" string and cannot correct the input.
3. **Double-submit race.** The only guard is the disabled DOM attributes driven by React `status`;
   `submit()` itself never checks `status` and there is no in-flight ref, abort signal, or idempotency
   key. Enter-key + click, or two rapid Enters before state commits, can fire two `authSignUp` calls.
   Outcomes: first succeeds and navigates while the second 409s and its `catch` sets
   `status="failed"`/fieldError — a visible error after the account was created and the session set;
   or both reach the server and attempt duplicate creation/owner claims. No timeout means a hung
   connection pins the button in `submitting` forever.
4. **Network down / lost response.** `authCapabilities` failure is handled with a Retry button, but a
   failed/hung `authSignUp` has no signal or timeout. Partial connectivity is the dangerous case: the
   POST creates the account and session but the response is lost; the generic error invites a retry,
   which now 409s and is misread as "email taken" although the user is actually signed in. There is no
   "did it succeed?" reconciliation (e.g. probe `authCapabilities`/session on `failed`) and no
   idempotency key to make a retry safe.
5. **Entry-level gaps (in scope as flow entry).** `Suspense fallback={null}` shows nothing during load
   and there is no `error.tsx`/boundary, so a render throw blanks `/sign-up`. `next` accepts `/\evil.com`
   (backslash), which some browsers normalize to `//evil.com` — an open-redirect reliability/safety
   edge on a freshly authenticated member.

## Proposed hardening (v0.4.0)

- **A. Branch on `RpcError.code` and show the safe message.** In `submit()`, `catch (err)` → narrow to
  `RpcError`: `CONFLICT` → "email already registered, sign in" with a `Link` to `/sign-in?email=…`;
  `BAD_REQUEST` → show `err.message` inline against the relevant field; `TOO_MANY_REQUESTS` →
  "too many attempts, wait and retry"; `INTERNAL`/unknown → generic. Risk: server `message` wording
  must stay user-safe — enforce with a test that asserts the envelope contract (spec §3.1), never
  render raw fetch text. Keep one translation key per code; no leak of internals.
- **B. Serialize submission.** Add `const inFlight = useRef(false)`; guard at the top of `submit()`
  (`if (inFlight.current || busy) return;`) and set/clear it around the await; keep the disabled
  attributes as a UI nicety, not the guard. Pass an `AbortController.signal` to `authSignUp` (the API
  already accepts one) and abort on unmount. Risk: an aborted-on-unmount request still reached the
  server — pair with C/E, and do not treat `AbortError` as a validation failure.
- **C. Timeout + idempotency for the POST.** Wrap the signup fetch in a client timeout (~10 s) via the
  signal, and generate a per-attempt idempotency key (UUID in `sessionStorage`, rotated per *submission
  session*, not per keystroke) sent with the request so a timed-out retry cannot duplicate the
  account. Risk: requires a backend contract — coordinate with the API owner; without it, at least
  make `status="failed"` offer a single explicit "Retry" button and reconcile state before retrying.
- **D. Reconcile ambiguous failure.** On ` failed` with no usable code (network/lost response), call
  `authCapabilities`/a session probe; if a session now exists, route to `next` instead of showing an
  error ("account likely created — continuing"). Risk: an extra round trip on genuine failures and a
  small chance of misclassifying; guard the probe with its own timeout and never block the UI on it.
- **E. Owner-race honesty.** If the server signals that an owner was just claimed by another account
  (new `CONFLICT` detail), show a dedicated "this deployment already has an operator" message rather
  than the email-taken copy. Risk: needs a server-side detail code; until then, fall back to A's
  CONFLICT branch and flag the ambiguity in docs/STATE.md as a spec question, not a silent guess.
- **F. Page-level resilience.** Replace `fallback={null}` with a minimal skeleton matching the screen's
  loading block, and add `apps/web/app/sign-up/error.tsx` (client, reset button) so a render throw is
  recoverable. Risk: an error boundary cannot catch errors in the boundary itself — keep it tiny.
  Harden `next` to reject backslashes and any value that is not a single leading `/` plus safe chars.
- **G. Test coverage (REQ-040).** Extend `apps/web/tests/sign-up.spec.ts` with Playwright route mocks:
  409 CONFLICT shows sign-in pointer; 400 BAD_REQUEST shows the server message; 429 shows retry
  copy; double Enter/click issues exactly one POST; lost-response retry reconciles to `next`;
  network-down capabilities keeps the Retry button. Risk: route-mock brittleness — reuse the existing
  auth fixture style; assert request counts, not timing.

## Explicitly deferred

- Full password-strength meter / zxcvbn — product/spec decision, not a reliability fix.
- Moving the screen to a server component with request-time capability read — incompatible with the
  static export; would force dynamic rendering for a public auth page.

## Risks & rollout

Ship A, B, F, G first (client-only, no server contract). Negotiate C/D/E with backend (idempotency
key, owner-claim detail, session probe) and gate them behind a feature check until the API exists.
No new npm dependencies; bundle impact negligible. Success: no double POST in tests, every `RpcError`
code yields distinct actionable copy, network loss never shows "email taken", and `/sign-up` never
blanks on a render throw.
