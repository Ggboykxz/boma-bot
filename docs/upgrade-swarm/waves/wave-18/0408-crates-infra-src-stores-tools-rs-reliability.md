# Worker #408 — `crates/infra/src/stores/tools.rs` — LENS: reliability — v0.4.0 proposal

- **Status:** PROPOSAL (read-only review; no source changed). CONFIRMED = verified by reading `tools.rs`, `rows.rs:475–519`, `tenant.rs:51–142`, `ports.rs:1834–1847`, `error.rs:10–50`, `card.rs:69–172`, `usecases.rs:8203–8260`, `migrations/0005_tools.sql`, `testkit/src/memory.rs:1295–1325,4075–4115` in this worktree; GUESS = needs a test/oracle run.
- **Scope:** `PostgresToolStore::catalogue_for` only — the per-run tool catalogue (REQ-016). Writes to `tool_installs` live outside this file and are covered as seams, not as owned code.
- **Spec note:** REQ-016 fixes the grant/empty-grant semantics and the conservative `Write` default for legacy rows. Skipping invalid rows, de-duplicating keys, or bounding the catalogue can change what a run sees → ADR + `docs/PARITY.md` + operator sign-off before shipping (GUESS decision).

## Summary

The SQL is individually simple and correct: RLS plus a tenant-scoped read, a PK that already forbids duplicate install rows, and a stable `ORDER BY tool_key`. Reliability breaks at the seams: the bot grant and the install list are read in **two separate transactions** (39–50, 56–70), one malformed `kind` value poisons the *entire* catalogue through `ToolKind::parse(...)?` (`rows.rs:515`), a missing bot returns an empty catalogue with no signal (51–53), and the assembled catalogue in `tool_catalogue` has no key-uniqueness check against built-ins/MCP/connectors. The Memory adapter does not mirror the PK, so tests can pass where production rejects or vice versa.

## Failure modes

### Stale / inconsistent reads (S)

- **S1 (MED, CONFIRMED):** two `with_tenant` transactions (39–50, 56–70) give no single snapshot. A grant rewrite or install toggle between them yields a catalogue that never existed at any point in time. It also costs two round trips.
- **S2 (MED, CONFIRMED mechanism / GUESS impact):** the second query uses `space_id` from the *bot row*, not from the caller. Under `System` context RLS is bypassed (`tenant.rs:95+`), so a bot moved between spaces mid-read can mix an old grant with a new space's installs. Under `Member` context RLS masks this by returning an empty list.
- **S3 (LOW, CONFIRMED):** a grant naming keys that are missing/disabled is silently ignored (74); the catalogue is quietly smaller than the operator intended, with no warning or metric.
- **S4 (LOW, GUESS):** no `LIMIT` and no store-level timeout; a corrupt or large install set inflates every run's prompt and can stall `tool_catalogue` (`usecases.rs:8223`).

### Invalid schemas (V)

- **V1 (HIGH, CONFIRMED):** `kind` is free text (`0005_tools.sql:6–12`, no CHECK) and `rows.rs:515` uses `?`. One bad row → `AppError::Rule(UnknownEventType)` for the whole catalogue, so **no run can start** because of one bad install. `tenant::unavailable` only wraps DB errors (47, 67), so this is a permanent, non-retryable error (`error.rs:49–50`).
- **V2 (MED, CONFIRMED):** `tool_spec` hardcodes `ToolEffect::Write` (`rows.rs:517`). Spec-sanctioned for legacy rows, but a read-only install is over-gated and there is no per-row `effect` column to ever correct it.
- **V3 (LOW, CONFIRMED):** no length/format constraint on `tool_key` or `description`; an oversized description inflates every prompt and bypasses the boundary validation the rest of the stack applies.
- **V4 (LOW, CONFIRMED):** missing bot → `Ok(Vec::new())` (51–53). A deleted/renamed bot looks like a valid empty catalogue; the real failure surfaces later as a confusing empty tool list in the run.

### Duplicate upserts (D)

