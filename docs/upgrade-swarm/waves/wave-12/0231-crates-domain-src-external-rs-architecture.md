# Worker 0231 — `crates/domain/src/external.rs` architecture proposal (v0.4.0)

Lens: architecture (structure, invariants, layering, coupling). Read-only review. CONFIRMED = read
this session in `external.rs`, `lib.rs`, `migrations/0054_external_agents.sql`, `usecases.rs`,
`ports.rs`, `infra/src/stores/external_agents.rs`, `spec/01-requirements.md` and ADR-026;
PROPOSAL = forward-looking; GUESS = inferred. No code was changed.

## Current structure (CONFIRMED)

`external.rs` is 39 lines and defines exactly two data structs:

- `ExternalAgent { bot_id: BotId, space_id: SpaceId, base_url: String, agent_name: Option<String>,
  secret_name: Option<String>, created_at: String, updated_at: String }` — the stored connection
  (one row per bot; absence of a row means the bot is internal per ADR-026 and migration 0054).
- `NewExternalAgent { bot_id, space_id, base_url, agent_name, secret_name, saved_at: String }` —
  the write input.

Both derive only `Debug, Clone, PartialEq, Eq`; no serde, no validation, no methods, no tests.
Identity is `bot_id` (no separate connection id); `secret_name` is a vault *reference*, never a
bearer value. `lib.rs` does not re-export `external::*`; callers use `boma_domain::external::…`.

Cross-layer facts (CONFIRMED):

- All invariants live outside the domain: `connect_external_agent` (`usecases.rs:9538`) calls
  `validate_message_text` and `external_agent_base_url_must_parse` (`:9506`: `reqwest::Url::parse`,
  host required, `https` except loopback `http`), trims trailing `/`, and resolves `secret_name`
  against the vault before writing. Migration 0054 enforces non-empty `base_url`, non-empty
  `secret_name` when present, `bot_id` primary key and RLS on `space_id`.
- Port `ExternalAgentStore` (`ports.rs:4343`) has `connect(config)` — no `TenantContext`, because
  `space_id` is inside `NewExternalAgent` — while `find`/`disconnect` take `TenantContext` (same
  asymmetry as sibling `acp.rs`).
- Adapters build the domain type by literal: `PostgresExternalAgentStore` upserts on `bot_id`
  conflict; `MemoryExternalAgentStore` (testkit) does the same. `api/src/wire.rs` uses separate DTOs
  with `base_url`/`agent_name`/`secret_name`, so the domain type is not leaked to clients.
- Sibling `acp.rs` (REQ-061) is a near-duplicate shape (`bot_id`, `space_id`, timestamps, one
  identity field); wave-11 worker 0207 U4 already proposes factoring the shared attachment shape.

Net (CONFIRMED): the module is a pure record with zero domain behaviour. Any in-process caller can
construct an `ExternalAgent` with an empty URL, a non-parsing address or a malformed secret name.
For a MUST requirement and the v0.4.0 bar (newtypes, invalid states unrepresentable, property tests
on domain invariants), that is the core architectural gap: the domain cannot be trusted as the
contract for REQ-059.

## Concrete upgrades (5), with risks

### U1 — Validated value types for the free-form strings (PROPOSAL)
Introduce `ExecutorBaseUrl` (absolute http(s), host present, https except loopback, no userinfo,
no fragment, trailing `/` normalised) and `SecretName` / `AgentName` (non-empty after trim, bounded)
newtypes; use them in both structs and make the app validator a thin delegation to one domain rule.
Keep the domain IO-free: reuse the repo's hand-validator pattern (`mcp::validate_mcp_server_url`)
rather than pulling `reqwest` into `domain`; if `url` is added instead, that needs an ADR and a
locked dependency.

Risks: (a) two owners of the URL rule would drift — decide one; (b) the error is
`AppError::Rule(DomainError::InvalidName("…"))` with specific messages, so keep them byte-identical
or the API error shape becomes a DEVIATION; (c) do not reject endpoints the current build accepts
(loopback http, trailing slashes); (d) touches usecase, port, adapters, testkit and wire mapping.

