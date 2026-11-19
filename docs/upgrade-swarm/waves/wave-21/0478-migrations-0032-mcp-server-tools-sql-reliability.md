# Worker #478 — `migrations/0032_mcp_server_tools.sql` — LENS: reliability — v0.4.0 proposal

- **Status:** PROPOSAL (read-only review; no source changed). CONFIRMED = verified this session by reading `migrations/0032_mcp_server_tools.sql`, `0029_mcp_servers.sql`, `0031_mcp_credentials.sql`, `0002_m1.sql`, `crates/infra/src/stores/mcp.rs`, `crates/app/src/usecases.rs` (probe), `crates/domain/src/mcp.rs`, `spec/01-requirements.md` (REQ-033), `spec/05-known-defects.md` (D-09) in this worktree. GUESS = needs an oracle/test run.
- **Scope:** the tool-catalogue table, its PK, its RLS policy, and `probed_at`. **Additive only:** every hardening below is a new object or a new forward-only migration `006X_mcp_server_tools_hardening.sql`; nothing edits 0032 in place.

## Summary

The table is lean and the write path is atomic (`replace_tools` DELETE + INSERT loop runs inside one `tenant::with_tenant` transaction, `stores/mcp.rs:313-335`), so a failed probe rolls back to the previous snapshot — a real strength. Reliability rests almost entirely on the writer, though: dedup is a bare PK with no conflict handling, the schema value is untyped JSON, tenant identity is inherited from the parent via a nested-RLS subquery, and no freshness is ever checked. The additive fixes are typed CHECKs, a deduping upsert primitive, a tenant-anchoring restrictive policy, and an opt-in freshness view.

## Failure modes

### Dedup (D)

- **D1 (HIGH, CONFIRMED):** PK `(server_id, name)` is the only dedup, and `replace_tools` uses plain `INSERT` (mcp.rs:321-331). A server that returns two tools with the same `name` violates the PK mid-loop; `tenant::unavailable` maps it to `Unavailable`, the transaction rolls back (old catalogue survives — good), then the probe marks the server `auth_state='error'` (usecases.rs:911-916). One duplicated tool name blanks an otherwise healthy server's catalogue. Sanitize is *identity-preserving* (`domain/mcp.rs:94-110`, refuses rather than rewrites, `TOOL_KEY_MAX=64` at :84), so collisions can only come from the server itself — but that is a plausible server bug. No `ON CONFLICT` / pre-dedup.
- **D2 (MED, CONFIRMED):** `input_schema jsonb NOT NULL DEFAULT '{}'::jsonb` (0032:10) accepts *any* JSON. A server returning `[]`, `"x"` or `null` stores a non-object that `catalogue_tools` (mcp.rs:345-354) hands verbatim to the model as a tool spec → malformed tool call / failed model turn. No `jsonb_typeof = 'object'` guard.
- **D3 (MED, CONFIRMED):** `description` and `input_schema` are unbounded (defaults only). The 64-byte name cap lives in Rust (`domain/mcp.rs:84`), not the DB; a hostile/buggy server can return megabyte descriptions/schemas, bloating rows, WAL and every catalogue scan. No DB length CHECK.
- **D4 (LOW, CONFIRMED):** `probed_at NOT NULL DEFAULT now()` and `replace_tools` never binds it, so the DB clock is used (good). But a legacy/test writer that binds it can store a future timestamp; no `CHECK (probed_at <= now())`.

### Row-level security (L)

