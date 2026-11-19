# Notification domain model — architecture proposal (v0.4.0)

Module: `crates/domain/src/notification.rs` (REQ-064 Web Push, REQ-065 destinations).
Lens: architecture. Scope: proposal only — no code was changed.

## Current shape (CONFIRMED — read this session)
- Two enums: `NotificationKind { Waiting, Completed }` and
  `NotificationDestinationKind { SlackWebhook, TeamsWebhook }`, each with `as_str()` but no inverse.
- Data: `NotificationSubscription` (endpoint, p256dh, auth, user_agent, fail_count, 2 stamps),
  `NewNotificationSubscription`, `NotificationPolicy` (4 bools + `daily_cap` + 2 stamps),
  `SetNotificationPolicy` (all Option patch fields), `NotificationDestination` (label/kind/ids only),
  `NewNotificationDestination` (carries raw `webhook: String`), `DestinationFlags` (2 bools).
- `NotificationPolicy::defaults(bot_id, now)` encodes the "missing row" rule; caller supplies the clock.
- `NotificationFailureOutcome { Ok, Recorded(u32), Deleted }`; `Deleted` comment names a
  three-permanent-failures rule but the threshold constant is not in this module.
- All timestamps are plain `String`; all counts are plain `u32`; ids are `String` (not the
  `ids.rs` newtypes). Module is pure — no IO/clock/async — which is correct for `domain`.
- Callers confirmed in app (`push.rs`, `usecases.rs`, `ports.rs`, `service.rs`), api (`wire.rs`,
  `rpc.rs`), infra stores (`notifications.rs`, `destinations.rs`), testkit and infra tests.

## Guiding principles
Keep the crate pure; make invalid states unrepresentable; keep the wire/DB strings byte-stable
(`"waiting"`, `"completed"`, `"slack_webhook"`, `"teams_webhook"`). The REQ-064/065 promise —
pushes carry **no conversation content** — should be a property of the type system, not of
adapter discipline. Any externally visible change is a DEVIATION needing `docs/PARITY.md` and
approval before shipping.

## Upgrade 1 — A domain `PushNotice` with no free-text field (content-free by construction)
- Current: the outbound payload is assembled in `app/push.rs::build_push_payload`; the domain
  exposes no notice type, so nothing stops a future caller from attaching a message body.
- Proposal: add `PushNotice { kind: NotificationKind, space_id: SpaceId, bot_id: BotId,
  run_id: RunId }` (all metadata, no `String` text) plus a pure `title(locale)` that renders
  from `kind` only. `build_push_payload` consumes that type; browser and webhook paths share it.
- Why: turns "content-free" from a test assertion into a structural guarantee for both channels.
- Risks: `RunId` must be a real domain id (confirmed in `ids.rs`); moving title text into domain
  crosses into user-facing copy — keep it a key/locale token, not a translated literal, or the
  crate acquires an i18n dependency. Text must stay byte-identical or record a DEVIATION.

## Upgrade 2 — Paired wire contracts for both enums
- Current: `as_str()` exists; DB decoding re-derives the variant ad hoc in the infra stores.
- Proposal: `impl TryFrom<&str>` with `DomainError` for both enums, plus `const ALL` and round-trip
  property tests (`as_str ∘ parse = id`) with REQ-064/065 in the test name.
- Why: one home for the wire contract as Discord/Teams-style kinds are added.
- Risks: unknown values must be rejected, never defaulted (a silent default could mis-route a
  push); confirm `DomainError` variant set before adding one (variant set not read this session —
  GUESS).

## Upgrade 3 — Debug-safe secrets: stop leaking webhook/subscription keys
- Current: `NewNotificationDestination` derives `Debug` while holding plaintext `webhook: String`;
  `NotificationSubscription` holds `endpoint`/`p256dh`/`auth` as plain `String`. The existing test
  only asserts `NotificationDestination`'s Debug is clean — the `New...` shape is unguarded.
- Proposal: a small `OpaqueSecret` newtype (`Display`/`Debug` print `"[redacted]"`, `expose()`
  only for the sealing adapter), used for `webhook`, `p256dh`, `auth`. Keep the domain secret-free
  by storing it only in the transient `New...` types; the stored `NotificationDestination` stays
  label+kind only.
- Why: a stray `tracing::debug!("{:?}", new_dest)` cannot leak a credential; matches the crate's
  "secrets never logged" rule and the file's own no-`secrecy` constraint.
- Risks: touches every constructor in api `wire.rs`, app `usecases.rs` and testkit (CONFIRMED
  callers); adding a dependency is out — implement the newtype locally. Do as one atomic commit.

## Upgrade 4 — One channel model instead of two flag sets
- Current: `NotificationPolicy` carries `destination_slack_webhook`/`destination_teams_webhook`
  plus 4 bools (with a `struct_excessive_bools` allow), while `DestinationFlags` repeats the two
  destination bools with no conversion between them.
- Proposal: a `NotificationChannels` bitset (`waiting`, `completed`, `slack`, `teams`) with
  `enabled(kind)`/`destinations()`, stored by `NotificationPolicy` and returned by
  `From<&NotificationPolicy> for DestinationFlags`. Keep the public bool fields as compatibility
  accessors in the first release.
- Why: removes drift between policy and destination flags; validates the "channel × kind" matrix once.
- Risks: public field/API changes break literal constructors across api/app/infra/testkit — stage
  shims; the `allow(clippy::struct_excessive_bools)` may only be dropped once fields are gone.

## Upgrade 5 — Bounded `DailyCap` / `FailCount` and a domain deletion rule
- Current: `daily_cap: u32` (0 silently disables all pushes, huge values unvalidated) and
  `fail_count: u32`; the "three permanent failures → delete" threshold lives in a comment on
  `NotificationFailureOutcome::Deleted` and is re-implemented in `infra/stores/notifications.rs`.
- Proposal: `pub const MAX_PERMANENT_FAILURES: u32 = 3`; newtypes `DailyCap` (clamped/validated
  at construction, default 20) and `FailCount` with `should_delete()`. Callers in push/store use
  those instead of raw comparisons.
- Why: makes the cap/failure invariants explicit and testable at the domain boundary (property
  tests for the 0/1/3/4 boundary).
- Risks: if the store currently deletes on a different condition (GUESS: 3 consecutive permanent
  failures in a UTC day, per the doc comment), moving the rule must preserve behavior — confirm
  against `infra/tests/notifications.rs` first; changing cap 0 semantics is a DEVIATION.

## Suggested sequencing
1 → 2 (pure type/wire hardening, low risk), then 3 (secret safety), then 5 (invariants), then 4
(highest blast radius). Each step: tests with REQ-064/REQ-065 in the name, then `cargo fmt --check`,
clippy `-D warnings`, tests, one conventional commit.

## Open questions / decisions needed (GUESS where noted)
- Does the push title belong in the domain as a locale key or stay in app? (Decision needed — it
  affects whether `PushNotice` can be fully pure.)
- Is `DomainError` gaining a parse variant acceptable, or should parsing reuse an existing one?
- Are channel bitsets/cap newtypes in v0.4.0 scope, or deferred? Log as proposals in `docs/STATE.md`
  and get owner approval before treating them as contract.
