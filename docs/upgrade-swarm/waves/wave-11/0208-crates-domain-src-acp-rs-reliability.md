# v0.4.0 reliability proposal — `crates/domain/src/acp.rs` (ACP attachment domain model)

Worker #208 · LENS=reliability · target v0.4.0 · READ-ONLY analysis (no code changed).

Status of claims: every "current state" statement below is CONFIRMED by direct reads of this
worktree — `acp.rs` (all 36 lines), `usecases.rs:9594-9665`, `ports.rs:4161-4164,4375-4405`,
`worker.rs:600-620`, `infra/src/stores/acp_agents.rs`, `migrations/0059_acp_agents.sql`,
`spec/01-requirements.md:620-632`, `app/src/acp.rs` (protocol id), and the sibling reliability
proposal for `app/src/acp.rs` (worker #164). Forward-looking items are proposals (GUESS until
implemented and tested). No tests were run for this proposal (READ-ONLY task).

## Current structure (CONFIRMED)

- The module is two plain data structs only: persisted `AcpAgent { bot_id, space_id, executor,
  created_at, updated_at }` and to-write `NewAcpAgent { bot_id, space_id, executor, saved_at }`.
  All fields are public; both derive `Debug, Clone, PartialEq, Eq`. No methods, constructors,
  validation, or state — the type makes no invalid state unrepresentable.
- `executor: String` is validated *only by callers*: attach checks registry membership
  (`usecases.rs:9621-9627`) and the worker re-resolves it at run time (`worker.rs:603-607`). The
  DB has a non-empty CHECK (`0059_acp_agents.sql:10`) but the domain type does not.
- `created_at`/`updated_at`/`saved_at` are opaque RFC3339 `String`s from a `Clock::now() -> String`
  port (`ports.rs:4161`), never parsed or ordered. Attach binds `saved_at` to both timestamps on
  insert; re-attach is `ON CONFLICT (bot_id) DO UPDATE SET executor, updated_at` — `created_at` is
  preserved, `updated_at` can move backwards under clock skew (`infra/.../acp_agents.rs:54-61`).
- There is no lifecycle field: absence of a row means "not attached", presence means "attached".
  No status, generation, session id, lease, or last-error is representable.
- Detach is a blind `DELETE` with no `rows_affected` check (`acp_agents.rs:86`): a no-op and a
  real removal are indistinguishable at the port boundary.
- The attachment is a static config. Each turn spawns a fresh process and opens a fresh
  `session/new` (`acp_provider.rs:278-303`); the standard client hardcodes `protocolVersion: 1`
  (`app/src/acp.rs:190`). The domain records none of this, so staleness is invisible at this layer.

## Failure modes

1. **Invalid transitions.** Attach is an unconditional upsert and detach an unconditional delete;
   there is no domain state machine. `NewAcpAgent → AcpAgent` is not a typed transition — any caller
   (usecases, infra, testkit fakes) can construct either struct with arbitrary fields, so invariants
   live in SQL and caller discipline. The worker reads the attachment once at claim
   (`provider_for`); a re-attach between claim and spawn silently switches the executor mid-run with
   no generation check. A detach followed by a crash-reporting success is indistinguishable from a
   real removal, weakening REQ-061's "never silently vanishes" guarantee.
2. **Protocol states.** `executor` is an unstructured string. If the compiled-in registry changes
   between attach and read (redeploy), the row names an unknown executor and the worker *silently
   falls back to the internal provider* with only a warn log (`worker.rs:610-611`): durable config
   and actual behavior diverge. The domain cannot say "valid for registry/protocol v1", cannot
   represent a v2 executor, and records no capability/approval expectation, so REQ-017 gate
   mismatches are only observable at session time.
3. **Session boundaries.** No session id, generation, `last_session_at`, or `last_session_error`
   exists in the model. A crashed executor (REQ-061 clause) is handled only in the app layer; the
   domain cannot trip a circuit breaker or surface a stale attachment. Unbounded timestamp strings
   allow `updated_at < created_at`, defeating any ordering/reconciliation. `AcpAgent` also permits a
   `bot_id` from one tenant and a `space_id` from another — only the store's scoping prevents it.