- **L1 (HIGH, CONFIRMED basis, GUESS impact):** the policy has no local `space_id`; tenancy is inherited from `mcp_servers.space_id` via a correlated `EXISTS` (0032:18-30). Two risks: (a) the subquery reads `mcp_servers`, itself `FORCE ROW LEVEL SECURITY` (0029:25-29) — nested policy evaluation may hide the parent from the active role and make every tool row invisible / every INSERT fail `WITH CHECK` even for a legitimate tenant; (b) if `mcp_servers.space_id` is ever UPDATEd, these rows silently change tenant with no write here. No composite FK pins `(id, space_id)`.
- **L2 (MED, CONFIRMED):** `OR boma.is_system()` is a total bypass keyed off a GUC string, and the closed `SystemReason` vocabulary is enforced only in Rust (defect D-09). Any future path setting an arbitrary reason crosses all tenants for these rows.
- **L3 (MED, CONFIRMED):** `USING` and `WITH CHECK` are identical and form a single default `ALL` policy — isolation itself is sound, and a probe without `boma.space_id` fails closed. But any hardening/backfill migration with DML under `FORCE RLS` and no `boma.space_id` / `boma.system_reason` set touches **0 rows silently** and a later `VALIDATE` passes vacuously (same trap as worker #422 L2).
- **L4 (LOW, CONFIRMED):** `CREATE TABLE` / `CREATE POLICY` are not `IF NOT EXISTS` / catalog-guarded and there is no `BEGIN/COMMIT` — a half-applied migration fails on retry (weaker than the `DO`-block migrations).

### Stale tools (T)

- **T1 (HIGH, CONFIRMED):** `catalogue_tools` filters only `s.auth_state = 'connected'` (mcp.rs:349); `probed_at` is never consulted. A server that goes dark but is never re-probed keeps its tools in every run's catalogue indefinitely — the model is offered tools that no longer exist. No TTL/freshness anywhere.
- **T2 (MED, CONFIRMED):** "never probed" and "probed OK with zero tools" are indistinguishable (both 0 rows); there is no per-server last-successful-probe marker, and `auth_state` can read `connected` with an empty set.
- **T3 (MED, CONFIRMED, cross-worker):** `mcp_servers.tool_count` is written only by `set_auth_state`, never by `replace_tools` (worker #354 T3); the cached count diverges from this table's row count. No trigger reconciles them.
- **T4 (LOW, CONFIRMED):** no index on `probed_at` and no partial "live tools" index; a future freshness-filtered read will scan. The PK prefix on `server_id` does cover today's join.

## Concrete hardening (additive, prioritized)

All land in a new, BEGIN/COMMIT-wrapped, idempotent migration; `SET LOCAL boma.system_reason = 'migration'` before any DML, and a `DO` guard that RAISEs if neither tenant nor system context is set (L3/L4).

- **P0 — H-D2:** `ADD CONSTRAINT mcp_server_tools_schema_is_object CHECK (jsonb_typeof(input_schema) = 'object') NOT VALID`, audit, then `VALIDATE`. Risk: pre-existing non-object rows fail validation — audit first.
- **P0 — H-D3:** `CHECK (octet_length(name) <= 64 AND octet_length(description) <= 8192 AND octet_length(input_schema::text) <= 65536) NOT VALID` (sizes GUESS, confirm from observed p99). Risk: too-tight caps reject legitimate verbose schemas.
- **P0 — H-D1:** additive function `boma.replace_mcp_tools(p_server_id text, p_tools jsonb)` doing one DELETE plus `INSERT ... SELECT DISTINCT ON (name) ... ON CONFLICT (server_id, name) DO UPDATE ... , probed_at = now()` — first/last-wins dedup, no whole-probe rollback. Risk: dedup winner is a behavior choice → ADR + `docs/PARITY.md`; must run as SECURITY INVOKER under RLS and be oracle-tested; adopting it is a store change outside this module's write scope.
- **P1 — H-L1:** add a `RESTRICTIVE` policy calling a `search_path`-pinned `SECURITY DEFINER` helper that returns `mcp_servers.space_id` for a server id and requires it to equal `current_space_id()` unless `is_system()` — removes dependence on nested-RLS semantics. Risk: SECURITY DEFINER is a privilege-escalation surface — pin `search_path`, revoke PUBLIC execute, and prove with an oracle test before enabling (GUESS until then).
- **P1 — H-T1:** additive view `boma.mcp_catalogue_tools_fresh` joining `s`+`t` with `s.auth_state='connected' AND t.probed_at > now() - p_ttl`; consumers opt in via a store change. Add `CREATE INDEX IF NOT EXISTS mcp_server_tools_probed_at ON mcp_server_tools (probed_at)` if adopted. Risk: TTL is a product decision; internal view, so no public-interface DEVIATION until consumed.
- **P1 — H-T2:** additive nullable `mcp_servers.last_probe_at timestamptz`, set in the same transaction as `replace_tools`, to distinguish never-probed from empty. Risk: store change required; define semantics for pre-existing `connected` servers (NULL = unknown).
- **P2 — H-T3:** `AFTER INSERT OR DELETE` trigger on `mcp_server_tools` recomputing `mcp_servers.tool_count`. Risk: competes with `set_auth_state`'s write — pick a single owner first; needs SECURITY DEFINER under FORCE RLS.
- **P2 — H-D4/H-L4:** add `CHECK (probed_at <= now()) NOT VALID` and make every new object catalog-guarded so the migration is retry-safe.

## Risks & mitigations

- H-D1's dedup winner is an externally visible catalog change → ADR + operator sign-off before the store calls the function; do not smuggle it in as "just hardening".
- H-L1's composite tenant pin is **not fully additive**: `mcp_servers(id, space_id)` needs a `UNIQUE` pair and these rows a `space_id` column to form a composite FK. Ship the restrictive policy now; treat the FK as a separate ADR.
- New CHECKs can reject pre-existing rows: always `NOT VALID` → audit query → `VALIDATE`, with a rollback script that drops only new objects.
- `FORCE RLS` makes migration DML invisible without the GUC (L3) — fail the migration loudly rather than validating zero rows.
- Freshness filter (T1) changes which tools a run sees → confirm TTL with product; measure catalogue latency before enabling the index.

## Acceptance tests (suggested names)

- `req_033_duplicate_tool_name_does_not_blank_catalogue`
- `req_033_non_object_input_schema_rejected_at_insert`
- `req_033_oversized_tool_description_rejected_at_insert`
- `req_033_tool_row_invisible_when_parent_server_is_other_space`
- `req_033_system_bypass_requires_closed_reason`
- `req_033_fresh_view_excludes_stale_probed_at`
- `req_033_replace_tools_stamps_db_clock_probed_at`

## Decisions needed (GUESS → operator/spec)

1. On duplicate tool names from a server, keep first or last? (blocks H-D1)
2. Freshness TTL for T1, and is a stale server's catalogue dropped or merely flagged?
3. Exact cap values for H-D3 from observed tool schema sizes.
4. May the closed `SystemReason` set be mirrored in the DB (L2), or is the Rust enum authoritative?
