# Worker 0429 — `migrations/0008_helper_runs.sql` architecture proposal (v0.4.0)

- Worker: 429 · Lens: architecture · Module: `migrations/0008_helper_runs.sql` (11 lines) · Target: v0.4.0.
- Scope: read-only review of this migration plus targeted reads of `migrations/0001_init.sql`,
  `0002_m1.sql:190-213,238-297`, `0003_steering.sql`, `0006_model.sql`, `0007_replies.sql`,
  `0012_attempt.sql`, `0015_run_routine_id.sql`, `0016_run_not_before.sql`,
  `0022_pending_tool_calls_status_check.sql`; `crates/domain/src/run.rs:162-220,246-322,346-392`;
  `crates/app/src/ports.rs:206-207`; `crates/app/src/worker.rs:493-500,576-589,734`;
  `crates/app/src/usecases.rs:7752,7896-7999`; `crates/infra/src/db.rs:1-20,67-74`;
  `crates/infra/src/stores/runs.rs:1-100`; `spec/01-requirements.md:155-159`; `spec/TRACE.md:36`.
  No product code changed; no tests run.
- "Current" claims are **CONFIRMED by source inspection** in this worktree; forward-looking items are
  proposals (**GUESS** until implemented and verified). English per repo disk convention.

## Current structure (CONFIRMED)

- The whole migration is two statements after a 7-line comment: `ALTER TABLE runs ADD COLUMN
  parent_run_id text REFERENCES runs (id) ON DELETE SET NULL` and a partial
  `CREATE INDEX runs_parent_run_id_idx ON runs (parent_run_id) WHERE parent_run_id IS NOT NULL`.
- `runs` itself (0002) is a tenant table: `FORCE ROW LEVEL SECURITY` keyed on `space_id`, with
  `id text PRIMARY KEY`, `space_id/bot_id/thread_id/user_id` FKs `ON DELETE CASCADE`, and an
  idempotency unique index `(thread_id, client_nonce) WHERE client_nonce IS NOT NULL`.
- The column is **overloaded**: domain `Run::is_helper()` reads
  `trigger = HelperResult && parent_run_id.is_some()`, while `is_bot_delegation()` reads
  `trigger = BotMessage && parent_run_id.is_some()` (`domain/src/run.rs:373-391`, REQ-019 and REQ-010).
  The migration header documents only the helper case, so the schema docs already lag the code.
- `runs.trigger` is free text with **no DB check constraint**; migration 0022 set the precedent of
  adding a vocabulary check later for `pending_tool_calls.status`. `RunTrigger::ALL` pins 8 values.
- Writes are centralized in `stores/runs.rs::create` (one INSERT over 15 columns, tenant-wrapped), but
  `parent_run_id` is re-selected in 8+ places (`rows.rs` mapping, retry at `usecases.rs:10031`, bot
  delegation at `:8193`), so a new column touches every one of those surfaces plus the memory adapter.
- Migrations are forward-only and embedded via `sqlx::migrate!("../../migrations")` (`db.rs`); SQLx
  records checksums, so **0008 must never be edited** — v0.4.0 changes belong in a NEW numbered migration.

## Guiding principle for v0.4.0

Keep 0008 immutable. Land additive, nullable/defaulted DDL in a new migration (e.g.
`0060_helper_runs_hardening.sql`) that expresses the parent/child contract in the schema instead of
only in Rust, while preserving REQ-019's "structured result, parent continues, no leaked steps"
behavior and REQ-010's shared column. Every statement must be `ADD ...` / `CREATE ...` /
`COMMENT ON ...`; no `DROP`, no type change, no backfill that can fail on an existing row.

## Concrete upgrades (5), with risks

1. **Pin the trigger vocabulary with an additive check constraint.** `ALTER TABLE runs ADD CONSTRAINT
   runs_trigger_check CHECK (trigger IN ('user','routine','webhook','bot_message','tickets','created',
   'helper_result','retry'))`, mirroring 0022 and `RunTrigger::ALL`. This is the precondition for any
   schema-level helper rule. *Risks:* any legacy/unknown trigger value makes the migration fail — audit
   with `SELECT DISTINCT trigger`, then use a reviewed backfill or `ADD CONSTRAINT ... NOT VALID`
   followed by `VALIDATE CONSTRAINT`; pinning the vocabulary means a future trigger needs a new
   migration (acceptable under forward-only discipline). No wire or DTO change.

2. **Add an explicit relation-kind discriminator for the shared `parent_run_id`.** New nullable
   `parent_relation_kind text` with `CHECK (parent_relation_kind IN ('helper','bot_delegation'))` and a
   backfill derived from `trigger`; add `CHECK ((parent_run_id IS NULL) = (parent_relation_kind IS NULL))`
   so a child must declare its kind. This removes Rust-only inference in `is_helper()` /
   `is_bot_delegation()` and gives the schema one authoritative name per relationship. *Risks:* two
   sources of truth (trigger and kind) can drift — the app must write both in one statement and a test
   must assert they agree; the column must be added to `NewRun`/`Run`, every `stores/runs.rs`
   INSERT/SELECT, `rows::RunRow`, the memory adapter and testkit fixtures (11+ sites); keep it out of
   serialized `Run` unless a DEVIATION is recorded.

