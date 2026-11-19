# Wave 15 / worker 0339 — `crates/infra/src/stores/connectors.rs` (architecture) — v0.4.0 proposal

Status: CONFIRMED where read directly from the tree under `rebuild-workspace/.worktrees/before-m5`;
items marked GUESS need verification before implementation. Read-only pass: no code was changed.

## Current structure (CONFIRMED)

- Layer: an `infra` adapter implementing `boma_app::ports::ConnectorStore` (`ports.rs:2593`). Business
  rules and the catalogue live in `boma-domain::connector`; the store holds no domain state of its
  own beyond SQL-to-port mapping.
- Type: `PostgresConnectorStore { pool: PgPool, secrets: SecretBox }`, `#[derive(Debug, Clone)]`,
  `new(pool, secrets)`; no per-space state, no trait objects.
- Six methods, each wrapped in `tenant::with_tenant` (transaction-scoped `boma.space_id` plus a
  closed system reason): `list`, `install`, `revoke`, `is_connected`, `connected_credential`,
  `catalogue_tools`.
- Two pure helpers: `builtin(id)` (linear find over `builtin_connectors()`) and
  `catalogue_tools(ids)` (in-memory join of ids to code specs — no N+1).
- Storage: `connector_installs` (migration 0034), PK `(space_id, connector_id)`, RLS policy
  `space_id = boma.current_space_id() OR boma.is_system()`, status CHECK `('connected','revoked')`,
  sealed credential plus nonce columns.
- Crypto: `space_keys::seal_for_space` / `open_for_space` (ADR-020, per-space DEK under the master
  key). `install` seals inside the tenant transaction; `list` never selects credential columns.
- A second adapter exists: `MemoryConnectorStore` in `testkit`, used by API/harness tests. Port
  changes must update both adapters.
- Coverage already exists at `crates/infra/tests/m1.rs:2094`, `crates/api/tests/rpc_surface.rs:2089`,
  plus domain/testkit `req_032_*` tests. This module itself has no unit tests.

## Invariants to preserve

1. The catalogue stays code; `ConnectorSummary` and wire fields stay unchanged (public interface).
2. Only `connected_credential` reveals a credential; no secret in logs or errors (REQ-054).
3. One transaction per call via `with_tenant`; RLS is the tenant boundary; pooling never carries a
   tenant to the next borrower (ADR-002).
4. `install` is an idempotent upsert that preserves `created_at`; a revoked install exposes no tools.
5. Postgres and Memory adapters stay behaviourally equivalent for REQ-032 goldens.

## Concrete upgrades

### U1 — Close the System-context gap (highest priority)

Change: add a private `space_id(context) -> Option<&SpaceId>` guard, mirroring Memory, so a System
context yields neutral results everywhere: `list` -> empty, `revoke` -> false, `is_connected` ->
false, `connected_credential` -> None, `catalogue_tools` -> empty (`install` already returns
NotFound). Optionally tighten the RLS policy to stop granting `boma.is_system()` on this table.
Why: RLS *lifts* the filter for System context (`tenant::as_system`), so today `revoke(System, id)`
is an UPDATE across all spaces, `connected_credential(System, id)` can fetch an arbitrary space's
secret (`fetch_optional`, no `ORDER BY`), and `list`/`catalogue_tools` fold rows from every space.
The Memory adapter explicitly returns neutral for System. CONFIRMED from code — this is a real
cross-tenant read/write seam, not a hypothetical.
Risks: a legitimate system caller would change behaviour. CONFIRMED: the only direct store call
sites are in `crates/app/src/usecases.rs` (966, 1003, 8257, plus dispatch at 1032/1044) and all pass
the caller's context through; GUESS whether any admin path supplies System. If cross-space views
are needed, add an explicit `..._for_space(&SpaceId)` port method and update both adapters/goldens.
Verify `ON DELETE CASCADE` still works under FORCE RLS before tightening the policy (GUESS).

### U2 — Typed status, one source of truth for "connected"

Change: add `ConnectorStatus { Connected, Revoked }` in `boma-domain::connector` with `as_str()` and
a checked `from_str()`; bind `status.as_str()` in SQL and compare typed values in all six methods
and in Memory. Keep the CHECK constraint as the DB-side backstop.
Why: the literals `"connected"`/`"revoked"` are repeated six or more times here, plus in Memory and
the migration; one typo silently disables tools or reveals a credential. The enum makes invalid
states unrepresentable (Rust standards). CONFIRMED repetition.
Risks: legacy rows with an unexpected status become invisible (fail closed — document it);
`ConnectorSummary` must remain `String`-based to avoid an externally visible change; `from_str`
must never map an unknown DB value to `Connected`.

### U3 — Make the credential read path cheaper without weakening key handling

Change: on the read path use a read-only `load_dek` (error if absent — a connected row implies a
key exists) instead of `load_or_create_dek`; only then consider a bounded, epoch-keyed DEK cache in
`SecretBox` with eviction and zeroization.
Why: every tool call runs `connected_credential` -> 1-2 SELECTs (plus a possible INSERT) just to
unwrap the space DEK, on the executor hot path. CONFIRMED call chain; GUESS on measured cost —
benchmark before and after.
Risks: a stale cache survives master-key rotation or space-key rekey, causing decrypt failures or
wrong-key reads, so explicit invalidation and a regression test are required. Never hold an
`RwLock` across `.await`. `space_keys.rs` notes a second inline DEK copy in `secrets.rs`; any scheme
change must keep both in sync or consolidate them under a separate ADR. Caching secrets lengthens
exposure — prefer the read-only load first and cache only if measurement justifies it.

### U4 — Cap credential size at the adapter boundary (id validation stays defence-in-depth)

Change: reject oversized credentials (e.g. > 8 KiB) before sealing; optionally re-check
`boma_domain::validate_connector_id` inside `install`/`revoke`/`is_connected`/
`connected_credential`.
Why: `install` seals and stores an unbounded input (CONFIRMED). Connector-id validation already
happens at the use-case layer (`usecases.rs:985, 1002, 1022`), so the store-side check is only
defence in depth against future internal callers — not a substitute for that layer.
Risks: a new rejection is externally visible — check `spec/05-known-defects.md` and the API layer
for the current contract, and record any change in `docs/PARITY.md` before shipping. The size limit
is a GUESS; take it from the spec, not by taste. Errors must stay `AppError::Rule` with no secret
echoed.

### U5 — Structured audit events on credential mutations

Change: emit `tracing` events (or reuse the existing audit store if its schema fits) for
install/revoke with `space_id`, `connector_id`, `auth_kind`, `outcome` and affected counts — never
the credential or ciphertext.
Why: installing or revoking a bearer credential is security-sensitive, yet these methods emit no
signal today (CONFIRMED: no `tracing`/audit calls in this file), while the tenant layer does log DB
refusals.
Risks: log-based golden tests may need updates. Writing an audit row adds a second write inside the
same `with_tenant` closure — check audit-store conventions first (GUESS). Prefer events-only to
avoid a new failure mode on the install path.

## Priority and sequencing

U1 -> U2 -> U3 (benchmark-gated) -> U4 -> U5.
U1 and U2 touch both adapters: run `cargo fmt --check`, clippy with `-D warnings`, and the REQ-032
tests (`infra/tests/m1.rs`, `api/tests/rpc_surface.rs`, testkit harness) before commit. U4 is a
behaviour change and needs a `docs/PARITY.md` entry. No public type change is proposed; if U1 needs
a new port method, that requires an ADR and operator approval before shipping.
