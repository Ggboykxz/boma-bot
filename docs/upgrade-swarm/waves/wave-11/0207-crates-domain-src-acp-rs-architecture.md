# Worker 0207 — `crates/domain/src/acp.rs` architecture proposal (v0.4.0)

Lens: architecture (structure, invariants, layering, coupling). Read-only review; scope is
crates/domain/src/acp.rs plus the call sites needed to judge it. Everything below is CONFIRMED
by reading source in this worktree; forward-looking impact is marked GUESS where inferred.

## Current structure (CONFIRMED)

`acp.rs` is 36 lines and defines exactly two data structs:

- `AcpAgent { bot_id: BotId, space_id: SpaceId, executor: String, created_at: String,
  updated_at: String }` — the stored attachment (one row per bot, absence = no ACP).
- `NewAcpAgent { bot_id, space_id, executor, saved_at: String }` — the write input.

Both derive only `Debug, Clone, PartialEq, Eq`; no serde, no validation, no methods, no tests,
no constants. Module doc says the domain "only names the reference" and that the known-executor
registry lives in the application layer.

Cross-layer facts (CONFIRMED):

- Registry: `crates/app/src/acp_provider.rs::executors()` returns `&'static [ExecutorSpec]`
  with one entry (`id: "opencode"`). `attach_acp_agent` (`usecases.rs:9609`) resolves the id here
  and rejects unknown values with `AppError::Rule`.
- Port: `AcpAgentStore::attach` takes `NewAcpAgent` but **no** `TenantContext`, whereas `find`
  and `detach` do (`ports.rs:4375`). The usecase populates `space_id` from context.
- Adapters: `PostgresAcpAgentStore` (`infra/.../acp_agents.rs`) upserts on `bot_id` conflict;
  `MemoryAcpAgentStore` (testkit) constructs `AcpAgent` literally. DB migration `0059` enforces
  `length(btrim(executor)) > 0`, RLS on `space_id`, and `bot_id` as primary key.
- Wire: `api/wire.rs` has separate `AcpAgentConnectedReply` / `DetectedExecutorsReply` DTOs, so
  the domain type is not leaked to clients — a strength worth preserving.
- `lib.rs` re-exports many domain types but **not** `acp::*`; callers use `boma_domain::acp::…`.
- Sibling `external.rs` (REQ-059) is a near-duplicate shape (`bot_id`, `space_id`, timestamps,
  plus `base_url`/`agent_name`/`secret_name`), also with no validation or tests.

Net: the module is a pure record with zero domain behaviour. For a MUST REQ-061 and the v0.4.0
quality bar (90% domain coverage, property tests on invariants, newtypes, no invalid states),
that is the core architectural gap: every invariant is enforced downstream or in SQL, so any
in-process caller can construct an invalid `AcpAgent`.

## Concrete upgrades (4), with risks

### U1 — Introduce a validated `ExecutorId` newtype and move the "known executor" decision behind a domain port
Declare `ExecutorId` via the existing `opaque_id!` macro (so ids stay opaque and non-empty), and
use it instead of `executor: String` in both structs. Keep the registry in the app layer, but
express the boundary as a domain trait, e.g. `trait KnownExecutor { fn is_known(&self, id:
&ExecutorId) -> bool; }`, and give `NewAcpAgent::for_bot(space, bot, executor, now)` as the only
constructor. This makes "unknown/empty executor" unrepresentable at the domain edge without
hardcoding the compiled registry into the domain crate.

Risks: (a) the app layer currently matches a `&str` against `&'static str` ids; passing a newtype
through `attach_acp_agent` touches usecase, port, both adapters and the testkit — mechanical but
wide (GUESS: ~15 call sites, all listed in the grep above). (b) Adding a domain trait shifts where
validation errors originate: `AppError::Rule(DomainError::InvalidName(…))` today; if the domain
returns a typed `DomainError::UnknownExecutor`, the API error shape must stay identical to remain
parity-safe. (c) Do **not** make `ExecutorId` a closed enum of known executors: that would
duplicate `executors()` and make adding a CLI executor require a domain change (registry drift).

