# Worker 0211 — `crates/domain/src/audit.rs` architecture proposal (v0.4.0)

Lens: architecture. Scope: read-only review of the 149-line file plus targeted reads of
`crates/app/src/ports.rs`, `crates/app/src/usecases.rs`, `crates/infra/src/stores/audit.rs`,
`crates/domain/src/lib.rs`, and migrations `0030/0045/0049/0052`. No code changed, no tests run.
Findings are **CONFIRMED by source inspection**; forward-looking statements are marked RISK/GUESS.
Anchor: REQ-058 (admin surfaces, `spec/01-requirements.md:473-479`); REQ-045 (audit export).

## Current structure (as read)

- `AuditKind` (8–45): 18 `Copy` unit variants, each with a hand-written `as_str()` arm (47–72).
  The module doc (1–4) still claims "One kind today … kept tiny on purpose" — stale.
- The closed set is encoded **three times**: enum + `as_str()`; SQL `CHECK (kind IN …)` rebuilt by
  migrations `0030`, `0045`, `0049`, `0052`; and the `security.` prefix filter
  (`ports.rs:2273`, `stores/audit.rs:37,43`). No `FromStr`/`TryFrom<&str>`/`ALL` iterator.
- The event itself is **not** a domain type: `NewAuditEvent` lives in `crates/app/src/ports.rs:2241`
  (typed `AuditKind`, typed `SpaceId`/`UserId`, free-form `target: String`, untyped `metadata: Value`)
  and the read model `AuditEventView` (`ports.rs:634`) is entirely stringly (`kind: String`,
  `space_id/actor_user_id: Option<String>`). `lib.rs:50` re-exports only `AuditKind` and
  `redact_audit_value`; `looks_like_secret` and `REDACTED` are `pub` but not re-exported.
- Redaction (74–117) matches 11 hard-coded **value** substrings and walks values recursively; it
  never inspects keys, so `{"password": "hunter2"}` or a JWT (`eyJ…`) survives. The export
  (`usecases.rs:8562`) applies it to metadata only, after the store has returned raw rows.
- Coverage gap: `AdminMemberRoleChanged`, `AdminMemberRemoved` and `ExportDownloaded` are declared
  and present in the SQL CHECK but have **no write site anywhere under `crates/`** (CONFIRMED by
  grep); the REQ-058 admin mutations that are implemented emit only grant/plugin kinds.

## Concrete upgrades

### 1. Promote the audit event to a typed domain model
Move a canonical `AuditEvent` (id newtype, `Option<SpaceId>`, `Option<UserId>`, `AuditKind`,
typed target/metadata, timestamp) into `crates/domain/src/audit.rs`; have `NewAuditEvent` and
`AuditEventView` become app-level projections of it. The domain crate stays IO/async-free.
- RISK: MEDIUM — touches every `append` call site and the store. Keep the JSONL keys and `kind`
  strings byte-identical (REQ-045 golden, `rpc_surface.rs:3888`) and preserve the insert-only
  invariant (`ports.rs:2261`).

### 2. Make the kind vocabulary single-source and round-trippable
Add `AuditKind::ALL`, `TryFrom<&str>`/`FromStr`, and a domain test asserting every `as_str()` is
unique and round-trips. Convert `AuditEventView.kind` to `AuditKind` at the infra boundary and
derive the SQL `CHECK` list from a committed test fixture so a new variant cannot land without a
migration. Keep the `security.` prefix semantics explicit (document which kinds carry it).
- RISK: LOW–MEDIUM — new methods are additive; changing the view's field type touches api/wire and
  the export path. GUESS: no existing test enumerates all 18 variants.

### 3. Replace value-only redaction with a typed metadata policy
Wrap metadata in a domain `AuditMetadata` newtype whose serializer always applies redaction; extend
`looks_like_secret` with a key-name denylist (`token`, `password`, `authorization`, `api_key`) plus
JWT/hex patterns, and add a proptest that no seeded marker survives. Decide explicitly whether
`looks_like_secret`/`REDACTED` join the `lib.rs:50` facade.
- RISK: MEDIUM — broader matching can blank legitimate fields (the source comment already warns
  about descriptions and fixtures); existing `req_045_*` goldens may change output, so record any
  change in `docs/PARITY.md` as a DEVIATION and get approval before shipping.

### 4. Close the REQ-058 audit coverage gap
Either emit `AdminMemberRoleChanged`/`AdminMemberRemoved` from the admin use cases and
`ExportDownloaded` from the export path, or remove all three from the enum and the CHECK in one
coordinated change. Add one test per admin mutation whose name carries `REQ-058` and asserts the
kind of the appended row.
- RISK: LOW–MEDIUM — writing rows changes audit-export content (DEVIATION check), but that is the
  intended REQ-058 behavior; removing values needs a migration, so prefer wiring them in.

### 5. Split the module and state the invariant
Reorganise into `audit/{kind,redact,event}.rs` behind the existing `pub use` facade and replace the
stale doc with the real rule: "add a variant ⇒ enum + `as_str` + SQL CHECK + round-trip test".
- RISK: LOW — mechanical; keep `boma_domain::{AuditKind, redact_audit_value}` paths and every
  `req_045_*` test name intact (traceability gate).

## Sequencing and gates

Order: 5 → 2 → 1 → 4 → 3 (doc/mechanical first, wire-affecting redaction last). After each step:
`cargo fmt --check`, `clippy -D warnings`, tests; preserve REQ-tagged rustdoc and the append-only
store. Update `docs/TRACEABILITY.md` only for genuinely new tests; mark any externally visible
change in `docs/PARITY.md` and wait for owner approval per `AGENTS.md`.
