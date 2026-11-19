# 0373 — `crates/infra/src/stores/plugin_installs.rs` — architecture (v0.4.0)

- Worker #373 · Lens: architecture · Target: v0.4.0 · Read-only review; no code was changed.
- Module: `crates/infra/src/stores/plugin_installs.rs` (126 lines) at `before-m5`
  (v0.3.0-rc5): the Postgres adapter for `PluginInstallStore`, exported as
  `PostgresPluginInstallStore` from `crates/infra/src/lib.rs:87`.
- Read for context (CONFIRMED by direct reads): `crates/app/src/ports.rs:2710–2796`
  (`PluginInstallOutcome`, `PluginInstallSummary`, `NewPluginInstall`, `PluginInstallStore`),
  `crates/app/src/usecases.rs:1328–1447` (`marketplace_install`, `record_install`),
  `migrations/0035_plugin_installs.sql`, `migrations/0028_skills.sql`,
  `crates/infra/src/tenant.rs`, `crates/infra/src/stores/skills.rs`,
  `crates/testkit/src/memory.rs:6013–6135` (`MemoryPluginInstallStore`),
  `crates/infra/tests/m1.rs:2155–2243` (REQ-034 round-trip),
  `docs/upgrade-swarm/waves/wave-14/0309-...plugin-rs-architecture.md`, `AGENTS.md`.

## Current structure (CONFIRMED)

`PostgresPluginInstallStore` is a newtype around `PgPool` with one `const fn new`; the trait impl
has exactly three methods, each wrapped once in `tenant::with_tenant` (one transaction, RLS scoped
via `boma.space_id`, `migrations/0035`). `list` (:32–63) fetches a 7-tuple with `query_as` and maps
it by hand into `PluginInstallSummary`, ordering by `created_at DESC, id ASC`; the timestamp text
comes from the DB function `boma.rfc3339`. `record` (:65–102) extracts `space_id` from a member
context (system → `AppError::NotFound`) and runs an `INSERT … ON CONFLICT (space_id, source) DO
UPDATE`, refreshing `resolved_sha`, `name`, `parts`, `skipped`, `created_at` and `updated_at`.
`remove_plugin_skills` (:104–125) is a `DELETE FROM skills WHERE origin = 'plugin' AND
plugin_source = $1`, with `rows_affected()` saturated through `u32::try_from(..).unwrap_or(u32::MAX)`.

Architecturally the file carries two different responsibilities: an install *ledger* (the
`plugin_installs` table) and a *skill revocation* command against a different aggregate (`skills`).
It owns no orchestration: `marketplace_install` (usecases.rs:1343–1412) calls `remove_plugin_skills`,
then a loop of `teach_skill`, then a loop of `mcp_server_add`, then `record_install` — each in its own
`with_tenant` transaction. There is no install/delete port method, so a failed mid-install leaves
revoked skills and no (or an old) ledger row.

## Guiding principle for v0.4.0

Make the ledger a typed, single-purpose port: one aggregate per store, one transaction per logical
write, and typed values at the store boundary. Preserve every public port signature, JSON field
name, error text and order-by clause unless the change is explicitly recorded as a DEVIATION in
`docs/PARITY.md` (AGENTS.md). Keep `parts`/`skipped` wire-compatible with `marketplace.list` and
the RPC view; REQ-034's "skipped parts named with reasons" contract is load-bearing.

## Concrete upgrades (5), each with a risk

1. **Move skill revocation to `SkillStore`.** Add `SkillStore::remove_by_source(plugin_source)` for
   the `DELETE` at :112–117 and drop `remove_plugin_skills` from `PluginInstallStore`, leaving this
   file a pure ledger adapter. *Risk:* the port method is implemented by both the Postgres and the
   `MemoryPluginInstallStore` testkit and called from `usecases.rs:1346`; removing it breaks those
   callers, and a wrong predicate would delete taught (`origin != 'plugin'`) skills. *Mitigation:*
   land the new method first, keep a deprecated forwarding method for one release, and replay
   `req_034_a_plugin_install_round_trips_through_postgres` to prove the taught skill survives.