### U2 — Add domain-level validation + unit/property tests carrying the REQ id
Add `AcpAgent::validate_executor(&str) -> Result<(), DomainError>` (trim/non-empty, length cap,
character allow-list mirroring the app registry ids) and a `#[cfg(test)] mod tests` with at least
`fn req_061_an_executor_id_is_validated()` plus a proptest for the non-empty/trim invariant and
round-trip through `Display`/`FromStr`. This mirrors `bot::Bot::validate_name`,
`connector::validate_connector_id`, and `computer::validate_*`, and is required by the quality bar
(unit tests in domain, REQ id in the test name — comments do not count).

Risks: (a) if U1 lands first, validate the newtype at construction, not twice; decide one owner
for the rule. (b) The allow-list must be a superset of `opencode` and any future registry id, or
valid new executors will be rejected — validate shape, never membership, in the domain. (c) No
wire/DB format change: these checks are additive and must not reject any value migration 0059
currently accepts.

### U3 — Rename `NewAcpAgent` to an upsert-shaped input and add a stored/new type split that documents lifecycle
The port doc says `attach` "replacing any previous attachment" and the SQL is `ON CONFLICT …
DO UPDATE`, so `NewAcpAgent` is misleading: it is an *upsert input*, not a "new" entity. Rename
to `AcpAgentUpsert` (or `AcpAgentInput`) and add a short doc noting that absence of a row means
"no ACP", that one bot has at most one attachment, and that the executor's own sign-in stays in
the CLI's login state (never in this struct). Add accessors `bot_id()`, `space_id()`,
`executor()` / `is_for(bot, space)` rather than exposing all fields publicly.

Risks: (a) a pure rename is internal-only (no serde, no wire DTO), so low risk, but it must be
atomic across usecase, port, infra, testkit and both test builders. (b) Adding accessors and
privatising fields would be a breaking change for the literal constructions in the testkit and
infra mappers — keep fields `pub` in v0.4.0 and introduce accessors additively to avoid churn.
(c) This is a usability/clarity change, not a behavioural fix; do not let it displace U1/U2.

### U4 — Factor the shared attachment shape with `external.rs` without leaking a generic into adapters
`AcpAgent`/`NewAcpAgent` duplicate `ExternalAgent`/`NewExternalAgent` in three fields and in the
"stored input + stored row" pattern. Extract a small `BotAttachment { bot_id, space_id,
created_at, updated_at }` (or a private macro) that both modules embed, and document the two
distinct identity fields (`executor` vs `base_url`/`agent_name`/`secret_name`) as the only real
difference.

Risks: (a) over-abstraction: adapters build `AcpAgent` with struct literals from a `sqlx::FromRow`
row; a nested generic breaks those literals and adds mapping boilerplate for little payoff.
(b) Flattening/serde attributes would change persisted or wire shape (DEVIATION) — there is no
serde on these types today, so keep it that way. (c) GUESS: benefit is modest; do this only after
U1/U2 are green, and stop if the diff touches the SQL mappers. A cheaper first step is a shared
`const fn` helper for timestamp copying, not a structural refactor.

## Explicit non-goals (with rationale)

- No status/lifecycle enum (`attached`, `missing`, `degraded`) on `AcpAgent`. REQ-061's
  missing/crashed-executor behaviour is already handled by `AcpProvider` and the worker
  (`acp_provider.rs`, `worker.rs:600`) through `AppError::Unavailable`; adding persisted status
  needs a migration and is an externally visible change — defer to a later wave.
- No `serde` derives on domain types. Wire DTOs are deliberately separate; deriving would invite
  domain leakage.
- No `Timestamp` newtype here. All sibling modules use plain `String` timestamps; changing that
  is a workspace-wide decision, not an `acp.rs` one.

## Suggested order and verification

U1 (type) → U2 (validation/tests) → U3 (naming/docs) → U4 (dedup, optional). Gate each step with
`cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, and the domain
test suite; confirm `bots.connectAcp` / `bots.detectedExecutors` wire shapes in `api/tests` are
unchanged (any difference is a recorded DEVIATION per AGENTS.md). Traceability: the new tests
must declare `req_061_…` in the function name.
