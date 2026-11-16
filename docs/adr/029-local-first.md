# ADR-029 — Local-first bring-up: loopback boundary, implicit local owner, no sign-in round trip

Status: accepted (2026-11-16, improvement program M15.4 — REQ-066).

## Context

REQ-001..REQ-058 were written for a deployed, multi-tenant product. The improvement program then
asked for a mode the deployment never had: the operator runs the binary on their own machine to
try it — or to drive it from code agents (opencode, Claude Code, scripts) — before trusting it
anywhere. In that mode the machine is the network boundary and the account system is an obstacle,
not a safeguard. The desktop shell already points at `http://127.0.0.1:3000`; the web bundle already
routes to `/app` when an authenticated probe answers 200; neither needed a new protocol.

## Alternatives considered

1. **Electron-only injection.** A synthetic token baked into the Electron session partition; the
   web code unchanged. Rejected: it authenticates only the Electron shell, not code agents or any
   other loopback client, and it teaches the server to trust a token it did not issue.
2. **Synthetic cookie format.** The server recognises a `local-loopback=1` cookie and treats it as
   the owner. Rejected: a bare flag that grants power with no binding to anything is the weakest
   possible credential shape, and it forks the session machinery for one mode.
3. **Server-side local-first mode (chosen).** `BOMA_LOCAL_FIRST=true`, meaningful only on a
   loopback bind; a request without a session cookie resolves as one implicit `local-owner`
   account seeded on first use; the capability report carries `localFirst: true` so the web skips
   the sign-in round trip; a request that *does* carry a cookie is strictly resolved, and a bad
   cookie is a 401.

Scoring (criteria from the workspace stack policy, in order):
- fits the requirement exactly, one mode for Electron **and** code agents: only (3) passes;
- security track record: (3) reuses the ordinary tenant machinery (one account, one space, one
  org, ordinary rows, ordinary RLS) instead of inventing a second identity path;
- simplicity: (3) is ~150 lines in the use-case layer plus a capability bit; (1)/(2) touch the
  same places with more special cases;
- no BYPASS surface: the mode is refused at boot on a non-loopback bind, never silently weakened.

## Decision

1. `BOMA_LOCAL_FIRST=true` (default `false`). `HttpConfig::with_local_first` refuses the
   combination with any non-loopback bind at startup (`LocalFirstNonLoopback`) — exit 1 with a
   legible message.
2. The implicit account is `local-owner@local.invalid` (deliberately unroutable), seeded on first
   use: one organization, one default space ("Local space"), one user, one membership. The
   throwaway password is generated, used only for its hash, and never stored or printed.
3. `authenticate()`: no cookie + local-first → implicit owner; cookie present → strictly the
   session machinery (invalid cookie = 401, as before). The web bundle reads `localFirst` from
   `/api/auth/capabilities` and redirects to `/app/` before rendering any sign-in UI; direct
   navigation to `/sign-in` or `/sign-up` does the same.
4. The seed is idempotent across restarts (find-or-create by address; the creating race resolves
   on `Conflict` and re-reads). Verified end to end: fresh DB → implicit owner appears once;
   restart → same rows, bots persist.
5. Spec: REQ-066 (SHOULD). Non-functional honesty: the mode is **not** a trust anchor for anything
   but the machine it runs on. Do not set it on a shared, proxied or routable host.

## Consequences

- Verification (all green, see `docs/STATE.md`): fresh bring-up, `bots.create` without credentials
  seeds the owner atomically, restart preserves it, bogus cookie still 401, non-loopback boot
  refuses, `curl /api/auth/capabilities` shows `localFirst: true`, the Playwright auth cluster
  replays the redirect.
- The capability report and the golden fixture are additive (`localFirst` field); the OpenAPI
  document and `spec/openapi.yaml` are in step; `spec/EXPORT.md` records both edits.
- The Electron shell needs no change: it already targets loopback.
- A signed-in-looking single-user deployment is one env var away; that is the point, and it is
  also the whole risk — documented here, in `README.md`, in `docs/UPGRADING.md` and in
  `.env.example`, and refused anywhere else at boot.
