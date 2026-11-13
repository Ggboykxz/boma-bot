# Before-M5 completion — design

Owner: M0–M4 audit + owner request “everything implemented before starting M5”.
Status: proposed — awaiting owner approval before any code.
Scope: close the open acceptance criteria of M0–M4, the known defect classes
(D-01..D-09) that are honestly in scope now, and a full agent tool loop behind the
existing `ModelProvider` port. Out-of-scope here: M5 integrations themselves,
M6 artifacts/pages, M7 channels, billing/BYOK catalogue beyond the thin adapter below.

## Intent and the why

The plan's milestone table and `spec/` state requirements in testable acceptance criteria.
“Tools are a per-run catalogue” (REQ-016) and “approvals gate side effects” (REQ-017)
are mostly implemented, but the loop that lets a model ask for a tool and get the answer back is not:
`ProviderRequest` ends at the prompt and `ProviderReply` ends at ordered text chunks.
Two MUST acceptance criteria are genuinely open in M0–M4:

- REQ-019: a parent run must receive a structured helper outcome and resume with it.
- REQ-022: a bot's memory must be available in a later run's context within the scope.

The defects (`spec/05-known-defects.md`) are the reference's real abuse surface; the ones whose
invariants can be enforced inside this build without shipping the M5–M7 features are in scope.
Their evidence bars are the spec's own tests (named `d_*` or REQ-tied), not a green checkmark that
only restates “no such route today”.

## Goals

