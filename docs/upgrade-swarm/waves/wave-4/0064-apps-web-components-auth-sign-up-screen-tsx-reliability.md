# 0064 — `apps/web/components/auth/sign-up-screen.tsx` — Reliability (v0.4.0)

Worker #064, wave 4, lens: reliability. Target: `apps/web/components/auth/sign-up-screen.tsx`
("use client"; REQ-040, spec §8.2) with adjacent reads `lib/api/rpc.ts#authCapabilities/authSignUp/throwErr/RpcError`,
`components/auth/sign-in-screen.tsx`, `lib/use-translation.ts`, `spec/01-requirements.md` §"sign-up can be
disabled", `spec/08-ui-ux.md` §8.2. Read-only; no product code changed. Claims: CONFIRMED = read in this
tree; GUESS = inferred, needs verification against a running server.

## Today (CONFIRMED)

- Capabilities effect keyed on `[reloadToken, router]`, one `AbortController` aborted only on unmount/
  reload; no timeout. Failure → static `phase="error"` with a working Retry (`load()` bumps `reloadToken`).
- `submit` does `setStatus("submitting")`, awaits `authSignUp({email,password,name,locale:"en"})`, then
  `router.replace(next)`. `busy = status === "submitting"` disables inputs and the button. The `catch`
  has no binding, discards the error, and shows one generic `t("signup.error")`.
- No `AbortSignal` passed to `authSignUp` (the RPC accepts one); no timeout; no idempotency key; no
  in-flight ref guard. `fieldError` is a single string, not per-field. `next` is guarded against absolute
  and `//` URLs but not backslash/encoded variants.
- Form renders whenever capabilities load; there is no `signUpEnabled`/`passwordAuth` gate, despite the
  file comment claiming a closed `/sign-up` is shown plainly and spec §8.2 requiring disabled sign-up
  to be refused. `locale:"en"` is hardcoded while `useTranslation()` derives the active locale from
  `useLocale()`. Skeleton is `aria-hidden` with no live region.

## Failure modes

1. **Duplicate owner / ambiguous outcome.** The screen promises the first account "may become the
   operator" (spec §8.2) but has no way to know who won. `authSignUp` is non-idempotent: two concurrent
   requests, or a retry after the server committed but the response was lost, yield a `CONFLICT` that is
   rendered as the same generic error as a dead server. The user is told "sign-up failed" although an
   account may exist and may already hold operator power; a blind retry can create a duplicate member or
   repeatedly fail forever (GUESS on exact server atomicity; the client cannot detect either case).
2. **Validation collapse.** Only HTML5 `required`/`type="email"` gate input; no length/strength rules,
   and whitespace-only values pass `required` then trim to `""`. Server `BAD_REQUEST`/422 (weak password,
   bad email, long name) and closed sign-up all collapse into one `t("signup.error")` — no field-level
   feedback, no code branch. Native browser tooltips bypass the i18n layer; the hardcoded `"en"` locale
   can store the wrong locale on the member and send English mail on a non-English UI (CONFIRMED code,
   GUESS on stored-language impact).
3. **Double submit.** `disabled={busy}` is render-level only; a fast second click/Enter before React
   commits, or a keyboard repeat, invokes `submit` twice. With no `submittingRef` and no signal, both
   requests fly; the second can overwrite a success with a `CONFLICT` error. After failure the button
   re-enables with no backoff, so hammering retry on a rate-limited server produces `TOO_MANY_REQUESTS`,
   again shown generically. Unmount mid-submit still calls `router.replace` on an unmounted tree.
4. **Network down / half-open connection.** Fetch has no timeout in either call: a captive portal or dead
   tunnel leaves the skeleton `aria-busy` forever, or leaves the button permanently disabled at
   `"submitting"`. A true network failure throws `TypeError` (no `RpcError.code`), which the bare
   `catch` cannot distinguish from `INTERNAL`/5xx, so "check your connection" and "try a different email"
   are indistinguishable. `load()` never resets `status`/`fieldError`. No `online`/`offline` awareness.

## Hardening (v0.4.0)

