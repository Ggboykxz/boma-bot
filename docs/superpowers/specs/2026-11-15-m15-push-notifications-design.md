# M15 — push notifications and release maturity

- Date: 2026-11-15
- Status: approved for planning (A design, single sign-off)
- Supersedes: nothing

## Intent and why

A person should be reached when a bot needs them and when a bot finishes the work they started,
even with the app closed. Today the web gets live updates only over the event stream (SSE) while
a tab is open; nothing wakes a phone or a closed browser. The improvement program lists this as
M15: push devices + destinations + per-bot caps. Desktop signing stays out of this document — no
Apple/Windows identity is provided, and the unsigned Electron pack stays the release artifact.

## Goals

- The app can push a Web Push (RFC 8030) notification through the browser's push service, after a
  member opts in per bot, per space.
- Triggers, chosen by the owner:
  1. **Waiting** — a run pauses on a person on a `WaitingInput` (approval card, question). Owner
     scope: `WaitingTakeover` (a computer ask) is **out** of M15.1. Initiator of the run is the
     only recipient. Run-user-set is always populated, including routine- and webhook-triggered
     runs, so they ping the member behind that trigger; a routine/webhook-triggered run **is**
     a waiting push too.
  2. **Completed/failed** — the run the member started reached `Completed` **or** `Failed`
     (not `Cancelled`). Initiator gets a push; push goes to the run initiator only.

  The run's initiator for a routine/webhook run is the member behind that trigger (the routine
  creator / the webhook guard owner) — whoever `Run.user_id` names.
- Owner decision: on `Completed`, the notified recipient is the **initiator of the run**, not the
  members named `@` in the reply.
- Per-bot policy: which kinds notify, a daily cap per bot. Policy is stored per (space, user, bot).
- Capability is invisible unless VAPID is configured — an operator who opts out sees no feature,
  no errors.
- Best-effort delivery: a failed push never blocks a run, never 5xx's an RPC success path.
- M15.2 adds **Slack/Teams destinations** behind the same policy gate: a space-wide channel
  configured by a member, delivered through the existing `MessagingTransport`.
