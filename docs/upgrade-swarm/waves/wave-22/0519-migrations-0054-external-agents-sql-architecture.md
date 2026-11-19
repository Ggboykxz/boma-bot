# 0519 — migrations/0054_external_agents.sql — architecture (v0.4.0)

Worker #519 · module `migrations/0054_external_agents.sql` · lens: architecture · READ-ONLY review of
`rebuild-workspace/.worktrees/before-m5` at v0.3.0-era source. Proposals only; no code changed.
Line refs are against this worktree. Claims tagged CONFIRMED (read this session) or GUESS (inferred);
no tests were run (read-only). Boundaries consulted: `migrations/0054` (whole file),
`crates/infra/src/stores/external_agents.rs` (L28–49 row mapping, L60–81 upsert, L98 delete,
L120–131 find), `crates/domain/src/external.rs` (L7–38), `crates/app/src/usecases.rs`
(L4776–4793 secret-name rule, L9506–9530 base-URL rule, L9538–9582 connect),
`crates/app/src/agui_provider.rs` (L23, L167–168 AG-UI binding), `spec/01-requirements.md` L486–498
(REQ-059), peers `0059_acp_agents.sql`, `0025_bot_secrets.sql` (L7–17 UNIQUE),
`0034_connector_installs.sql` (L13 status switch), `0002_m1.sql` L141–143 (`bots.id` PK,
`bots.space_id`), and the wave-22 migration-reliability doc (NOT VALID → VALIDATE, BYPASSRLS,
no edits to the original migration).

## What it already does right (keep, do not re-propose)

- One row per bot via `bot_id PRIMARY KEY` + `ON DELETE CASCADE` from both `bots` and `spaces`
  (0054 L5–6) — absence of a row means internal, matching the REQ-059 model (CONFIRMED).
- `ENABLE` + `FORCE ROW LEVEL SECURITY` with a symmetric `USING`/`WITH CHECK` tenant predicate
  (L19–25) fails closed and blocks cross-tenant writes; keep it byte-identical (CONFIRMED).
- Non-empty `base_url` and non-blank-when-present `secret_name` CHECKs (L12–15), server-defaulted
  timestamps (L10–11), and a `space_id` index (L17) for the tenant-scoped read (CONFIRMED).
- No bearer value is ever stored: `secret_name` is a vault reference, and the app resolves it at
  connect time (`usecases.rs` L9560–9567) before writing (CONFIRMED).

## Structural gaps → concrete upgrades (all additive, forward-only)

### U1. The RLS trust anchor (`space_id`) is not tied to the bot's real tenant

- `bot_id` and `space_id` are independent FKs (0054 L5–6); nothing in the DB says the row's
  `space_id` equals `bots.space_id`. The upsert's conflict target is `bot_id` alone and it *sets*
  `space_id = EXCLUDED.space_id` (store L63–64): a system-context writer can silently re-home a
  connection, and the RLS predicate then trusts the moved row (CONFIRMED on SQL; GUESS that any
  caller does this today). `connector_installs` is coherent by construction (PK `(space_id,
  connector_id)`); this table is not.
- P1. In a NEW migration add `bots_id_space_id_key UNIQUE (id, space_id)` (redundant over the PK
  but a legal FK target; `0002` L142–143), then `ALTER TABLE bot_external_agents ADD CONSTRAINT
  ... FOREIGN KEY (bot_id, space_id) REFERENCES bots (id, space_id) ON DELETE CASCADE NOT VALID`,
  audit for mismatches, then `VALIDATE` under BYPASSRLS/`is_system()`. P2. Add a `BEFORE UPDATE`
  trigger refusing `NEW.space_id <> OLD.space_id` so tenant binding is immutable (reconnect
  instead). *Risk:* existing mismatched rows block VALIDATE — repair additively, never relax;
  the FK needs an extra constraint on `bots` (still additive); immutability makes bot moves
  between spaces require disconnect/reconnect (correct, but a behavior change to document).

### U2. `secret_name` is a soft reference that can dangle

- The comment promises a pointer into the space's bot-secret vault (0054 L2–3), but there is no FK;
  `bot_secrets` already exposes `UNIQUE (space_id, bot_id, name)` (`0025` L16). A later
  `bot_secret_forget` (`usecases.rs` L4858–4872) can remove the target after a successful connect,
  leaving the row pointing at nothing; REQ-059 then fails at run time with a legible reason, but
  the bad state was representable (CONFIRMED on schema; GUESS on run-time frequency).
