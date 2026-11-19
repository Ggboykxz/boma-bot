# 0100 — `apps/web/components/thread/notifications-panel.tsx` — reliability lens (v0.4.0)

Module: `apps/web/components/thread/notifications-panel.tsx` (372 lines, read this session).
Lens: reliability. Companion: 0099 (architecture) — this doc targets failure modes, not structure.
Spec anchors: REQ-064 (Web Push, MUST) and REQ-065 (destinations, MUST), local id M15.1.
Status: "Reliability surface" is CONFIRMED by reading the file, `lib/api/rpc.ts:1533–1684` and
`public/sw.js` this session; code paths are CONFIRMED, their user impact is GUESS until the proposed
Playwright tests exist. Read-only review; this document is the only write.

## Reliability surface (CONFIRMED)

- `phase` is derived once on mount: `notificationsStatus()` → `getRegistration().pushManager.getSubscription()`
  → `notificationsPolicyGet`; nothing later re-checks permission, subscription validity or server state.
- `toggle()` calls `pushManager.subscribe` then `notificationsSubscribe`; its `catch` only sets `phase="failed"`.
  Unsubscribe calls the server first, then `subscription.unsubscribe()` (lines 160–162).
- `refreshDestinations()` maps any error to `[]`; the mount-time destinations load is a nested
  fire-and-forget. No focus refresh, polling or response sequencing exists anywhere in the file.
- `patchPolicy()` swallows `policySet` failures (`.catch(() => null)`) and sends on every keystroke of
  `dailyCap`; replies are applied in arrival order.
- `sw.js` builds the notification URL from the push payload, defaults to `/app/` only when `url` is
  absent, and `notificationclick` calls `client.navigate(url)` on the first window (sw.js:7–48).
- Every RPC helper accepts an optional `AbortSignal`; the component passes none (rpc.ts:1543+).

## Failure modes

1. **False "on" → missed pushes (high).** If `notificationsSubscribe` fails after
   `pushManager.subscribe` succeeded, the browser holds a live subscription the server never recorded.
   `catch` sets `failed` for this mount, but the next mount reads `getSubscription()`, sees it and sets
   `phase="on"` — the UI promises notifications that will never arrive. Symmetric risk on teardown:
   server unsubscribe can commit while local `subscription.unsubscribe()` fails, leaving a browser
   subscription the server ignores (CONFIRMED code path; outcome GUESS).
2. **Transport failure disguised as "disabled" (high).** A rejected `notificationsStatus()` is caught
   at lines 143–147 and sets `phase="disabled"`, rendering only the muted "unavailable" note. A
   transient backend/network blip is indistinguishable from push genuinely being off, with no retry
   and no error. Likewise `isPushSupported()===false` (e.g. a non-secure context) renders `null`
   silently — the user never learns why the panel vanished (CONFIRMED).
3. **Drifting permission / rotated subscription (high).** Nothing listens for notification-permission
   changes, `pushsubscriptionchange`, or window focus. If the user revokes permission in browser
   settings, or the push service rotates/expires the endpoint, the panel keeps showing "on" and no
   pushes arrive. No re-subscribe handshake exists in this file (CONFIRMED; rotation cadence GUESS).
4. **Stale / wiped destination list (medium).** `destinations` is loaded once on mount; a create or
   delete in another tab/session never appears. `refreshDestinations()` replacing the list with `[]`
   on any error — including the refresh after a successful create/delete — renders a wrong, empty
   list with no error (only create/delete exceptions set `destinationEnabledFlags`). Overlapping
   refreshes have no sequencing, so a slower older reply can win (CONFIRMED code; race GUESS).
5. **Lost policy writes / stale checkboxes (medium).** `patchPolicy` discards errors, so a checkbox
   that failed to persist still looks checked; rapid toggles and per-keystroke `dailyCap` apply replies
   in arrival order, so an older reply can overwrite newer user intent (CONFIRMED).
