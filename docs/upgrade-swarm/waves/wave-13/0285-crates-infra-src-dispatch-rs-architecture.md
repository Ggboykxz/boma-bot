# Worker 0285 — `crates/infra/src/dispatch.rs` architecture proposal (v0.4.0)

- Worker: 285 · Lens: architecture · Module: `crates/infra/src/dispatch.rs` · Target: v0.4.0.
- Scope: read-only review of the 63-line module plus targeted reads of
  `ports.rs:2356-2383,2540-2571`, `mcp_http.rs`, `mcp_stdio.rs`, `events.rs`,
  `lib.rs:24,47`, `bin/commands.rs:141-144`, spec REQ-033/050,
  `spec/04-nonfunctional.md`, `docs/adr/011-channels.md` and sibling wave briefs.
- No product code changed; no tests run (read-only assignment). "Current" claims are
  **CONFIRMED by source inspection**; forward-looking items are GUESS.

## Current dispatch structure (CONFIRMED)

- One struct `McpTransportDispatch { http: HttpMcpTransport, stdio: StdioMcpTransport }`
  implements the `McpTransport` port (`list_tools`/`call_tool`). It is the single outbound MCP
  seam: `ports.rs:2543` lets a deployment swap transports without touching a business rule, and
  `bin/commands.rs:141` builds it once, wrapped in `Arc`.
- Routing is a 3-line `match server.transport.as_str()` duplicated in both methods: `"stdio"` →
  child process, `_` → HTTP. No enum, no validation, no log, no metric, no timeout and no input
  cap at this layer.
- The `_` arm is fail-open: any unknown/typo'd/empty value (e.g. `"SSH"`, `""`) silently becomes
  an HTTP call. The HTTP arm performs network egress behind `check_outbound_url`, so a malformed
  value can turn an intended non-HTTP server into a network call instead of a refusal.
- Both arms are built eagerly: `new()` always constructs a `reqwest::Client` (failure →
  `AppError::Unavailable`) and always carries the stdio allow-list, even for a single-arm deploy.
- The module doc claims "adding a third transport means extending this match, nothing else."
  Optimistic: a third arm also needs a struct field, a constructor change, a new adapter module,
  a new `McpServerRecord.transport` value and bin wiring (CONFIRMED by the arms' full footprint).
- Security controls live in the arms, not the router: SSRF/redirect-refusal/1 MiB caps in
  `mcp_http.rs:52-128`; bare-program allow-list checked at add time in usecases and again at
  spawn in `mcp_stdio.rs:60-63`. The router only delegates.
- No tests live in this file; routing is exercised only indirectly via testkit's
  `ScriptedMcpTransport` and integration tests. The unknown-transport branch is untested.

## Webhook / inbound structure (CONFIRMED)

- Inbound webhooks are a different adapter family and never route through this dispatcher: bot
  webhook (`webhook_wake`, REQ-050, uniform 401 for unknown bot and bad bearer, golden
  `bot-webhook-denied.json`), messaging webhooks (REQ-048), billing webhook (HMAC, REQ-051).
  Payload caps differ per endpoint (64/256/512 KiB, `spec/04-nonfunctional.md`).
- Unlike the MCP side, there is no single port/router for inbound webhooks: verification, parsing
  and caps are split across usecases, `infra/messaging_http.rs` and API routes. The good property
  here — one dispatcher behind one port — has not been mirrored there.
- "Dispatch" is overloaded (RPC `dispatch`, `dispatch_tool`, `dispatch_routines`, worker
  `dispatch_tool_calls`, this MCP router). `McpTransportDispatch` is easy to confuse with them.

## Target structure for v0.4.0 (GUESS)

```
infra/dispatch.rs        RoutingMcpTransport: typed transport key + exhaustive match
infra/mcp_http.rs        HTTP arm: SSRF, redirect refusal, caps, bearer (unchanged)
infra/mcp_stdio.rs       stdio arm: allow-list, spawn-per-call (unchanged)
app/ports.rs             McpTransport port; transport kind newtype at the store boundary
infra/*webhook*          inbound webhooks: one shared verify/cap seam (owner-gated proposal)
```

## Concrete upgrades (with risks)

### U1 — Typed transport key, fail-closed routing
Parse `server.transport` into `enum McpTransportKind { Http, Stdio }` (newtype at the store
boundary) and match exhaustively; unknown values return `AppError::Rule`, never HTTP. Add a direct
unit test per branch, including unknown.
*Risk:* invalid values stored today that "worked" as HTTP would start failing — behaviour change;
check DB data and goldens, record a DEVIATION if any client depends on it. Keep one parser shared
with the MCP store.

### U2 — Unambiguous name and a documented boundary
Rename to `McpTransportRouter` (keep a deprecated alias for the `lib.rs:47` re-export if needed)
and expand the module doc to state what it is NOT (RPC/tool/routine dispatch, webhook wake).
Rustdoc the port as the single outbound MCP seam.
*Risk:* touches the public re-export and `bin/commands.rs`; purely internal, but must land as one
coordinated commit so build and docs stay consistent.

### U3 — Granular, injectable construction
Split `new()` so arms build independently, or inject both transports; a stdio-only deployment
should not fail at boot because TLS init failed. Keep fail-fast for HTTP deployments (validate at
startup via typed config).
*Risk:* lazy HTTP moves the TLS failure from boot to first call (loses fail-fast) and adds a
first-request error path — decide with an ADR; changing `new()`'s signature touches bin wiring and
test doubles.

### U4 — Centralise cross-cutting concerns at the seam
Add one router `tracing::span` (transport chosen, server id, latency; never the bearer), one
authoritative timeout, and a bounded `input` size (mirror the arms' 1 MiB). Keep SSRF and allow-list
checks in the arms where they are already correct.
*Risk:* two timeouts race — pick one authority or the arms' 30s/10s become meaningless; capping
input beyond what arms accept rejects clients that previously reached a server (DEVIATION, verify
goldens); ensure no `SecretString` or payload reaches logs.

### U5 — Mirror the seam for inbound webhooks (proposal only)
Introduce a shared inbound-webhook port/router for uniform HMAC/constant-time verification, uniform
401/404 responses and per-endpoint payload caps, with the same REQ-tagged tests. Keep it separate
from `dispatch.rs` — outbound MCP and inbound webhooks are different seams.
*Risk:* cross-crate refactor over REQ-048/050/051 and ADRs 011/012/023; the uniform-denial golden is
parity-visible and must stay green; a new public interface needs owner sign-off. Coordinate with the
webhook waves rather than duplicating them.

## Sequencing / cross-cutting risks

1. U1 + U2 first (types and naming, parity-locked), with U1's direct routing tests.
2. U3 then U4 (construction and cross-cutting), one commit each; preserve fail-fast and arm guards.
3. U5 is a separate, owner-gated workstream — propose, do not implement here.
4. After each step: `cargo fmt --check`, `clippy -D warnings`, REQ-033 (and 048/050/051 for U5)
   tests; REQ ids in test names (`tools/trace_check.py` counts declarations, never comments).
5. No UI/i18n impact. Every "current" claim is CONFIRMED by named source only; re-run the suite in
   the implementing session before flipping any status to verified-by-test.
