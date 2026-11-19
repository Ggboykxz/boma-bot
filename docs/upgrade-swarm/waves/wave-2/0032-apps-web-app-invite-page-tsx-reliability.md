# 0032 — `/invite/page.tsx` (invite accept) — reliability lens

Target: `apps/web/app/invite/page.tsx` + the client it mounts,
`components/invite/accept-screen.tsx` (the page is a 10-line Suspense shell; every failure mode
lives in the screen). Spec: spec/08-ui-ux.md §8.2, spec/01-requirements.md REQ-042, ADR-013.
Status of claims: CONFIRMED = read in this tree; GUESS = inferred, needs verify.

## Current behavior (read, not run)

- Page renders `<Suspense fallback={null}><InviteScreen /></Suspense>`; blank flash while the
  client bundle loads.
- Screen reads `token` from `useSearchParams` (raw, unvalidated beyond truthy check), then
  `invitationPreview(token)` → `GET /api/v1/invites/{token}` with `credentials: "omit"`.
- Preview errors collapse to one `unavailable` phase: `invitationPreview` throws
  `RpcError("NOT_FOUND", …, status)` for *any* non-2xx (404, 429, 500, offline-as-TypeError via
  the catch-all) — the screen never sees the difference (CONFIRMED, rpc.ts:981-993).
- Accept (`invitations.accept`, cookie session) maps only `code === "NOT_FOUND"` to the
  "invalid, expired or already used" copy; everything else → generic error; `401` triggers the
  root `UnauthorizedRedirect` → `/sign-in?next=/invite?token=…` (CONFIRMED).
- On success: `setPhase("accepted")` then `router.replace("/app/")`. `accepted` has **no render
  branch** — it falls through to the ready screen with `status` stuck at `submitting`, so the
  button stays disabled at "Accepting…" with no success copy if navigation stalls or fails
  (CONFIRMED).
