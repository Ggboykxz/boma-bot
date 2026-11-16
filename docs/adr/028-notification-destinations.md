# ADR-028 — Notification destinations: sealed webhooks, per-member opt-in, Owner/Admin control

Status: accepted (2026-11-16, milestone M15.2 — REQ-065).

## Context

M15.1 delivered a content-free Web Push notification; teams working in Slack/Teams could still
miss the "bot is waiting on you" cue they would have welcomed in the channel they live in.
M15.2 adds workspace-wide Slack/Teams destinations alongside the per-member browser push.

## Decision

1. **Space-owned, not bot-owned.** Destinations persist per space; the same channel receives
   waiting/completed pushes for every bot in the space when the initiating member opts in per
   (space, bot) — the policy is the same seam as M15.1's waiting/completed pair plus two new
   boolean destination flags (`destination_slack_webhook`, `destination_teams_webhook`).
2. **Opt-in per (member, bot).** No member is ever pushed to a shared channel without the
   member setting the flag for that bot in the policy form. A destination sends exactly one
   message per notification: the title only — no names, no thread URL, no text of the
   conversation, no tool names or arguments.
3. **Owner/Admin manage destinations.** Registration, rotation and removal of an incoming
   webhook are Owner/Admin-only operations — the blast radius of "push this to #incidents" is
   the whole space, and anything narrower let any member redirect everyone's attention or leak
   our notifications to their own channel.
4. **Sealed at rest, like bot secrets.** The webhook URL is sealed under the space's DEK in the
   same machinery (`space_keys` + AEAD-256-GCM) that holds bot secrets; RPC replies never carry
   the webhook. None of the read paths (RPC, Debug, logs, exports) ever surface it; the test
   suite asserts this on every list call.
5. **Best-effort end-to-end.** Destination delivery never fails or delays a run; a refusing or
   timing-out channel is logged and dropped silently. Destinations draw from the same
   per-(member, bot) UTC-day cap as browser pushes — five slots remain five slots no matter how
   many destinations are registered.
6. **Slack needs the Slack host.** A Slack-shaped kind refuses anything not under the
   hooks.slack.com prefix, because nothing else claims to be Slack; teams are accepted as-is over
   HTTPS. Both kinds refuse anything that is not HTTPS and any URL containing credentials.

## Consequences

- Migrations `0057_notification_destinations.sql`, `0058_notification_destination_flags.sql`.
- The policy row, wire view, panel, and RPC carry two new booleans; nothing user-visible changes
  for members who never touch the form.
- The channel receives text that would otherwise reach only the initiator; any member who
  reads #incidents sees "A bot needs your answer" — by design the same content the browser push
  carries, nothing more.
- Revocation is one RPC (`destinationsDelete`); the seal is removed with the row; the webhook
  itself stays valid until the operator revokes it on Slack/Teams — that is on the operator to
  finish. The cap keeps an unattended destination from spamming forever: it dies to the same
  cap as everything else.