### U2 — One constructor plus invariant assertions, with REQ-059 tests in the module (PROPOSAL)
Add `NewExternalAgent::for_bot(space, bot, base_url, agent_name, secret_name, now) ->
Result<Self, DomainError>` as the only trusted construction path, an `ExternalAgent::is_for(&bot,
&space)` helper, and a `#[cfg(test)] mod tests` with `fn req_059_…` unit tests plus a proptest
(non-empty URL/secret name survive round-trip; invalid input is rejected). This mirrors
`bot::Bot::validate_name` and the connector/computer validators.

Risks: (a) infra/testkit use struct literals — keep fields `pub` in v0.4.0 and add accessors
additively, or the change becomes a breaking refactor for no behavioural gain; (b) do not
double-validate with U1; (c) tests must declare the REQ id in the function name —
`tools/trace_check.py` counts declarations, not comments; (d) zero coverage today, so add tests
before tightening validation, not after.

### U3 — Name the write input for what it is: an upsert (PROPOSAL)
`connect` is documented as "replacing any previous configuration" and the SQL is `ON CONFLICT …
DO UPDATE`, so `NewExternalAgent` is misleading. Rename to `ExternalAgentUpsert` (or `Input`) and
document the three invariants explicitly: absence of a row = internal bot, at most one connection
per bot, credential is a vault name only. Add `bot_id()`/`space_id()` accessors.

Risks: (a) atomic rename across usecase, port, infra, testkit, memory store and `build.rs` (GUESS:
~12 sites); (b) no serde and no wire DTO, so parity-safe, but keep the rename in one commit with
no format change; (c) do not let a clarity-only change displace U1/U2.

### U4 — Factor the shared bot-attachment shape with `acp.rs` (PROPOSAL)
Extract `BotAttachment { bot_id, space_id, created_at, updated_at }` (or a private macro) embedded
by both modules, leaving `base_url`/`agent_name`/`secret_name` vs `executor` as the only real
difference; coordinate with worker 0207 U4 so the two proposals do not diverge.

Risks: (a) over-abstraction — adapters and testkit build these structs from `sqlx::FromRow`
literals, and nesting breaks those literals with no payoff; (b) serde flatten would change a
persisted/wire shape (DEVIATION) — keep serde off; (c) GUESS: modest benefit, do this last; a
shared `const fn` timestamp helper is the cheaper first step, and stop if the diff reaches SQL mappers.

### U5 — Give the provider a typed target instead of raw strings (PROPOSAL)
Add a domain-level `ExecutorTarget { base_url, agent_name, credential: CredentialRef }` with
`CredentialRef::{VaultSecret(name), None}`, produced by `ExternalAgent::target()` and consumed by
`AgUiProvider`; this makes "no credential" vs "named vault bearer" explicit at the boundary and
stops raw `String`s propagating into the app. Align with worker 0167's payload/codec work and
ADR-026's requirement that run/thread identity be sent unchanged.

Risks: (a) `CredentialRef` carries a name only — reveal stays in `worker.rs`; (b) keep the type
internal and unserialised, or an API change needs a DEVIATION entry; (c) no status/liveness field —
the run state machine owns failure per ADR-026; (d) coordinate with 0167 to avoid conflicting
executor config types.

## Explicit non-goals (CONFIRMED rationale)

- No persisted `status` enum: failure belongs to REQ-012's run state machine; a column needs a
  migration and changes the wire shape.
- No `serde` derives or credential values on domain types: wire DTOs stay separate; the bearer
  never lands in the DB.
- No `Timestamp` newtype here: every sibling module uses plain RFC3339 `String`; that is a
  workspace-wide decision.
- No remote thread bookkeeping: "the AG-UI thread is our thread" (ADR-026).

## Suggested order and verification

U1 → U2 → U3 → U5 → U4 (optional). Gate each step with `cargo fmt --check`,
`cargo clippy --all-targets --all-features -- -D warnings` and the domain test suite; keep
`bots.connectExternal` / `bots.agentStatus` wire shapes and migration 0054 unchanged (DEVIATION
otherwise). Tests declare `req_059_…`; keep the frozen REQ-001..058 floor untouched.
