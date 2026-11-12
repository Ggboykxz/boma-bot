# ADR-005 — Realtime: server-sent events for conversations, WebSocket for the screen stream

Status: proposed (awaiting owner go) — this is DEC-07

## Context

Threads must stream tokens, tool cards, run status and typing without the client polling
(REQ-020), survive a reconnect without gaps (REQ-005), and pass through self-hosted reverse proxies
that are often configured conservatively. The computer screen needs a bidirectional low-latency
channel (REQ-028, REQ-030). The reference fans events out through Postgres `LISTEN/NOTIFY` and serves
conversations over SSE.

## Alternatives

1. **SSE for conversations (down only, HTTP POST for writes) + WebSocket only for screen/keyboard
   input.**
2. WebSocket for everything, with a fan-out layer and sticky sessions.
3. Long polling for conversations, WebSocket for the screen.
4. SSE for everything including the screen stream, with input over HTTP.

## Scoring

| Criterion | 1 SSE+WS screen | 2 WS all | 3 poll+WS | 4 SSE all |
|---|---|---|---|---|
| 1 fits the spec | 1 | 1 | 3 (latency, load) | 3 (input over HTTP adds a round trip per keystroke) |
| 2 maturity | 1 | 1 | 1 | 1 |
| 3 security | 2 (same-origin, cookie auth, no subprotocol negotiation) | 2 | 1 | 2 |
| 4 performance on poor networks | 1 (plain HTTP, proxies, auto-reconnect) | 2 | 4 | 2 |
| 5 simplicity | 1 | 3 (fan-out, sticky) | 2 | 2 |
| 6 typed contract | 1 | 1 | 1 | 1 |

## Decision

Option 1.

- `threads.subscribe` responds `text/event-stream`; every frame carries `threadId`, `seq`, `type`,
  `payload`, `createdAt`. The client stores the last `seq` and reconnects with `Last-Event-ID`; the
  server replays from the event log, then continues live. Heartbeats every 15 s keep proxies from
  closing idle streams.
- Writes stay ordinary procedure calls; the stream is a projection, never the source of truth.
- The screen stream is a WebSocket that carries frames from the computer provider one way and
  input events the other, authenticated by a short-lived sealed capability (not the session cookie),
  so a leaked stream URL cannot act as the user.
- Fan-out is Postgres `LISTEN/NOTIFY` with an in-process fallback for single-instance deployments;
  the fallback must be visible in `/readyz` so an operator knows multi-instance is unsafe without the
  database listener.

## Consequences

- SSE through a proxy needs `proxy_buffering off` and a long read timeout; both are documented in the
  deployment guide, and a health check verifies a stream opens.
- No sticky sessions required for conversations, which keeps horizontal scaling boring.
- The screen path needs its own authorization story (capability + generation), specified in REQ-030
  and tested for stale generations.
