# Worker #353 — `crates/infra/src/stores/mcp.rs` architecture proposal (v0.4.0)

Lens: architecture (structure, layering, boundaries, invariants, error model). Read-only review;
no source was changed. Layout claims are **CONFIRMED** from a source read of `mcp.rs`,
`app/src/ports.rs` (`McpServerStore`, `NewMcpServer`, `McpServerRecord/Summary`, `McpTool`),
`infra/src/tenant.rs`, `migrations/0029_..0032`, `app/src/usecases.rs` call sites,
`spec/01-requirements.md` (REQ-033) and `spec/05-known-defects.md` (D-05). Impact notes are
**GUESS** until a build/test run verifies them.

## Current structure (as built)

- One ~623-line adapter, `PostgresMcpServerStore { pool: PgPool, secrets: SecretBox }`, implementing
  the 12-method `McpServerStore` port: `list`, `create`, `get`, `remove`, `reveal_bearer`,
  `replace_tools`, `catalogue_tools`, `set_auth_state`, `store_oauth_session`,
  `take_oauth_session`, `reveal_client_secret`, `store_bearer`. CONFIRMED.
- Layering is correct: the port and DTOs live in `app`, the policy (URL/SSRF validation, stdio
  allow-list, D-05 private-endpoint audit) lives in `app/usecases.rs` / `domain`, and sealing is
  delegated to `space_keys::{seal_for_space, open_for_space}` over the deployment vault. The store
  persists only sealed bytes; `list`/`get` never carry credentials and the two `reveal_*` methods
  are the sole read paths. CONFIRMED.
- `with_tenant` opens a transaction and sets transaction-local `boma.space_id`, so the multi-step
  writes are atomic (`replace_tools` DELETE+INSERTs; `take_oauth_session` DELETE…RETURNING plus
  cleanup DELETE); no lock is held across `.await`, and forced RLS is the tenant boundary.
  Parameterized SQL throughout; migration CHECKs pin transport/auth_state and require url XOR
  command. CONFIRMED.
- Weak boundaries: (a) `create` carries ~20 `*_in` clones solely to feed the `Box::pin(async move …)`
  closure, and `SummaryRow` is duplicated as an inline tuple in `list` while `get` destructures a
  12-element positional tuple in-place; (b) five methods repeat `SELECT space_id FROM mcp_servers
  WHERE id = $1` before sealing/unsealing; (c) `create` explicitly rejects `TenantContext::System`
  but every other method silently relies on `as_system` RLS bypass, so the tenant boundary is
  asymmetric and undocumented; (d) all failures collapse to `AppError::Unavailable` (only unique
  violation → `Conflict` in `create`), conflating DB outage, vault/unseal failure and bad
  caller-supplied `auth_state`; (e) the three `oauth_*` columns may be partially populated while
  `list` derives `oauth_configured` from just one of them. CONFIRMED.

## Proposed upgrades for v0.4.0

**U1 — Deduplicate the seal path behind one private helper.** Add
`with_server_vault(conn, secrets, server_id, |space_id| …)` (or a small `SealedLookup`) that performs
the `space_id` lookup, `seal_for_space`/`open_for_space` and the single `NotFound` mapping, then use
it in `reveal_bearer`, `reveal_client_secret`, `store_bearer`, `store_oauth_session` and
`take_oauth_session`. Also move the clones out of `create` by sealing inside a helper that borrows
the `NewMcpServer` fields. Purely structural, behavior-preserving.
*Risk:* changes five hot paths at once; a subtle difference in the `None`-on-missing-secret vs
`NotFound`-on-missing-server semantics must be preserved and tested. **GUESS** on compile complexity
of the higher-ranked closure signature.

**U2 — Make the tenant boundary explicit and uniform.** Either reject `TenantContext::System` in
all methods as `create` already does, or document exactly which internal flows may use `as_system`
and assert the invariant at the store boundary. For member contexts, add an explicit
`AND space_id = $context` predicate alongside RLS as defense in depth (the current `WHERE id = $1`
in `get`/`remove`/`set_auth_state`/`reveal_*`/`store_bearer` depends entirely on FORCE RLS).
*Risk:* adding a System rejection can break a legitimate sweep/health path — audit the
`as_system` call sites first; a predicate change is a DEVIATION only if it alters an id-based
cross-tenant read, so stage as an assert + test before enforcing. **GUESS** on System usage.

**U3 — Replace positional tuples with named row types and mappers.** Define private
`ServerSummaryRow`, `ServerRecordRow`, `OAuthSessionRow` structs deriving `FromRow`, and
`fn summary_from`/`fn record_from` beside the queries; delete the duplicated inline tuple and
bottom-of-file `SummaryRow` alias. This makes a column reorder a compile-time or named-field error
instead of a silent field swap.
*Risk:* mechanical but touches every query; must keep SQL column order/fixtures byte-stable. Add a
test pinning each DTO field to its column before refactoring. **CONFIRMED** duplication; **GUESS** on
`FromRow` ergonomics with `boma.rfc3339(created_at)`.

**U4 — Enforce the OAuth config invariant in the schema.** Add a versioned migration with a CHECK
that the three `oauth_*` URL/id columns are all present or all absent, plus a one-time backfill that
nulls partial sets (or a decision to keep and whitelist them). Derive `oauth_configured` from that
single invariant rather than from `oauth_authorization_url IS NOT NULL`.
*Risk:* partial rows may already exist, so the backfill silently changes what `list` reports for
those servers — an externally visible change that must be recorded in `docs/PARITY.md` or deferred
behind a flag. Data migration needs a rollback plan. **GUESS** on real-world partial rows.

**U5 — Improve error fidelity without widening the public error surface.** Introduce a private
adapter that separates database outage, vault/unseal-integrity failure and caller-supplied invalid
`auth_state`, logging detail at `tracing` debug and mapping to the existing `AppError` variants at
the port boundary. Validate `auth_state` against the four allowed values before the UPDATE so a bad
value is a typed reject, not `Unavailable`.
*Risk:* must never leak driver/vault text to clients (REQ-054); keep the stable API error format.
Adds branches needing integration tests; changing the error code for an invalid state is visible,
so confirm no caller relies on `Unavailable`. **CONFIRMED** current collapse.

## Cross-cutting notes (not separate upgrades)

- Keep the D-05 private-endpoint audit in `app/usecases.rs`; it is a business rule, not a store
  concern, and the store only supplies the `allow_private` flag. Do not move it here.
- `replace_tools` is already atomic via `with_tenant`; the per-tool INSERT loop is a throughput
  concern for the reliability lens, not an architecture defect.
- `catalogue_tools` returns server *name* keys (names are `UNIQUE(space_id, name)`) but drops
  `probed_at`, so callers cannot judge freshness; surfacing it would change the port and need an ADR.

## Priority / verification

Order U3 → U1 → U5 → U2 → U4 (schema/data last). Run `cargo fmt --check`,
`cargo clippy -- -D warnings` and the REQ-033 infra integration tests after each step; mark each
claim **CONFIRMED** only with command output. Record any visible change (U2, U4, U5) in
`docs/PARITY.md` and open a decision in `docs/STATE.md` where the spec is silent.
