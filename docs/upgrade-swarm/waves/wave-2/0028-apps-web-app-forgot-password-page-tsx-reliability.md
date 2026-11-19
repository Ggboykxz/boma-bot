# 0028 — `/forgot-password/page.tsx` (password recovery request) — reliability lens

Target: `apps/web/app/forgot-password/page.tsx` (6-line static shell) + the client it mounts,
`components/auth/forgot-password-screen.tsx`; wire in `lib/api/rpc.ts` (`authForgotPassword`),
server in `crates/api/src/authroutes.rs`, use case in `crates/app/src/accounts.rs`.
Spec: spec/08-ui-ux.md §8.2, REQ-040/041, ADR-013 (static export). CONFIRMED = read in tree; GUESS = inferred. No code changed.

## Current behavior (read, not run)

- Page renders `<ForgotPasswordScreen />`; state: `idle | submitting | sent | failed`,
  one email field, one submit button.
- On any non-OK response it shows `forgot.error.generic` ("The request could not be
  completed.") inline with `role="alert"`; the email value is kept. On OK it swaps to the
  "if an account exists…" confirmation. The wording never reveals registration (CONFIRMED,
  spec §8.2 satisfied at the UI layer).
- The RPC wrapper accepts an `AbortSignal` but the screen never passes one; there is no
  timeout, no retry, no client-side cooldown, and no branch on the error `code`
  (`TOO_MANY_REQUESTS` is indistinguishable from a 500 here) (CONFIRMED).
- Server side (for context, not this page's scope): `POST /api/auth/forgot-password` is
  rate-limited to 5 attempts per IP per hour; unknown/malformed addresses get a local hash
  and the same `200 {"ok":true}`; recovery tokens live 30 minutes (`RECOVERY_TTL_SECONDS =
  1800`), are single-use, and all outstanding tokens for the user are revoked on reset
  (CONFIRMED).

## Failure modes

1. **Unknown email.** Server answers byte-identical `200 {ok:true}`; the screen shows the
   same confirmation. This axis is correct today (CONFIRMED). The residual risk is
   server-side timing: the known-address branch does mint + insert + audit while the
   unknown branch does one hash (the backend comment admits the mint-and-insert work is not
   equalized). No client change can close this; flag it, do not patch it here.
2. **Token expiry.** Recovery links die after 30 minutes. The `sent` copy says nothing
   about expiry, single-use, or where the link arrives (the local bring-up mode prints the
   link to the operator instead of mailing it). A person who waits, or whose link is
   consumed by a mail scanner that follows URLs, lands on `/reset-password` with a token
   the server refuses word-identically to "never existed" — and no in-product path back to
   request a fresh link except the generic back link (CONFIRMED).
3. **Rate limiting.** 5/hour per IP on forgot-password. The screen collapses 429 into the
   same generic error as a 500: a legitimate user behind a NAT/office egress gets "The
   request could not be completed." with no wait guidance, and an attacker cannot tell
   their own bucket from an outage. Double-activation before React re-renders disables the
   button can fire two POSTs (state flip is async; no ref guard) (CONFIRMED).
4. **Enumeration resistance.** Honest today for the happy path and for wording (test
   `REQ-040 … answers the same words regardless` exists in `apps/web/tests/auth.spec.ts`).
   Two soft spots: (a) `throwErr`'s non-JSON fallback copies raw server text containing
   `email` (<300 chars) into `RpcError.message` — the screen never renders it, so it is
   safe today and fragile tomorrow; (b) the `failed` vs `sent` render shapes differ, which
   is fine, but any future "helpful" distinction (e.g. per-code copy) would re-open the
   channel — the discipline must be documented, not assumed.
5. **Ambiguous network failure.** No timeout/abort: on a hung connection the form sits at
   "Sending…" with disabled inputs indefinitely. If the request reached the server but the
   response was lost, retry mints a second live token; both are single-use with 30-minute
   TTL and die together on reset, so this is benign — but the UI gives no signal that a
   resend is legitimate.
6. **Capability drift.** `/sign-in` hides the forgot-password entry when
   `capabilities.passwordReset === false`; the `/forgot-password` route renders and submits
   regardless, so a deployment that disabled reset exposes a form that silently does
   nothing useful. The static export means the page cannot know at request time without a
   client-side capabilities fetch (CONFIRMED that sign-in does exactly that).
7. **Input hygiene.** Email is `trim()`ed but not lowercased client-side (server does
   `to_ascii_lowercase`, CONFIRMED) and has no `maxLength`; a 10 KB address passes browser
   validation and relies on the server's body limit to refuse it — a wasted round trip and
   a confusing generic error.
8. **Focus and recovery.** On `sent`/`failed` the confirmation is swapped in with no focus
   move; a keyboard/screen-reader user is left on a removed submit button.

## Proposed hardening (v0.4.0), with risks

- **H1. Branch the error path on `RpcError.code`.** `TOO_MANY_REQUESTS` → dedicated,
  non-enumerating copy ("Too many attempts from this network; wait and try again."), all
  other codes stay generic. *Risk:* the 429 copy discloses the rate limiter exists — it
  already discloses nothing about accounts; acceptable. Keep wording identical for
  known/unknown addresses (the 429 fires before any account check, CONFIRMED).
- **H2. In-flight guard + timeout.** A `useRef` submitted-latch beside the state machine,
  plus `AbortSignal.timeout(15_000)` passed to `authForgotPassword`; on abort, show the
  generic error with an inline "Try again". *Risk:* aborting a request the server already
  processed — safe by the reset semantics above; document that retries are legitimate.
  Never auto-retry (would burn the 5/hour budget).
- **H3. Cooldown on 429.** After a 429, disable submit for a countdown (60 s, or
  `Retry-After` when present). *Risk:* minor annoyance behind shared egress; client-side
  only, trivially bypassed — do not present it as enforcement.
- **H4. Expiry-aware `sent` copy.** "…a recovery link was issued. It is single-use and
  expires within 30 minutes. Check your mailbox — or the place the operator reads." New
  i18n keys ×3 locales (en/fr/he, all present today). *Risk:* none for enumeration as long
  as phrasing stays conditional; wording review required for fr/he tone.
- **H5. Capabilities gate.** Fetch `authCapabilities()` on mount; when `passwordReset ===
  false`, render a short "password reset is not offered on this deployment; contact the
  operator" notice instead of the form. Capabilities fetch failure must fail open (render
  the form) so an outage does not strand reset-capable deployments; log the failure code
  only. *Risk:* one extra public request per page load; needs the sign-in pattern reused,
  not duplicated.
- **H6. Normalize input.** `trim().toLowerCase()`, `maxLength={254}`, keep `type="email"`
  and `required`; no client-side account-existence heuristics, ever. *Risk:* none; server
  remains authoritative.
- **H7. Focus management.** On `sent`, move focus to the confirmation container
  (`tabIndex={-1}` + `role="status"`); on `failed`, keep focus in the form. *Risk:* none.
- **H8. Error-envelope hygiene.** In `throwErr`, drop the raw-text fallback that sniffs for
  "email" (or strip it here): never let server text into `RpcError.message` on auth
  endpoints. *Risk:* loses a debug aid; compensate with `tracing::debug!` server-side
  (already present).
- **H9. Extend the Playwright REQ-040 test** to assert: (a) identical DOM wording and
  identical confirmation for 200-known vs 200-unknown (route-mocked), (b) 429 renders the
  new H1 copy and disables submit, (c) no substring of a server-provided error body ever
  reaches the DOM. *Risk:* test churn only.
- **H10. Do not fix in this page, flag for ADR:** server-side timing equalization on the
  unknown-address branch, and a per-email (not only per-IP) forgot-password bucket that
  answers identically either way. Both are invisible to the client and belong in
  `crates/app`; per-email bucketing must reuse the byte-identical response or it becomes
  an enumeration oracle.

**Explicitly not proposed:** distinguishing unknown-address from known-address anywhere in
the UI; client-side rate limiting as a security control; auto-resubmission; "check your
spam" as the only guidance (operator-read deployments exist).

## Bottom line

The page is honest today but brittle: every non-OK collapses to one message, there is no
timeout or double-submit defense, the `sent` copy hides the 30-minute single-use lifetime,
and the route ignores the capabilities gate `/sign-in` honours — H1–H10 are small, local, reversible.
