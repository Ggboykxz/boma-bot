# 0424 — `migrations/0005_tools.sql` reliability proposal (v0.4.0)

Worker #424 · module `migrations/0005_tools.sql` (34 lines) · lens: reliability (constraints, RLS
durability under FORCE, dedup) · READ-ONLY review of `rebuild-workspace/.worktrees/before-m5`
(v0.3.0-rc5). Spec anchor: REQ-016 (per-run tool catalogue, MUST); REQ-043 / ADR-002 (RLS tenant
isolation); AGENTS.md (versioned, forward-only, additive-only migrations).

"Current behavior" is CONFIRMED by reading this file, `migrations/0002_m1.sql:20-62,247-280`,
`migrations/0004_cards.sql`, `crates/infra/src/stores/tools.rs`, `crates/infra/src/stores/rows.rs:475-519`,
`crates/domain/src/card.rs:143-174`, `crates/domain/src/bot.rs`, `crates/infra/src/db.rs` this session.
Failure modes and hardening are GUESS (no migration run or test executed). Proposal only; additive
only — all hardening goes in a NEW numbered migration (0060+, 0059 is current max); 0005 is never
edited. Complements worker #408's store proposal (0408); this note owns the DDL.

## 1. Current behavior (CONFIRMED)

- `tool_installs` (:6-14): `space_id text NOT NULL REFERENCES spaces(id) ON DELETE CASCADE`,
  `tool_key text NOT NULL`, `description text NOT NULL`, `kind text NOT NULL`,
  `enabled boolean NOT NULL DEFAULT true`, `created_at timestamptz NOT NULL`,
  `PRIMARY KEY (space_id, tool_key)`. No CHECK, no other index, no DEFAULT on `created_at`.
- `bots.enabled_tools text[] NOT NULL DEFAULT '{}'` (:17), no GIN index, no element/duplicate CHECK;
  empty = "every enabled install" (REQ-016, `bot.rs`, `tools.rs:74`).
- DO block (:19-34) sets `ENABLE` + `FORCE ROW LEVEL SECURITY` (:24-25) and policy
  `tool_installs_tenant_isolation` (:27-31):
  `USING (space_id = boma.current_space_id() OR boma.is_system()) WITH CHECK (...)`. Helpers live in
  `0002_m1.sql:27-46` and read transaction-scoped `boma.space_id` / `boma.system_reason`.
- Migrations embed via `sqlx::migrate!` (`db.rs`), forward-only, each file in its own transaction;
  editing 0005 in place would break checksums on already-migrated databases.
- The store only reads (`WHERE space_id=$1 AND enabled ORDER BY tool_key` + in-memory grant filter);
  the `tool_installs` writer is outside this worktree (a seam covered by 0408 D5).

## 2. Failure modes

### 2.1 Constraints (CONFIRMED shape, GUESS impact)
- `kind` is free text. `rows.rs:515` uses `ToolKind::parse(...)?`; one bad value (`"Builtin"`,
  `"builtin "`, a future `"http"`) fails the *entire* catalogue with permanent `UnknownEventType` —
  no run starts because of one install (0408 V1). The accepted set is exactly
  `builtin|connector|mcp|skill|memory` (`card.rs:162-172`), unenforced in the DB.
- No length bound on `tool_key`/`description`; an oversized description inflates every prompt and
  bypasses boundary validation (0408 V3).
- `created_at NOT NULL` with no DEFAULT burdens the out-of-file writer; adding DB `now()` later
  without a decision mixes two clocks.
- `enabled_tools` has no domain constraint: duplicates/unknown/wrong-case keys are possible; unknown
  keys silently shrink the catalogue (0408 S3) with no DB-level signal.

### 2.2 RLS (CONFIRMED mechanism, GUESS likelihood)
- FORCE RLS scopes even the owner. Any backfill UPDATE without a system reason affects **zero** rows
  yet reports success — a quiet integrity failure, the main trap for the CHECKs below.
- A table rebuild (`CREATE TABLE ... LIKE`, dump/reload, rename dance) to add a constraint drops
  FORCE RLS, the policy and grants; nothing here asserts they survive (same exposure as 0420).
- `CREATE POLICY` has no `IF NOT EXISTS`; a manual partial apply fails on a duplicate name instead of
  converging (unlikely under forward-only sqlx — GUESS).
- `ON DELETE CASCADE` from `spaces` is policy-checked too: a member deleting their own space works
  (children share `space_id`); a system-path delete needs `boma.is_system()`. A stray mismatched
  `boma.space_id` would leave hidden orphan installs rather than a loud error.

