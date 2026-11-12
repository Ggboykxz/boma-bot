# ADR-004 — Agent runtime: a native Rust loop, ported from the reference's semantics

Status: proposed (awaiting owner go) — this is DEC-06

## Context

The run executor is the product: it resolves a model, assembles a per-run tool catalogue, streams
tokens, enforces approvals before side effects, accepts steering, delegates helper runs, and records
attempts under a lease. The reference implements this in TypeScript (`PiAgentRuntime` plus a tool
catalogue of ~40 built-ins plus MCP). The spec makes the observable behaviour mandatory (REQ-016 to
REQ-021) and the rebuild is Rust-mandatory. AGENTS.md forbids silently guessing.

## Alternatives

1. **Reimplement the loop in Rust**: an `agent` module in the domain/app crates with a provider
   trait, a tool trait, and the run state machine driving both.
2. Keep a TypeScript sidecar that owns the loop and expose it over an internal RPC; the Rust process
   orchestrates runs, leases and persistence.
3. Adopt a third-party agent framework (in either language) and adapt its events onto our contract.

## Scoring

| Criterion | 1 Rust loop | 2 TS sidecar | 3 framework |
|---|---|---|---|
| 1 fits the spec | 1 | 3 (second runtime, duplicate event mapping) | 3 (semantics differ; approvals would be bolted on) |
| 2 maturity | 3 (new code) | 2 | 2 |
| 3 security | 1 (one process, one tool gate) | 3 (extra attack surface, second dependency tree) | 4 (opaque internals) |
| 4 performance | 1 | 3 (IPC per token) | 3 |
| 5 simplicity | 2 | 5 (two languages, two deployments) | 3 |
| 6 typed contract | 1 | 3 | 4 |
| 7 licence | 1 | 1 | varies |

## Decision

Option 1, with the loop kept small and the semantics explicit:

- `domain::run` owns the state machine (already decided in ADR-002's schema and REQ-012) and is pure:
  given an event and a run, it returns the next transition or an error.
- `app::agent` owns the turn: build the prompt, call the provider through a `ModelProvider` port,
  stream deltas onto the run's event log, dispatch tools through a `ToolGate` that consults approval
  rules, and finish with a status transition.
- Tools are a registry of `Tool` implementations with typed input/output and an effect class
  (read-only / mutating / spend). MCP tools are adapters into the same registry (REQ-033).
- The SSE stream and the persisted events are the same sequence: the loop writes events, the
  transport reads them, so a reconnect cannot miss a delta (REQ-005, REQ-020).

## Consequences

- Model SDK maturity is the main risk: the provider port must be thin (HTTP + SSE), with per-provider
  adapters and no shared "framework" assumptions. A provider that ships only a TypeScript SDK is
  called over its HTTP API directly.
- The reference's tool catalogue is a behaviour list, not code to copy: each tool is reimplemented
  against this spec's acceptance criteria.
- The loop becomes unit-testable without a network: a scripted provider and a fake tool set are
  test doubles, not shipped providers (the reference ships fakes in production packages, which
  `05-known-defects.md` flags as a misconfiguration hazard).
