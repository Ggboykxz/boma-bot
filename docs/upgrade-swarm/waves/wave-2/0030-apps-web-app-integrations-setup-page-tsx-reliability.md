# 0030 — apps/web/app/integrations/setup/page.tsx — reliability (v0.4.0)

Worker #030 (retry) · `apps/web/app/integrations/setup/page.tsx` · reliability · READ-ONLY review of
`before-m5`. The module is a 16-line server component (`Suspense` around the client screen); the failure surface
lives in `components/integrations/integrations-setup.tsx`, `lib/api/rpc.ts` (`call()`), the Slack callback
in `crates/api/src/public.rs`, and the Rust messaging store/use-cases. READ-ONLY; no code ran; file
facts CONFIRMED by reading, test status GUESS.

Spec anchors: spec/08-ui-ux.md §8.7 (`/integrations/setup` — post-OAuth landing: confirm the connection,
surface a partial-install report rather than a green check), §8.1/§8.9 (a blank panel is a bug;
failure states must be named), REQ-032/REQ-034/REQ-048, REQ-057 (a11y).

## What is already right (keep, don't re-propose)

- `Promise.allSettled` for the three reads; AbortController cancels in-flight reads on unmount;
  skipped plugin parts ARE surfaced per plugin with reason (REQ-034 honoured in shape); plugin-source
  link uses `noopener noreferrer nofollow`; skeleton with `aria-busy` exists.

## Failure modes → hardening

### F1. Total load failure renders a blank page, indistinguishable from "nothing installed"
Rejected results from `allSettled` are silently dropped (integrations-setup.tsx:67-86); `phase` then flips
to `"ready"`. If all three RPCs fail (offline, 500, 401 with no `unauthorizedHandler` toast surface here),
the screen renders header + nothing — no error, no retry, no empty state. House style already solves this
(`admin/plugins-screen.tsx`: `error` phase + `role="alert"` panel + `common.retry`; `activity-screen.tsx`
same). `call()` has no timeout either (rpc.ts:245), so a hung RPC pins the skeleton forever (§8.1).
P1. Track per-section status (`loading|ready|failed`) or a single `loadFailed` flag. On any rejection,
    render the plugins-screen pattern: `role="alert"`, one sentence, a Retry `Button` that re-runs the
    effect (`reloadToken` state). Distinct copy for 401/session-expired ("sign in again" link) vs generic
    failure. Emptiness of a section asserts only what was evidenced by a fulfilled call; a failed or empty
    section is hidden with an inline note, never silently.
P5. Add `AbortSignal.timeout(10_000)` default in `call()` (opt-out per call site) so F1's failure path is
    reachable at all — otherwise "hang" never becomes a named failure.

### F2. OAuth/callback errors dead-end outside the app; the page never hears about them
The Slack callback (`public.rs` ~line 610, redirect :635) reaches `/integrations/setup?installed=slack`
only on success. Every failure — unknown/expired/replayed single-use `state`, Slack refusing the code,
missing `bot_id` — falls into `messaging_deny()` (:440): a bare 401 JSON `{"error":"Unauthorized."}`
document, no app chrome, no next step (uniformity is correct for the public API, wrong for the member's
browser). Back-button refresh of the callback URL lands the same way. Separately, a repeat install of the
same workspace dead-ends: store does `ON CONFLICT (platform, workspace_id) DO NOTHING` then errors `Conflict`
when the pre-existing row is not the new id (`stores/messaging.rs` ~:258-285) → same bare 401; nothing is
stored, the member never learns why.
P2. Callback failure should `303` to `/integrations/setup?error=install_failed` (single generic code; keep
    the uniform 401 for the API/public callers, only the browser redirect changes). Page renders F1-style
    alert naming the failure + a "try the install again" link back to the messaging install entry. The
    `installed` param keeps working for success; `error` and `installed` are mutually exclusive in the UI.
    (Server-side Conflict-on-reinstall needs its own fix: upsert or idempotent success — flag to the
    messaging wave, out of this module.)

### F3. The `?installed=` confirmation banner asserts success it cannot verify — and echoes raw input
`installedFromParam` (:22) splits/lowercases/trims but validates nothing and the banner renders
"Installed: <input>" unconditionally (:108-117). The callback only ever emits `installed=slack`, but the
param is client-forgeable: a crafted link paints a success banner with arbitrary (possibly phishing-toned)
text on a trusted-looking screen. And if the install actually failed post-redirect, or `messaging.list`
rejects, the banner still claims "Installed: slack" over an empty/contradictory page.
P3. Allowlist the known platform vocabulary (`slack|telegram|<generic>`); drop unknown values. Only render
    the banner when the corresponding read corroborates (fulfilled `messaging.list` containing an active
    installation of that platform, or fulfilled `marketplace.list` for plugin installs); otherwise render
    a neutral "we could not confirm the install — check the list below" note. Never let the query string
    outrank server state.

