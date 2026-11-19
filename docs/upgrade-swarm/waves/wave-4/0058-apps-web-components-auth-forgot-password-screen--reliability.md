# 0058 — `components/auth/forgot-password-screen.tsx` — reliability lens (v0.4.0)

Target: `apps/web/components/auth/forgot-password-screen.tsx` (77 lines; the client mounted by
`app/forgot-password/page.tsx`). Wire: `authForgotPassword()` in `lib/api/rpc.ts` (`throwErr` →
`RpcError{code,status}`), server `crates/api/src/authroutes.rs`, use case `crates/app/src/accounts.rs`.
Spec: spec/08-ui-ux.md §8.2, REQ-040/041. CONFIRMED = read in tree this session; GUESS = inferred.
Read-only: no code changed. Companion to wave-2 doc 0028 (route-level); this one is screen-level.

## Current behavior (read, not run)

- State `idle | submitting | sent | failed`, one email field, one submit button. `submit` is a
  `useCallback` on `[email, t]`: set `submitting`, `authForgotPassword({email: email.trim()})`,
  `sent` on resolve, else `failed` + `t("forgot.error.generic")` (CONFIRMED).
- Button and input are `disabled={status === "submitting"}`; form carries `aria-busy`. No
  `AbortSignal` is passed (RPC accepts one); no timeout, retry, cooldown, focus move, or branch on
  `RpcError.code` (CONFIRMED).
- `throwErr` yields `TOO_MANY_REQUESTS` for 429 and `INTERNAL` for 500 with distinct server
  messages; the screen discards both and shows one generic line (CONFIRMED).
- `forgot.sent` copy never states the 30-minute, single-use lifetime of the recovery token, nor that
  a hung/resent request may mint a second (CONFIRMED; TTL from companion doc 0028).
- Server answers byte-identical `200 {ok:true}` for known/unknown addresses; UI wording identical
  (CONFIRMED). REQ-040 test mocks 200 for two addresses and asserts identical `forgot.sent` (CONFIRMED).

## Failure modes

1. **429/500 collapse.** Every non-OK becomes `forgot.error.generic` ("The request could not be
   completed."). A legitimate user behind shared egress (5/hour/IP limit) gets no wait guidance;
   an operator cannot tell rate limiting from an outage from the UI (CONFIRMED).
2. **Double-submit.** The `setStatus("submitting")` flip is async; two quick activations before React
   re-renders can dispatch two POSTs. Guards are disabled-state only — no synchronous latch/ref.
   Benign for data (re-mint is single-use, 30-min tokens all revoked on reset) but burns 2 of the
   5/hour attempts (CONFIRMED; real-world frequency GUESS).
3. **Expiry copy gap.** `sent` confirms issuance but says nothing about 30-minute/single-use, so a
   person who waits, or whose mail scanner follows the link, lands on `/reset-password` with a token
   the server refuses word-identically to "never existed", with no in-product "request a new one"
   besides the back link (CONFIRMED).
4. **Enumeration honesty.** Happy path and wording are honest (byte-identical response, identical
   copy, REQ-040 test). Soft spots: (a) `throwErr`'s non-JSON fallback copies raw text containing
   "email" (<300 chars) into `RpcError.message` — never rendered here today, fragile if a future
   "helpful" branch prints it; (b) any future per-code or per-address distinction re-opens the
   channel. Honesty is a discipline to preserve, not a guarantee (CONFIRMED / likelihood GUESS).
5. **Hung request.** No timeout: on a stalled connection the form sits at "Sending…" with disabled
   inputs indefinitely. If the request reached the server but the response was lost, retry mints a
   second benign token, but the UI gives no signal that resend is legitimate (CONFIRMED).
6. **Capability drift.** `/sign-in` hides the reset entry when `capabilities.passwordReset === false`;
   this screen renders/submits regardless (CONFIRMED via companion doc 0028). Flag, not fix here:
   the gate is a route-level/ADR decision.
7. **Input hygiene.** `trim()` only: no lowercase (server lowercases), no `maxLength`; a huge address
   passes browser validation and wastes a round trip to body-limit rejection (CONFIRMED).

## Proposed hardening (v0.4.0), with risks

- **S1. Branch on `RpcError.code` (typed catch).** Import `RpcError`; in `catch`, if `err.code ===
  "TOO_MANY_REQUESTS"` show a dedicated non-enumerating `forgot.error.rate` line ("Too many attempts
  from this network; wait and try again."); every other code keeps `forgot.error.generic`. *Risk:*
  discloses that a rate limiter exists — nothing about accounts, and 429 fires before account lookup,
  so no enumeration channel; wording stays identical for known/unknown.
- **S2. Synchronous in-flight latch.** Add `useRef(false)` guard checked/set at top of `submit`,
  cleared in `finally`; keep disabled-button state as UX. *Risk:* none for correctness; ensure the
  latch resets on unmount-abort so navigation mid-flight cannot leave it stuck.
- **S3. Timeout + abort.** Pass `AbortSignal.timeout(15_000)` to `authForgotPassword`; on abort show
  generic error and allow one retry. *Risk:* aborting an already-processed request is benign (S2 doc
  semantics); document resend as safe. **Never auto-retry** — it burns the 5/hour budget.
- **S4. 429 cooldown.** On rate-limit, disable submit for a countdown (`Retry-After` when the RPC
  surfaces it, else 60 s). *Risk:* minor annoyance behind shared egress; client-only and trivially
  bypassed — present as UX, never as an enforcement control.
- **S5. Expiry-aware `sent` copy.** Extend `forgot.sent` (or add `forgot.sent.lifetime`) to state
  single-use, "~30 minutes", and "request a new one if it expired", keeping the conditional "if an
  account exists" prefix. Add i18n keys in en/fr/he. *Risk:* must not become an enumeration oracle —
  keep the conditional phrasing; fr/he tone needs review (all 3 locales exist today).
- **S6. Enum-honesty guardrails.** Never render `RpcError.message`; in `throwErr`, drop or strip the
  raw-text "email" sniff on auth endpoints and log server-side via `tracing::debug!`. Add a code
  comment/ADR note: per-code copy only for transport/rate-limit codes, never address existence.
  *Risk:* loses a debug aid; compensated server-side.
- **S7. Normalize input.** `email.trim().toLowerCase()`, `maxLength={254}`, keep `type="email"` +
  `required`; no client-side account-existence heuristics, ever. *Risk:* none; server authoritative.
- **S8. Focus management.** On `sent`, move focus to a `role="status" tabIndex={-1}` confirmation;
  on `failed`, keep focus in the form. *Risk:* none; improves keyboard/SR path (WCAG 2.2 AA gate).
- **S9. Tests (REQ-040).** Extend `auth.spec.ts`: mock 429 → S1 copy + submit disabled; 500 → generic
  copy; two rapid clicks → exactly one POST; assert no server error-body substring reaches the DOM
  for both known/unknown. *Risk:* test churn only; keep the existing identical-words assertion intact.

**Explicitly not proposed:** any known-vs-unknown distinction in UI; client-side rate limiting as a
security control; auto-resubmission; fixing server-side timing equalization or capabilities gating
here (flag for ADR/route level, see 0028).

## Bottom line

The screen is enumeration-honest but reliability-thin: all failures collapse to one message, nothing
stops a double-submit or a hung request, and the success copy hides the token's 30-minute single-use
life. S1–S9 are small, local, reversible; the only cross-cutting touches are i18n keys (S5) and the
`throwErr` raw-text fallback (S6).