- Backend (`invite_accept`, usecases.rs:8865-8955): uniform `NotFound` for unknown/used/expired
  token *and* for caller-email mismatch; idempotent membership insert (loser of a concurrent
  accept reads back the winner's row); `invitations.accept` flip is the single decision point;
  audit `security.invite_accepted` on every join. Preview route is per-IP rate-limited
  (60/h, `INVITE_PREVIEW_MAX`) and answers the same 404 body for any dead token.

## Failure modes

1. **Expired / invalid token.** Preview → uniform 404 → "This invite does not exist or it
   expired." Correct by design. Gap: a *transient* 5xx/429 lands on the same sentence with no
   retry, so a flaky network reads as "invite is dead" and the invitee may request a re-invite.
2. **Already-used invite.** Backend refuses used tokens as `NotFound` (deliberate, anti-enumeration)
   — indistinguishable from expired. This is correct; the frontend must not try to "helpfully"
   distinguish. Idempotent re-accept by the *same* user already succeeds, which is the right
   answer for a double-click or a replayed link.
3. **Session conflict.** Signed in as an address ≠ invited address: backend returns the same
   `NotFound`; the screen shows "invalid, expired or already used" with no hint that a
   *different account* is the problem and no sign-out/switch-account affordance inline. The
   sign-in/sign-up links with `next=` are present but unlabeled for this case (CONFIRMED).
   Also: session expiring between preview and accept → `401` → full redirect to sign-in; the
   pending accept is abandoned, not retried.
4. **Enumeration.** Preview: uniform 404, per-IP cap, token stored hashed, `x-robots-tag: noindex`,
   `no-referrer`, correlation templates the path (`/api/v1/invites/{token}`), `credentials:"omit"`,
   static export means the token never transits HTML. Solid. Remaining soft spots: the frontend
   flattens 429/5xx into the same UI as 404 (safe but useless), and the raw token is interpolated
   **unencoded** into the inner `next=` URL (`/invite?token=${token}` inside an already-encoded
   outer query) — a token containing `&`/`#`/`?` (malicious or merely unusual) corrupts the
   roundtrip and could smuggle a different `next` through the sign-in hop (CONFIRMED,
   accept-screen.tsx:101-108). Sign-in guards `next` with a `/`-prefix check, but the corruption
   is real.
5. **Hangs and dead-ends.** No timeout on preview fetch or accept call (abort only on unmount);
   a hung accept leaves "Accepting…" forever. No error boundary below the page; a render throw
   blanks the whole `AuthShell`. `Suspense fallback={null}` shows a white flash.
6. **Post-accept limbo.** Successful accept with failed/slow `router.replace` → disabled frozen
   button, no confirmation, no manual link to `/app/` (CONFIRMED).

## Proposed hardening for v0.4.0

1. **Typed preview failures (keep the uniform 404).** In `rpc.ts`, let `invitationPreview`
   propagate status: branch `429` → "Too many requests, try again shortly" (+ `Retry-After` if
   present), `5xx`/network/timeout → retryable generic error with a **Retry** button (re-runs
   the effect via a `retryNonce` state). Keep 404/410/expired/used/email-mismatch byte-identical.
   *Risk:* copy drift between the three dead-token paths; mitigate with one shared sentence key.
2. **Timeouts.** `AbortSignal.timeout(15s)` on the preview fetch and the accept `call`; on
   abort, show the retryable error, not `unavailable`. *Risk:* slow self-hosted deployments —
   15s is generous; log timeouts client-side without the token.
3. **Fix the `next` roundtrip.** Build the inner URL with `encodeURIComponent(token)` (or a
   `URLSearchParams`), and add a Playwright roundtrip test with a hostile-token fixture
   (`a&b=x#y`). *Risk:* none beyond the test; touch no other call sites.
4. **Dedicated `accepted` screen.** Render the existing `invite.accepted` i18n key (already in
   en/fr/he) plus a manual "Go to the app" link; wrap `router.replace` in try/catch and rely on
   the static `<Link>` as the fallback when client navigation fails. Also transition on
   navigation failure instead of leaving the frozen button. *Risk:* double navigation if both
   fire — harmless.
5. **Session-conflict affordance (no backend change).** Below the accept error, when the error
   is the `NOT_FOUND` accept refusal, add a muted line: "Signed in as the wrong person?
   Sign out and re-open this link." linking to sign-out with `next=/invite?token=…`. Do **not**
   distinguish email-mismatch from dead-link in copy — the uniform refusal is a security
   invariant, not UX debt. *Risk:* the wording must not invite probing; keep it generic.
6. **Error boundary + real fallback.** Route-level `error.tsx` for the invite subtree (retry +
   sign-in link) and a skeleton `Suspense` fallback in `page.tsx` instead of `null`.
   *Risk:* one more file; standard Next pattern.
7. **Accept retry semantics.** On `TOO_MANY_REQUESTS` from accept, show the rate-limit copy and
   keep the button enabled after a short cooldown; on `401`, let the root redirect do its job
   (it already preserves `next` with the token — after re-auth the user lands back here).
   *Risk:* accept is not idempotent-keyed client-side, but the server flip makes replay safe;
   document that reliance in the screen comment.
8. **Tests (extend `apps/web/tests/invite.spec.ts`, routes mocked).** Cases: preview 429 →
   retryable copy, retry recovers; preview 500 → retry; accept 401 → lands on `/sign-in` with
   `next` intact (assert encoded inner URL); accept `NOT_FOUND` → uniform invalid copy +
   sign-out affordance; successful accept → accepted screen with manual `/app/` link; hostile
   token roundtrip. *Risk:* Playwright runtime grows ~2 min; keep mocks, no network.
9. **Observability.** Client-side counter/log for preview/accept failure *kind* (never the
   token, never the invited email); server already audits `security.invite_accepted` and
   templates correlation paths — no server work needed. *Risk:* accidental token logging —
   enforce by type (pass `status`/`kind` only).

## Risks / invariants not to break

- The uniform-refusal invariant (unknown = used = expired = wrong-email) is the whole
  anti-enumeration posture; any hardening that differentiates those is a regression, however
  helpful it feels. 429/network/timeout *may* differ — they leak nothing about the token.
- Rate limiting stays per-IP on the preview route; do not add per-token or per-account limits
  that themselves become an oracle.
- The invited email address must never appear in UI, log, or error under any branch.
- Static export (ADR-013) forbids server-side per-token rendering — all fixes stay in the
  client + existing RPC surface; no new endpoints without a spec change.
- Silent token exposure: keep `credentials: "omit"` on preview; never move the token into a
  header or body that other routes might log.

## Verification

`bun run test:e2e -- invite` (mocked), `tsc`/`lint` for the two touched files, then a manual
pass against a seeded deployment: valid link, dead link, used link, wrong-account accept,
offline accept (devtools throttling), and a `429` from the preview route.
