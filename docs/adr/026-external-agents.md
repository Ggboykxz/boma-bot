# ADR-026 — External agents through AG-UI (bots backed by any AG-UI executor)

Status: accepted (2026-11-15, milestone M12 — REQ-059).

## Context

OpenBot's headline is that any agent built on any framework can be a coworker in your workspace.
It achieves this through AG-UI (an HTTP/SSE protocol with events for tokens, tool calls, approvals
and status) and per-harness images. Boma Bot ships no such interop; every bot is bound to our
single in-process model provider abstraction. Your call (2026-11-15): improve Boma Bot to OpenBot's
level without rebuilding, and I decide the shape.

Constraints: our run state machine (queued → leased → running → completed/cancelled, REQ-012),
the approvals rail (REQ-017), and memory/routines are the product's spine — an external agent must
inhabit exactly that lifecycle rather than introduce a parallel one.

## Decision

1. **New REQ-059 — External agents.** A bot may be created or promoted to *external* with
   `{name, agentBaseUrl, agentName?, credentialSecretId?}`; storing that is a new write-gated path
   so credentials never land in the DB unsealed. Calls to it go through our provider port as a new
   `AgUiProvider` implementation: one HTTP/SSE call per turn, `thread_id = our thread id`,
   `run_id = our run id`, `user_id`, `space`/`bot` names, every message plus the memory digest our
   bot would already send.
2. **Approvals still flow through cards.** The remote agent's tool calls surface as `tool_call`
   events → our existing `park_tool_calls`/card rail captures write-class arguments → the person
   approves in the composer → the approval is relayed to the AG-UI executor as its response event.
   The executor may refuse; our run records the refusal verbatim. Our Read tools never leave the
   process; our Write tools always park.
3. **The AG-UI thread is our thread.** No remote thread bookkeeping anywhere; a replay of our own
   nonce scoping applies unchanged. The external executor sees an agentic turn, not a prompt.
4. **Failure semantics match ours, not theirs.** Network error, non-2xx, malformed SSE, or a
   turn that exceeds our run timeout: the run becomes `failed` with a safe reason (REQ-021), the
   person is told, and retries ride the same automatic retry/lease logic as internal runs. The
   external agent never gets to park our run on its own clock.
5. **Nothing else moves.** Internal bots keep the in-process provider; group rooms, routines,
   skills, computers, and the UI stay exactly as they are. External bots are a *kind* of provider
   attachment, not a second product.
6. **Spec authority extended.** The improvement program may extend the spec along these lines
   (M12 M13 M16…), each behind its own ADR and its own REQ id(s). The frozen M0–M11 contract is
   the floor and is never rewritten to fit the improvement.

## Alternatives considered

- **A webhook bridge instead of AG-UI in-process**: an adapter service forwards AG-UI↔ours. One
  more moving part, two timelines to reconcile, and the same result as one provider. Rejected.
- **MCP-as-agent (an MCP server per agent)**: MCP is a *tool* protocol, not a turn protocol; no
  streaming turn, no approvals semantics, no thread identity. Rejected (we keep MCP for tools).
- **Build our own "external agent" protocol**: violates the point of AG-UI and locks our customers
  into us.
- **Defer to REQ tickets for a future milestone**: the owner asked for the interop now.

## Consequences

- New surface area: migration(s) for the config row (additive-only), the provider implementation,
  and new RPC (`bots.connectExternal`, `bots.agentStatus`) — tested to REQ-059's acceptance
  criteria, including refusal paths, approvals round-trip, and an unreachable executor.
- The 13 framework adapters are *out of scope* here; they land inside any client operator who
  wants them, but the protocol admits them without change.
- If a deployment has no external executors configured, behavior is identical to v0.1.0.

## Evidence

Docs: `docs/IMPROVEMENTS.md` (analysis), this ADR, REQ-059 tests (forthcoming), M12 CI runs.
