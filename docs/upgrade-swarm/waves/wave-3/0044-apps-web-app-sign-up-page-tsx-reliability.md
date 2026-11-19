# 0044 · apps/web/app/sign-up/page.tsx · LENS=reliability · v0.4.0 proposal

## Scope note (CONFIRMED — read at `before-m5`)

`page.tsx` is a 5-line `<Suspense fallback={null}>` shell around `SignUpScreen`; it holds
no state. Every reliability mode lives one hop away in `components/auth/sign-up-screen.tsx`,
`lib/api/rpc.ts` (`authCapabilities`, `authSignUp`, `throwErr`), and the server path
(`crates/api/src/authroutes.rs::sign_up` → `crates/app/src/accounts.rs::sign_up` → infra
`create_account`). Index-only edits would be theater; this hardens the flow across that
boundary and keeps `page.tsx` trivial. Complements 0043 (architecture). No overlap with 0042/0041.

## Current behavior (CONFIRMED from source)

- Mount: `authCapabilities(signal)` (abort on unmount) → `ready`/`error`; `localFirst === true`
  redirects `/app/`; else render skeleton → form.
- `submit`: `setStatus("submitting")` → `await authSignUp({email, password, name, locale:"en"})`
  with **no** `AbortSignal` and no timeout → `router.replace(next)`; **any** rejection maps to
  `status="failed"` + `signup.error` ("the address may already exist"). `next` is same-origin-checked.
- `RpcError` carries `code`/`status`; `throwErr` preserves HTTP status; the screen ignores both.
- Server: rate-limit 5 per 600 s per IP; email validated + lowercased (`Email::new`); password
  8–256 chars; name required non-empty after trim; account+org+space+`owner` membership written in
  one transaction; duplicate email → unique index (`0002_m1.sql:72`) → `Conflict` → 409
  `"That already exists."`; domain rule failure → 400 `"The request is not valid."` (these two
  share the `signup.error` bucket in the UI).
- i18n key `signup.closed` exists (en/fr/he) but is **dead code** — never rendered; capabilities
  has **no** sign-up open/closed field; server has no closed/allow-list check (REQ-040 unmet).

## Failure modes

1. **Double-submit race.** `busy` derives from `status`, which lags a render. Two fast
   activations (Enter mashing, double click before flush) both enter `submit`. Both POST with the
   same email: first wins, second gets 409. The UI shows "address may already exist" for an
   account that *was* created; the losing caller's session cookie may already be set. No idempotency
   key exists, so a retried-after-timeout submit that actually succeeded re-POSTs into a guaranteed
   409 and a false "already exists" alarm. (CONFIRMED client race; server 409 path CONFIRMED.)
2. **Error black hole.** `catch {}` collapses every class into `signup.error`: 400 weak password /
   bad email (misattributed to address-taken), 409 genuine conflict, 429 rate limit (user retries →
   deeper lockout under a shared IP), 5xx, and offline `TypeError`. `status` is never reset to
   `idle` on the failure path beyond `"failed"`, and there is no per-field error routing.
   (CONFIRMED `sign-up-screen.tsx:submit`.)
3. **Duplicate-owner / first-account bootstrap is unenforced.** UI copy promises "first account
   becomes operator; every later invite lands as a member," but sign-up always creates a *separate*
   org+space with `owner` membership (`infra/src/stores/accounts.rs:74,98`). Nothing arbitrates
   "first" on a fresh deployment and no closed/allow-listed mode exists, so two concurrent fresh
   sign-ups both become owners of their own orgs and the copy overpromises; on a deployment meant
   to be closed, the form renders and fails only after submit, misattributed (see 2), instead of
   the plain `signup.closed` state the screen was designed (and translated) to show. (CONFIRMED
   code paths; product intent from `signup.first-account` copy and REQ-040.)
4. **Network down / hung socket.** `authCapabilities` aborts on unmount but has no timeout — a
   black-holed connection pins the skeleton forever. `authSignUp` has no signal and no timeout at
   all — the submit button spins forever with no escape and no way to distinguish offline from
   slow from dead. Capabilities failure shows a Retry but ignores `navigator.onLine` / `online`
   events, so an offline user hammers a doomed endpoint; `localFirst` can only redirect after the
   capabilities fetch succeeds, so even loopback mode stalls behind the network it should bypass.
   (CONFIRMED missing timeout/listener.)
5. **Capabilities-error conflation.** A 5xx/429/network capabilities failure and a genuinely
   unavailable sign-up are indistinguishable from, and render the same as, a misconfigured deployment;
   Retry on a permanently-failing endpoint is an infinite no-op loop with no backoff.
   (CONFIRMED; retry wired to `reloadToken` only.)
