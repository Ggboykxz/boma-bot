# ADR-027 — Web Push notifications and their delivery contract

Status: accepted (2026-11-15, milestone M15 — REQ-064).

## Context

The improvement program opened a gap: a bot that finishes while nobody has the app open is
invisible. SSE covers a watching tab; nothing else reaches a closed browser, a sleeping phone,
or another device the member is on. The owner's chosen shape (2026-11-15): Web Push in-app first,
then destinations (Slack/Teams) and k3d single-node — desktop signing stays blocked on an
Apple/Windows identity. The spec floor is frozen, so the requirement is a new id, REQ-064.

Three constraints shape the design: the web app is a **static export served by the Rust binary**
(so a service worker must ship as a file of that export and routes must not 404 it); tenant
isolation lives at Postgres RLS (`boma.current_space_id`), so new tables inherit that shape; and
the run lifecycle — the state machine is REQ-012, every write parks on a card, `park_tool_calls`
is idempotent — is the honest hook for a waiting-on-a-person push.

## Decision

1. **Initiator-only.** The push goes exclusively to `Run.user_id`: the member whose message
   started the run. Group-thread co-members are not notified in M15.1 — the waiting card is
   answerable by them in-app; the push exists because the message's author stepped away. The
   same recipient rule covers `Completed` and `Failed` (`Cancelled` never pings).
2. **Content-free by construction.** The payload is exactly `{v, kind, title, tag, url}` where
   `title` comes from a fixed three-string catalog keyed on `users.locale` (en / fr / he,
   en-US fallback). No text, no names, no tool names or arguments, no thread content — a
   `req_064_payload_is_content_free` test pins the key set.
3. **New REQ-064 (MUST).** Subscriptions, per-(space, user, bot) policy (`notify_waiting`
   default true, `notify_completed` default false, `daily_cap` default 20) and the log that
   enforces the cap, all in `0056_notifications.sql` with the tenant RLS idiom. The cap is
   counted UTC-day-anchored on successful sends only.
4. **Best-effort end to end.** Push delivery is never on the failure path of a run: missing
   VAPID → disabled silently; cap reached → skipped silently; 404/410/401/403 ×3 → subscription
   deleted, the rest ignored; transient failure → counter bump, no deletion. No RPC 5xx returns
   from push code; no run fails because a push failed.
5. **Once per card, never per replay.** The waiting push fires inside the `if let Some(card)`
   block of `park_tool_calls`, so a crash replay that absorbed the card creation does not
   double-ping. `Completed`/`Failed` fire at the two terminal transition sites in `worker.rs`.
   `WaitingTakeover` is out of M15.1; `Failed` maps onto the same `notify_completed` flag.
6. **VAPID fail-fast when present, off when absent.** `BOMA_VAPID_PRIVATE_KEY` (PEM or raw
   base64url) and `BOMA_VAPID_SUBJECT` are read at boot; absent → push is built but inert
   (`notifications.status` answers `enabled: false`); malformed → fail-fast, like
   `BOMA_MASTER_KEY`. The operator never has to pick between safe and configured — both are
   explicit states.
7. **SSRF boundary at subscribe.** The endpoint is member-supplied; the handler parses, refuses
   plain HTTP, credentialed authority and unregistered schemes, and defers to an
   `EndpointSafetyChecker` port (infra DNS-resolving adapter; HTTPS-only, no private/loopback/
   link-local/CGNAT, no metadata endpoints) before storing. Tests script the checker; the
   deployment wires it.
8. **Scope of M15.1 itself is explicit.** Destinations (Slack/Teams as alternate sinks, REQ-065)
   are a follow-up slice with their own ADR; desktop signing stays blocked on an owner identity;
   Web Push is browser-scoped. Mobile stays deferred.

## Consequences

- New tables `notification_subscriptions`, `notification_policies`, `notification_log`; trace
  budget unchanged (three missing: REQ-049, REQ-056, REQ-061); REQ-064 is covered by 14 named
  tests (domain, memory store, PG store, app delivery, RPC and one e2e).
- The web app ships `sw.js` (public export; the Rust static handler serves it).
- A member who wants Slack/Teams instead of (or in addition to) browser push waits for M15.2;
  the policy table is already the shared gate both slices enforce.
- `web-push` pulls `ece` → `openssl`; `Cargo.lock`, the Dockerfile builder stage and CI both
  carry OpenSSL headers (pkg-config + libssl-dev).
- Notifications are opt-in per bot; nothing appears for a member who has not asked.

## Alternatives considered

- **SSE toasts only** — closes the loop for an open tab, unreachable for a closed browser; the
  goal is reaching the closed case, rejected.
- **Design against Teams/Slack first** — leaves members without those services unreachable;
  Web Push is the baseline everybody gets with a browser. Not rejected, deferred to M15.2 as
  the second channel of the same policy.
- **Per-target notification routing UI** — beyond this slice; the (space, user, bot) policy is
  the coarse knob M15.1 ships; granular routing is M15.2 material.
