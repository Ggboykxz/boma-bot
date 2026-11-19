# Upgrade proposal — `crates/infra/src/stores/models.rs` (architecture lens, v0.4.0)

Worker #363. Scope: architecture-only review. READ-ONLY on source; this document is the sole write.

## Current structure (CONFIRMED — file read this session)

`PostgresModelStore` (238 lines) implements `boma_app::ports::ModelStore` with two
clusters of behavior under one adapter:

- **Space-scoped model credentials** (`model_credentials`): `put_credential` (upsert,
  AES-256-GCM sealed under the space DEK), `forget_credential`, `list_credentials`,
  `has_credential`. `put_credential` derives `space_id` from `TenantContext::Member`;
  `System` returns `NotFound`.
- **User-scoped model preference** (`user_model_preferences`): `preference` /
  `set_preference`, keyed by `user_id`. `System` yields `Ok(None)` / `Err(NotFound)`.

Cross-cutting mechanics: `tenant::with_tenant` supplies the RLS-scoped connection;
`SecretBox` wraps/unwraps DEKs and seals the API key; all DB errors map through
`tenant::unavailable`. `space_id`/`user_id` are cloned out of the context before the
`Box::pin` closure, so no borrow crosses `.await` (CONFIRMED).

Structural observations:

1. **Private DEK helper duplicates a shared module.** `PostgresModelStore::space_key`
   (lines 26–68) reimplements load-or-create + wrap/unwrap that already exists in
   `crate::space_keys::load_or_create_dek` / `seal_for_space` (CONFIRMED — see
   `crates/infra/src/space_keys.rs`). `mcp.rs` already uses the shared helper;
   `secrets.rs` keeps a third copy. Three copies of key-derivation logic drift on
   rotation, nonce handling, and error mapping.
2. **Read model contradicts the port contract.** `ModelStore::list_credentials`
   documents "connected ones only" (`crates/app/src/ports.rs:3773`), but the SQL
   selects all rows with no `WHERE status = 'connected'` (line 138). `has_credential`
   does filter (line 167). CONFIRMED parity defect.
3. **Inconsistent tenant scoping.** `put_credential` binds `space_id` explicitly;
   `forget`/`list`/`has` omit it and rely solely on RLS. Defense-in-depth and
   query-plan predictability favor an explicit predicate everywhere. CONFIRMED.
4. **No read path for the sealed key in this store.** The ciphertext is written and
   verified in `crates/infra/tests/m1.rs:3206`, but no `ModelStore` method opens it
   (ports have no "reveal"). GUESS: consumption happens elsewhere or the key is
   write-only; needs verification before changing the port.
5. **No security observability.** Credential put/forget are security-relevant
   mutations with no `tracing` span or audit event; AGENTS.md requires structured
   logs with no secrets. CONFIRMED absence in this file.
6. **Coarse error mapping.** `tenant::unavailable` flattens constraint violations;
   sibling `mcp.rs` maps unique violations to `AppError::Conflict`. CONFIRMED diff.
7. **Preference asymmetry.** `System` → `Ok(None)` for read but `Err(NotFound)` for
   write; plausible but undocumented — keep as a DEVIATION note, not a change.
8. **Write is not one transaction.** DEK mint (up to 4 statements) and the credential
   upsert run on the RLS connection; races are handled by `ON CONFLICT DO NOTHING` +
   re-SELECT, so correctness holds, but the semantics are implicit. CONFIRMED.

## Concrete upgrades for v0.4.0

### U1 — Consolidate onto `crate::space_keys` (priority: high)

Delete `space_key` and call `space_keys::seal_for_space(conn, &secrets, &space_id,
&api_key)` in `put_credential`, mirroring `mcp.rs`. Optionally expose a single
`space_keys::seal_for_space`-backed helper everywhere and retire the duplicate in
`secrets.rs` under a separate ADR.

*Risk:* low; behavior is identical load-or-create on the same table. Confirm the
shared helper's `SecretString` type matches `NewModelCredential::api_key`
(CONFIRMED both `secrecy::SecretString`). Watch for the inline-vs-shared copy being
intentionally "stable for the recorded path" (`space_keys.rs:5`) — that comment
suggests the duplication may be deliberate; decide with an ADR before removal.

### U2 — Make `list_credentials` honor "connected only" (priority: high)

Add `WHERE status = 'connected'` to the `list_credentials` query so the adapter
matches the port doc and `has_credential`. Add a REQ-052-named regression test that
writes a non-connected row directly and asserts it is not listed.

*Risk:* low and contract-aligning, but it is an externally visible behavior change
if any caller currently sees disconnected rows — record in `docs/PARITY.md` as a
DEVIATION per AGENTS.md and wait for approval before shipping.

### U3 — Explicit `space_id` on every credential statement (priority: medium)

Bind `space_id` in `forget_credential`, `list_credentials`, `has_credential` in
addition to RLS. Extract the context match into one small private
`fn space_id_of(context) -> Result<String, AppError>` used by the four methods, and
a parallel `user_id_of` for preference methods, removing the repeated
`TenantContext::System` branches.

*Risk:* low; must preserve the exact `System` outcomes (`NotFound` for writes,
`Ok(None)` for `preference`). Requires migrations/schema check that `list`/`has`
RLS and explicit filters do not disagree.

### U4 — Security audit events (priority: medium)

Emit structured `tracing` events (or write to the existing audit store) for
`put_credential` and `forget_credential`: `space_id`, `provider`, outcome, actor
`user_id`. Never log the key, ciphertext, nonce, or DEK. Reuse the project's
request-id/audit conventions.

*Risk:* medium — adds a dependency on the audit boundary and could leak identifiers
if logged verbatim; confirm with a test that no secret bytes reach logs. Do not
expand the `ModelStore` port; keep instrumentation inside the adapter.

### U5 — Typed DB error mapping and single-writer transaction (priority: low)

Map unique/FK violations to `AppError::Conflict`/`NotFound` as `mcp.rs` does, and
wrap DEK load-or-create + upsert in one explicit RLS transaction so the write is a
single unit. Add a REQ-052 integration test for concurrent puts to the same
`(space_id, provider)`.

*Risk:* medium — transaction boundaries interact with `with_tenant` and RLS; a
wrong nesting could bypass the tenant guard. Requires testcontainers-based
concurrency coverage before shipping.

## Verification status

- CONFIRMED: items 1, 2, 3, 5, 6, 8 from direct file reads; port contract from
  `ports.rs:3773`; test coverage shape from `m1.rs:3194–3220`; shared helpers from
  `space_keys.rs:23–93`; RLS/sibling patterns from `mcp.rs` and `secrets.rs`.
- GUESS: item 4 (no consumer found in this repository's `ModelStore` surface —
  verify before any port change).
- No code was changed; no tests were run by this worker.
