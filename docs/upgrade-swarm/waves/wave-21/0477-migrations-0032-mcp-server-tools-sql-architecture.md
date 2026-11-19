# v0.4.0 architecture proposal — `migrations/0032_mcp_server_tools.sql` (MCP tool catalogue table)

Worker #477 · MODULE=migrations/0032_mcp_server_tools.sql · LENS=architecture · target v0.4.0 · READ-ONLY analysis (no code changed, no tests run).
Claim status: "current structure" is CONFIRMED by direct reads in this worktree on 2026-11-19 (0032, 0029, 0030, 0031, 0005, spec/01-requirements.md REQ-033, spec/05-known-defects.md D-05, `crates/infra/src/stores/mcp.rs:296-371`, `crates/infra/tests/m1.rs:2027-2364`); every forward-looking item is a proposal (GUESS until implemented and measured).

## Current structure (CONFIRMED)

- 0032 is pure additive `CREATE TABLE`: `server_id`, `name`, `description DEFAULT ''`, `input_schema jsonb DEFAULT '{}'`, `probed_at timestamptz DEFAULT now()`, PK `(server_id, name)`, FK `server_id -> mcp_servers ON DELETE CASCADE`.
- RLS `ENABLE` + `FORCE` + one policy whose `USING` and `WITH CHECK` are byte-identical correlated `EXISTS (SELECT 1 FROM mcp_servers s WHERE s.id = server_id AND (s.space_id = current_space_id() OR is_system()))`. This is the indirect pattern of 0029 (`mcp_oauth_sessions`), 0037 (`pages`), 0038 (`tickets`), not the direct `space_id = ...` form used by ~40 other tenant tables.
- The table is a private read cache: `catalogue_tools` (`mcp.rs:339-371`) joins `mcp_servers`, filters `auth_state='connected'`, orders by `s.name, t.name`, never touches the network (matches the header contract). `replace_tools` (`mcp.rs:296-337`) does DELETE-all + one INSERT per tool inside a single `with_tenant` transaction — atomic as the comment claims, but N+1 round trips.
- `mcp_servers.tool_count` (0029) is maintained separately via `set_auth_state(..., tool_count, ...)`. 0032 stores no probe outcome, so "probe failed / zero tools" is indistinguishable from "server genuinely has no tools", and count drift cannot be detected from this table.
- No index beyond the PK, no CHECK constraints, no COMMENTs, no migration-time assertion that RLS is forced, and no REQ-033-named test beside the migration (coverage lives in `crates/infra/tests/m1.rs`).

## Guiding principle for v0.4.0