6. Minor: `locale:"en"` hardcoded regardless of active UI locale; password manager `autoComplete`
   hints are correct, but the password field is not cleared on failure; `name` has no `required`
   and no client check while the server rejects blank-after-trim — another 400 misattribution;
   `throwErr` can surface raw non-JSON body text containing "email" (<300 chars) unsanitized. (CONFIRMED.)

## Concrete hardening (v0.4.0)

1. **In-flight guard, not just state.** `submit` takes an `inFlight` `useRef<boolean>` checked and
   set synchronously before any `setState`, released in `finally`; keep `aria-busy` + disabled
   inputs. Add `AbortSignal.timeout(15_000)` to the `authSignUp` fetch (argon2 hashing can be slow;
   15 s is generous — a named constant), and abort on unmount. If `router.replace` returns without
   navigation, reset to `idle`. *Risk:* timeout may fire after the server committed — the account
   exists but the user sees an error; mitigate via the soft-worded 409 path (item 3), never re-POST.
2. **Classify failures by code, never by prose.** Branch on `RpcError.code`: `BAD_REQUEST` →
   field-level message (min-8 password, invalid email, name required — mirror the three domain
   rules; keep server strings out), `CONFLICT` → "address already registered" + link to `/sign-in`,
   `TOO_MANY_REQUESTS` → rate-limit message with 60 s cooldown timer, `FORBIDDEN` → treat as
   closed-sign-up state (see 4); network/`TypeError` and 5xx → retryable message with Retry that
   keeps entered data. Clear password, keep email/name. *Risk:* UI now couples to the server's
   error-code contract (`spec/03-api-contract.md §3.1`); version the mapping and fall back to the
   generic message for unknown codes.
3. **Make 409 recoverable.** On `CONFLICT`, offer "Sign in instead" (carry `email` and `next` to
   `/sign-in?next=…`) and, when the user believes the account is theirs, a forgot-password link.
   Because a timed-out submit may have actually succeeded, a `CONFLICT` after a 15 s hang should be
   soft-worded ("If this is the first attempt, your account may have been created — try signing in").
   *Risk:* none beyond copy honesty; wording must stay symmetric with REQ-040 (never reveal which
   addresses are registered).
4. **Report sign-up availability instead of discovering it.** Extend `AuthCapabilities` with
   `signUp: "open" | "closed" | "allowlisted"` (server: derive from deployment config; when absent
   default to `open` to stay compatible). Render the already-translated `signup.closed` state when
   closed/allowlisted, hide the form, link to `/sign-in` and the operator contact. Enforce
   closed/allow-listed sign-up server-side per REQ-040 with a single transaction gate
   (`SELECT … FOR UPDATE` on a deployment-meta row or advisory lock; `users` count / allow-list
   consulted inside it) so two concurrent first sign-ups cannot both become owners. *Risk:* new
   wire field is a contract change — old clients ignore it (serde default), new client against old
   server must treat missing as `open`; the bootstrap gate adds one row read to sign-up (negligible)
   and must be tested under concurrency.
5. **Network-down honesty.** Add `navigator.onLine` check before submit + `online`/`offline`
   listeners to flip a persistent, dismissible banner; submit disabled while offline (with reason
   text). Give `authCapabilities` the same timeout class (10 s) and 2-step backoff on Retry
   (1 s → 4 s), resetting on success. `localFirst` redirect should proceed without awaiting more
   than the timeout. *Risk:* `navigator.onLine` is advisory (captive portals lie) — never use it to
   hard-block, only to hint; keep the manual retry.
6. **Client-side parity with server rules.** Trim email; lowercase before submit; `required` +
   `minLength={8}` + maxLength 256 on password (matches `MIN/MAX_PASSWORD_LENGTH`); `required` on
   name; `maxLength` on name/email (bound at 320 B). Pass `locale` from `useTranslation()`'s active
   locale instead of hardcoded `"en"`. Use `throwErr`'s code path for non-JSON bodies but never
   render raw body text. *Risk:* duplicating rules risks drift — generate the constants from the
   spec or add a contract test asserting client `minLength` == server `MIN_PASSWORD_LENGTH`.
7. **Failure-path polish.** `status` returns to `idle` on success path start; wrap the screen in
   the same error boundary used elsewhere; keep `fallback={null}` in `page.tsx` but render the
   skeleton as the Suspense fallback to avoid a blank flash on slow static shells. *Risk:* none.

## Suggested gates

`pnpm --filter @boma/web test` (unit: error-code mapping table), Playwright a11y spec gains a
closed-sign-up fixture, Rust integration test: concurrent first sign-up admits exactly one owner
(bootstrap gate), contract test: client minLength === server MIN_PASSWORD_LENGTH.

## Explicit non-goals

No change to password hashing, session cookie attributes, rate-limit algorithm (tracked D-01),
or the invite flow. No new identity provider. Server items are limited to the capabilities field
and the bootstrap gate; everything else is client-side.
