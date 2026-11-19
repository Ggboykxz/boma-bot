# Upgrade proposal — `crates/domain/src/mcp.rs` (lens: reliability)

- Worker: 242
- Target version: v0.4.0
- Lens: reliability — protocol drift, connection state, tool schema drift
- Evidence basis: `mcp.rs` (313 lines), `error.rs`, `spec/01-requirements.md` (REQ-033),
  `spec/02-data-model.md`, `spec/04-nonfunctional.md`, `spec/05-known-defects.md` (D-05), and call
  sites in `app/src/usecases.rs` / `infra/src/mcp_http.rs`, read this session. Code/spec facts are
  **CONFIRMED**; runtime/provider consequences are **GUESS**.

## Scope

Pure domain module (no IO/async): `McpTransport`, server-name validation, tool-name sanitisation,
registration-time URL shape check, stdio command splitting and allow-list matching. All failures are
`DomainError::InvalidName(String)` (CONFIRMED in `error.rs`). Reliability question: can a stale or
ambiguous server/tool fact survive silently because the domain cannot represent the fact exposing it?

## Failure modes

**FM-1 — Protocol version is not a domain fact (CONFIRMED structure; GUESS frequency).** `McpTransport`
encodes only `Http`/`Stdio`; the `McpServer` data-model row has no protocol revision. Infra pins
`PROTOCOL_VERSION` in `initialize` and discards the server's answer; nothing is persisted per server.
A server changing revision (e.g. 2024-11-05 → 2025-03-26/06-18) fails `initialize` on reconnect with
an opaque error, and Boma Bot cannot tell which revision produced a cached tool list or negotiate a
fallback. `FromStr` also accepts `"http"` although registration requires HTTPS — vocabularies disagree.

**FM-2 — Capabilities are discarded (CONFIRMED).** `initialize` sends `"capabilities": {}` and only the
`tools` array of `tools/list` is decoded (`mcp_http.rs`). A server without tool support, or whose
capabilities changed, still installs and fails only when a bot calls a tool.

**FM-3 — No connection-state type (CONFIRMED shape).** Unlike `Connection` (pending/connected/revoked/
error in the data model), `McpServer` has only `auth state`: no `status`, `last_seen`,
`tools_refreshed_at` or transition rules. "Never probed", "OAuth expired", "stdio process dead" and
"stale tool list" are indistinguishable to callers. Validation runs once at registration
(`add_mcp_server`); reconnect does not re-check the stored URL (GUESS: infra compensates per call).

**FM-4 — stdio-disabled is not representable (CONFIRMED).** `McpTransport::Stdio` always parses and
`split_command` accepts any safe-shape command; fail-closed lives in `app`/config (`stdio_allowed`,
`AppError::StdioDisabled`). A stdio server can therefore exist in memory without deployment approval.

**FM-5 — Tool identity is name-only; schema drift is invisible (CONFIRMED in infra).** The domain
contract is `sanitize_mcp_tool_name` alone. `decode_tools` clones `inputSchema` verbatim without
validating or hashing it; `call_tool` re-sanitises the name but never checks arguments against the
current schema. Renamed parameters or changed types yield a call built for the old schema. Discovered
names are also not deduplicated, so duplicate sanitised names give ambiguous dispatch.

**FM-6 — Unsafe tool names vanish silently (CONFIRMED).** `decode_tools` skips them with only
`tracing::warn!`; `app` also filters by sanitise. No count, audit event or in-band signal reaches the
operator. `sanitize_mcp_tool_name` also allows leading/trailing `-`/`_` and `--` that server-name
validation refuses — grammar drift.

**FM-7 — URL validator is shape-only; spec requires runtime SSRF denial (CONFIRMED both).**
`validate_mcp_server_url` does no DNS resolution, redirect re-check, port rule or loopback check;
its metadata list is four exact strings, so `https://127.0.0.1/`, `https://metadata.goog/` and
numeric/redirect variants pass (D-05; `spec/04-nonfunctional.md` requires post-DNS and per-redirect
rejection). It is reused for the OAuth `token_url`, which the deployment itself POSTs to.