3. **Bound delegation depth and forbid self-reference at the database layer.** Add
   `parent_depth integer NOT NULL DEFAULT 0 CHECK (parent_depth >= 0 AND parent_depth <= 4)` plus
   `CHECK (parent_run_id IS NULL OR parent_run_id <> id)`; optionally a small `BEFORE INSERT OR UPDATE`
   trigger that walks the parent chain (recursive CTE) to reject cycles. `worker.rs:493` already notes
   the loop is bounded in code; this makes the bound durable. *Risks:* trigger functions need migration
   privileges and unit tests; the recursive walk adds write cost per child insert; the cap is a policy
   choice — REQ-019 is SHOULD, so clamping a legitimate deep chain is a parity/devex trade-off to
   record; backfill `parent_depth` for existing rows before validating.

4. **Add a query-shaped composite index (additive).** Create
   `runs_parent_status_idx ON runs (parent_run_id, status) WHERE parent_run_id IS NOT NULL` (consider
   leading `space_id` if reconciliation is tenant-scoped), justified by the worker's "find active
   children of this parent" access pattern; keep the old index until query plans are measured.
   *Risks:* two overlapping partial indexes add write amplification — retiring the old one is
   non-additive and must be a separate, agreed migration; confirm the exact predicates in
   `stores/runs.rs` and reconciliation first, or the new index is dead weight (GUESS until `EXPLAIN`
   evidence exists).

5. **Give the structured hand-off a first-class, additive home.** Add a thin `run_helper_results`
   table (`run_id text PRIMARY KEY REFERENCES runs (id) ON DELETE CASCADE`, `parent_run_id text
   REFERENCES runs (id) ON DELETE SET NULL`, `outcome jsonb NOT NULL`, `completed_at timestamptz NOT
   NULL DEFAULT now()`) for the structured result REQ-019 asks for, plus
   `COMMENT ON COLUMN runs.parent_run_id` documenting both helper and bot-delegation use. The existing
   `role=tool` message stays the delivery mechanism. *Risks:* the table becomes a second source of
   truth next to the tool message — define which wins on replay and keep `complete_helper_run` writing
   both in one transaction; the `jsonb` shape is a schema-visible data contract (validate in Rust,
   record any external exposure as a DEVIATION); the new table inherits `runs` RLS concerns, so add the
   tenant policy from birth (REQ-043) rather than retrofitting.

## Sequencing and cross-cutting risks

- Order: 1 → 2 → 5 → 3 → 4. Constraint/discriminator first (they make later rules expressible),
  bookkeeping next, depth trigger after, index last once query shapes are confirmed.
- Tenant invariant: a child's parent should share `space_id`/`thread_id`. A composite FK cannot use
  `ON DELETE SET NULL` on the NOT NULL tenant columns, so enforce same-tenant/same-thread via a
  trigger or keep it app-owned (`delegate_helper_run` already reuses the parent context) — decide in
  an ADR, do not silently assume the plain FK covers it (RI checks on the referenced table bypass RLS).
- Keep both adapters contract-equivalent: every new column/table must appear in the memory twin and
  `conformance/golden_world.rs` fixtures so REQ-named tests stay meaningful.
- Verification gate per step: `cargo fmt --check`, `clippy -D warnings`, unit + `tests/` integration,
  golden and OpenAPI contract suites; if offline SQLx metadata is introduced (none is committed today
  — GUESS), regenerate it in the same change; any externally visible change goes to `docs/PARITY.md`.

## Evidence

CONFIRMED — read this session: `migrations/0008_helper_runs.sql:1-11`, `0001_init.sql:1-18`,
`0002_m1.sql:190-213,238-297`, `0003_steering.sql`, `0006_model.sql`, `0007_replies.sql`,
`0012_attempt.sql`, `0015_run_routine_id.sql`, `0016_run_not_before.sql`,
`0022_pending_tool_calls_status_check.sql`; `crates/domain/src/run.rs:162-220,246-322,346-392`;
`crates/app/src/ports.rs:206-207`; `crates/app/src/worker.rs:493-500,576-589,734`;
`crates/app/src/usecases.rs:7752,7896-7999`; `crates/infra/src/db.rs:1-20,67-74`;
`crates/infra/src/stores/runs.rs:1-100`; `spec/01-requirements.md:155-159`; `spec/TRACE.md:36`.
GUESS — exact reconciliation query predicates behind item 4, whether a depth cap conflicts with any
golden fixture, and whether any DTO currently serializes `parent_run_id` in a way item 2 would disturb
(verify before implementation).