2. **Give `record` atomic replace semantics.** Expose a store-level `replace(install, skills,
   mcp_servers)` (or a shared transaction handle) so revoke → rewrite → ledger-upsert is one
   `with_tenant` call, replacing today's four-plus independent transactions. *Risk:* this pulls
   `SkillStore`/`McpServerStore` writes into the store layer and crosses into other workers'
   modules (usecases, skills, mcp); a naive reimplementation would hold a lock or borrow a
   connection across `.await`, violating AGENTS.md. *Mitigation:* keep `usecases` as the orchestrator
   and pass one borrowed `PgConnection` through a private helper; add a test that an injected
   failure mid-install rolls back to the previous state. If that proves too invasive, document
   the current partial-state window in `docs/STATE.md` instead of silently leaving it.

3. **Decode `parts` and `skipped` into typed values at the boundary.** Introduce private
   `InstallParts { skills: u32, mcp_servers: u32 }` and `SkippedPart { kind, name, reason }` structs,
   `serde_json::from_value` them on `list`, validate, then re-emit the *same* `serde_json::Value`
   shape the port exposes; the `Vec<Value>` → `Value` asymmetry (:37 vs :93) becomes explicit.
   *Risk:* existing rows have no DB-level shape check (`parts`/`skipped` are plain `jsonb` in
   `migrations/0035`), so a fail-closed decode could make `marketplace.list` reject installs written
   by older versions. *Mitigation:* audit stored shapes (GUESS — no canonical constraint found),
   default missing keys to today's values (`{}` / `[]`), keep serde field names byte-identical, and
   add a round-trip test asserting `parts["skills"] == 2` as in m1.rs:2218.

4. **Resolve upsert identity and timestamp semantics.** On reinstall the row keeps its original
   `id` (the `UPDATE` at :80–85 never sets `id`), so the fresh `pin_*` id returned by
   `record_install` (usecases.rs:1427) is not persisted and cannot be found via `list`; meanwhile
   `created_at` is reset to `now()` and therefore means "last replaced", not "installed".
   *Risk:* either fix is externally visible through `PluginInstallSummary` — a DEVIATION needing
   `docs/PARITY.md` — and changing `created_at` ordering affects the catalogue order. *Mitigation:*
   pin current behavior in a test first, then choose the least-surprising option (stable id +
   rename the column to `replaced_at` via a versioned migration), and confirm the RPC view before
   shipping. Do not change ORDER BY `created_at DESC, id ASC` without a parity entry.

5. **Tighten error and observability discipline.** Add a `tracing::instrument` span per method
   carrying operation and canonical `source`/resolved SHA on failures, and drop the saturating
   `unwrap_or(u32::MAX)` at :124 in favour of a checked count (or document saturation). *Risk:*
   logging a raw plugin source can leak credential-bearing refs, and changing the returned count
   type alters the port signature used by the testkit; generic driver text is already logged at
   `warn` by `tenant::unavailable` (tenant.rs:153–155). *Mitigation:* log only the canonical source
   and SHA (never the raw URL), keep `AppError::Unavailable`/`NotFound` variants and messages
   unchanged, keep the `u32` return but return `u32::try_from(..).unwrap_or(0)` only after proving
   no caller depends on saturation.

## Cross-cutting risks / sequencing

- Sequence 3 → 4 → 5 → 1 → 2: local, signature-preserving work first; port-surface changes next;
  the cross-module transaction redesign last and only with the other workers' agreement.
- Freeze the public port signatures, JSON field names, error text, limit values and ORDER BY while
  doing 3–5. `domain/src/plugin.rs` canonical keys, `usecases.rs` orchestration and the
  `SkillStore`/`McpServerStore` writes are other modules — coordinate, do not duplicate.
- Gate each step with `cargo fmt --check`, `clippy -D warnings`, the REQ-034 infra round-trip and
  RPC tests, goldens, and `tools/trace_check.py` REQ-034 naming. No new dependency is needed.

## Out of scope for this module

Source grammar / canonical key (`domain/src/plugin.rs`), GitHub acquisition (`infra/src/plugin.rs`,
wave-14 0309), plan building and `marketplace_install` orchestration, the skill and MCP-server
writes, and the wire/RPC view of `PluginInstallSummary`. This proposal only restructures the
ledger adapter in `plugin_installs.rs` and the port methods it implements.
