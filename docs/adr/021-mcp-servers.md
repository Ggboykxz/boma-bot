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
4. **stdio enabled behind `MCP_STDIO_ALLOWED_COMMANDS` now.** Deferred, not rejected; the
   acceptance criterion is the default-off case, and the spawn path is not exercised by any
   deployment today. The refusal names the reason. The allow-list wire-up is a config change in
   the binary plus a spawned JS-free transport (tokio process pipes, newline-delimited JSON-RPC),
   and it lands with the first allow-listing deployment, not before.

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
  call, plain `application/json` (SSE framing not parsed yet), 30 s timeout, 1 MiB body cap.
- stdio servers are refused at registration with a static, honest message (`AppError::StdioDisabled`
  → HTTP 400 with the explanation). MCP OAuth (`start`/`complete`) is **not** implemented yet;
  bearer-token auth is. Both are tracked in docs/STATE.md as the remaining gap to REQ-033's
  "with OAuth where needed" criterion; the reference flow and endpoints need a live auth server
  to validate against, which this build does not have.

## Consequences

- A deployment that never set `allow_private` cannot be tricked into a private fetch: every SSRF
  path goes through one function with five refusal reasons, tested.
- An operator cannot flip one variable to open private networks for everyone; they must grant it
  per server and per space, and they can audit every use after the fact (`audit_events`).
- REQ-033 is honestly **partial**: remote MCP with bearer + per-space allow-list + stdio
  default-refusal are done; OAuth and the stdio spawn are not. The gap is named, not faked.
