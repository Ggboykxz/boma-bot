# 0060 — apps/web/components/auth/reset-password-screen.tsx — reliability lens (v0.4.0 proposal)

Worker #060 · module `apps/web/components/auth/reset-password-screen.tsx` (114 lines, `"use client"`) · lens: reliability · READ-ONLY review of `before-m5`; proposal only, no code edited. Markers: CONFIRMED = read in tree this session, GUESS = inferred.

## Scope and sources read

The component (full), `lib/api/rpc.ts` (`authResetPassword:897-911`, `throwErr:937-956`, `RpcError`, `call`), `components/auth/forgot-password-screen.tsx`, `crates/api/src/authroutes.rs:163-196` (rate limiter + handler), `crates/app/src/accounts.rs:206-212` (`RECOVERY_REFUSED`), `spec/08-ui-ux.md §8.2`, `spec/01-requirements.md` REQ-040/041, `apps/web/tests/auth.spec.ts:104-127`, sister proposals 0039 (this page's architecture) and 0028 (forgot-password reliability). This doc stays on the reliability axis; token-surface/structure changes are deferred to 0039.

## Failure modes

1. **Expired token — no recovery path.** CONFIRMED: server refuses expired links with `RECOVERY_REFUSED` ("invalid or expired") and a typed envelope; the screen collapses every throw into `reset.error.invalid` (`line 58`). Spec §8.2 deliberately keeps expiry/used/invalid wording identical (anti-enumeration), but the screen offers only a generic back link to `/sign-in` (`lines 109-111`) — no route to request a fresh link. A person whose link died after 30 min (`RECOVERY_TTL_SECONDS`, per 0028) has no in-product next step and cannot tell "expired" from "typo". The token also stays in the URL (`line 25` only reads it), so a dead token leaks through history and same-origin `Referer` (see 0039 up1/2).

2. **Replay / consumed token.** CONFIRMED: the link is single-use and replay is refused with the *same* wording as invalid (`crates/testkit` replay test, `accounts.rs`). Two UI paths trigger it: (a) `router.replace("/app/")` runs on success but browser Back can restore the cached screen with token + form still present, so a second submit replays a consumed token and shows the generic error; (b) double-activation before React re-renders disables the button can fire two POSTs with one token — one succeeds, the replay one rejects. Both are safe server-side but render as a confusing "invalid" after a password that *was* changed. GUESS: the envelope code for a refused token is a 4xx mapped to `INTERNAL` or `BAD_REQUEST` — needs confirming against `spec/03-api-contract.md` before any code branch.

3. **Validation gap.** CONFIRMED: local checks are `length >= 10` (`MIN_PASSWORD_LENGTH`, a client-only constant) and confirm-match (`lines 32-40`); no `maxLength` on either input. Risks: (a) silent drift from the server password policy (min length, allowed chars) — the client rejects something the server accepts or forwards something it refuses, both surfacing as generic "invalid"; (b) a very large password round-trips until the server body limit refuses it; (c) `password.length` counts UTF-16 units, not characters, so a multi-byte password near the limit can be mis-judged (GUESS — verify the server's length rule). The confirm check is correct and exact; keep it.

4. **Network down / hung connection.** CONFIRMED: `authResetPassword` accepts an `AbortSignal` but the screen passes none (`line 53`); there is no timeout, retry, or unmount abort. A hung TCP connection leaves the form at "Submitting…" with disabled inputs indefinitely, and unmount writes into stale state. Worse, an ambiguous failure (request reached the server, `Set-Cookie` issued, response lost) is rendered as `reset.error.invalid` — the password *was* changed and the person *is* signed in, yet the UI says the link is invalid, inviting a retry with a now-consumed token. CONFIRMED: reset signs the user in with a fresh session on success, so "invalid" on an ambiguous response is materially misleading.

5. **Rate limit collapse (429).** CONFIRMED: `reset-password` is capped at 5 attempts/hour per IP (`authroutes.rs:22-23`). The screen folds 429 into the same message as 5xx/expired, so a user behind shared egress gets no wait guidance, and a genuine expiry after several tries is indistinguishable from a bucket exhaustion.

## Proposed hardening (v0.4.0), with risks

- **R1. Uniform "expired / try again" secondary action, not a validity oracle.** On any failed submit keep the single `reset.error.invalid` text (unchanged for expired/used/invalid — spec §8.2) but always render a link to `/forgot-password` ("Request a new link") beside the existing back link. This gives a recovery path without differentiating outcomes. Pair with 0039 up1 (strip token from URL on mount) so a dead token is not retained. *Risk:* extra copy must be identical for every failure cause; if the link appears only on some codes it becomes an enumeration channel. New i18n keys ×3 locales (en/fr/he).

- **R2. In-flight latch + unmount abort against replay.** Add a `useRef` submitted-latch beside `status`; set it synchronously before the POST and clear it only on terminal failure (so retry stays possible), never on success. Create an `AbortController` in an effect, pass its signal to `authResetPassword`, and abort on unmount. This makes double-submit and post-unmount state writes impossible. *Risk:* must reset the latch on failure or a transient error strands the form; coordinate with 0039 up3 if the state moves to a reducer.

- **R3. Source min length from the server contract; cap input size.** Replace the hard-coded `10` with a value exported from the shared auth/password-policy module (0039 up3) and add `maxLength` to both fields. Keep the client check a courtesy; server stays authoritative. *Risk:* if no shared policy exists yet, keep the constant but document it and pin it with a test against the server rule; maxLength must not reject passwords the server accepts — verify first.

- **R4. Timeout + honest ambiguous-failure copy.** Pass `AbortSignal.timeout(~15 s)` (generous for poor networks) plus the unmount signal. On timeout/network/5xx show a distinct "could not complete — try again" message, never `reset.error.invalid`, because transport failure reveals nothing about token validity. On any ambiguous failure also note that the password may already have been changed and point to `/sign-in` — do **not** auto-retry (single-use replay). *Risk:* a fixed timeout punishes slow links (mitigate with generous value + manual retry); distinct copy is enumeration-safe only because it never varies by token or address.

- **R5. Separate 429 handling.** Branch on `RpcError.code === "TOO_MANY_REQUESTS"` (guard shared with 0028 H1) to dedicated "too many attempts — wait" copy, optionally a client-only countdown from `Retry-After`; fire before any token check, so it stays uniform. *Risk:* countdown is trivially bypassed and is UX, not enforcement; needs a seconds-vs-HTTP-date test matrix; affects fr/he copy.

- **R6. Session-aware success redirect.** On resolve, `router.replace("/app/")` then `router.refresh()` so the static-export client picks up the new cookie session instead of a cached shell. *Risk:* one extra nav/refresh; must not run before the `Set-Cookie` is applied (CONFIRMED the handler sets it on the same response).

## Explicitly rejected / deferred

- Never reveal whether a token is expired vs used vs invalid — server authority + §8.2 anti-enumeration (that is R1's constraint, not a code branch).
- No client-side token-validity cache or pre-validation RPC — it duplicates server state and creates an oracle.
- No automatic resubmission — single-use tokens make blind retries unsafe.
- Structural fixes (query→fragment token handoff, reducer, metadata/noindex, Suspense skeleton) belong to 0039; do not duplicate here.

## Success criteria / tests (extend the REQ-040 Playwright suite)

- Route-mocked expired/used/invalid/429/500/timeout each render the correct copy; all token-shaped failures render byte-identical text and always show the R1 "new link" action.
- Two synchronous clicks on Submit produce one POST; unmount mid-flight writes no state and aborts the fetch.
- Mismatch still caught locally (`posted === null`) and the success path still asserts `{token, password}` + redirect to `/app/` (keep existing pins green).
- Timeout/500 does not say "invalid" and does not auto-retry. Replay after Back shows the uniform message with a fresh-link path.
- Re-run the axe suite; the error `role="alert"` and the new link stay keyboard-reachable at 360 px, light/dark.
