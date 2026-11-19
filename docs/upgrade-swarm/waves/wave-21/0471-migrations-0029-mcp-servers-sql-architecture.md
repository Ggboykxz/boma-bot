# v0.4.0 architecture proposal — `migrations/0029_mcp_servers.sql` (MCP server schema)

Worker #471 · LENS=architecture · READ-ONLY review (no source, migration or spec changed).

Status of claims: all "current state" statements below are **CONFIRMED** by direct reads of
`0029_mcp_servers.sql`, its successors `0030_audit_events.sql`, `0031_mcp_credentials.sql`,
`0032_mcp_server_tools.sql`, `crates/infra/src/stores/mcp.rs`, `crates/infra/src/mcp_http.rs`,
`crates/infra/src/net_safety.rs`, `crates/domain/src/mcp.rs`, `crates/infra/tests/m1.rs`,
`spec/01-requirements.md` (REQ-033), `spec/02-data-model.md`, `spec/05-known-defects.md` (D-05)
and `docs/QUALITY_BAR.md`. All forward-looking items are proposals (**GUESS** until applied,
migrated on a seeded database and tested).

## Current structure (CONFIRMED)

- `:8-23` creates `mcp_servers` with a `text` PK, a per-space `UNIQUE (space_id, name)`, a two-value
  `transport` CHECK that enforces URL/command mutual exclusivity, a four-value `auth_state` CHECK,
  `allow_private` (D-05's per-space exception), a nullable `tool_count` cache, and `created_at` /
  `updated_at`. No length caps, no id/name grammar, no URL scheme check at the boundary.
- `:25-29` and `:32-56` enable `ENABLE` + `FORCE` RLS on two tables. `mcp_servers` uses the direct
  `space_id = current_space_id() OR is_system()` predicate; `mcp_oauth_sessions` uses a correlated
  `EXISTS` subquery over `mcp_servers` for both `USING` and `WITH CHECK` (the same idiom as 0032).
- The one pending OAuth attempt per server is modelled as `server_id` PK with a plaintext `state`, a
  sealed verifier/nonce pair and `expires_at`. Expiry is enforced by the store's `DELETE ... WHERE
  server_id = $1 AND state = $2 AND expires_at > $3 RETURNING ...` (`stores/mcp.rs:472-494`), not by
  the database; there is no creation timestamp on the session.
- Writes maintain `updated_at = now()` explicitly (`stores/mcp.rs:386`, `:566`); there is no trigger,
  so a future writer that forgets it silently leaves stale metadata.
- Real boundary validation lives in Rust (`domain::validate_mcp_server_url`, `net_safety::
  check_outbound_url`): HTTPS required unless `allow_private`, private-address DNS check, stdio
  allow-list. The schema deliberately stores only flags; it cannot express those rules itself.
- `0031` adds nullable sealed credential columns to `mcp_servers`, `0032` stores probed tools, and
  `0030` writes one `audit_events` row per private call. All follow the same pattern: a new
  forward-only migration, never an edit to a recorded one. `sqlx::migrate!` wraps each in a
  transaction (`crates/infra/src/db.rs:20`).
- `spec/02-data-model.md:135-137` lists `BotMcpServer` as a per-bot/per-space join row; no migration
  in this tree creates such a table. The observed grant mechanism is the free-form
  `bots.enabled_tools` array from `0005_tools.sql` (keys shaped `mcp.<server>.<tool>`).

## Guiding principle for v0.4.0

Additive only: ship one or more new forward-only migrations (e.g. `0061_mcp_servers_hardening.sql`)
that `ADD` columns, `CHECK ... NOT VALID` constraints, comments and indexes. Never edit `0029`
or its successors, never drop a column or rewrite existing values beyond a documented backfill,
and never change a public field: any externally visible delta is a DEVIATION to record in
`docs/PARITY.md` before shipping (AGENTS.md, QUALITY_BAR.md).

## Upgrades (5 concrete items)

1. **Pin identity and transport shape at write time.** Add `CHECK (id ~ '^[A-Za-z0-9_-]{8,128}$')`,
   `CHECK (length(name) BETWEEN 1 AND 128)` and `CHECK (url IS NULL OR (length(url) <= 2048 AND
   url ~ '^https?://'))` / same length cap on `command`, all `NOT VALID` first; audit with
   `SELECT ... WHERE ... NOT ...`, then `VALIDATE CONSTRAINT` once clean. *Risk:* testkit seeds and
   imported rows use arbitrary ids/names, so VALIDATE may fail; a grammar also blocks future id
   formats until migrated. *Mitigation:* derive the regexes from observed values and the domain
   validator, attach `COMMENT ON CONSTRAINT ... IS 'REQ-033 invariant'`, and keep the domain check
   load-bearing (SQL is defense in depth, not a replacement).

2. **Give OAuth sessions a creation time and pairwise checks.** Add `created_at timestamptz NOT NULL
   DEFAULT now()` to `mcp_oauth_sessions`, plus `CHECK (state <> '')`, `CHECK (length(state) <=
   512)`, `CHECK (sealed_verifier <> ''::bytea AND sealed_verifier_nonce <> ''::bytea)` and
   `CHECK (expires_at > created_at)` (`NOT VALID`, backfill `created_at = expires_at - interval
   '10 minutes'` for old rows before VALIDATE). *Risk:* a wrong backfill makes VALIDATE fail or
   invents history; `expires_at` is caller-supplied, so a future legitimate long/short TTL breaks
   the CHECK. *Mitigation:* audit existing TTLs in `m1.rs` fixtures first, keep the constraint
   permissive (only `expires_at > created_at`), and leave the authoritative expiry check in the
   `DELETE ... RETURNING` statement unchanged.

3. **Make `updated_at` self-maintaining.** Add a `BEFORE UPDATE` trigger on `mcp_servers` that only
   sets `NEW.updated_at = now()`, `SECURITY INVOKER` with `SET search_path = pg_catalog`, and use
   `CREATE OR REPLACE FUNCTION` + `DROP TRIGGER IF EXISTS` / `CREATE TRIGGER` for re-runnability.
   *Risk:* under `FORCE ROW LEVEL SECURITY` a lax `search_path` can resolve the wrong objects, and
   an invisible extra write can surprise a system-context update; the two existing UPDATEs already
   set the column, so the trigger is redundant today. *Mitigation:* add an integration test that an
   update bumps only `updated_at`, and confirm the trigger never writes another table.

4. **Remove the correlated RLS subquery on OAuth sessions (security-sensitive).** Add `space_id
   text` to `mcp_oauth_sessions`, backfill it under `boma.is_system()` from the parent server, set
   `NOT NULL`, then replace the `EXISTS` predicate with the direct
   `space_id = current_space_id() OR boma.is_system()` policy. *Risk:* this touches the tenant
   boundary (D-05/REQ-043): a missed backfill makes rows invisible, and a wrong value can leak a
   session across spaces. *Mitigation:* assert zero `NULL`s and zero mismatches against
   `mcp_servers.space_id` before flipping the policy; keep the old policy body in a regression test
   so both predicates are proven equivalent, and re-run the `tenancy.rs` REQ-033 cases. If parity
   review judges the policy swap too risky for v0.4.0, ship only the column/backfill and defer the
   flip with a `docs/STATE.md` entry.

5. **Resolve the `BotMcpServer` gap without creating a second source of truth.** Decide explicitly
   whether `bots.enabled_tools` (`mcp.<server>.<tool>` grants) already satisfies
   `spec/02-data-model.md:137` or whether a real join table is required. If required, add it
   additively: `bot_mcp_servers (space_id, bot_id, server_id, PRIMARY KEY (space_id, bot_id,
   server_id))` with composite FKs and the standard RLS policy — no consumer change in this
   migration. *Risk:* an unconsumed table is dead schema, and two grant mechanisms (array + join
   rows) would disagree silently; composite FKs also require additive `UNIQUE (space_id, id)` on
   the referenced tables. *Mitigation:* audit `tool_catalogue`/`builtin_tools` first; if the array
   is sufficient, write the decision in `docs/STATE.md` and add a `COMMENT ON TABLE mcp_servers`
   documenting where per-bot enablement lives instead of creating the table. Do not add a feature
   outside the spec (AGENTS.md).

## Cross-cutting notes (not separate upgrades)

- Sequence 1 → 2 → 3 → 4 → 5: prove data hygiene, add metadata, then touch RLS, then decide the
  join table. Each item is an independent `ALTER`/`CREATE`, so partial application stays safe.
- Keep the `allow_private` / audit contract intact: D-05 requires one `audit_events` row per private
  call (0030), and that table is other workers' module; do not add triggers here that duplicate it.
- Verify on a scratch database: apply to seeded state, run `cargo fmt --check`, `clippy -D warnings`,
  the REQ-033 integration tests (`tools/trace_check.py` counts declaration lines) and golden replay
  before claiming any of the above works. No migration here has been applied or benchmarked.