- P3. Add a composite FK `(space_id, bot_id, secret_name) REFERENCES bot_secrets (space_id,
  bot_id, name)` with `MATCH SIMPLE` (NULL `secret_name` skips the check — correct for unauthenticated
  executors) and `ON DELETE NO ACTION`, `NOT VALID` → audit → `VALIDATE`. P4. Require the app to
  clear `secret_name` before forgetting the secret (app/ADR change, outside this module's writes).
  *Risk:* `ON DELETE SET NULL` would null the NOT NULL `space_id`/`bot_id` too, so it is not usable —
  do not propose it; `NO ACTION` means forget fails until the connection is updated (a deliberate,
  legible failure), which must be tested before shipping.

### U3. The AG-UI assumption is implicit, contradicting the protocol-agnostic promise

- REQ-059 says "a documented agent protocol" (`01-requirements.md` L487), yet the only implementation
  is AG-UI (`domain/external.rs` L12; `agui_provider.rs` L167–168) and the table has no discriminator.
  `0059_acp_agents.sql` already shows a second executor family with its own table (CONFIRMED).
- P5. Add `protocol text NOT NULL DEFAULT 'ag-ui' CONSTRAINT bot_external_agents_protocol_check
  CHECK (protocol IN ('ag-ui'))` — a single-valued enum today, extensible by adding a value later
  without rewriting the table. *Risk:* the default keeps existing rows and v0.4.0 behavior
  unchanged; exposing/choosing a second protocol is a DEVIATION + ADR + owner approval, not a
  schema-only change. Do not add speculative A2A/ACP values before an implementation exists.

### U4. No lifecycle or legible-failure state

- Unlike `connector_installs.status` (`0034` L13), this row cannot be paused or degraded without
  deletion, and REQ-059's "legible reason" (`01-requirements.md` L493–494) lives only in a run
  error. Operators cannot disable a moved/broken endpoint without losing its config (CONFIRMED).
- P6. Additively add `status text NOT NULL DEFAULT 'enabled' CHECK (status IN ('enabled',
  'disabled'))`, `failure_count integer NOT NULL DEFAULT 0 CHECK (failure_count >= 0)`,
  `last_error text`, `last_succeeded_at timestamptz` (all nullable/defaulted). P7. Have the worker
  skip `disabled` rows and stamp failure state (app change, gated). *Risk:* status filtering is an
  externally visible behavior change → DEVIATION/ADR; keep the default `'enabled'` so v0.4.0 is a
  pure schema addition. Cap `last_error` length to avoid an unbounded log sink.

### U5. Field shapes are only guarded in the app, not at the boundary

- `base_url` has just a non-blank CHECK (L12); https/loopback parsing lives in
  `usecases.rs` L9506–9530. `agent_name` and `secret_name` have no length cap, and `secret_name`
  does not mirror the `^[A-Z][A-Z0-9_]{0,63}$` rule enforced for vault names (`0025` L15,
  `usecases.rs` L4776–4793) (CONFIRMED). A buggy/system writer can store a malformed value the
  read path returns verbatim (GUESS on downstream tolerance).
- P8. In a NEW migration ADD `CHECK (secret_name IS NULL OR secret_name ~ '^[A-Z][A-Z0-9_]{0,63}$')`
  plus `octet_length` caps on `base_url`/`agent_name`/`secret_name` (e.g. 2048/256/64), each `NOT
  VALID` → audit → `VALIDATE` under BYPASSRLS. P9. Reuse `validate_secret_name` for any new writer
  path. *Risk:* do **not** encode the URL scheme as a SQL regex — `reqwest::Url` already handles
  ports/IPv6/paths and a regex would reject legitimate endpoints; dirty rows block VALIDATE, so
  gate on the audit and repair; size caps must come from real payloads (GUESS on current sizes).

## Rollout (v0.4.0) and cross-cutting risks

- Wave A (safe, additive): P3/P5/P6/P8 schema additions with harmless defaults and `NOT VALID`
  constraints; P1's audit query first, index/PK additions only; P2/P7 tests appended to the existing
  REQ-059 store/RPC tests. Wave B (gated): U1 FK + trigger after backfill proof; U2 FK after
  app-clears-secret flow; U5 VALIDATE after the audit. Wave C (owner approval): protocol choice,
  status-based routing, any app change.
- R1. All validation/backfills must run under BYPASSRLS/`is_system()` or FORCE RLS hides violating
  rows (wave-22 convention). R2. Never edit 0054 — every item is a new forward-only migration; no
  drops, renames, type changes, or RLS widening. R3. Coordinate U2 with the bot-secrets owner
  (`0025`) and U3/U4 with the ACP (`0059`) and AG-UI provider owners so the one-row-per-bot model and
  the "absence means internal" contract stay intact; cross-tenant and reconnect paths need
  REQ-059-named tests before shipping.
