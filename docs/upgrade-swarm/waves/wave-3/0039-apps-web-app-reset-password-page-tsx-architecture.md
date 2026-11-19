# 0039 — apps/web/app/reset-password/page.tsx — architecture lens (v0.4.0 proposal)

Worker #039 · module `apps/web/app/reset-password/page.tsx` (11 lines) · lens: architecture · READ-ONLY review of `before-m5`. Proposal only; no code edited. Focus: reset-password flow structure and its token surface.

## What the file is — and where the mass lives

`page.tsx` is a correct static-export-friendly thin server component: `<Suspense fallback={null}>` wrapping `ResetPasswordScreen` (CONFIRMED). The `Suspense` exists because the screen calls `useSearchParams()` — a static export (`output: "export"`, ADR-013) cannot prerender a page that reads the query string at request time. Keep this routing/rendering split; all architecture debt sits one level down in `components/auth/reset-password-screen.tsx` (114 lines, `"use client"`).

Sister sources read this session: `reset-password-screen.tsx` (full), `forgot-password-screen.tsx`, `app/forgot-password/page.tsx`, `auth-shell.tsx`, `lib/api/rpc.ts` (`authResetPassword`, `AuthCapabilities`), `lib/i18n.ts` (`reset.*` en/fr/he), `spec/08-ui-ux.md §8.2`, `spec/01-requirements.md` REQ-040/041, `tests/auth.spec.ts` (REQ-040 reset test), `tests/a11y.spec.ts`, and wave-2 proposal 0027 (forgot-password architecture) — which shares this flow; this proposal stays scoped to the reset half.

## Flow structure and token surface, today (all CONFIRMED by reading source)

- Token enters as a **query param** `?token=…`, read client-side via `searchParams.get("token") ?? ""`. It is never stripped: the only `router.replace` target is `/app/`, so the token persists in history, tab state, and any same-origin `Referer` emitted after mount.
- Phase state is `idle | submitting | succeeded | failed`. On resolve the screen sets `succeeded` then immediately `router.replace("/app/")` — `succeeded` is never rendered (dead state). On any throw: `failed` + one generic `reset.error.invalid`.
- Local checks before submit: length ≥ `MIN_PASSWORD_LENGTH = 10` and confirm-match. The constant is a client-only copy with no shared source; the server stays authoritative.
- `authResetPassword({ token, password })` is called with **no `AbortSignal`** although the RPC accepts one — slow networks allow set-state-after-unmount; there is no timeout.
- `catch {}` is untyped: network down, 429, 5xx and an expired/used token all collapse to the same message. That uniformity is intentional anti-enumeration (spec §8.2, `reset.subtitle` "single-use"), so any differentiation must not reveal token validity.
- No `metadata` export and no `robots: noindex` on the page; no route-level `loading.tsx`/`error.tsx`. With `fallback={null}`, first paint of the static export is a blank frame (CLS risk).
- Missing-token state is handled well: `AuthShell` + `reset.missing-token` + link back to `/forgot-password`.
- Capabilities advertise `passwordReset` + `resetUrl` (`rpc.ts:803-806`), and sign-in conditionally renders the recovery link (per 0027), but `/reset-password` itself renders and fires the RPC regardless of the flag.
- REQ-040 Playwright test drives `/reset-password/?token=…`, asserts mismatch is caught locally (`posted === null`), then asserts the POST body carries `{token, password}` and redirects to `/app/`. The a11y suite also loads the route with `?token=test-token`. Both pins constrain token-surface changes.

## Concrete upgrades for v0.4.0

### 1. Strip the token from the URL immediately after reading it
On mount, copy the token into state, then `router.replace("/reset-password")` (same path, no query) so history, back/forward and same-origin referrers stop carrying it. Keep the in-memory copy only; do not persist to `localStorage`/`sessionStorage`.
*Risks:* (a) a refresh after stripping loses the token — mitigate by rendering the existing missing-token state, which is the safe failure mode; (b) the REQ-040 test waits for `**/app/**`, not the intermediate replace, so it should still pass, but the a11y route constant pins the query form — keep accepting it; (c) this does nothing for tokens already in access logs — pair with upgrade 2.

