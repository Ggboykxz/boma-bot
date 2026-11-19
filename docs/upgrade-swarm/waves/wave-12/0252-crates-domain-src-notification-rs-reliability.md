# 0252 — `crates/domain/src/notification.rs` — reliability lens — v0.4.0 proposal

Worker #252 · module `crates/domain/src/notification.rs` (286 lines) · lens: reliability · READ-ONLY
review of `rebuild-workspace/.worktrees/before-m5` (v0.3.0-rc5). Proposal only; no code changed.
Spec anchors: REQ-064 (Web Push), REQ-065 (destinations). CONFIRMED = read this session; runtime
outcomes marked GUESS. Per AGENTS.md this is an audit-style note: findings only, owner approval is
required before any behavior change ships.

## What the module is today

Pure data shapes, no constructors/validators/invariants; every field is a raw `String`/`u32`.
CONFIRMED:
- `NotificationSubscription`/`NewNotificationSubscription` carry `endpoint`, `p256dh`, `auth`
  (the client's push keys/secret) and `user_agent`; all derive `Debug`/`PartialEq`.
- `NotificationPolicy::defaults` = waiting on, completed off, cap 20, both destinations off;
  `SetNotificationPolicy.daily_cap: Option<u32>`; `NotificationFailureOutcome::{Ok,Recorded,Deleted}`.
- `NotificationDestination` is label+kind only; its `Debug` is guarded by a test asserting no
  webhook/ciphertext/nonce (lines 168-191). `NewNotificationDestination.webhook` is a raw `String`
  sealed "on the way in" by the adapter, not by this type.
- `webhook_prefix()`: `Some(hooks.slack.com)` for Slack, `None` (any https) for Teams (lines 231-238).

All enforcement lives outside the module: RPC (`rpc.rs:4218-4287`, `:4341-4372`, `:4441-4478`), app
(`push.rs`, `usecases.rs:9850-9880`), infra (`stores/notifications.rs`) and migration `0056_notifications.sql`.
The domain owns none of it, so alternate write paths (fixtures, restores, future callers) can construct
invalid states — the same validation vacuum as `external.rs`.

## Failure modes

1. **Content leakage (CONFIRMED shape, GUESS runtime).** Derived `Debug` on `NotificationSubscription`,
   `NewNotificationSubscription` and `NewNotificationDestination` prints `endpoint`, `p256dh`, `auth`
   and the raw `webhook`; the only redaction test covers `NotificationDestination`, not the `New*`
   shapes. Any `tracing` of an incoming subscribe/create value leaks push keys or a webhook secret.
   The "content-free" guarantee is a convention in `push.rs::build_push_payload` (five fixed keys,
   `title_for` catalog), not a domain invariant: nothing stops a future caller putting run/thread text
   into a payload built from these types. Separately, Teams destinations accept any https URL with no
   `endpoint_safety` screening (`usecases.rs:9864` checks only the Slack prefix, https and no `@`), so
   a space admin can point a destination at an internal service — a content-exfil/SSRF path.

2. **Dedup (CONFIRMED).** Subscription dedup is a Postgres unique index on the *raw* endpoint string
   (`0056:21-22`); no normalization (host case, trailing slash, default port) means the same endpoint
   can be stored twice and delivered twice. The upsert resets `fail_count = 0`
   (`notifications.rs:99`), so a client re-subscribing resurrects an endpoint the 3-strike rule just
   deleted. `notification_log` has no dedup/idempotency key (`0056:54-61`; `record_send` uses a fresh
   `new_id("log")`), and `build_push_payload`'s `tag: run:{run_id}` only de-dups in the browser —
   Slack/Teams get duplicate posts on a retried notify. Transient failures also bump `fail_count`
   (`ports.rs:4520-4522`, `notifications.rs:357`) with deletion only on a *permanent* third strike,
   so a flaky key accumulates a hidden strike count that a later permanent hit deletes early.

3. **Cap bypass (CONFIRMED).** `push_one` reads `count_today` once (`push.rs:340-352`) then loops over
   *all* subscriptions calling `deliver` with no per-send re-check (`:369-384`); with N subscriptions
   and one slot left, N pushes go out — an N-fold bypass of the shared per-(space,user,bot) cap.
   The check is read-then-write, not a transactional reservation, so concurrent runs all see "under
   cap" and overshoot; `push_destinations` repeats the pattern with its own `remaining` local
   (`:187-200`). `daily_cap = 0` is accepted by the RPC — the comment at `rpc.rs:4339-4340` claims it
   is rejected but the code only rejects `> 10_000` (`:4347`) — silently muting a member. The DB CHECK
   is only `>= 0` (`0056:40`); conversion saturates/zeros (`notifications.rs:75`, `:253`), so a
   negative/corrupt row becomes cap 0 and a huge one becomes `i32::MAX`. The domain has no cap type or
   invariant to anchor any of this.

4. **Invalid keys (CONFIRMED shape, GUESS runtime).** `p256dh`/`auth` are checked only for non-empty
   trim at `rpc.rs:4256`; the migration CHECK is non-empty only (`0056:15-18`); no base64url decode,
   no length/shape check, no domain constructor. Garbage keys persist and fail at
   `web_push::SubscriptionInfo`/`builder.build()` (`push.rs:415-431`), which calls `note(false)` —
   non-permanent, so `fail_count` climbs but the 3-strike delete never fires: a zombie subscription
   that fails every send forever. Endpoint validation (https, no credentials, `endpoint_safety`) is
   likewise RPC-only; non-RPC construction paths bypass it.

## Concrete hardening for v0.4.0, with risks

- **H1 — secret-safe shapes.** Manual `Debug` for the subscription and `New*` structs that redacts
  `endpoint`, `p256dh`, `auth` and `webhook` to `[redacted]`; keep `NotificationDestination` as-is.
  Wrap `auth`/`webhook` in `secrecy::SecretString` at the boundary (app/API, not domain-only).
  *Risk:* wire/DB shape change if the wrapper reaches DTOs; keep it at the edge and add a test that
  neither `Debug` nor `to_string()` contains a sample key/webhook.
- **H2 — validated constructors (domain-owned invariants).** Add `NewNotificationSubscription::try_new`
  and `NotificationDestinationKind::validate_webhook`: strict base64url-length checks for the keys,
  https/no-credential endpoint parse, and Teams webhook screening via the existing `EndpointSafetyChecker`
  port. Make the cap a `DailyCap` newtype clamped to `1..=10_000` used by `defaults` and `SetNotificationPolicy`.
  *Risk:* narrowing accepted inputs is externally visible for restored/legacy rows — audit `0056`
  data and record a DEVIATION if a value valid today becomes invalid.
- **H3 — dedup key + re-arm guard.** Give `notification_log` a stable `(space,user,bot,run,kind)` dedup
  key (or carry `run_id` in the domain event) and make `record_send` idempotent; stop resetting
  `fail_count` on re-subscribe for an unchanged endpoint, or require an explicit `force`. *Risk:*
  needs a migration and a store signature change; a DEVIATION if a client relied on re-subscribe as a
  revive. Prefer an additive column + partial unique index.
- **H4 — atomic cap reservation.** Reserve the cap in one DB statement (`INSERT ... SELECT ... WHERE
  count < cap RETURNING`) so check-and-increment is atomic, and re-check per subscription (fix the
  N-fold bypass). Decide the `daily_cap = 0` semantic explicitly (mute vs reject) and align the RPC
  comment/DB CHECK. *Risk:* changes send counts under concurrency — intended, but differential-test
  against the reference and log a PARITY entry.
- **H5 — zombie cleanup.** Treat repeated builder/construct failures as permanent too (delete after N)
  and fail-fast on invalid keys at subscribe so they never persist. *Risk:* a transient local crypto
  fault could delete a healthy subscription; use a threshold distinct from 404/410.

## Risks / non-goals

- **R1** Domain stays pure and IO-free: validators are shape-only; network/DNS checks stay behind the
  app's `EndpointSafetyChecker` port.
- **R2** No secret *value* in types, logs, errors or golden fixtures — only redacted markers/names.
- **R3** H2–H5 change visible behavior (accepted inputs, send counts, revive semantics) — route through
  `docs/STATE.md` + `docs/PARITY.md`, wait for owner approval. Keep payload `{v,kind,title,tag,url}`
  byte-identical; REQ-064/065 goldens must not move.

## Success criteria / tests (REQ-named)

- `REQ_064_subscription_debug_redacts_endpoint_keys_and_auth`.
- `REQ_064_new_subscription_rejects_blank_or_non_base64url_keys` (proptest over arbitrary strings).
- `REQ_064_daily_cap_rejects_zero_and_above_max_at_the_domain_boundary`.
- `REQ_064_cap_cannot_be_exceeded_by_multiple_subscriptions_or_concurrent_sends`.
- `REQ_064_resubscribe_does_not_revive_a_deleted_dead_endpoint`.
- `REQ_065_new_destination_debug_never_prints_the_webhook`.
- `REQ_065_teams_webhook_is_screened_for_private_and_credentialed_targets`.

## Bottom line

The domain is a passive data bag: secrecy, dedup, cap and key validity are all conventions in the
app/RPC/DB layers, so alternate write paths persist invalid or secret-bearing state, the cap is
check-then-act and bypassable N-fold, and invalid keys become undeletable zombies. H1/H2 are local and
reversible (best first wave); H3–H5 change visible behavior and need owner sign-off. No code changed.
