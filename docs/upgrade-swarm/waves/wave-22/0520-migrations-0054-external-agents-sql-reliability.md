# 0520 — `migrations/0054_external_agents.sql` — reliability (v0.4.0)

Worker #520 · module `migrations/0054_external_agents.sql` · lens: reliability · READ-ONLY review of
`rebuild-workspace/.worktrees/before-m5` (v0.3.0-rc5). Proposal only; no code, SQL or gate changed.
Line refs are against this worktree. **CONFIRMED** = read this session; **GUESS** = needs a test.
Scope: 0054 (25 lines) and its consumers: `crates/infra/src/stores/external_agents.rs` (137 lines),
`crates/infra/src/tenant.rs:107-145`, `crates/infra/src/db.rs:19-20,67-74`,
`migrations/0002_m1.sql:27-42,141-153`, `migrations/0025_bot_secrets.sql:7-17`, `0059_acp_agents.sql`,
`spec/01-requirements.md:486-498` (REQ-059), `docs/adr/026-external-agents.md`, and wave-15 #346 /
wave-22 #482 / wave-21 #470 proposals (do not re-propose their store-layer fixes).

## Baseline — keep, do not re-propose

- One row per bot: `bot_id text PRIMARY KEY`, both FKs `ON DELETE CASCADE`, `space_id NOT NULL`
  (0054:5-6) — cardinality idempotence and no orphans (CONFIRMED).
- `ENABLE` + `FORCE RLS` with symmetric `USING`/`WITH CHECK` (0054:19-25) fails closed when the GUC
  is unset (`current_setting(..., true)` → NULL; `is_system()` → false) (0002:27-42; CONFIRMED).
- `base_url`/`secret_name` non-blank CHECKs (0054:12-14); timestamps DB-defaulted (0054:10-11);
  a `space_id` index (0054:17); DDL is transactional via `sqlx::migrate!` (db.rs:19-20; CONFIRMED).

## Failure modes

### F1 — Session state: "connected" is row presence, not a live, coherent state (high)

- `created_at`/`updated_at` are written from the caller's `saved_at` (store 61-79), so the DB
  defaults are bypassed and a replay with an older clock regresses the timestamp; row presence is
  the only connect signal — a dead endpoint or rotated/missing secret still reads as connected
  (CONFIRMED shape; interleaving GUESS). REQ-059 wants a legible failure at run time, not a stale
  "connected" claim.