- **D1 (LOW, CONFIRMED):** Postgres cannot store duplicate installs — `PRIMARY KEY (space_id, tool_key)` (`0005_tools.sql:14`) — and `ORDER BY tool_key` is deterministic. No in-file fix needed.
- **D2 (MED, CONFIRMED):** the Memory adapter pushes rows unconditionally (`memory.rs:1310–1316`), so a duplicate seed produces two `ToolSpec`s there but one in production. Test/oracle divergence.
- **D3 (MED, CONFIRMED downstream):** `tool_catalogue` concatenates built-ins, installs, MCP and connector tools with no uniqueness check (`usecases.rs:8223–8259`). A colliding key yields two entries with different effect classes — the approval gate may key on the wrong one.
- **D4 (LOW, GUESS):** `enabled_tools` may contain repeated keys; the `any()` filter (74) is O(grant × installs) with no dedup. Harmless today, but wasteful on large grants.
- **D5 (MED, GUESS seam):** the out-of-file writer has no visible content fingerprint. If it upserts with `DO NOTHING`, a replay with a changed `description`/`kind` is silently dropped — the same divergent-replay defect fixed for pending calls in wave 17.

## Concrete hardening (prioritized)

- **P0 — one transaction (S1/S2):** merge both reads into a single `with_tenant` closure: fetch the bot, then the installs on the same connection (or one CTE joining `bots` → `tool_installs`). Preserve the anchor-on-bot semantics and verify both `Member` and `System` RLS paths. Removes the TOCTOU window and a round trip.
- **P0 — isolate bad rows (V1/V3):** parse `kind` per row; on failure emit `tracing::warn!` + a counter and **skip that row**, rather than failing the catalogue. Alternative (stricter, requires migration): add `CHECK (kind IN ('builtin','connector','mcp','skill','memory'))` plus a length bound and backfill/quarantine bad rows first. Pick fail-open vs fail-closed by ADR — dropping a tool can violate REQ-016, failing shut blocks runs.
- **P0 — key uniqueness at assembly (D2/D3):** de-duplicate by `key` with a documented precedence (built-in > install > MCP > connector), and make `seed_tool_install` replace on `(space_id, tool_key)` to mirror the PK. Land both in one change so tests stop lying.
- **P1 — divergent upsert guard (D5):** in the writer seam, fingerprint `(description, kind, enabled)`; on `ON CONFLICT (space_id, tool_key)` reject a different fingerprint instead of silently keeping the stale row, mirroring the wave-17 pending-call fix.
- **P1 — observability (S3/V4):** counters for skipped-invalid rows, grant keys with no matching install, empty catalogue (bot missing vs genuinely no tools), and catalogue size; traced fields `bot`/`space`/`count`, no PII. Distinguish "bot absent" from "no installs" before returning `Ok(Vec::new())`.
- **P2 — bounds & timeouts (S4):** wrap the DB work in a timeout (`tokio::time::timeout`) and add a configurable `LIMIT` with a traced truncation; assert the Memory adapter behaves identically.

## Risks & mitigations

- Skipping invalid rows and de-duplicating keys are externally visible catalogue changes → DEVIATION: ADR, `docs/PARITY.md`, operator sign-off; keep the `Write` default for legacy rows.
- A `CHECK` constraint migration fails on existing bad rows → backfill/quarantine before applying; migrate in a low-traffic window.
- Merging transactions changes RLS scoping → differential-test `Member` vs `System` contexts; never bypass `with_tenant`.
- Memory/Postgres fixes must ship together or integration tests give false green.
- Do not widen a transaction across external calls; `tool_catalogue` stays a pure read.

## Acceptance tests (suggested names)

- `req_016_catalogue_uses_one_consistent_snapshot`
- `req_016_invalid_tool_kind_does_not_block_all_tools`
- `req_016_colliding_tool_keys_keep_one_spec` / `req_016_memory_and_postgres_catalogues_agree`
- `req_016_missing_bot_empty_catalogue_is_observable`
- `req_016_divergent_upsert_is_rejected` (after P1)
- `req_016_catalogue_read_is_bounded` (after P2)
