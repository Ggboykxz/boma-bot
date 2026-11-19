# 0027 — apps/web/app/forgot-password/page.tsx — architecture lens

- Target: `before-m5` (v0.3.0-rc5), module `apps/web/app/forgot-password/page.tsx` (6 lines).
- Read-only review; no code edited. Sister files read because the page is a pure delegate:
  `components/auth/forgot-password-screen.tsx` (77 lines), `components/auth/reset-password-screen.tsx` (full),
  `components/auth/auth-shell.tsx`, `app/reset-password/page.tsx`, `lib/api/rpc.ts` (`authForgotPassword`,
  `authResetPassword`, capabilities block), `lib/i18n.ts` (`forgot.*` / `reset.*`), plus `spec/08-ui-ux.md` §8.2,
  `spec/01-requirements.md` REQ-040.
- Spec anchors: `spec/08-ui-ux.md §8.2` (Password recovery: identical response whether or not the address exists;
  reset screen validates token and password locally, then signs the user in), REQ-040 (MUST), REQ-041 (`no-store` on
  `/api/auth/*`).

## The page's role, today

`/forgot-password` is one of two public entry routes (with `/reset-password`) for the recovery flow. The page itself
is exactly right: a server component, five meaningful lines, delegating to `<ForgotPasswordScreen />`. That shape
should survive v0.4.0 — routing and metadata belong in `app/`, rendering in `components/`. The architecture debt is
all one level down: the flow is two client screens joined only by a URL convention, with no shared contract.

Confirmed observations (read this session):

- `ForgotPasswordScreen` is `"use client"`, calls `authForgotPassword({ email: email.trim() })` with **no
  `AbortSignal`** even though `authForgotPassword` accepts one — set-state-after-unmount is possible on a slow network.
- Uniform-response anti-enumeration is honored in code and comment: any server answer yields the same `sent` state;
  any throw yields one generic `forgot.error.generic`. Correct; keep it.
- `catch {}` is untyped — network down, 429 (rate limited), 5xx and 4xx are indistinguishable, so no retry-after
  guidance can be shown even when the server sends one.
- `/reset-password` page wraps the screen in `<Suspense fallback={null}>`; token arrives as a **query param**
  (`/reset-password?token=…`), is read client-side via `useSearchParams()`, and is **never stripped from the URL** —
  it persists in browser history, tab title state and any same-origin referrer emitted afterwards.
- `useSearchParams` in a static export (`output: "export"`) is the reason for the Suspense wrapper; `fallback={null}`
  means a blank frame on first paint.
- Reset screen sets `status: "succeeded"` and immediately `router.replace("/app/")` — `succeeded` is a dead state,
  never rendered. `MIN_PASSWORD_LENGTH = 10` is duplicated client-side with no shared source.
- Capabilities endpoint reports `passwordReset` and `resetUrl` (`lib/api/rpc.ts:803-806`); `sign-in-screen.tsx:170`
  hides the recovery link when disabled — but `/forgot-password` and `/reset-password` render and fire the RPC
  regardless. The gate exists on one door only.
- No `metadata` export on either recovery page (no tab title, no `robots: noindex`); no route-level
  `loading.tsx` / `error.tsx`; token-bearing URL makes `noindex` table stakes.
- Back navigation is duplicated in the sent state: a `Button → router.push("/sign-in")` **and** the `Link` to
  `/sign-in` rendered below the conditional.
- Emails themselves are out of this module's scope, but the token is single-use per the catalogue wording
  (`reset.subtitle`); the client has no way to distinguish "already used" from "expired" — by design, and fine.

## Flow structure in v0.4.0

Request (`/forgot-password`) → uniform `sent` confirmation → email link (`/reset-password?token=…` or fragment) →
token read once → local checks (length ≥ 10, confirm match) → `POST /api/auth/reset-password` → session cookie set →
hard replace to `/app/`. Two routes, one implicit state machine, zero shared code. The v0.4.0 shape: keep both pages
as thin server components, hoist shared recovery chrome and metadata into a small `app/(auth)/` layout or a
`components/auth/recovery/` feature folder, and give the two screens one typed contract (`useRecoveryFlow` or a plain
`lib/auth/recovery.ts`) that owns the phase machine, the abort wiring and the constants.

## Proposals