- M15.3 adds a **k3d single-node Helm chart** and its CI lint; it is the minimum viable
  single-node deployment and does not default TLS (operator's cluster concern).

## Non-goals / anti-goals

- No conversation **content** in a push body, ever (title + tag + URL only).
- No push without an explicit member opt-in (`notifications.subscribe` requires a permission grant
  on the browser).
- No background suppression by the app itself — the browser's push service is the queue; the
  server only sends.
- No spam when the member is watching: when the thread page is open and focused, an in-app toast
  is enough and the push may be skipped. (Best effort, no focusing API from a push; skipped when
  the tab is known-open via `navigator` state at subscription time. This is advisory, not a rule.)
- No desktop signing in this document.
- Mobile stays deferred (owner decision pre-M15).

## Constraint found during recon

Web Push needs a secure context. `http://localhost:3000` works (loopback is a valid secure
context). A public, plain-HTTP deployment will not offer `Notification` permission — the UI must
surface "push unavailable" instead of a dead toggle. Production is expected to put TLS in front
(Caddy/Traefik or the chart's ingress). The k3d chart in M15.3 must not pretend to provide TLS
by default.

## Approach (picked: A)

Server-side subscriptions, sent by the worker, RFC 8030 + VAPID, best-effort.

- `apps/web` ships a static service worker at `public/sw.js`, served as a file of the static
  export (extension/dot rules allow dotted files; the Rust static handler does not block `/sw.js`).
- One subscription row per (space, member, endpoint). A member may subscribe the browser once per
  space (unique key on the endpoint replaces the row on re-subscribe).
- One policy row per (space, member, bot). Missing row behaves as: `waiting: true`,
  `completed: false`, `daily_cap: 20`.
- `notification_log` records every successful send for cap counting (UTC day) and audit.
- `fail_count` per subscription; at 3 consecutive failures with a 404/410 status the subscription
  row is deleted (the endpoint is dead). Transient (5xx, network) failures only bump the counter,
  which resets on success.
- Payload `{ v: 1, kind: "waiting" | "completed", title, tag, url }` — the serialized keys are
  exactly these five. No message text, no tool arguments, no member names beyond the bot name the
  member already knows. The server builds `title` from the member's `users.locale` using a small
  built-in catalog (en-US / fr / he; anything else falls back to en-US).

### Why the other two approaches do not fit

- **B — SSE toasts only.** Cheapest, reuses the event stream, but the tab must be open. The whole
  point is reaching a closed browser/app; B fails the goal.
- **C — Destinations only (Slack/Teams), no Web Push.** The owner wants both; Teams/Slack excludes
  any member without those services. Web Push is the baseline everyone gets with a browser.

## Slice plan (each pushed with CI green before the next)

### M15.1 — Web Push core (REQ-064, new MUST)

- Service worker (`apps/web/public/sw.js`): `push` → `self.registration.showNotification`;
  `notificationclick` → focus or open `BOT_URL`; no fetch, no caching, minimal.
- Migration `migrations/0056_notifications_core.sql`
  - `notification_subscriptions (id TEXT PK, space_id TEXT NOT NULL REFERENCES spaces(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE, endpoint TEXT NOT NULL,
    p256dh TEXT NOT NULL, auth TEXT NOT NULL, user_agent TEXT, fail_count INT NOT NULL DEFAULT 0,
    created_at timestamptz NOT NULL DEFAULT now(), updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (space_id, user_id, endpoint))`
    RLS on `space_id = boma.current_space_id() OR boma.is_system()`, `FORCE RLS`.
  - `notification_policies (space_id TEXT NOT NULL REFERENCES spaces(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    bot_id TEXT NOT NULL REFERENCES bots(id) ON DELETE CASCADE,
    notify_waiting BOOLEAN NOT NULL DEFAULT true,
    notify_completed BOOLEAN NOT NULL DEFAULT false,
    daily_cap INT NOT NULL DEFAULT 20,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (space_id, user_id, bot_id))`
    Same RLS shape.
  - `notification_log (id TEXT PK, space_id TEXT NOT NULL, user_id TEXT NOT NULL, bot_id TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('waiting', 'completed')), sent_at timestamptz NOT NULL DEFAULT now())
    INDEX (space_id, user_id, bot_id, sent_at)` — RLS same.
- App layer: a port `NotificationStore` (PG + in-memory double) and a pure `PushService` in
  `crates/app/src/push.rs`:
  - `status(ctx) -> { enabled, vapid_public_key: Option<String> }`
  - `list_subscriptions / subscribe / unsubscribe`
  - `get_policy(ctx, bot_id) / set_policy(ctx, bot_id, ...)`
  - `count_today(ctx, bot_id)` / `record_send(...)`
  - `notify_waiting_run(ctx, run)` / `notify_completed_run(ctx, run)` — build the payload, check the
    member's policy, count today's sends, POST via `web-push 0.11`. Best-effort: every error path
    logs and returns, never fails a run.
- Worker: hook the sites where the member is actually stranded, not the metric helper alone:
  - **Waiting** — at the card-creation site (the approval card is created inside
    `usecases::park_tool_calls`; likewise a question card when one is parked). The push fires
    once per card actually created, not per call, never on a re-park that dedupes against an
    existing card.
  - **Completed/failed** — at the terminal transition sites (`worker.rs` `announce_run_finished`
    for `Completed`, and the `fail_run` path for `Failed`). `Cancelled` gets nothing.
- Wire RPCs (new namespace `notifications`):
  - `notifications.status {} -> { enabled, vapidPublicKey? }`
  - `notifications.subscribe { endpoint, p256dh, auth, userAgent? } -> { id }`
  - `notifications.unsubscribe { endpoint } -> { removed }`
  - `notifications.policy.get { botId } -> { notifyWaiting, notifyCompleted, dailyCap }`
  - `notifications.policy.set { botId, notifyWaiting?, notifyCompleted?, dailyCap? } -> same`
- Web UI: a `NotificationsPanel` in the thread header next to the external-agent panel
  (mirror the same interaction weight). Toggle "Notify me" → browser permission → subscribe/unsubscribe;
  per-bot checkboxes for *waiting* / *completed* + a numeric cap field; all i18n'd in en/fr/he.
  No new settings page.
- Spec: `spec/01-requirements.md` gains REQ-064 (Web Push, MUST) with
  rationale; `spec/EXPORT.md` gets the provenance row; PLAN/STATE updated.
- Tests: Rust usecases tests named `req_064_*`; `req_064_waiting_run_pings_the_initiator` against
  stubbed HTTP; web e2e `notifications-panel.spec.ts` following the external-agent pattern.
- Web Push payload is "content-free by construction": a unit test asserts the serialized payload
  keys are exactly `{v, kind, title, tag, url}` and that `title` contains no message text.
- **Subscribe endpoint validation**: HTTPS-only; the endpoint host must not be private,
  loopback, link-local, or CGNAT (checked at subscribe time against the parsed host, reusing the
  existing net-safety host screening used by messaging installs). Any violation → rule error.
- **VAPID keys**: operator provides `BOMA_VAPID_PUBLIC_KEY` + `BOMA_VAPID_PRIVATE_KEY` +
  `BOMA_VAPID_SUBJECT` (e.g. `mailto:admin@example.com`). If unset → feature disabled, warning at
  startup. The doc does not auto-generate keys; there is no hidden place where a key could end up.

### M15.2 — Slack/Teams destinations (REQ-065, new MUST)

- `notification_destinations` table (migration `migrations/0057_notification_destinations.sql`):
  `(id TEXT PK, space_id TEXT NOT NULL, kind TEXT NOT NULL CHECK (kind IN ('slack_webhook','teams_webhook')),
    label TEXT NOT NULL, ciphertext BLOB NOT NULL, nonce BLOB NOT NULL, created_at timestamptz NOT NULL DEFAULT now())`
  RLS like above. The webhook is sealed under the space DEK via the existing `secret_box` machinery
  (same row flow as `space_keys`; no new crypto).
- `notification_policies` gains: `destination_browser BOOLEAN NOT NULL DEFAULT true`,
  `destination_slack_webhook BOOLEAN NOT NULL DEFAULT false`,
  `destination_teams_webhook BOOLEAN NOT NULL DEFAULT false` — or, if exactly one destination exists
  of a kind, the boolean names it. (Final shape picked in M15.2 planning; the policy intent is
  "which channels this bot uses".)
- Wire RPCs added to the same namespace: `notifications.destinations.list/create/delete` and the
  policy flags above. Destinations are space-wide (one webhook per space), **Owner/Admin only** —
  a member may not create or delete a channel the whole space reads.
- The policy gate for a destination send is the **initiator's own policy row** and its cap: the
  capability (the webhook's existence as configured) is the opt-in, the cap is per member and per
  bot like every other channel. A cap hit skips both channels silently.
- Sending: `PushDestination` adapter; Slack POSTs `{ "text": payload.url-as-link }`,
  Teams POSTs a `MessageCard` JSON. Sent through `MessagingTransport::post_json` with the same
  SSRF rules as messaging installs (HTTPS, no private hosts unless the install opted in — for a
  webhook the hosts are Slack/Teams, so they are fixed per kind and no arbitrary URL is ever
  fetched).
- Same best-effort rule, same policy gate, cap counted across all channels.
- Spec: REQ-065 with provenance in `spec/EXPORT.md`; tests `req_065_*`; PLAN/STATE updates.

### M15.3 — k3d single-node Helm chart

- `deploy/helm/boma-bot/` (chart v0.1.0): app Deployment (serve), worker Deployment (share image),
  migrate Job (hooks), `ConfigMap` for non-secret env (`BOMA_LOG_FORMAT`, `BOMA_JOBS=postgres`,
  `BOMA_REALTIME=postgres`, `BOMA_WEB_ROOT` not needed — no frontend in the image by default;
  serve API + static export only if the export is mounted via a separate image/pvc, mirroring compose),
  `Secret` for `BOMA_MASTER_KEY` + `POSTGRES_PASSWORD` (created from values or `existingSecret`),
  Service `ClusterIP`, optional Ingress (class + TLS secret ref, default off).
- `values.yaml` carry-commented for: image repo/tag/digest, replica counts, par env, ingress on/off.
- `helm lint` + `helm template` run in a new `chart` CI job (installs helm via the official install
  script; no cluster required) so a broken chart fails the push like a broken build.
- `docs/UPGRADING.md` gains a "single-node k3d" section: `k3d cluster create --port 8080:3000@loadbalancer`,
  `helm install boma ./deploy/helm/boma-bot --set image.tag=0.1.0`, health path `/ready`.
- No REQ-claim on the chart itself; release engineering, not product surface.

## Data model and tenant rules

All new tables are space-scoped with the same RLS idiom as the rest of the schema
(`boma.current_space_id() OR boma.is_system()`, `FORCE ROW LEVEL SECURITY`). The subscriptions
table is not consulted outside the member's own tenant. The notification log is append-only; cap
counting is `COUNT(*) WHERE sent_at >= date_trunc('day', now() AT TIME ZONE 'utc')`.

## Delivery contract (what fails in which way)

- A waiting push fires **once per card created**, not on every replay through the park path, so
  approval cards and question cards ping exactly once per wait. Re-park after a steering pass
  parks a new card and pings once per new card.


- Missing VAPID → `notifications.status` answers `enabled: false`; `subscribe` is refused with a
  rule error; UI shows unavailable. No RPC 500s anywhere.
- Endpoint returns 404/410 **or 401/403** (permanent) thrice → row deleted, `fail_count` reset,
  zero user-visible surfaces.
- Network/5xx (transient) → `fail_count` bumps; subscription stays; cap not reached.
- Cap reached → send skipped, log nothing; the run is unaffected.
- Push service slow (timeout 5s) → same best-effort path; log a warning, move on.

## Provenance

- Spec extension: REQ-064 (M15.1), REQ-065 (M15.2) — recorded in `spec/EXPORT.md` alongside
  REQ-059/060/061. The floor stays frozen; these are new ids.
- `docs/adr/027-notifications.md` records: channel set, delivery contract, per-bot policy default,
  key-generation stance.

## VAPID key handling

Exactly like `BOMA_MASTER_KEY`: present + valid → feature enabled; **absent** → feature disabled
with a key warning (`notifications.status` answers `enabled: false`); **malformed** public key,
private key, or subject → fail-fast at boot (typed config). No cached-generated key, ever.

## What's left to the builder

- Exact HTTP client wire-up to `web-push` (sync encode → async POST is the crate's model; fine
  inside a `spawn_blocking` if needed).
- Exact `sw.js` deterministic registration timing in the UI router (`serviceWorker.register` on the
  notifications toggle; unregister on unsubscribe if this is the last subscription for this
  browser).
- Cap count implementation detail across the two send paths (shared helper in `push.rs`).
- Where `notifications.status` surfaces in `/rpc/...` (wire struct ids), i18n key naming.
- `sw.js` handling in the compose health-check script (nothing needed; static export is copied).
- Chart templates' exact minimal env the values must carry (`BOMA_JOBS=postgres`,
  `BOMA_REALTIME=postgres`); job ordering (`helm.sh/hook` on migrate) vs compose's `depends_on`.
- Sequencing the three slices across pushes (M15.1, M15.2, M15.3).

## Open questions (none for the builder)

No open questions. REQ-049 (voice) and REQ-056 (mobile) remain gated on the owner; desktop signing
remains blocked on an identity.