1. REQ-019: helper run completes → parent receives a structured result message on its thread
   (role `tool`, marked as the parent's helper outcome) → parent run resumes from
   `WaitingInput`/`Queued` to a normal continuing pass, and the helper's internal steps do not
   appear as separate chat messages or run\_id leakage.
2. REQ-022: when the provider builds the prompt, the scoped memory (bot scope, then space-shared)
   is available in the request context. Write side already exists. Implementation is
   deterministic: the scripted provider can assert the injected paths/contents.
3. Tool loop: provider emits typed tool calls; the worker dispatches only catalogued tools;
   read-only executes; mutating/spend require an approval card whose answer executes the effect
   exactly once; result loop returns tool outputs to the model until a final answer, a cap, or
   cancellation; refusal (tool not granted) is recorded, never executed.
4. Scripted provider in tests scripts `tool_call`/`final` events; same tests cover the gated
   path without a network. BYOK/OpenAI-compatible adapter passes the same dispatch suite through
   a recorded fixture (offline), never a live spend in tests.
5. D-01..D-09 obligations proven with evidence in tests/docs as they can be, honestly noting where
   an anchor route belongs to a later milestone.
6. No silent weakening: every phase re-runs the gates (fmt, clippy `-D warnings`, nextest,
   deny, machete, trace, secret) and the Compose/image path stays CI-only here.

## Non-goals

- M5 connector installs, MCP remote/stdio, plugin submissions, skills catalogue (D-05/D-06 keep
  the D-01 counter as their shared invariant; the endpoints land at M5/M7).
- Artifact/page/board features (D-03 stays open until uploads exist — M6).
- Billing/spend ledger model (D-related to spend uses the effect boundary, not a ledger).
- Replacing the scripted provider in production; BYOK is additive behind the same port.

## Constraints

- Spec wins; a DEVIATION needs owner approval and is recorded in `docs/PARITY.md`.
- REQ id in the *test name* for requirement/defect tests (`d_01…`, `req_019…`).
- No secrets, no live provider keys in tests/CI.
- Typed, boundary-validated config; secrets via `secrecy`; no free-text system reasons.
- Effects exactly-once via idempotency keys; no lock across `.await`; bounded streams.

## Approaches considered

**A — close M0–M4, then D-01..D-09, then the tool loop (recommended).**
REQ-019/REQ-022 are local to the claimable pass and the prompt builder; they land first with
small, reviewable diffs. The defect sweep reuses the same stores and can land before the loop,
making the loop's refusal path meaningful. The loop then builds on a run that already resumes
after a helper.

**B — build the tool loop first.**
Faster visible agent, but memory-not-in-context and helper-resume stay open until after;
tool-call tests become flaky because the outcome of a delegated run is unspecified.

**C — BYOK provider first as a separate adapter, no loop.**
Does not close REQ-019/022; adds network maintenance without the dispatch path the BYOK adapter
must ultimately exercise.

Recommendation: **A**.

## Parts and how they get built

### Phase 1 — M0–M4 acceptance closure
- REQ-019: parent run pauses (or receives) a structured result when the helper completes; write a
  `role=tool` message on the *parent* thread carrying the helper outcome, tagged to the parent run;
  resume parent; helper's internal messages are visible only under the helper run (role=tool/tool),
  not re-emitted as top-level chat messages.
  - Tests: `req_019_the_parent_receives_the_helper_result_and_continues`
    (testkit memory store: seeded parent + spawned helper + scripted tool/final),
    `req_019_the_helper_internal_steps_do_not_surface_in_the_parent_chat`.
  - Ordering: helper completion triggers the structured write in the completion path
    (`write_reply`/`set_run_status` area), not a new RPC.
- REQ-022: prompt assembly reads scoped memory documents (bot, then shared) and passes them inside
  `ProviderRequest` as a typed `memory: Vec<MemoryContext>` (or a documented equivalent in the same
  request struct) — the scripted provider asserts it; the wire prompt stays text for now,
  documented so the agent engine can consume it later.
  - Tests: `req_022_a_memory_entry_is_available_in_a_later_run_context`
    (write memory, run, assert the request the provider received).

### Phase 2 — D-01..D-09 (in-scope now)
- D-01: shared counter port backed by Postgres (`infra::stores::rate_limit`), windowed upsert per
  (key, window), memory adapter for tests; two instances enforce one limit; `BOMA_TRUSTED_PROXIES`
  resolves the client peer already (D-04). Not wired to routes that do not exist yet.
  - Tests: `d_01_two_instances_enforce_one_shared_limit`.
- D-04: already delivered in `config`/`http` boundary — re-verify in test from a non-listed peer.
- D-07: per-procedure body caps already in `HttpConfig`; upload procedures opt-in; re-verify.
- D-08: auth configuration assertions already in tests (cookie attributes, session lifetime);
  re-verify and name them `d_08_…`.
- D-09: `SystemReason` allow-list + test that a user-supplied reason string is refused;
  CI grep for call-site discipline (`as_system` used only through the typed enum).
- D-02: already done (`d_02_the_development_mail_route_is_not_served` + CI grep); record in PARITY/STATE.
- D-03/05/06: not implemented here (anchor milestones M5–M7); record as open with the dependent path.

### Phase 3 — Tool loop (scripted then BYOK)
- Provider port gains a typed turn: either `Chunks(Vec<String>)` or `ToolCalls { calls: Vec<ToolCallRequest> }`.
  `ScriptedProvider` returns scripted calls/final. OpenAI-compatible adapter maps wire
  `tool_calls`/`finish_reason` to the same types; `secrecy` for keys; no live calls in tests
  (fixtures).
- `ToolGate` in `app` dispatches catalogue entries; effect classes: `read`, `write`, `spend`.
  `write`/`spend` create a pending card and park the run; `read` executes. On card answer, the
  effect executes once (idempotency key `tool:{run_id}:{call_id}`); result is appended as a
  `role=tool` message; the loop continues until final answer, cancellation, or a loop cap
  (bounded, documented).
- Refusal of an out-of-catalogue tool is recorded on the run and returned to the model as a tool
  error (REQ-016).
- Computer tools are dispatched to the use cases proved in M4 (bounded, behind takeover/running gates).
- Tests: `req_016_a_requested_tool_the_bot_may_use_is_dispatched`, …`is_refused_and_recorded`,
  `req_019_helper_result_…`, `d_xx` loop-cap test; scripted provider tests only, then a recorded
  OpenAI fixture replay.

## Approaches for later phases

D-03/05/06: each lands when its anchor feature exists; the shared counter (D-01) is the common
invariant and is built in phase 2 so those milestones inherit it.
Nothing outside this scope ships.

## Calls made (owner to check)

1. REQ-022 is implemented as a **typed context payload** on `ProviderRequest`, not prose concatenated
   into the prompt text: deterministic in tests, and avoids an injection of uncontrolled memory
   bytes into the prompt.
2. Helper outcomes are persisted as messages with role `tool` carrying the structured result, not a
   new top-level message type; REQ-019's "do not leak into chat" refers to *internal helper steps*,
   not to the structured outcome the parent must receive.
3. D-03/05/06 are explicitly out of scope now and carry their dependency in `docs/STATE.md`; they are
   not "implemented" by asserting absence.
4. BYOK ships as an additive adapter; no production key or live call is used in CI.
5. No new DEVIATION is intended; if one emerges, it is written to `docs/PARITY.md` and waits for approval.

## Success criteria

- `req_019_*`, `req_022_*` named tests pass; every phase gate suite is green at push.
- `golden computer-takeover-conflict` and all other implemented goldens still replay.
- `d_01` two-instance limiter test green; D-04/07/08/09 obligations have named tests.
- The tool-loop suite runs fully offline (scripted + recorded fixture).