### 2.3 Dedup (CONFIRMED)
- PK `(space_id, tool_key)` already forbids duplicate installs and supplies the index for
  `WHERE space_id=$1 ... ORDER BY tool_key`; `ON CONFLICT (space_id, tool_key)` is the correct upsert.
  No second index — write cost only.
- `enabled_tools` element duplicates are not deduplicated; `tools.rs:74` `any()` ignores them but the
  filter is O(grant × installs) (0408 D4).
- Cross-source key collisions (built-in/install/MCP/connector) are resolved downstream with no
  uniqueness check (0408 D3); a CHECK cannot fix that. The out-of-file writer has no conflict
  fingerprint — `DO NOTHING` on a changed `description`/`kind` keeps a stale row (0408 D5).

## 3. Concrete hardening for v0.4.0 (additive, GUESS)

1. **New migration `0060_tools_hardening.sql`** (verify next free number); never edit 0005.
2. **CHECK on `kind`**: `ADD CONSTRAINT ... CHECK (kind IN ('builtin','connector','mcp','skill','memory'))
   NOT VALID`, then `VALIDATE CONSTRAINT` in the same file (short lock). Pre-flight count offenders
   and backfill/quarantine first. *Risk:* an unknown legacy kind aborts VALIDATE; the vocabulary must
   track `ToolKind::parse` — a new family needs a follow-up migration. Converts a per-run failure into
   a write-time failure.
3. **Length CHECKs**: `char_length(tool_key) BETWEEN 1 AND 128`, `char_length(description) <= 4096`,
   also NOT VALID + VALIDATE. *Risk:* bounds must match Rust validation; oversized legacy rows need
   quarantine; caps the prompt-inflation path.
4. **RLS invariant test (real PG)**: assert `pg_policies` still has `tool_installs_tenant_isolation`
   for `tool_installs` and `relrowsecurity`/`relforcerowsecurity` are true after hardening. *Risk:*
   the name is generated in 0005's DO block; a rename there must update the test.
5. **System-reason backfill rule**: any accompanying data fix runs under a system reason, in batches,
   with an expected-row-count assertion — never assume an UPDATE succeeded under FORCE RLS.
6. **Comments (zero risk)**: `COMMENT ON COLUMN bots.enabled_tools` (empty = all, duplicate/unknown
   keys ignored) and `COMMENT ON COLUMN tool_installs.kind` (closed vocabulary), to stop writers
   reintroducing the gap.
7. **Deferred — do not ship:** a dedup trigger on `enabled_tools`, an enum type replacing `text`
   (type rewrite touches grants), or a GIN array index. Write cost/rebuild risk outweighs the gain;
   handle dedup in the writer/store seam (0408 P0/P1).

## 4. Risks / non-goals / gates

- Additive only: no edits to 0005, no drops/renames/rebuilds/policy changes; keep the PK and FK CASCADE.
- 2/3 are write-time-visible (a previously accepted bad row is now rejected): record in
  `docs/STATE.md` + `docs/PARITY.md`, wait for owner sign-off, ADR if a DEVIATION. Tests carry REQ-016
  in the name (`trace_check.py` counts declarations, not comments).
- CHECKs fail on existing bad rows — pre-flight counts + quarantine are mandatory; low-traffic window.
  `unsafe_code` forbidden; clippy `-D warnings`; no `unwrap`/`expect`/`panic` outside tests; never move
  `boma.space_id` out of the tenant module (ADR-002).
- Writer-side dedup/fingerprint (0408 D5) and cross-source collisions (0408 D3) are seams, not DDL —
  track in 0408. All likelihood/impact stays GUESS until a migration run and the tests below pass.

## 5. Suggested tests (integration, real PG)

- `req_016_tool_installs_kind_check_rejects_unknown_kind`
- `req_016_tool_installs_length_check_rejects_oversize_description`
- `req_016_tool_installs_rls_policy_survives_hardening`
- `req_016_owner_update_without_system_reason_touches_no_rows`
- `req_016_tool_installs_pk_forbids_duplicate_keys`
- `req_016_enabled_tools_duplicate_keys_are_ignored`

## Bottom line

The DDL gets the two structural guarantees right — a PK making duplicate installs impossible and a
FORCE-RLS policy scoped to `space_id` — but leaves `kind`/`tool_key`/`description` unconstrained (one
bad row can poison every run's catalogue) and asserts nothing about the policy surviving a rebuild.
The cheap reversible wins (NOT VALID + VALIDATE CHECKs, policy-invariant test, column comments) are
additive and safe once bad rows are quarantined; the remaining gaps — array dedup, conflict
fingerprints, cross-source collisions — live in the writer/store seam (0408), not in this migration.