Treat this table as a private projection: the externally visible contract is the ordered `Vec<(String, McpTool)>` from `catalogue_tools`. Every upgrade is a NEW migration (0031's rule — a recorded migration is never edited), data-additive, nullable/defaulted so existing rows and `MemoryToolStore` stay in lockstep. No stored value reaches the model turn without a dual-adapter test and, if observable, a `docs/PARITY.md` entry. Keep D-05's per-space scope and ADR-002's single tenant-setting invariant intact.

## Upgrades (5 concrete items)

1. **Denormalize `space_id` for direct RLS (HIGH value, MEDIUM/HIGH risk).**
   New migration: `ADD COLUMN space_id text REFERENCES spaces(id) ON DELETE CASCADE`, backfill from `mcp_servers`, then switch the policy to the direct `space_id = current_space_id() OR is_system()` predicate used everywhere else. Removes a correlated parent lookup (and nested RLS on `mcp_servers`) for every row of every catalogue read.
   *Risk:* `mcp_servers.space_id` is not immutable (`UNIQUE(space_id, name)`), so denormalization can drift; the policy swap is a structural replacement, not pure DDL addition, and needs an ADR.
   *Mitigation:* keep the old EXISTS predicate as a temporary `CHECK`/trigger on writes during transition, add a trigger or store-layer write that sets `space_id` from the parent, retain `FORCE RLS`, and prove equivalence with member-of-space / member-of-other / system conformance tests. No wire change.

2. **Persist probe outcome so failures are not silent (HIGH value, LOW/MEDIUM risk).**
   Add nullable/defaulted `probe_status text DEFAULT 'ok'`, `probe_error text`, and optionally `schema_version integer DEFAULT 1` (new migration). The current DELETE+INSERT leaves no trace of a failed or partial probe; with these columns a degraded server can be reported in diagnostics without polluting the catalogue.
   *Risk:* new columns must not leak into `catalogue_tools` (that would be an externally visible DEVIATION); `MemoryToolStore` must mirror them or golden parity drifts.
   *Mitigation:* defaults keep all existing rows valid; read the columns only from a diagnostics path; add REQ-033-named tests for ok / failed / stale states and compare both adapters element-by-element.

3. **Make stored shape valid at rest (MEDIUM value, MEDIUM risk).**
   Add via new migration: `CHECK (jsonb_typeof(input_schema) = 'object')` and `CHECK (btrim(name) <> '')`, using `NOT VALID` followed by `VALIDATE CONSTRAINT` after auditing existing rows. Implements the AGENTS.md rule "make invalid states unrepresentable" for the JSON schema and empty tool names.
   *Risk:* MCP servers may legitimately return a non-object or empty schema; a strict constraint could reject a valid probe.
   *Mitigation:* normalize/validate at the network boundary (`mcp_http.rs` / `replace_tools`) before insert, add a `NOT VALID` window so live writes are not blocked, and record a PARITY decision if any real server payload must be rejected.

4. **Freshness index + count-drift inventory view (MEDIUM value, LOW/MEDIUM risk).**
   Add `CREATE INDEX mcp_server_tools_probed_at ON mcp_server_tools (probed_at)` for stale-probe sweeps, plus a `security_invoker = true` view joining per-server row counts against `mcp_servers.tool_count` so drift after `replace_tools` is observable.
   *Risk:* index benefit is unmeasured (GUESS); the PK `(server_id, name)` already covers the main scan and adds no extra write cost only if the new index is justified. A view with the wrong security context could leak counts cross-tenant.
   *Mitigation:* gate the index on `EXPLAIN` evidence from a seeded space; force `security_invoker` so RLS applies; a REQ-033 test asserts `tool_count == COUNT(*)` after a probe. Do NOT auto-repair counts in a trigger — that changes `set_auth_state` behaviour and needs approval.

5. **Pin the invariants at migration time and in tests (MEDIUM value, LOW risk).**
   Add `COMMENT ON TABLE/COLUMN`, plus an idempotent `DO $$ ... $$` assertion in the new migration that raises if RLS is not enabled/forced or the tenant policy is missing. Add REQ-033-named integration tests next to the catalogue tests: atomic replace (bad row rolls back and old rows survive), cross-tenant isolation, empty catalogue, and `ORDER BY s.name, t.name` stability.
   *Risk:* migration-time assertions must stay idempotent and version-tolerant (catalog names differ across PG versions); extra tests add CI time.
   *Mitigation:* query `pg_class.relrowsecurity` / `pg_policies` defensively, no dependency changes, `cargo fmt --check`, `clippy -D warnings`, and golden/contract suites on every step.

## Cross-cutting risks / sequencing

- Sequence: 2 -> 5 -> 3 -> 4 first (additive columns, assertions, constraints, diagnostics), then 1 alone behind an ADR. Item 1 is the only one that rewrites a policy, so keep it last and independently revertable.
- `replace_tools` is N+1 today; a single `unnest($1::text[], $2::text[], $3::jsonb[])` upsert would be faster but is app-side, not migration structure — treat as a separate proposal and keep the DELETE+INSERT atomicity intact in the meantime.
- Never change the returned order, the empty-catalogue meaning, or the network-free read guarantee without an approved DEVIATION. All claims above about current code are CONFIRMED from the cited reads; all proposed values, risks and benefits are GUESS until implemented and measured.
