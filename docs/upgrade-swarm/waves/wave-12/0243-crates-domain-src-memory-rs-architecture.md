# Worker 0243 — `crates/domain/src/memory.rs` architecture proposal (v0.4.0)

Lens: architecture (structure, invariants, layering, coupling). Read-only review; scope is the module
plus the call sites needed to judge it. CONFIRMED = read in this worktree this session
(`memory.rs`, `lib.rs`, `error.rs`, `ids.rs`, `artifact.rs`, `card.rs`, `app/src/ports.rs`,
`app/src/usecases.rs`, `app/src/worker.rs`, `app/src/acp_provider.rs`, `infra/src/stores/memory.rs`,
`infra/src/stores/rows.rs`, `api/src/wire.rs`, `spec/01-requirements.md`,
`spec/golden/memory-write.json`, sibling proposals 0231/0233); PROPOSAL = forward-looking;
GUESS = inferred. No code was changed.

## Current shape (CONFIRMED)
- The module is 156 lines: `MemoryScope { Bot, Shared }` with `as_str()` (lowercase) and `FromStr`
  returning `DomainError::InvalidName`; `#[serde(rename_all = "lowercase")]`. No `Display`, no
  `parse` method — unlike sibling enums (`card.rs::ToolKind` uses `parse` + `UnknownEventType`).
- `MAX_MEMORY_PATH_LENGTH = 512`; `validate_memory_path` rejects empty, over-long, absolute,
  trailing-slash, empty/dotted-segment, control-char and backslash paths. Length is **bytes**
  (`path.len()`), while `artifact.rs` counts `chars()` — an unrecorded inconsistency. No depth
  cap, no trim, no content bound.
- `MemoryDocument { space_id, bot_id: Option<BotId>, path: String, content: String,
  revision: u64, updated_at: String }` — all public, derives serde, no constructor. `scope()` is
  derived from `bot_id.is_some()`. Invalid states are representable (bad path, arbitrary revision).
- Five in-file `req_022_…` tests cover path and scope string rules; `lib.rs:83` re-exports only
  `MemoryDocument, MemoryScope, validate_memory_path` (not the length constant).
- Cross-layer: `ports.rs::NewMemoryDocument` + `MemoryStore::{write,read,list,remove}` take
  `(TenantContext, Option<&BotId>)`; `usecases.rs::resolve_memory_scope` validates the
  `(scope, botId)` combinations and bot existence (IO correctly in app), but `memory_write` then
  discards the resolved scope with `let _ = scope;` and the store re-derives it from `bot_id`.
- `infra/stores/memory.rs` builds identity as `format!("mem:{space}:{scope}:{bot}:{path}")`,
  derives `"bot"|"shared"` again from `bot_id`, and branches into two SQL upserts (bot UNIQUE vs
  shared partial index); revision grows by `+1` and each write appends to `memory_revisions` in one
  tenant transaction. `remove` deletes only the current row — the port doc says "and its revision
  chain"; that contract is not honoured (GUESS: orphan rows remain).
- `rows.rs::memory_document` converts `revision: i64 -> u64` with `unwrap_or(u64::MAX)` — a silent
  lossy boundary conversion with no domain type to own the rule. `api/src/wire.rs` uses separate
  camelCase DTOs, so the domain serde derive is not the wire contract; the golden fixes
  `path`/`revision`/`updatedAt` in the reply.

## Guiding principles
Keep the domain pure (no IO/clock/async/DB types). Make invalid states unrepresentable. Keep wire
fields, `as_str()` values, error messages, the `document_id` string and public re-exports
byte-stable; anything else is a DEVIATION needing `docs/PARITY.md` and approval. Prefer additive
changes: no SQL, migration, timestamp newtype or API shape change in this release.