- **Make submit single-flight and cancellable.** `const inflight = useRef(false)` + `const ac = useRef<AbortController|null>(null)`; at the top of `submit`: `if (inflight.current) return; inflight.current = true;` reset in `finally`. Pass `ac.current.signal` to `authSignUp` and abort on unmount. Guard `router.replace` with `!signal.aborted`.
- **Idempotency for the ambiguous outcome.** Generate one `crypto.randomUUID()` per attempt and send it as an idempotency key; server must dedupe and replay the original result (wire/OpenAPI + `crates/api/src/wire.rs` change — DEVIATION, owner sign-off). Until the contract lands: on timeout/network error show a distinct "couldn't confirm, try again or sign in" copy instead of the generic failure, and never silently retry a possibly-committed sign-up.
- **Error taxonomy.** Import `RpcError`; `catch (error)`: classify `error instanceof RpcError ? error.code : "NETWORK"` (also treat `AbortError` as cancellation, not failure). Map `CONFLICT` → "email already registered" + link to `/sign-in?email=...`; `BAD_REQUEST` → field errors; `TOO_MANY_REQUESTS` → bounded exponential backoff; `INTERNAL`/5xx → retryable generic; `NETWORK` → offline copy. Keep sign-up copy consistent with the spec's "never say whether the address exists" rule where it applies.
- **Real validation.** Add client checks mirroring server rules (trim/reject empty, email format, min password length, name cap) behind `noValidate` so messages come from i18n keys, not browser tooltips; use a `{name?,email?,password?}` error map, focus the first invalid field, and keep the server authoritative. Replace `locale:"en"` with the active locale from `useLocale()` (expose it alongside `useTranslation`).
- **Bound and clarify capabilities.** Compose the unmount controller with `AbortSignal.any([controller.signal, AbortSignal.timeout(12000)])`; on timeout → error phase (Retry already exists). Add `role="status"` "Loading sign-up options" for the `aria-hidden` skeleton. Reset `status="idle"` and clear field errors in `load()`. Tighten `next` to reject `\` and encoded separators, not just `//`.
- **Closed sign-up.** Add `signUpEnabled` (and reuse `passwordAuth`) to `AuthCapabilities`; when false render a plain "sign-ups are closed" panel with a sign-in link instead of a form that always fails (contract change — DEVIATION). Interim fallback: map `CONFLICT`/`FORBIDDEN` to that copy. Track `navigator.onLine` via `online`/`offline` listeners to disable submit with offline copy.

## Risks

- Idempotency key and `signUpEnabled` are contract changes: regenerate the typed client, never hand-write a duplicate; record both in `docs/PARITY.md` and wait for owner approval before shipping user-visible flow changes.
- `AbortSignal.any`/`timeout` and `crypto.randomUUID()` need an evergreen baseline — verify browserslist; provide a fallback if not.
- Client validation must mirror, not override, server rules; a laxer client only shifts the failure later. Do not let the CONFLICT→sign-in link leak account existence beyond what the server already reveals.
- Removing hardcoded `"en"` and adding error/offline/closed keys changes stored locale and visible copy — new keys must land in every locale or fall back cleanly (i18n/RTL gate, REQ-057).
- Backoff, extra focus moves and new selectors can break existing REQ-040 Playwright coverage — update tests, never weaken them; keep WCAG 2.2 AA (visible focus, `role="alert"`, 360 px, light/dark, reduced motion).
- Operator claim is server-owned: keep the wording as "may become" until a capability confirms first-account status; do not imply a guarantee the client cannot verify.

## Tests (Playwright, REQ-040)

loading→ready; capabilities timeout→error→retry→ready; offline submit with no request; double-click yields exactly one `POST /api/auth/sign-up`; ambiguous timeout shows confirm copy; `CONFLICT`→sign-in with email; `BAD_REQUEST` per-field focus; `TOO_MANY_REQUESTS` backoff; seeded closed sign-up shows closed panel; active locale sent; unmount mid-submit does not navigate; axe scan + keyboard-only path. Mark CONFIRMED only after running against the seeded server.
