# ADR-011 — Channels: e-mail ingestion, messaging webhooks and bot webhooks as signed adapters

Status: proposed (awaiting owner go)

## Context

Bots receive work over e-mail (REQ-047), messaging platforms (REQ-048), per-bot webhooks (REQ-050) and
repository events. Each channel has its own authentication scheme and its own body cap
(`04-nonfunctional.md`). `05-known-defects.md` requires uniform failures so ids cannot be enumerated.

## Alternatives

1. **One `Channel` port with per-provider adapters; a shared ingress layer handles signature
   verification, body caps, replay protection and uniform errors before any domain code runs.**
2. One bespoke route per provider with its own validation, as the reference does.
3. A generic "webhook" route with a provider field.

## Scoring

| Criterion | 1 port + ingress | 2 bespoke | 3 generic |
|---|---|---|---|
| 1 fits the spec | 1 | 2 | 2 |
| 2 maturity | 1 | 1 | 1 |
| 3 security | 1 (one place to get comparison, caps and replay right) | 3 (N implementations of the same rules) | 4 (provider field becomes an authorization decision) |
| 4 performance | 1 | 1 | 1 |
| 5 simplicity | 2 | 2 | 1 |
| 6 typed contract | 1 | 1 | 2 |

## Decision

Option 1. The ingress layer owns: body cap per channel, signature/HMAC verification in constant time,
timestamp window and replay cache, uniform `401` for unknown target or bad signature, and a
`SystemReason::ChannelIngress` tenant bypass that only the ingress may use. Adapters translate a
verified payload into a `ChannelMessage` and never touch repositories.

## Consequences

- Adding Slack-class support is an adapter plus a signature verifier, and the replay/caps tests are
  reused.
- The webhook route for a bot resolves the bot by id inside the system context but answers identically
  for "unknown bot" and "bad secret" — asserted by a golden (`bot-webhook-denied.json`).
- E-mail ingestion is bounded at 25 MiB and parses in a blocking task with a timeout; a malformed
  message is accepted-and-ignored rather than retried forever.