### F4. Partial-setup truth is half-rendered; revoked/dead installs are invisible
- Plugin skipped parts render (good), but the `partial` phase computed from them (:73-79) has **no render
  branch anywhere** — it is write-only state; the "partial" outcome is indistinguishable from "ready".
- `messaging.list` returns rows of ALL statuses (store `list()` has no status filter), yet the screen
  filter is `status === "active"` only (:113): `removed` rows vanish silently, and there is no health
  signal for a live-but-dead install. Token revocation (Slack app uninstalled / `invalid_auth`) is never
  detected: outbound delivery logs `status >= 400` and swallows (`usecases.rs` ~:3275), nothing is marked.
  A dead install looks perfect on the one screen whose job is post-install honesty (REQ-048).
P4. (a) Delete or implement the `partial` phase — minimal: compute `hasSkipped` inline and give the plugins
     section a visible "some parts were skipped" header note (copy exists: `integrations.plugins-note`).
     (b) Render non-active installations in a collapsed "previously connected" list with their status
     (never hide evidence). (c) Add a per-installation health line when the wire can carry it (e.g.
     `lastDeliveryOkAt` / `lastDeliveryError` set by `deliver_external_reply` on 4xx/invalid_auth); until
     the wire grows it, the screen must say "this screen cannot see platform-side revocation" rather than
     imply health. A "Reconnect" affordance routes to the install flow. Treat server change as a separate
     wave; the page-side contract is additive, not breaking.

### F5. Smaller hardening
- `plugin.source` goes straight into `<a href target=_blank>` (:140): allowlist `https?` (and known git
  hosts) before render; drop the link otherwise. React does not sanitise href schemes.
- Dead `AuthShell` import (:8) and unused `useRouter`/`router` (:4, :48) — remove (lint hygiene; static
  export still type-checks them).
- `skipped.map((part, index) =>` uses `key={index}` (:152) — key by `part.part` + index is acceptable but
  prefer the part name; `String(part["part"] ?? part)` renders `[object Object]` for nested objects —
  coerce defensively (`typeof === "string" ? … : JSON.stringify`).
- Suspense `fallback={null}` (page.tsx) = blank first paint; use the screen's skeleton.

## axe / a11y notes
The page is in the a11y route list (`tests/a11y.spec.ts`). After P1-P4: failure panels use `role="alert"`;
banner + reconciliation note need `aria-live="polite"` (installed banner already has it); retry button
keeps visible focus (`boma-focus`); no colour-only states; skipped report stays text; skeleton `aria-hidden`;
new copy must hold 4.5:1 both themes — new strings × 14 locales (en…zh) otherwise the catalogue falls back.

## Risks and trade-offs
- R1. Goldens/REQ-057 pin this page's landmarks: F1's error panel and F4(b)'s extra section change
  landmarks — review against fixtures before merging; DEVIATION, never silent drift.
- R2. F2 touches the callback route (Rust) + page contract (`?error=`): owning-wave coordination required;
  keep the API's uniform 401 byte-identical for non-browser callers (security property — do not weaken).
- R3. P5 (10 s RPC timeout) affects every procedure; list long-poll procedures and opt them out — mechanical
  but wide; gate with existing e2e fixtures.
- R4. F3/F4(c) depend on data the wire does not carry yet (corroborating list filter is client-side fine;
  health fields are additive server work). Ship page-side first with honest wording, then wire health.
- R5. F4(b) could surface stale "removed" installs and confuse re-install; copy must say why + link the
  re-auth flow; pair with the server-side reinstall idempotency fix or re-install still dead-ends (F2).
- R6. Error copy must never include server internals; `RpcError.message` is safe to show per rpc.ts, keep it.

## Explicit non-goals
No redesign of the install flows (Slack/Telegram/generic), no per-bot assignment fiction, no retry queue for
platform delivery (deliberate: duplicate delivery worse than none), no i18n system rework, no visual restyle,
no admin/plugins surface.

## Suggested v0.4.0 rollout
Wave A (page-only, independently revertable): P1 (error+retry), P3 (banner allowlist + corroboration),
P4(a,b), F5 items. Tests: Playwright — all-three-fail, one-fail, hung (fake timeout), `?installed=evil`,
`?installed=slack` with list rejecting, reinstall `?error=install_failed`; axe on `/integrations/setup`
light/dark/RTL. Wave B (cross-surface): P5 timeout, P2 callback `?error=` redirect + page branch, P4(c)
health fields + admin-visible revocation signal + server reinstall idempotency.
Acceptance: no bare `catch` decides this screen; a failed load is named and retryable; the confirmation
banner never outranks server state; skipped parts and non-active installs are visible, honest evidence.

_Scope: line refs against `before-m5`; READ-ONLY, no code/tests/builds ran; server-side items GUESS-verified
by reading; wire health contract is a proposal, not an agreed schema._
