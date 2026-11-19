# Worker 0127 — `crates/api/src/automation.rs` — architecture proposal (v0.4.0)

- Lens: architecture. Requirement: REQ-072 (local automation API and CLI).
- Status: PROPOSAL only. This worker wrote no code. Read-only except this file.

## What I inspected (CONFIRMED)
- Full `crates/api/src/automation.rs` (524 lines), six handlers plus `router()`.
- `crates/api/src/router.rs:100-159` — the surface is nested at `/api/v1/automation`
  **outside** the global `TimeoutLayer`, with its own equivalent layers (CONFIRMED).
- `spec/01-requirements.md:579-585` — REQ-072 MUST also covers "start a fresh chat" and
  "the answer returns to the script" (CONFIRMED).
- `grep -rni automation` over `openapi.rs`, `config.rs`, `spec/openapi.yaml`, `tests/`:
  no OpenAPI document entries and no tests naming REQ-072 (CONFIRMED for those paths).
- `crates/app/src/usecases.rs` exposes `create_space`, `send_message`, `routines_run_now`,
  `answer_card`; no dedicated fresh-chat usecase was found by name (CONFIRMED by grep;
  interpretation GUESS).

## Current architecture (CONFIRMED)
- `router()` owns its state and layers; root nests it without supplying state. Six routes:
  agents list, send message, routine run, approvals list/answer, run get.
- Every handler repeats the same three gates inline: surface-on check → `bearer_ok` →
  `automation_context`. Agent allow-list (`agent_enabled`) is checked in only three of
  the six handlers. Error shapes are hand-built per branch (401 bare, 403/404/503/408 mixed).
- Long-poll `wait=true` is a 250 ms `tokio::time::sleep` loop in the handler, bounded by
  `min(request.timeout_seconds, config)` **and** a router-level `TimeoutLayer` of
  `config + 5`. Two timeout authorities for one request.
- Wire types (`AgentView`, `ApprovalCardView`, request structs) are private, hand-serialized
  with `serde_json::json!`; approvals `kind` is obtained via `serde_json::to_string` + `trim_matches`.
- IDs are converted `String → RunId::new → as_str` mid-handler; a parse failure is mapped
  to `ApiError::unavailable()` (503).

## Gaps against the architecture bar (CONFIRMED / GUESS)
1. AuthN/AuthZ is copy-pasted, not a boundary: a future handler can silently omit a gate.
2. Allow-list covers bots but not approvals or runs: a token scoped to agent A can list
   **all** pending cards, answer **any** card, and read **any** run (CONFIRMED from code).
3. No `utoipa` schemas and no contract-test coverage — automation can drift from
   `spec/openapi.yaml` unnoticed (CONFIRMED by grep).
4. REQ-072's "fresh chat" and "answer returns to the script" are not implemented: `wait=true`
   returns `{runId,status}` only, and no chat/space-creation route exists (CONFIRMED).
5. No REQ-named tests; no client-disconnect cancellation; no rate limit on this surface
   (CONFIRMED from file + grep; rate limit absence GUESS as policy decision).

## Upgrades (v0.4.0)

### U1 — One `AutomationGuard` extractor as the trust boundary
Replace the repeated three gates with a `FromRequestParts<AppState>` extractor that
resolves surface-on, constant-time bearer check, and `(TenantContext, User)` once, plus a
scoped-gate helper returning the existing `uniform_refusal()` unchanged.
- Why: an endpoint cannot compile without passing the boundary; one place fixes error shape.
- Risk: error precedence (404 when off / 401 when token bad) and response bytes must stay
  byte-identical for parity. Mitigation: golden tests for all three refusals before refactor.

### U2 — Close the approval/run authorization scope
Carry the allow-list into `approvals_list`, `approvals_answer`, and `runs_get`; resolve the
owning bot of each card/run and return the uniform 403 when out of scope. Add a narrow port
(`card_owner_bot`, `run_owner_bot`) in `boma_app` rather than reaching into `boma_infra`.
- Why: today the allow-list is cosmetic for half the surface (CONFIRMED).
- Risk: needs new store queries (N+1 on list — batch them); if the spec intends
  deployment-global approvals this is a DEVIATION — record in `docs/PARITY.md` and ask before shipping.

### U3 — Typed, documented wire contract
Derive `Serialize`/`Deserialize`/`ToSchema` on all DTOs, add `#[serde(deny_unknown_fields)]`
to requests, replace `json!` responses and the `to_string`/`trim_matches` `kind` hack with a
typed enum, and register all six routes in `openapi.rs` so the contract test sees them.
- Why: satisfies the "generate clients from OpenAPI" and contract-test gates (AGENTS.md).
- Risk: `deny_unknown_fields` can reject tolerant existing callers (DEVIATION candidate);
  OpenAPI diffs vs `spec/openapi.yaml` must be triaged, not auto-accepted.

### U4 — Complete REQ-072: fresh chat + answer text
Add `POST /api/v1/automation/agents/{bot_id}/chats` mapping to the existing fresh-chat
usecase, and extend the `wait=true` result with the terminal answer (last bot message of
the run's thread) so "the answer returns to the script" holds. Keep `wait=false` unchanged.
- Why: two explicit REQ-072 clauses are currently unmet (CONFIRMED).
- Risk: the answer's wire shape and chat-creation body are unspecified (GUESS) → write to
  `docs/STATE.md` → Decisions needed before shipping; guard both by the agent allow-list.

### U5 — Single timeout authority + cancellable long-poll
Compute one effective deadline in the extractor and pass it to `await_terminal_run`;
keep the router `TimeoutLayer` only as a documented backstop (or drop it with an ADR).
`select!` the poll against client disconnect / shutdown token, and prefer a store wait
notification if a port exists, retaining 250 ms polling as fallback. Replace `RunId::new`
re-parsing with typed ids and a typed error instead of `unavailable()`.
- Why: two timeouts and uncancellable polls waste capacity and complicate shutdown draining.
- Risk: a push-based wait changes latency profile and adds a port; benchmark before adopting.

## Suggested order (small steps, test-first)
U1 → U3 → U5 (refactors, parity-locked) → U2 → U4 (behavior, needs operator sign-off).
Each step: `cargo fmt --check`, `clippy -D warnings`, REQ-072 integration tests.

## Verification for v0.4.0
- `fn req_072_automation_refusals_are_stable()` — 404/401/403 byte-for-byte golden.
- `fn req_072_token_scoped_to_one_agent_cannot_answer_other_cards()`.
- `fn req_072_wait_returns_terminal_answer()` and `fn req_072_fresh_chat_route()`.
- Contract test: generated OpenAPI contains all six (plus new) automation routes.
- All claims above: CONFIRMED = read from named files this session; GUESS = design judgment.
