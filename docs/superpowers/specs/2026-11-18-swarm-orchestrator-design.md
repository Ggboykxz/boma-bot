# Design — swarm orchestrator with Queen and consensus (2026-11-18)

## Intent

The owner wants Boma Bot to reach GrokBot-level multi-agent orchestration while staying open
source and provider-agnostic: a Queen agent coordinates a variable number of worker agents and
settles between their results with an explicit, auditable consensus rule (majority, weighted vote
or arbitration). Workers may be internal providers or local ACP executors (OpenCode, Claude Code,
and further rows added one at a time under REQ-061).

## Goals / non-goals

- Goals: variable swarm size; one Queen that spawns, collects and decides; explicit consensus
  modes; every vote and the final decision recorded in the room; provider-agnostic workers; no
  hosted dependency.
- Non-goals: replacing group delegation (REQ-010); one-hop rule changes; model-mediated
  "winner declares itself"; cloud/account services; Pro UI.

## Approaches

1. **Rust-native swarm usecase + deterministic consensus (chosen).** A `swarms` record owns a
   Queen run and child worker runs. The Queen calls structured tools (`swarm.spawn`,
   `swarm.collect`, `swarm.decide`); each worker returns a structured verdict into a vote row.
   Consensus is computed in Rust, never by asking a model who won: majority (>50% of cast votes),
   weighted vote (per-role weights fixed at spawn), or arbitration (Queen writes the deciding
   rationale on a tie). The room gets a `System` block with the tally and the decision.
2. Prompt-only orchestration over group rooms (rejected): no deterministic tally, no audit record,
   no guaranteed worker count, and the one-hop delegation rule is bypassed in spirit.
3. External CLI-only orchestrator (rejected): not part of the product, unusable from the web UI.

## Interfaces (proposed)

- New additive migration: `swarms` (id, thread_id, space_id, mode, status, queen_run_id,
  created_at), `swarm_members` (swarm_id, run_id, role, weight), `swarm_votes` (swarm_id, member_id,
  verdict, rationale, created_at) — all RLS/tenant scoped like other domain tables.
- New REQ ids (to be added to `spec/01-requirements.md` with provenance in `spec/EXPORT.md`):
  - **REQ-076** — Swarm orchestration with a Queen and variable worker set.
  - **REQ-077** — Deterministic consensus: majority, weighted vote, arbitration.
- RPC: `swarms.create` (`{threadId, mode, workers: [{botId|executor, weight?}]}`) →
  `SwarmView`; `swarms.vote` (`{swarmId, verdict, rationale}`); `swarms.decide` (`{swarmId}`) →
  decision with tally; `swarms.get` (`{swarmId}`).
- UI (BoardUI, free tier): a swarm panel in the room showing Queen status, member verdicts, the
  tally and the final decision; semantic tokens only, `cx()`, dark mode via tokens.
- Observability: structured events for spawn/vote/decide, tenant scoped, no secrets in logs.

## Consensus rules

- Majority: a verdict wins with more than half of **cast** votes; otherwise undecided.
- Weighted: sum of `weight × verdict`; highest wins, tie goes to arbitration.
- Arbitration: only on tie/undecided; the Queen's rationale is stored with the decision and shown
  in the room. No silent tie-break by iteration order.

## Constraints

- Additive migration only; no destructive schema change; existing runs/rooms untouched.
- Tests named with REQ ids (unit consensus tests + PG-backed store tests + Playwright UI test).
- ADR required (this document is paired with `docs/adr/032-swarm-orchestrator.md`).
- Trace budget and both release boot gates stay green; new REQ ids raise the missing-budget only
  until their tests land, with a documented note.