### 2. Evaluate a fragment-based token handoff (`#token=…`) as a DEVIATION
Change the emailed link to carry the token in the URL fragment, which is never sent to the static host or proxy logs; the token then only travels in the POST body to the API, matching §8.2's "validate then submit". During a transition window accept **both** `?token=` and `#token=` so already-sent links keep working.
*Risks:* (a) externally visible change → record in `docs/PARITY.md` and wait for approval; (b) the emailed-link generator is outside this module — coordinate with the backend/email owner; (c) the fragment is invisible to the server, so any server-side prevalidation must move to the POST; (d) both test routes use the query form — keep it supported. Treat as a decision, not a default; do upgrade 1 regardless.

### 3. Collapse the dead phase state; own the flow in one typed module
Replace the hand-rolled `useState` quartet with a small `useReducer` phase machine (`idle → submitting → succeeded | failed`, with `succeeded` rendered as a brief confirmation or removed), and extract a pure `lib/auth/recovery.ts` (no React) that owns token read/normalize/strip, the phase types, `MIN_PASSWORD_LENGTH` (ideally sourced from a server password policy — GUESS, verify against `spec/03-api-contract.md`), and the `reset.*` catalogue keys. The forgot-password half can consume the same module (per 0027).
*Risks:* (a) over-abstraction for two screens — keep it functions/types, not a class hierarchy; (b) moving validation earlier must not drift from server rules — the client check stays a courtesy; (c) any copy change re-triggers the fr/he i18n parity requirement.

### 4. Abort, timeout and typed errors — without building a validity oracle
Pass `AbortSignal.timeout(~10–15 s)` / an unmount controller into `authResetPassword`; introduce an error-code guard beside the existing `RpcError`/`throwErr` helpers. Map 429 → rate-limit copy (from `Retry-After` when present), network/5xx → "could not complete, try again", and keep every token-shaped failure (expired/used/invalid) on the **single** `reset.error.invalid` message.
*Risks:* (a) differentiating 429 is safe for enumeration only if the wording never varies by token or email — keep it uniform; (b) `Retry-After` needs a seconds-vs-HTTP-date test matrix; (c) a fixed timeout punishes poor networks — keep it generous and add an explicit retry; (d) three locales for any new string.

### 5. Page-level metadata, noindex and a real Suspense fallback
Export `metadata` from `page.tsx` ("Choose a new password", `robots: { index: false }`) and replace `fallback={null}` with an `AuthShell`-shaped token-matched skeleton so the static export's first paint is not blank.
*Risks:* (a) the skeleton joins the axe/golden snapshots — re-run the REQ-040 Playwright spec and axe suite; (b) no server session gates this route, so metadata must not imply one; (c) `noindex` is belt-and-braces, not a substitute for upgrades 1–2.

## Constraints / non-goals

- Keep `page.tsx` a thin server component; no dynamic server logic (ADR-013 static export). Token stays client-side only — no Bearer token in JS or web storage; the session remains the httpOnly cookie (REQ-041).
- Preserve the anti-enumeration contract: never reveal whether a token/address is valid; the server is authoritative.
- Capability-gating on `passwordReset` is a product call (see 0027 / `docs/STATE.md`), not a silent UI change here.
- Out of scope: password-strength meters beyond the spec, changing the POST contract, client-side token-validity caching, BoardUI Pro components.

## Suggested landing order

1 → 3 (small, mechanical) → 4 (safety) → 5 (structure, rides with goldens/axe) → 2 (DEVIATION, needs approval + email-owner coordination). Every step keeps accepting the query form until upgrade 2 is approved and shipped.