- `secret_name` is free text with no FK to `bot_secrets(space_id, bot_id, name)` (0025:7-17); a
  deleted/renamed secret leaves a config that can never authenticate (CONFIRMED; GUESS that the
  current path always keeps them in sync — only the use-case does, wave-15 #346 F2).
- `base_url` has only a non-empty CHECK (0054:12): no scheme/length guard, so a malformed or
  unreachable URL is stored and only fails at turn time (CONFIRMED).

### F2 — RLS: correct policy, fragile session inputs and a coherence hole (high)

- The policy trusts `boma.space_id`/`boma.system_reason` GUCs set transaction-locally by
  `set_config(..., true)` (tenant.rs:107-145). Fail-closed today, but (a) any future plain session
  `SET` leaks a tenant across pooled connections, and (b) `is_system()` is true whenever
  `system_reason` is non-empty, so a stale/uncleared system GUC grants cross-tenant reads/writes
  of this config (CONFIRMED by construction; regression GUESS). ADR-002's CI grep is the only guard.
- `FORCE RLS` subjects owner-run maintenance to the policy: a future backfill/`VALIDATE` on this
  table sees only rows of the current space unless run as BYPASSRLS/`is_system()` (wave-22 #482 R1;
  CONFIRMED convention).
- The policy keys on the row's own `space_id`; nothing in the DB ties it to `bots.space_id`
  (0002:143). The store's `connect` scopes from the payload while `find`/`disconnect` scope from
  `TenantContext` (wave-15 #346 F2): a bot X row carrying space A while bot X lives in space B is
  invisible to B yet blocks B's attach via the `bot_id` PK — a permanent ghost row (CONFIRMED shape,
  GUESS that a caller produces it).

### F3 — Dedup: PK is cardinality-idempotent, not recency- or fault-safe (medium)

- `ON CONFLICT (bot_id) DO UPDATE` (store 61-71) gives one row per bot, but the unconditional update
  plus caller timestamp means duplicate attaches are deduped in count, not in version; there is no
  generation/etag and no monotonic guard (CONFIRMED; see wave-15 #346 A).
- Concurrent upserts can raise `40001`/`40P01`, and a conflict row invisible to RLS collapses to
  `Unavailable`; `disconnect` returns `Ok(())` on zero rows, so a cross-tenant delete is silently
  "successful" (CONFIRMED store 98-105; wave-15 #346 F3/D).
- No uniqueness across `(space_id, base_url, agent_name)`: two bots may point at the same endpoint;
  whether that is a bug or a feature is a spec silence (GUESS — do not add uniqueness without asking).

## Concrete hardening (additive only — new objects in a new forward-only migration; never edit 0054)

- **H1 — DB-authoritative, monotonic timestamps.** Add a shared `boma.touch_updated_at()` trigger
  (`BEFORE UPDATE`, `NEW.updated_at = now()`, keep `created_at` immutable) and let it override the
  caller value; ADD COLUMN `version bigint NOT NULL DEFAULT 1` for an app CAS/fencing. *Risk:* the
  RPC's returned `updated_at` changes from app `saved_at` to DB time → DEVIATION in `docs/PARITY.md`;
  default `version = 1` so existing rows migrate cleanly; do not add a CHECK that rejects stale
  retries in v0.4.0 (would change duplicate-connect semantics).
- **H2 — bot/space coherence guard.** Add `UNIQUE (id, space_id)` on `bots` (additive constraint)
  and a composite FK from `bot_external_agents (bot_id, space_id) → bots (id, space_id)`; a
  `BEFORE INSERT OR UPDATE` trigger can instead raise if the row's `space_id <> ` the bot's. *Risk:*
  existing mismatched rows block validation — audit under BYPASSRLS first and repair additively;
  this does NOT replace the auth-layer ownership check (a system GUC can still write any space), so
  keep it as defence-in-depth, not the tenant boundary.
- **H3 — secret-name grammar.** Add `CHECK (secret_name ~ '^[A-Z][A-Z0-9_]{0,63}$')` matching
  0025:15, plus `COMMENT ON COLUMN` documenting "name only, never a value". *Risk:* reject rows with
  legacy names → audit, `NOT VALID` then `VALIDATE`; a real FK to `bot_secrets` is too strict because
  secrets are rotatable/deletable — leave referential liveness to the app (wave-15 #346 R1/E).
- **H4 — base_url shape.** Add `CHECK (octet_length(base_url) <= 2048 AND base_url ~ '^https?://')`
  (AG-UI is HTTP/SSE, ADR-026). *Risk:* GUESS that no non-HTTP scheme is used — audit first; a cap
  sized below real URLs rejects legitimate endpoints, so derive the number from stored data.
- **H5 — RLS/fail-closed regression tests.** REQ-059 tests: pooled connection returned with no GUC
  → zero rows; system GUC set then cleared → cross-tenant read denied; non-system role cannot
  `UPDATE ... SET space_id`; ghost row (bot in B, config in A) is detectable. Extend the ADR-002 CI
  grep to forbid session-level `SET boma.space_id` / `SET boma.system_reason` outside `tenant.rs`.
  *Risk:* test-only; no gate weakened.
- **H6 — migration staging.** Keep DDL in one transaction (db.rs); add H1–H4 as `ADD COLUMN /
  ADD CONSTRAINT ... NOT VALID` then `VALIDATE` in a later step, all under BYPASSRLS/`is_system()`
  because FORCE RLS would otherwise hide violating rows; run `boma migrate` from empty and from 0054.

## Non-goals / risks

- No edit/drop/rename/type-change to 0054, no RLS widening, no new non-tenant bypass, no sweeper or
  trigger-driven "auto-repair" of relationships (hides the defect). H1/H3/H4 change validation for
  hypothetical non-conforming writers → DEVIATION entries + owner approval before shipping.
- Store-layer fixes (session context on `connect`, error classification, honest deletes) belong to
  wave-15 #346; this proposal only adds DB guards they can lean on. Cross-cutting: coordinate H2/H3
  with the `bots` (wave-15 #332) and `bot_secrets` (wave-21 #464) owners.
- CONFIRMED = reads this session only; no tests run (read-only). Re-check GUESS items with a
  testcontainers probe before promoting any proposal.
