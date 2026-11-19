# Worker #297 — `crates/infra/src/mcp_stdio.rs` architecture proposal (v0.4.0)

Lens: architecture. Scope: structure, layering, lifecycle, protocol handling, error and
observability boundaries. Read-only review; no source was changed. All layout statements below are
**CONFIRMED** from a source read of `mcp_stdio.rs`, `mcp_http.rs`, `domain/src/mcp.rs`,
`infra/src/dispatch.rs`, `infra/tests/mcp_stdio.rs`, `spec/05-known-defects.md` and ADR-021. Impact
notes are **GUESS** until a build/test run verifies them.

## Current structure (as built)

The module is a single flat file (~212 lines) with four responsibilities interleaved:

1. **Config / allow-list gate** — `StdioMcpTransport { allowed: Vec<String> }`, checked twice
   (here at spawn and in `app/usecases.rs` at registration) via `domain::split_command` and
   `domain::program_allowed`. Defense in depth is sound; the policy decision lives in `domain`,
   which is the right layer. The security-critical parsing is pure and unit-tested.
2. **Process lifecycle** — `rpc()` splits the command, re-checks the allow-list, then spawns one
   short-lived child per RPC with `env_clear` + `PATH`/`HOME`, piped stdin/stdout, `Stdio::null()`
   stderr and `kill_on_drop(true)`, then `drop(stdin)` + `child.wait()`. One spawn per call is an
   intentional parity decision (ADR-021): no lingering process, crash dies with the request.
3. **Protocol engine** — three hardcoded writes (`initialize` id 1, `notifications/initialized`,
   the call id 2) and a `read_line` that skips blank/malformed/other-id lines and maps JSON-RPC
   `error` to `AppError::Rule(InvalidName)`. Framing, correlation and error mapping are embedded in
   `rpc`.
4. **Trait adapter** — `McpTransport::list_tools` / `call_tool`, reusing `mcp_http::decode_tools`.

Separation from the HTTP sibling is good (`McpTransportDispatch` selects by transport); the weak
boundaries are *within* this file: framing is tangled with process control, and the error model
collapses every failure into `Unavailable` or `Rule(InvalidName)`.

## Proposed upgrades for v0.4.0

**U1 — Split framing from process lifecycle.** Extract a private `StdioSession` that owns the child
plus its stdin/stdout, and a `JsonRpcClient` that owns handshake state and a monotonic request id.
`rpc()` becomes: open session → handshake → `client.request(method, params)` → drop session. Keep
spawn-per-call exactly as-is so no externally visible lifecycle changes.
*Risk:* mechanical but touches the whole file; request-id sequencing changes wire order. Must keep
the `initialize → initialized → call` sequence to stay byte-compatible with fixtures (GUESS).

**U2 — Fix the 1 MiB cap to be per-line, not per-stream.** `BufReader::new(stdout).take(MAX_LINE_BYTES)`
limits the *cumulative* bytes read across the whole session, not one JSON-RPC line as the doc comment
claims. Replace with a per-line bounded read (e.g. `read_until` on a capped buffer, rejecting a line
that exceeds 1 MiB).
*Risk:* a server emitting >1 MiB of small lines total would start working — a defect fix but a
potential behavior change; record in `docs/PARITY.md` if judged externally visible.

**U3 — One deadline for the whole RPC.** `REQUEST_TIMEOUT` currently restarts on every `read_line`
and `write_line`, so a noisy server can stretch one call to N × 10 s (each malformed line buys a fresh
10 s). Wrap the entire `rpc()` in one `tokio::time::timeout` and pass a shared deadline into reads;
align the budget with the HTTP transport's 30 s, not a per-read 10 s.
*Risk:* too-short a shared budget could reject slow legitimate servers; pick the budget deliberately
and test both handshake and call phases.

**U4 — Typed internal errors + bounded diagnostics.** Replace the flat `AppError` mapping with a
private `enum StdioError { Spawn, Timeout, Protocol, Refused(String), ChildExited }`, mapped to
`AppError` at the `McpTransport` boundary (stable API preserved). Pipe stderr into a capped buffer
(e.g. last 4 KiB) on a concurrent task and include it in `tracing` with the server name and request
id — never in the client-facing error.
*Risk:* stderr must be drained concurrently or a chatty child blocks on a full pipe; more surface
to test. No secrets may enter logs (AGENTS.md observability/security rules).

**U5 — Validate the handshake result.** Today the `initialize` response is read then discarded
(`let _ = initialized`). A typed `InitializeResult` should check `protocolVersion` (and record
`serverInfo`/capabilities) before the call. Refuse a materially incompatible version, or — to avoid
a parity break — log a warning and continue for minor drift.
*Risk:* strict refusal changes behavior for servers returning a different/omitted version; default
to warn-and-continue unless the spec tightens, keeping parity and noting the choice in ADR-021.

## Cross-cutting notes (not separate upgrades)

- Keep the two-layer allow-list check; it is the load-bearing control for D-05/REQ-033. Any refactor
  must preserve "refused before any process exists" (the `StdioDisabled` integration test).
- Consider `current_dir` set to a dedicated temp dir as part of U1's sandbox hardening; **GUESS** —
  could break servers relying on cwd, so gate behind an ADR rather than bundling silently.
- Every change above needs its REQ-033 test name kept and new tests added (per-line cap, shared
  deadline, malformed-line spam, incompatible version) before implementation per AGENTS.md.
