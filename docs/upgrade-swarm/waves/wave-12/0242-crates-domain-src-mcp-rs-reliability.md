# Upgrade proposal — `crates/domain/src/mcp.rs` (lens: reliability)

- Worker: 242
- Target version: v0.4.0
- Lens: reliability — protocol drift, connection state, tool schema drift
- Evidence basis: `mcp.rs` (313 lines), `error.rs`, `spec/01-requirements.md` (REQ-033),
  `spec/02-data-model.md`, `spec/04-nonfunctional.md`, `spec/05-known-defects.md` (D-05), plus call
  sites in `app/src/usecases.rs` and `infra/src/mcp_http.rs`, read this session. Code/spec facts are
  **CONFIRMED**; runtime/provider consequences are marked **GUESS**.

## Scope

Pure domain module (`no IO, no async`): `McpTransport`, server-name validation, tool-name
sanitisation, registration-time URL shape check, stdio command splitting and allow-list matching.
All failures today are `DomainError::InvalidName(String)` (CONFIRMED in `error.rs`). Reliability
question: can a stale or ambiguous server/tool fact survive silently because the domain cannot
represent the fact that would expose it?

## Failure modes

**FM-1 — Protocol version is not a domain fact (CONFIRMED structure; GUESS frequency).**
`McpTransport` encodes only `Http`/`Stdio`; neither `mcp.rs` nor the `McpServer` data-model row
records an MCP protocol revision. Infra pins `PROTOCOL_VERSION` in the `initialize` request and
discards the server's answered version; nothing is persisted per server. A server that moves from
one revision to another (e.g. 2024-11-05 → 2025-03-26/06-18) fails `initialize` on reconnect with
an opaque error, and Boma Bot cannot record which revision produced a cached tool list, negotiate a
fallback, or tell operators which servers are behind. `FromStr` also accepts `"http"` although
registration requires HTTPS — two vocabularies disagree.

**FM-2 — Capabilities are discarded (CONFIRMED).** `initialize` sends `"capabilities": {}` and only
the `tools` array of `tools/list` is decoded (`mcp_http.rs`). A server without tool support, or one
whose capabilities changed, still installs and only fails when a bot calls a tool.

**FM-3 — No connection-state type in the domain (CONFIRMED shape).** Unlike `Connection`
(pending/connected/revoked/error in the data model), `McpServer` has only `auth state`: no
`status`, `last_seen`, `tools_refreshed_at` or transition rules. "Configured but never probed",
"OAuth expired", "stdio process dead" and "tool list stale" are indistinguishable to callers.
Registration validates once (`add_mcp_server`); reconnect does not re-check the stored URL, so a
URL that was safe at registration is trusted forever (GUESS: infra compensates per call).

**FM-4 — stdio-disabled is not representable here (CONFIRMED).** `McpTransport::Stdio` always
parses and `split_command` accepts any safe-shape command; fail-closed lives in `app`/config
(`stdio_allowed`, `AppError::StdioDisabled`). A stdio server record can therefore exist in memory
without the deployment allowing it — the invalid state is representable until a later layer catches it.

**FM-5 — Tool identity is name-only; schema drift is invisible (CONFIRMED in infra).** The domain
contract for a tool is just `sanitize_mcp_tool_name`. `decode_tools` clones `inputSchema` verbatim
without validating it or storing a hash/version; `call_tool` re-sanitises the name but never checks
the arguments against the current schema. A server that renames parameters or changes types between
refreshes gets `tools/call` built for the old schema — wrong call or provider error. Discovered names
are also not deduplicated, so a server returning the same sanitised name twice yields ambiguous dispatch.

**FM-6 — Unsafe tool names vanish silently (CONFIRMED).** `decode_tools` skips them with only
`tracing::warn!`; `app` also filters by `sanitize_mcp_tool_name`. No count, audit event or in-band
signal reaches the operator, who later sees "tool not found". `sanitize_mcp_tool_name` additionally
permits leading/trailing `-`/`_` and `--` that server-name validation refuses — grammar drift.

**FM-7 — URL validator is shape-only but spec requires runtime SSRF denial (CONFIRMED both).**
`validate_mcp_server_url` does no DNS resolution, redirect re-check, port rule or loopback check;
its metadata list is four exact strings, so `https://127.0.0.1/`, `https://metadata.goog/` and
numeric/redirect variants pass (D-05; `spec/04-nonfunctional.md` requires rejection after DNS and on
redirects). It is reused for the OAuth `token_url`, which the deployment itself POSTs to. Without a
documented non-authoritative contract and an infra re-check on every connect/redirect, this gate is
mistaken for the SSRF boundary.

