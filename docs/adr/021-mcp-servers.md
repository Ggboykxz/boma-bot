# ADR-021 — MCP servers: outbound guard first, per-space private allow-list, stdio refused by default (REQ-033, defect D-05)

Status: Accepted (2026-11-13, decision recorded by the rebuild)

## Context

`spec/01-requirements.md` REQ-033: a member can add an MCP server by URL (with OAuth where
needed) and, only when the deployment allows named commands, a local stdio server. Spec §4.5:
"**SSRF is denied by default.** Schemes, credentials, non-standard ports, private and metadata
addresses are rejected after DNS resolution, and redirects are re-checked." Defect D-05: "MCP can
reach private endpoints through a deployment-wide flag → **Scope the allowance per space, require
an explicit allow-list, emit an audit event each time it is used**." The acceptance examples:

- Given a remote MCP server with OAuth, when the member connects, then tools are discovered and usable.
- Given stdio disabled (default), when a local command server is added, then it is refused explaining why.

## Alternatives

1. **Deployment-wide flag (`MCP_ALLOW_PRIVATE=1`) to lift the SSRF guard for everyone.** Rejected:
   exactly the D-05 anti-pattern; one flipped env var opens every private range for every tenant.
2. **Per-space, per-server `allow_private`, audited per use (chosen).** The exception is scoped to
   one server in one space, off by default, and every call (probe and tool call) writes
   `audit_events` with kind `mcp.private_call`, target = server name, action in metadata. A
   deployment can grep its own rows.
3. **Metadata endpoints releasable by the allow-list.** Rejected without standing: 169.254.169.254
   and its aliases are never legitimate MCP targets; the guard refuses them even with
   `allow_private`. A member with a real need for a private endpoint still gets every other
   private range.
4. **stdio gated by `MCP_STDIO_ALLOWED_COMMANDS` from the start.** The acceptance criterion is
   the default-off case, which holds at both layers: registration refuses a stdio server whose
   program is not listed (`StdioDisabled`), and the spawn transport re-checks the allow-list
   before any process exists. Both are shipped and tested; the empty default is the shipping
   configuration.

## Decision

- `crates/infra/src/net_safety.rs` is the single outbound guard: HTTPS unless the caller holds
  the per-space allow-list (which also releases plain HTTP, needed by LAN services without TLS);
  port 443 otherwise; credentials refused; resolution is per-call; **any** resolved private
  address refuses; metadata endpoints never pass; redirects are the caller's to refuse and
  `HttpMcpTransport` sets `Policy::none()`.
- Migrations `0029_mcp_servers.sql`, `0031_mcp_credentials.sql`, `0032_mcp_server_tools.sql`
  (`mcp_servers`, `mcp_oauth_sessions` reserved, `mcp_server_tools`), all RLS; credentials sealed
  under the per-space DEK (ADR-020 machinery), never listed, `reveal_bearer` the only read path.
- Probed tools are persisted (`mcp_server_tools`) and the run catalogue reads them, never the
  network; every MCP tool is **Write-class**, so a call goes through the existing approval gate
  (REQ-017), exactly-once effects included. The tool key is `mcp.<server>.<tool>`, both parts
  sanitised (lowercase-kebab server, `[A-Za-z0-9_.-]` tool).
- `HttpMcpTransport` speaks JSON-RPC `initialize`/`tools/list`/`tools/call` over one HTTP POST per
  call, answer `application/json` or the first `data:` message of an `text/event-stream` SSE
  answer (`first_sse_message`), 30 s timeout, 1 MiB body cap, redirects refused.
- stdio servers are refused at registration with a static, honest message (`AppError::StdioDisabled`
  → HTTP 400) unless the deployment allow-lists the program (`MCP_STDIO_ALLOWED_COMMANDS`,
  comma-separated bare programs; empty by default). When listed, `McpTransportDispatch` routes
  them to `StdioMcpTransport`: one child per call, `env_clear` + `PATH`/`HOME` only,
  newline-delimited JSON-RPC over pipes (`initialize` → `notifications/initialized` → call),
  10 s per round trip, 1 MiB line cap; shell syntax is refused by shape before spawn.
- OAuth (PKCE, RFC 7636) is shipped: `mcp.oauth.start` returns the authorization URL (S256
  challenge, `state`, ten-minute session under the deployment's vault, redirect
  `http://127.0.0.1/boma-mcp/callback` — no local HTTP listener; the member pastes the callback
  URL's `code`+`state` into `mcp.oauth.complete`). The verifier is single-shot in
  `mcp_oauth_sessions` (sealed, deleted at exchange time regardless of outcome). The exchange
  is form-encoded at the token endpoint (`OAuthTokenClient`: HTTPS, no redirects, 30 s,
  64 KiB cap), the bearer is sealed under the space's DEK, the server is marked `connected`,
  and the tools are discovered — the spec's acceptance. The token URL answers to the same
  outbound guard and per-space allow-list exception as the MCP endpoint itself.

## Consequences

- A deployment that never set `allow_private` cannot be tricked into a private fetch: every SSRF
  path goes through one function with five refusal reasons, tested.
- An operator cannot flip one variable to open private networks for everyone; they must grant it
  per server and per space, and they can audit every use after the fact (`audit_events`).
- REQ-033 is **complete** for this milestone's scope: remote MCP over HTTP (bearer + OAuth) and
  stdio behind an explicit allow-list, both guarded, both sealed, both with the catalogue tools
  persisting between probes. A member signing into an OAuth-only server is the path that most
  deserves a live rehearsal before release (a real third-party AS); it is tracked in docs/STATE.md
  as a validation item, not a gap in the build.
