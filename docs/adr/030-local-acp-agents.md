# ADR-030 — Local CLI agents as ACP subprocesses (M23, REQ-061)

Status: accepted (2026-11-16, milestone M23 — REQ-061).

## Context

REQ-061 commits this deployment to supporting local command-line agent runtimes (installed on the
machine that runs the server) attached as bots: each gets an isolated per-run workspace, the run
lifecycle (REQ-012), and the approval gateway (REQ-017). This is nightly-labs/openbot's core shape
(12+ ACP-capable CLIs over Agent Client Protocol) reimplemented under our charter: no code or
assets of theirs, no `danger-full-access` posture, everything behind our spec and our gates.

The upstream reference for the protocol is the **Agent Client Protocol (ACP)** — JSON-RPC 2.0 over
stdio, method names like `initialize`, `session/new`, `session/prompt`, `session/cancel`, with
session-update notifications streamed back to the client. Local agents run as subprocesses;
remote agents over HTTP/WS are out of scope for M23.

## Alternatives considered

1. **AG-UI external agents only (what we shipped).** External agents over HTTP/WebSocket with our
   M12 runner. Rejected for M23: ACP is the interop standard every major coding CLI speaks; making
   each one fit AG-UI means per-provider adapters and drifts from the ecosystem.
2. **A provider-by-provider native integration (Codex app-server, Claude Code's own protocol, …).**
   Rejected: that is N protocol implementations and N maintenance burdens — exactly the burden ACP
   removes. Native app-server support may come later per provider if ACP proves insufficient.
3. **Spawn the CLIs over ACP (chosen).** One JSON-RPC client for all ACP-capable executors, a
   registry of known executors (command, startup args, env scrub, workspace policy), a stub
   harness for tests, and the ACP wire replayed deterministically.
4. **Reuse our MCP stdio transport for ACP.** Rejected: MCP and ACP are different protocols over
   the same shape (JSON-RPC/stdio); sharing code would couple two independent specs.

Scoring (workspace criteria, in order): (1) fits REQ-061 and the spec seams (computers, runs,
approvals) — only (3) and (4) qualify, and (4) loses on (5) simplicity; (2) maturity — ACP has
multiple implementations shipping today (Claude Code, Kimi, Hermes, Cursor, Cline, Devin, Copilot
via `--acp`, Mistral `vibe-acp`, OpenCode); (3) security — the subprocess is ours to pin down
(env scrub, working dir, stdio caps), same threat model as REQ-033 stdio MCP servers; (5)
simplicity — one protocol, one adapter layer; (6) type-safety — JSON-RPC decoded through serde
with strict structs, no free-form passthrough; (7) license — ACP documentation is public
(agentclientprotocol.com); reimplemented from spec, no code copied; (8) a11y — n/a (backend).

## Decision

1. A new `AcpExecutor` port in `crates/app` (kind-aware: known executors + `custom`):
   - `known` entries carry: `id`, `display_name`, `command`, `args`, `probe_args` (how to detect the
     binary, e.g. `--version`), and no credentials ever — provider sign-in stays with the CLI's
     own login state on this machine.
   - `custom` entries: an operator-supplied command + args registered in config, validated as we
     do for stdio MCP (allow-list, bare program names, no inline secrets).
2. Per-run workspace: one directory per run (`BOMA_ACP_WORKSPACES/<run_id>`), empty at claim time,
   `0700`, and removed or retained per `BOMA_ACP_KEEP_WORKSPACES` (default: retain on failure for
   debugging, remove on success; ADR note: mirrors computers' home policy).
3. Run lifecycle unchanged: the worker claims the run (existing leases), spawns the executor,
   pumps stdio with bounded buffers + a wall-clock timeout (default 30 minutes, cap per executor),
   maps notifications to run events (message-update deltas), and records the answer attributed to
   the bot in the room.
4. Approval gateway (REQ-017) unchanged: tool calls the executor requests go through the same
   write-class approval path; the person still sees and answers cards. ACP's own permission
   prompts are surfaced as approval cards, never auto-approved.
5. Executor missing/unreachable: the run fails **legibly** (`ACP_EXECUTOR_MISSING` classification)
   before the first byte, never a partially-executed turn (REQ-061 acceptance criterion 2).
6. Initial executor set (M23 first slice): **OpenCode**, **Claude Code** (`claude --acp` if the
   CLI supports it, else the documented invocation), plus the stub ACP agent for tests.
   Remaining providers (Codex, Grok, Gemini, Cursor, Cline, Kimi, Devin, Copilot, Mistral,
   Hermes) land one at a time — each is a matrix row in the plan, each behind the same adapter
   contract, each with a wire test against a recorded stub.
7. Local model servers (REQ-075) are a sibling slice: loopback discovery (Ollama `GET /api/tags`,
   LM Studio `GET /api/v1/models`-style), best-effort, never blocks boot.

## Consequences

- The `computers` subsystem is not reused for workspaces; a lighter `RunWorkspace` port exists for
  ACP dirs. Computers stay the heavyweight isolation path; ACP workspaces are scoped directories.
- New deps: none for the first slice beyond what exists (tokio process, serde_json). If we need a
  full ACP client crate later, it goes behind DEPENDENCIES.md with license check.
- Tests: golden wire replay for the stub (`initialize` → `session/new` → `session/prompt` →
  `session/update` → `session/close`), request-timeout test, missing-binary test, and a
  per-provider matrix as executors land. The stub is committed as a test fixture under
  `crates/testkit` — no provider binary is ever downloaded by CI.
- Provenance: ACP documentation (`agentclientprotocol.com`) was consulted for method names only;
  no SDK or implementation was copied. Provider startup commands were read from nightly-labs'
  public README (documented behavior), not from their source.