## Concrete hardening (with risks)

1. **Validated newtypes for executor and time.** Add `ExecutorId` (reject blank/overlong/control
   chars, trim-normalize, keep `as_str`/`AsRef<str>`) and a `Timestamp` newtype (parse RFC3339,
   order by instant). Construct `AcpAgent`/`NewAcpAgent` only through checked constructors.
   *Risk:* touches every construction site (usecases, infra `into_domain_row`, testkit memory store,
   wire DTOs); `Timestamp` is cross-crate. *Mitigation:* ship `ExecutorId` first with `TryFrom<String>`
   so serialization is byte-identical; treat `Timestamp` as a separate ADR; add proptests for
   normalization and ordering.

2. **Typed lifecycle and generation.** Add `AcpAgentStatus { Active, Pending, Failed { reason, since } }`
   as an additive column (default `Active`), a `generation: u64` bumped on every attach, and a
   `transition()` method that permits only legal edges (Pending→Active, Active→Failed, Failed→Active,
   any→removed at the store). Replace direct field writes.
   *Risk:* migration + wide usecases/worker wiring; a stale client writing an old status must not
   downgrade a newer one. *Mitigation:* bump generation in the same `ON CONFLICT` statement, guard
   updates with `WHERE generation <= $new`, keep the public wire reply shape unchanged initially.

3. **Session-correlation fields for boundary detection.** Add `last_session_at`, `last_session_error`,
   and an `executor_fingerprint` (registry/protocol version at attach time). In `provider_for`,
   compare the captured `(executor, generation, fingerprint)` to the re-read row; on mismatch or a
   recent `Failed`, abort with a legible "executor changed / last session failed" run outcome
   instead of silently using the internal provider.
   *Risk:* one extra scoped read per run and a store signature change; do not hold the run lock
   across the await. *Mitigation:* fold into the existing `find` call, emit a metric on mismatch,
   and keep retry policy in the app layer — never auto-replay prompts (non-idempotent).

4. **Observability on detach.** Return `DetachOutcome { removed: bool }` internally (or log/metric the
   `rows_affected == 0` case) so a no-op is distinguishable, while keeping the port's public
   `Result<(), AppError>` and wire reply. *Risk:* port/adapter/fake signature churn if the type
   escapes. *Mitigation:* keep the boolean crate-internal; add detach-twice and
   detach-after-reattach tests with `REQ-061` in their names.

5. **Tenant and time invariants at the boundary.** `AcpAgent::new` rejects inconsistent pairs and
   enforces `updated_at >= created_at`; change the DB conflict target to `(bot_id, space_id)` and add
   a CHECK that `updated_at >= created_at`. *Risk:* a skewed clock could reject a legitimate attach.
   *Mitigation:* clamp `updated_at = max(now, created_at)` in the service rather than erroring, with
   the DB CHECK as defense in depth; add a cross-tenant construction test.

## Cross-cutting risks / sequencing

- Order 1 → 4 → 2 → 3 → 5: validated types first (cheap, local), observability next, then the
  migration-backed status/generation, then session correlation, then DB constraints.
- Do not duplicate worker #164's work: process supervision, protocol validation, deadlines, and
  cancellation safety belong in `app/src/acp.rs`. This module only supplies types that make a stale
  or invalid attachment *detectable* before a turn starts.
- Wire shapes stay unchanged; any externally visible change (e.g. a status in a reply) is a
  DEVIATION requiring a `docs/PARITY.md` entry and approval (AGENTS.md).
- Verification gate per step: `cargo fmt --check`, `clippy -D warnings`, domain unit + proptest,
  testkit/infra integration for attach→reattach→detach and unknown-executor fallback, with
  `REQ-061` in every new test name (`tools/trace_check.py` counts declarations only).

## Out of scope

Stdio process lifecycle, wire protocol-version negotiation, executor registry contents, worker retry
and backoff policy, and the approval-card UI. This proposal is limited to the domain types that
describe an ACP attachment and the transitions/session boundaries they can express.