**FM-8 — Command doc/impl mismatch and allow-list confusion (CONFIRMED code).** `split_command`'s doc
requires an absolute path or bare name, but the code only checks characters: `../bin/x` passes.
`program_allowed` matches basename OR full path, so any path ending in an allow-listed name is
accepted (GUESS: PATH/relative confusion). No token-count/byte cap: a command registers then fails
at spawn (`E2BIG`).

**FM-9 — Coarse errors, thin tests (CONFIRMED).** Every failure is `InvalidName(String)`, so callers
cannot choose fail-fast vs retry. The four external-format parsers (name, tool name, URL, command)
have no property tests, contrary to the repo rule for parsers of external formats.

## Concrete hardening for v0.4.0

1. **H-1 (FM-9): typed errors + proptest first.** Add `DomainError::{InvalidTransport,
   InvalidMcpServerName, InvalidMcpToolName, InvalidMcpUrl, UnsafeCommand}` (keep `InvalidName` for
   generic cases); add `proptest` invariants: no panic, ASCII-only outputs, idempotent sanitisation,
   no `..` segments. Additive enum; check exhaustive matches.
2. **H-2 (FM-8): enforce the documented command rule.** Reject relative parent segments, cap token
   count/bytes, and compare canonical paths instead of basenames for relative inputs. Risk:
   previously accepted commands may be refused — review allow-list docs, file a DEVIATION if config
   shape changes.
3. **H-3 (FM-6): fail-loud tool loss.** Return a `skipped_tools` count from discovery instead of a
   bare `warn!`, surface it in the probe summary and an audit event, and refuse duplicate sanitised
   names. Keep install succeeding (REQ-033 promises discovery, not all-or-nothing). Risk: probe DTO
   change — check `spec/openapi.yaml` / `docs/PARITY.md`.
4. **H-4 (FM-1, FM-2): model the handshake.** Add a `protocol_version` newtype and capability flags
   to `McpServer` (or a `McpHandshake`), move `PROTOCOL_VERSION` from infra to the domain, and persist
   the negotiated revision with each tool list. Risk: touches stored record and adapters — add
   optional fields with defaults first; DEVIATION review before any wire/storage change.
5. **H-5 (FM-3, FM-4): pure connection-state type.** Add `McpConnectionState` (Configured, Probing,
   Connected{version, tools_at}, AuthExpired, Error, Disabled) with `transition()` returning
   `Err(InvalidTransition)` on illegal leaps, and encode "stdio allowed" as a construction
   precondition. Keep process/HTTP lifecycle in infra. Risk: needs app-store plumbing; land the
   type + tests first.
6. **H-6 (FM-5): tool catalogue identity.** Add `McpToolDescriptor { name, schema_hash, description }`
   and `register_tools(current, discovered) -> ToolDelta` that dedups, refuses collisions and marks
   added/removed/schema-changed so cached arguments can be invalidated. Compute the hash in infra.
   Risk: new public API — DEVIATION check; no wire change expected.
7. **H-7 (FM-7): URL contract + revalidation.** Document the validator as shape-only, add loopback/
   link-local and non-standard-port refusal (or an ADR that infra re-checks post-DNS and on every
   redirect), and re-run it on reconnect and token refresh. Coordinate with D-05's per-space
   allow-list and audit event. Risk: tightening rejects previously accepted URLs.

## Risks & ordering

H-1/H-2 are behaviour-preserving for valid inputs — do first. H-3 changes the probe summary;
H-4/H-6 touch public types and stored records; H-5 needs adapter work; H-7 is externally visible.
Each in its own commit with `cargo fmt --check`, `clippy -D warnings`, tests, and a `docs/PARITY.md`
entry if wire/disk shape moves. Land H-5 before H-4/H-6 so state and schema deltas share one model.

## Tests to add (REQ id in names)

- `req_033_transport_version_is_recorded_and_negotiated`
- `req_033_connection_state_rejects_illegal_transition`
- `req_033_stdio_server_cannot_be_configured_when_disabled`
- `req_033_tool_schema_change_is_reported_as_delta`
- `req_033_duplicate_tool_names_are_refused`
- `req_033_unsafe_tool_names_are_counted_not_silent`
- `req_033_url_revalidation_on_reconnect_refuses_loopback`
- `req_033_command_rejects_relative_parent_segments`
- `req_033_mcp_parsers_are_panic_free` (proptest)

Line count: 119 (<120). Status: proposal only, no code changed (READ-ONLY worker).