6. **Click-through to a missing target (medium).** Push payloads are built server-side; `sw.js`
   navigates to whatever `url` it was given. A push for a bot/thread the member can no longer access
   (revoked, deleted, or left over from the false-"on" case) opens a dead deep link; `sw.js` falls
   back to `/app/` only when `url` is absent, not when the target no longer exists. The panel's part
   is to stop producing subscriptions/state that cause such pushes and keep links on a stable route
   that degrades gracefully (CONFIRMED code; target lifecycle GUESS).

## Concrete hardening (v0.4.0)

- **Reconcile, don't trust, local state.** After mount and after subscribe/unsubscribe, compare the
  browser `getSubscription()` with server truth. If the server has no endpoint for a live local
  subscription, self-heal by re-posting `notificationsSubscribe`, or clear the local subscription if
  permission is not granted — never set "on" on a local-only subscription. In `toggle()`'s failure
  path, best-effort `subscription.unsubscribe()` so a half-subscribed browser cannot appear enabled;
  derive `phase` from a fresh status read rather than an optimistic `setPhase`.
- **Make transport failure a first-class state.** Separate "disabled by deployment" from "could not
  reach the server": on a `notificationsStatus` rejection show a translated error + Retry, never the
  muted unavailable note. Give `isPushSupported()===false` a short, non-blocking explanation where
  policy allows so a non-secure context is diagnosable. Playwright cases must carry REQ-064/065 names.
- **Detect drift.** Listen to `navigator.permissions.query({name:"notifications"}).onchange` plus a
  `visibilitychange`/focus re-check to re-derive `phase`; on permission loss stop claiming "on" and
  offer re-enable. Add a `pushsubscriptionchange` handshake (coordinated with `sw.js`) that
  re-subscribes and re-registers the new endpoint with the server.
- **Sequence list and policy state.** Add a per-key generation counter; apply only the newest
  destinations/policy reply. Never turn a failed refresh into `[]` — keep the previous list and show a
  non-blocking error with Retry. Refresh destinations on focus (and, if the spec allows, a slow poll)
  so cross-admin changes surface while keeping the last good list on transient failure.
- **Make policy writes honest.** Keep an optimistic local value but roll it back and show an inline
  `role="alert"` when `policySet` fails; debounce `dailyCap` and commit on blur/Enter, clamped to
  `0..10000`, with the Rust server remaining authoritative. Tag replies with the generation so an old
  response cannot overwrite a newer toggle.
- **Deep-link contract.** Coordinate with the backend/SW so every push carries a validated, stable
  route; have `sw.js` fall back to a hub that handles "thread missing" with recovery, and avoid
  sending to subscriptions reconciled as stale. Keep webhook URLs sealed and out of this view.

## Risks / trade-offs

- Self-healing re-subscribe may double-register on a flaky network; server `subscribe` must be
  idempotent by endpoint and reconciliation must not loop. Test: fail the second subscribe and assert
  a single recovered subscription.
- Rolling back a local subscription on error can undo a server commit that actually succeeded;
  always reconcile with a fresh status read instead of assuming either side won.
- Permission/focus listeners add re-renders and can fight a user mid-toggle; guard with the same
  generation counter and a `busy` latch released in `finally` on every path.
- A "transport failure" state is new externally visible behaviour; leave the genuine `enabled:false`
  copy unchanged, add new strings via i18n keys, and record any wire-visible change as a DEVIATION.
- Debounced policy writes change perceived responsiveness and wire timing; confirm the Rust contract
  tolerates suppressed intermediate values, otherwise record a DEVIATION and seek approval.
- SW-side changes (`pushsubscriptionchange`, deep-link fallback) live outside this file; keep them in
  a separate change and gate on REQ-064/065 Playwright tests plus `spec/golden/ui/*.json` replay.
- No push payload, policy value or webhook URL may ever carry or surface conversation content.
