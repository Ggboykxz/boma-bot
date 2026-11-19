# 0456 — migrations/0021_pending_tool_calls.sql — reliability (v0.4.0)

Worker #456 · module `migrations/0021_pending_tool_calls.sql` · lens: reliability · READ-ONLY review of
`rebuild-workspace/.worktrees/before-m5` at v0.3.0-era source (workspace says `0.3.0-rc5`). Proposals
only; no code changed. Line refs are against this worktree. Claims tagged CONFIRMED (read this
session) or GUESS (inferred); no tests were run (read-only). Boundaries consulted: `migrations/0022`
(status CHECK), `migrations/0055_signin_requests.sql` (index + RLS precedent), `migrations/0012`,
`migrations/0008`, `spec/01-requirements.md` REQ-016/017 (L129–144), `spec/02-data-model.md` L56–61,
`spec/03-api-contract.md` L46 (`threads.answer` resolves exactly once), `spec/golden/thread-answer.json`.

## What it already does right (keep, do not re-propose)

- `PRIMARY KEY (run_id, call_id)` (L18) gives insert-time dedupe of the same tool call for one run
  (CONFIRMED); three `ON DELETE CASCADE` FKs (L8–11) prevent cross-parent orphans (CONFIRMED).
- `FORCE ROW LEVEL SECURITY` + tenant policy on `space_id` (L26–30) matches the `signin_requests`
  pattern in 0055 L23–29 and fails closed when no space context is set (CONFIRMED).
- `arguments jsonb NOT NULL DEFAULT '{}'::jsonb` (L13) avoids NULL-deserialize crashes (CONFIRMED);
  the status vocabulary is pinned by 0022 L5–7 (CONFIRMED).

## Failure modes → concrete hardening

### F1. Constraint gaps: "applied" without a result, answered before created, non-object jsonb

- `status`, `answered_at`, `result` are independent columns (L14–17): nothing stops
  `status='applied'` with `result IS NULL`, a `result` on a still-`pending` row, or `answered_at <
  created_at` (clock skew / bug). 0022 only constrains the string (CONFIRMED). The Rust `ToolCall`
  deserializer expects an object; a scalar/array `arguments` or `result` can panic a parked run on
  replay, and unbounded jsonb can be a storage/DoS sink (GUESS on caller discipline). P1. In a NEW
  migration `0060_pending_tool_calls_hardening.sql` (never edit 0021/0022), ADD CHECKs:
  `status='pending' <=> answered_at IS NULL`, `status='applied' => result IS NOT NULL`,
  `answered_at >= created_at`, `jsonb_typeof(arguments)='object'`, `jsonb_typeof(result)='object'`,
  and a `pg_column_size` cap. P2. Add `NOT VALID` first, then `VALIDATE CONSTRAINT` in a separate
  step to avoid a long exclusive lock on a live table. Risk: existing dirty rows block VALIDATE —
  add a read-only audit query to the migration PR and repair via an additive backfill, never by
  relaxing the rule.

### F2. Tenant coherence: `space_id` can disagree with the referenced run/thread

