# Worker #529 — `migrations/0059_acp_agents.sql` — architecture (v0.4.0)

- Worker #529 · lens: architecture (structure, boundaries, coupling, debt) · target v0.4.0.
  Status: PROPOSAL, READ-ONLY; no code/test changed. CONFIRMED = read this session; GUESS = inferred.

## 0. Module and evidence read

- Module: `0059_acp_agents.sql` (21 lines, REQ-061) — the attachment row making a bot a local ACP
  teammate. CONFIRMED context: `spec/01-requirements.md:620-634`, `spec/03-api-contract.md:59-61`,
  `crates/domain/src/acp.rs:1-36`, `crates/infra/src/stores/acp_agents.rs:49-124`,
  `crates/app/src/usecases.rs:9603-9666`, `crates/app/src/acp_provider.rs:27-47` (registry:
  `opencode` only), `crates/app/src/worker.rs:596-630` (`provider_for` precedence),
  `crates/api/src/rpc.rs:4030-4098`, sibling `0054_external_agents.sql:1-25`,
  `docs/STATE.md:210-215,383-434` (M23, issue #18), peer doc #487. GUESS: ADR-030 and
  `BOMA_ACP_KEEP_WORKSPACES` implementation not re-read.

## 1. Current structure (CONFIRMED)

- One table `bot_acp_agents` (:4-11): `bot_id text PK → bots(id) ON DELETE CASCADE`, `space_id text
  NOT NULL → spaces(id) ON DELETE CASCADE`, `executor text NOT NULL` + non-empty `btrim` CHECK (:10),
  `created_at`/`updated_at timestamptz DEFAULT now()`. One index `bot_acp_agents_space(space_id)`
  (:13). Static RLS — `ENABLE` + `FORCE` + one `FOR ALL` policy with symmetric `USING = WITH CHECK
  = (space_id = current_space_id() OR is_system())` (:15-21), same shape as 0054/0056 (#487).
- Boundaries: one row per bot; absence = internal model; the executor string is validated only at
  the app boundary against `acp_provider::executors()` (usecases:9615-9622, Err `Rule`); domain/DB
  hold no credential (acp.rs:3-6). Reads are tenant-scoped (`WHERE bot_id=$1 AND space_id=$2`,
  store :108-119) so a foreign-space row reads as `None`.

## 2. Architecture traits worth addressing (CONFIRMED unless tagged)

1. **Registry ownership split.** The allow-list lives in compiled Rust (`executors()`, one row),
   the DB stores unconstrained `text` (only `btrim` non-empty). A raw INSERT can name an unknown
   executor; the worker then logs and silently falls back to the internal model (worker:615) — the
   storage layer cannot express "known executor". STATE #18 records the one-row registry gap.
2. **Two clocks / upsert identity.** `attach` binds a caller RFC3339 `saved_at` to both timestamps
   on insert and updates only `executor, updated_at` on conflict (store :54-61); the `DEFAULT now()`
   is dead on the normal path. `created_at` is preserved, `updated_at` moves, `space_id` never does
   (PK is bot-scoped) — same dual-clock debt as 0038 (#487 §1.4).
3. **Ephemeral detection.** `bots.detectedExecutors` probes PATH with a 2 s timeout and returns a
   list but persists nothing (rpc:4030-4044, spec :60-61); there is no last-seen, version, or health
   column, so the schema cannot answer "was this executor present / when was it last used".
4. **Precedence is app-only.** `provider_for` prefers ACP over AG-UI (worker:598-630), yet a bot
   may hold a row in **both** `bot_acp_agents` and `bot_external_agents` (0054) at once; nothing in
   either schema forbids or records the dual attachment.
5. **No audit linkage in the schema.** attach/detach are `tracing`-logged (usecases:9645) but not
   tied to `audit_events` (0030) here; that boundary sits in the app layer.

## 3. Guiding principle for v0.4.0

0059 is immutable: sqlx verifies a checksum, so editing its bytes breaks `boma migrate` on every
deployed DB. Every change ships as a NEW forward-only additive file (e.g. `0060_acp_executors.sql`,
`0061_acp_agents_columns.sql`) that only adds tables, nullable/defaulted columns, constraints
`NOT VALID` → `VALIDATE`, indexes and read-only views — never DROP/RENAME/ALTER TYPE/DELETE/rewrite.
Keep table/column names, the `btrim` CHECK, FK cascade actions and the RLS predicate byte-compatible.
Anything externally visible (new DTO field, changed fallback) is a DEVIATION for `docs/PARITY.md`
with owner approval before shipping (AGENTS.md).

## 4. Concrete upgrades (5), each with a risk

**U1 — Freeze 0059 and add an additive shape/RLS canary (do first).** Never edit 0059; add a
fresh-PG test asserting the exact shape (one table, PK+two FKs and cascade actions, the `btrim`
CHECK, one index, `ENABLE`+`FORCE`+`FOR ALL` policy with `USING = WITH CHECK`), plus a header
recording the boundary: attachment metadata only, no credential and no workspace state.
*Risk:* checksum immutability forbids edits; a semantic assertion fails intentionally when a new
column lands. *Mitigation:* one shared expected-shape constant, updated in the same commit as any
new column, and regenerate `.sqlx` offline metadata.

**U2 — Give "known executor" a DB-readable reference, additively.** New migration: `CREATE TABLE
acp_executors (id text PRIMARY KEY, command text NOT NULL)` seeded with the compiled ids, then
`ALTER TABLE bot_acp_agents ADD CONSTRAINT bot_acp_agents_executor_fk FOREIGN KEY (executor)
REFERENCES acp_executors(id) NOT VALID`, audit, then `VALIDATE`. Keep the existing `btrim` CHECK;
declare the table an introspection mirror, not the validator (the compiled registry still gates
`connectAcp`). *Risk:* two owners (Rust + lookup) drift when a provider is added/removed, and a
legacy unknown-id row blocks VALIDATE; deleting a row later is a data change, not additive.
*Mitigation:* keep the registry constant and seed in one commit, gate VALIDATE on an audit, and
never INSERT/DELETE lookup rows from a runtime path in v0.4.0 (GUESS: coordinate with issue #18).

**U3 — Add optional health/usage columns, defaulted so behavior is unchanged.** New migration:
`ADD COLUMN enabled BOOLEAN NOT NULL DEFAULT true`, `ADD COLUMN last_used_at timestamptz`,
`ADD COLUMN executor_version text` (all additive; the two nullable ones never read by today's
queries), and a read-only view `acp_agents_status(space_id, bot_id, executor, enabled, last_used_at)`
diagnosing attachments per tenant. *Risk:* columns stay empty until a writer is added, and the view
is a second source of truth that can drift; surfacing any of it in a DTO is a DEVIATION.
*Mitigation:* diagnostic-only in v0.4.0, no write path changed, keep `is_system()` and FORCE intact,
and run any backfill under BYPASSRLS/is_system (FORCE otherwise filters violating rows).

**U4 — Add format/length guards on `executor` without touching 0059.** New migration: `ADD
CONSTRAINT bot_acp_agents_executor_len CHECK (length(executor) <= 128) NOT VALID`, audit, VALIDATE;
consider a `^[a-z0-9][a-z0-9_-]*$` regex only after confirming every registry id fits.
*Risk:* a regex duplicates Rust's id format and can drift; a dirty legacy value blocks VALIDATE;
too-small a cap refuses a legitimate future executor. *Mitigation:* derive the rule from one shared
constant, size the cap from real ids, audit first, never relax to pass.

**U5 — Surface ACP-vs-AG-UI precedence structurally (coordinate first).** New migration adds a
read-only view `bot_effective_provider(space_id, bot_id, kind, target)` joining `bot_acp_agents` and
`bot_external_agents` with the worker rule (ACP wins), plus `CREATE INDEX CONCURRENTLY
bot_acp_agents_space_bot ON bot_acp_agents (space_id, bot_id)` if a "list ACP bots in space" path
appears. *Risk:* the view encodes app precedence in SQL and silently diverges if `provider_for`
changes, dual attachment remains possible (the view documents, not prevents, it), and it touches
0054's table owned by another brief. *Mitigation:* diagnostic-only, add a test comparing the view to
`provider_for` for a seeded dual-attached bot, stage CONCURRENTLY outside the migration transaction,
and get the 0054 owner's sign-off.

## 5. Cross-cutting sequencing / risks

- Order **U1 → U2 → U3 → U4 → U5**: freeze/test, then reference, columns, guards, read model. Each
  is a separate additive file; U3/U4/U5 need ADR/parity approval before any visible effect.
- This brief owns only migration structure. Registry expansion, `BOMA_ACP_*` knobs, workspace GC,
  permission-poll timing and resolver error typing stay with their owners (STATE M23 "NOT in scope").
- Regenerate `.sqlx`; CI runs `boma migrate` on fresh PG and replays the REQ-061-named tests. Keep
  the RLS predicate, `is_system()` escape hatch and FK cascade actions byte-identical; a new
  nullable column must not change a public DTO. Mark every claim CONFIRMED/GUESS (AGENTS.md).

## 6. Out of scope

Editing 0059; DROP/RENAME/type changes; weakening RLS or adding a non-tenant bypass; persisting
`detectedExecutors` results; forbidding dual attachment; registry expansion; audit-trigger writes —
each forbidden by the checksum invariant, a DEVIATION, or another brief. Status: proposal only;
READ-ONLY worker, no code changed and no tests run. Line budget: <120.