**FM-8 — Command shape doc/impl mismatch and allow-list confusion (CONFIRMED code).** `split_command`'s
doc says the program must be an absolute path or bare name, but the code only checks the character
set: `../bin/x` passes. `program_allowed` matches basename OR full path, so any path ending in an
allow-listed name is accepted (GUESS: PATH/relative-path confusion). No cap on token count or byte
length, so an over-long command registers successfully then fails at spawn (`E2BIG`).

**FM-9 — Coarse errors, thin tests (CONFIRMED).** Every failure is `InvalidName(String)`, so callers
cannot programmatically choose fail-fast vs retry. Only hand-picked unit tests exist; the four
external-format parsers (name, tool name, URL, command) have no property tests, contrary to the repo
rule for parsers of external formats.

## Concrete hardening for v0.4.0

1. **H-1 (FM-9): typed MCP errors + property tests first.** Add `DomainError::{InvalidTransport,
   InvalidMcpServerName, InvalidMcpToolName, InvalidMcpUrl, UnsafeCommand}` (keep `InvalidName` for
   generic cases) and `proptest` invariants for all four parsers: no panic, ASCII-only outputs,
   idempotent sanitisation, no `..` path segments. Additive enum change; check exhaustive matches.
2. **H-2 (FM-8): enforce the documented command rule.** Reject relative parent segments and cap
   token count/bytes in `split_command`; make `program_allowed` compare canonical full paths, not
   basenames, for relative inputs. Risk: previously accepted commands may be refused — review
   `MCP_STDIO_ALLOWED_COMMANDS` docs and file a DEVIATION if the config shape changes.
3. **H-3 (FM-6): fail-loud tool loss.** Return a `skipped_tools` count from discovery instead of a
   bare `warn!`, surface it in the probe summary and an audit event, and refuse duplicate sanitised
   names. Keep install succeeding (REQ-033 promises discovery, not all-or-nothing). Risk: probe
   DTO change — check `spec/openapi.yaml` / `docs/PARITY.md`.
4. **H-4 (FM-1, FM-2): model the handshake in the domain.** Add a `protocol_version` newtype and
   capability flags to the `McpServer` value (or a `McpHandshake`), move `PROTOCOL_VERSION` from
   infra into the domain, and persist the negotiated revision with each tool list so drift is
   attributable. Risk: touches the stored record and adapters — add optional fields with defaults
   first; DEVIATION review before any wire/storage change.
5. **H-5 (FM-3, FM-4): pure connection-state type.** Add `McpConnectionState` (Configured, Probing,
   Connected{version, tools_at}, AuthExpired, Error, Disabled) with `transition()` returning
   `Err(InvalidTransition)` for illegal leaps, and encode "stdio allowed" as a precondition so a
   Stdio server cannot be constructed when the allow-list is empty. Keep process/HTTP lifecycle in
   infra (domain stays IO-free). Risk: needs app-store plumbing; land the type + tests first.
6. **H-6 (FM-5): tool catalogue identity.** Add `McpToolDescriptor { name, schema_hash, description }`
   and `register_tools(current, discovered) -> ToolDelta` that dedups, refuses collisions and marks
   added/removed/schema-changed so cached arguments can be invalidated and the change surfaced.
   Compute the hash in infra (domain remains pure). Risk: new public API — DEVIATION check; no wire
   change expected since infra already holds the schema JSON.
7. **H-7 (FM-7): URL contract + revalidation.** Document `validate_mcp_server_url` as a shape-only
   gate, add loopback/link-local and non-standard-port refusal (or record an ADR that infra re-checks
   after DNS and on every redirect), and re-run it on reconnect and token refresh. Coordinate with
   D-05's per-space allow-list and audit event. Risk: tightening rejects previously accepted URLs.

## Risks & ordering

- H-1 and H-2 are behaviour-preserving for valid inputs — do first.
- H-3 changes an externally visible probe summary; H-4/H-6 touch public types and stored records;
  H-5 needs adapter work; H-7 is externally visible. Each in its own commit with
  `cargo fmt --check`, `clippy -D warnings`, tests, and a `docs/PARITY.md` entry if the wire/disk
  shape moves. H-4/H-6 should follow H-5 so state and schema deltas share one model.

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

Line count: ~112 (<120). Status: proposal only, no code changed (READ-ONLY worker).