- `space_id`, `thread_id`, `run_id` are three independent FKs (L8–11) with no constraint that they
  describe the *same* run. A writer bug (or a `boma.is_system()` path) can park a call whose row says
  space A but whose `run_id`/`thread_id` belong to space B; RLS then lets A read B's `name`,
  `arguments` and `result` (analysis; GUESS that no caller does this today). P3. Add `UNIQUE (id,
  thread_id, space_id)` on `runs` and a single additive composite FK
  `(run_id, thread_id, space_id) REFERENCES runs(id, thread_id, space_id) ON DELETE CASCADE`, keeping
  the old FKs for compatibility. P4. Keep the RLS `WITH CHECK` (L30) as the write-side tenant guard;
  add a test that a cross-tenant `UPDATE ... SET space_id` is rejected for a non-system role.
  Risk: BACKFILL must prove all existing rows are coherent before the FK is VALIDATED; `FORCE RLS`
  means validation/maintenance runs as a role subject to RLS — verify with a BYPASSRLS/is_system
  role and a cross-tenant fixture (GUESS on the validator's scan context).

### F3. Expiry and stale approvals: pending rows live forever and vanish silently

- There is no `expires_at`/TTL: a park nobody answers accumulates indefinitely, and there is no
  partial index to drive a sweeper (CONFIRMED). All three cascades (L8–11) delete the row outright,
  so cancelling/deleting a run or thread makes an approver's cached card return 404 with no refusal
  audit (CONFIRMED). P5. Add `expires_at timestamptz` (nullable, set by the writer) plus a partial
  index `ON pending_tool_calls (expires_at) WHERE status='pending' AND expires_at IS NOT NULL` and a
  bounded periodic sweep that marks stale rows `refused`/`answered_at=now()` instead of deleting.
  P6. Add a cleanup/cancel trigger that transitions `pending -> refused` with `answered_at` before
  a parent cascade, so the refusal is auditable. Risk: auto-refusal of a still-valid approval is a
  behavior change — keep the sweeper opt-in/disabled in v0.4.0 and ship only the column+index as
  additive (a DEVIATION/ADR + docs/STATE.md proposal if behavior is ever enabled).

### F4. Exactly-once under concurrency, and missing access-path indexes

- "Execute exactly once" currently relies on `ExternalEffect.effectKey` (spec 02 L60, REQ-017),
  not on this table: two concurrent `threads.answer` calls can both read `status='pending'` and both
  proceed unless the `UPDATE` is conditional (CONFIRMED from spec; GUESS on store SQL). P7. Require
  `UPDATE ... WHERE status='pending' AND call_id=$2 RETURNING` as the sole transition path, and add a
  DB guard: a BEFORE UPDATE trigger allowing only `pending->applied|refused` and refusing `applied`
  re-transitions, setting `answered_at` atomically. P8. Add `CREATE INDEX CONCURRENTLY
  pending_tool_calls_space_pending ON pending_tool_calls (space_id, status) WHERE status='pending'`
  (mirrors 0055 L21) and `(thread_id)` for card lookups: today the PK `(run_id, call_id)` cannot
  serve "list pending approvals for a space" and a seq-scan grows with history (CONFIRMED).
  Risk: `CONCURRENTLY` cannot run inside the migration transaction — stage it as a separate
  statement/step and verify the framework supports it; triggers change write semantics (test the
  golden `thread-answer.json` double-answer path).

## Explicit non-goals / rejected

- No edits to 0021/0022, no column drops/renames, no type changes: all proposals are additive
  (`ADD COLUMN/CONSTRAINT` with `NOT VALID`, new indexes, optional trigger). Never weaken RLS, add
  a non-tenant bypass, or auto-execute effects from a cleanup job. Out-of-scope features go to
  docs/STATE.md, not into this migration.

## Rollout (v0.4.0) and cross-cutting risks

- Wave A (safe, additive): P2 audit query; P1/P5 columns + `NOT VALID` constraints + partial
  indexes; P4 cross-tenant RLS test; P8 CONCURRENTLY indexes. Wave B (gated): P3 composite FK after
  backfill validation; P7 conditional-update/trigger with the golden double-answer test. Wave C
  (needs ADR + owner approval): P6 cancel-refusal audit, P5 sweeper behavior enabled by default.
- R1. Every new constraint must be added `NOT VALID` then VALIDATE under a BYPASSRLS/is_system role,
  or FORCE RLS can hide violating rows. R2. P3's UNIQUE on `runs` is new schema on a hot table —
  build its index CONCURRENTLY. R3. P6/P5 alter visible approval lifecycle → DEVIATION, ADR,
  docs/PARITY.md and REQ-017-named tests before shipping; default them off in v0.4.0. R4. P1 jsonb
  caps must be sized with real `arguments`/`result` payloads (GUESS on current sizes) to avoid
  refusing legitimate tool calls.