### 1. Add `metadata` (+ `robots: noindex`) to both recovery pages; give reset a real Suspense fallback
`export const metadata` on `app/forgot-password/page.tsx` and `app/reset-password/page.tsx` ("Reset your password",
`robots: { index: false }`). Replace `fallback={null}` with a minimal `AuthShell`-shaped skeleton so first paint in the
static export is not blank. Risks: (a) trivial diff, but the skeleton markup joins any golden/axe snapshot of
`/reset-password` — run the REQ-040 Playwright spec and the axe suite; (b) no server session gates these routes, so
metadata must never imply otherwise; (c) `noindex` is belt-and-braces — the token URL should be unlisted anyway.

### 2. Strip the token from the URL as soon as it is read; evaluate a fragment-based handoff
On mount, copy `searchParams.get("token")` into state, then `router.replace("/reset-password")` so history and any
later same-origin navigation stop carrying it. Larger option: change the emailed link to
`/reset-password#token=…` (fragment never reaches the server or proxy logs — the token only travels in the POST body,
which §8.2 already implies). Risks: (a) the fragment variant is an externally visible change → DEVIATION, record in
`docs/PARITY.md` and wait for approval; (b) already-sent email links must keep working — the screen should accept
**both** `?token=` and `#token=` during a transition window; (c) `router.replace` before the user submits loses the
token on accidental refresh — mitigate by holding it in memory only and rendering the missing-token state on reload
(that is the safe failure mode; do not persist it in web storage). Do (2a) regardless; treat (2b) as a decision.

### 3. Capability-gate the recovery routes, not just the sign-in entry point
Read `passwordReset` from the capabilities payload (already fetched by sign-in; `tests/capabilities.ts` has the
fixture) and render a "password recovery is disabled on this deployment" state instead of the form when false.
Risks: (a) fail-open vs fail-closed — a misconfigured deployment that hides the flag must not strand users who need
recovery; default to showing the form but logging, or fail closed with a support-link message — needs a product call
recorded in `docs/STATE.md`; (b) adds a capabilities fetch round-trip and a loading state to a public page; (c) cached
capabilities (`no-store` applies to `/api/auth/*`, not necessarily to capabilities) — verify cache headers before
relying on freshness. Server remains the boundary; this only aligns the UI with REQ-040's "deployment chooses available
methods".

### 4. Abort, timeout and typed errors on the request path
Pass an `AbortSignal.timeout(…)` (or a controller aborted on unmount) into `authForgotPassword`/`authResetPassword`;
introduce `isRpcErrorCode(err, …)` next to the existing `RpcError`/`throwErr` helpers and map 429 → "too many attempts,
try again in N minutes" (from `Retry-After` if present), keep every other failure on the uniform anti-enumeration
message. Render or delete the dead `succeeded` status. Risks: (a) a fixed timeout punishes legitimate slow requests on
poor networks — use ~10–15 s and surface "still trying" only if the design allows; (b) distinguishing 429 reveals rate
limiting, not account existence — safe for enumeration, but the wording must not vary by email; (c) `Retry-After`
parsing needs a test matrix (seconds vs HTTP-date); (d) unifying on `instanceof RpcError` funnels fetch TypeErrors to
the generic branch — correct per the error envelope, but cover network-down in the Playwright path.

### 5. Extract one typed recovery-flow module (`lib/auth/recovery.ts`) and share the constants
Own the phase machine (`idle → submitting → sent|failed`, `idle → submitting → succeeded|failed`), `MIN_PASSWORD_LENGTH`
(ideally sourced from the server's password policy if one exists — GUESS, verify against `spec/03-api-contract.md`),
the token read/normalize/strip logic, and the catalogue keys, in one place consumed by both screens. This gives
REQ-040 one testable contract instead of two copies. Risks: (a) over-abstraction for two screens — keep it a module of
functions/types, not a class hierarchy; (b) any semantic drift in shared copy re-triggers the fr/he translation
requirement in `docs/UI.md`; (c) moving validation earlier must not duplicate server-side rules that can drift — the
client check stays a courtesy, the server stays authoritative.

## Explicit non-goals

No client-side caching of token validity, no "does this email exist" preflight, no change to the `POST` contract, no
password-strength meter beyond the spec, no BoardUI Pro components, no rate-limit bypass UX. The server refuses what
it refuses; the UI only says so honestly, identically, and early.