## Upgrade 1 — One domain address instead of `(scope, Option<BotId>)` (PROPOSAL)
Introduce `MemoryOwner { Bot(BotId), Shared }` and `MemoryAddress { space_id: SpaceId,
owner: MemoryOwner, path: MemoryPath }`, with `MemoryOwner::parse(&str, Option<&BotId>)` owning the
validation currently done in `resolve_memory_scope`, and `MemoryScope::from(&MemoryOwner)` replacing
`MemoryDocument::scope()`. `usecases.rs` then parses once and passes the address through the port;
the store derives scope from `owner` instead of re-deciding it, and `document_id` becomes a method
on `MemoryAddress`.
Risks: (a) wide mechanical touch across usecase, port, both adapters, testkit and wire parsing;
(b) the `document_id` string must stay byte-identical or stored revision keys/export data break;
(c) the DB `scope` column stays a denormalised mirror — choose one source of truth or document the
dual write; (d) bot-existence checks must stay in the app layer (IO), not migrate down.

## Upgrade 2 — `MemoryPath` newtype as the only valid path (PROPOSAL)
Make `validate_memory_path` the constructor of `MemoryPath::new`, use `MemoryPath` in
`MemoryDocument` and `NewMemoryDocument`, and keep the free function as a thin delegating
re-export. Add a documented depth/segment rule and decide explicitly whether 512 means bytes or
characters.
Risks: (a) changing the length unit or adding depth limits changes what is accepted — DEVIATION,
so v0.4.0 should keep 512 bytes and only record the rule; (b) infra/testkit build documents from
rows by literal — keep fields public or add a crate-visible `from_stored` path that re-validates;
(c) add a `proptest` for the parser before tightening it, so rejection behaviour is observable.

## Upgrade 3 — Typed `Revision`, keep concurrency semantics unchanged for now (PROPOSAL)
Add `Revision(u64)` with `next()`, `as_u64()` and a checked conversion from the DB `i64`, so the
`unwrap_or(u64::MAX)` in `rows.rs` moves into one domain rule. Document monotonicity on the type.
Do **not** add `expected_revision`/CAS in v0.4.0 unless approved.
Risks: (a) the SQL `ON CONFLICT … revision = revision + 1` is load-bearing for the golden
(`revision: 4`) and must not change; (b) any CAS/optimistic-concurrency field is a wire and
behaviour change — record as DEVIATION and wait for approval; (c) wire DTOs stay `u64` via `From`.

## Upgrade 4 — Contract tests for the cross-layer promises (PROPOSAL)
Add `req_022_…` tests for scope round-trips, path boundaries (empty, 512/513 bytes, dotted,
backslash, control, Unicode), `Revision::next`, and the address parser's four invalid
combinations. Add a test that states the `MemoryStore::remove` "removes its revision chain"
contract so the current current-row-only behaviour becomes a visible finding in `docs/STATE.md`
rather than an implicit lie. Reuse the `proptest` path generator from Upgrade 2.
Risks: (a) tests must declare the REQ id in the function name (`tools/trace_check.py` counts
declarations); (b) if a test contradicts the spec or original, record the conflict, do not weaken
the test; (c) do not assert the new address type's internals — assert observable rules.

## Upgrade 5 — Unify the scope vocabulary with sibling enums (PROPOSAL)
Give `MemoryScope` a `Display` impl and a `parse` associated function as the single internal
entry point, keeping `FromStr` and the current `InvalidName` messages as a compatibility shim.
Risks: (a) a dedicated `DomainError::UnknownMemoryScope` would change the API error shape —
default to no variant change; (b) `Display` must return exactly `as_str()`; (c) lowest value,
land last and drop it if it touches error mapping tests.

## Explicit non-goals (CONFIRMED rationale)
- No IO, async, clock or SQL types in `domain`; bot existence and tenant checks stay in `app`.
- No new timestamp type: every sibling module uses RFC3339 `String` (worker 0231/0233).
- No removal of the `MemoryDocument` serde derive, wire field rename, or content-size cap: a cap
  changes accepted inputs (DEVIATION) and should first be checked against the original as oracle.

## Suggested order and verification
U4 (contract tests, no behaviour change) → U1 (address type) → U2 (path newtype) → U3 (revision
newtype) → U5 (optional cleanup). Gate each step with `cargo fmt --check`,
`cargo clippy --all-targets --all-features -- -D warnings` and the domain test suite; replay
`spec/golden/memory-write.json` and keep `memory.write/read/list/remove` wire shapes unchanged.
