# Worker #422 — `migrations/0004_cards.sql` — LENS: reliability — v0.4.0 proposal

- **Status:** PROPOSAL (read-only review; no source changed). CONFIRMED = verified this session by reading `migrations/0004_cards.sql`, `migrations/0001_init.sql`, `migrations/0002_m1.sql`, `migrations/0021_pending_tool_calls.sql`, `migrations/0022_pending_tool_calls_status_check.sql`, `crates/infra/src/stores/cards.rs`, `spec/01-requirements.md` (REQ-007/017), `spec/02-data-model.md`, `spec/05-known-defects.md` in this worktree; GUESS = needs an oracle/test run.
- **Scope:** the two M2 tables (`cards`, `external_effects`), the partial dedup index, and the tenant-RLS `DO` block. **Additive only:** every hardening below is a new object or a relaxing default in a *new* forward-only migration; nothing here drops or rewrites 0004 in place.

## Summary

The schema is small and the two idempotency keys are well chosen, but it relies on the writer for almost every invariant: `status`, `kind` and effect-key vocabulary are free text, `created_at` has no DB clock, `answered_at`/`answer` consistency is unenforced, and RLS binds only `space_id` (never the referenced thread/run space). A corrupt or raced writer can therefore park a run on an unanswerable row, mis-bind a card across tenants, or erase the exactly-once ledger on run deletion. The fix set is additive constraints, composite FKs, covering indexes and a small archive trigger.

## Failure modes

### Constraints (C)

- **C1 (HIGH, CONFIRMED):** `cards.status` is `text NOT NULL DEFAULT 'pending'` with no CHECK (0004:13). Compare `pending_tool_calls`, pinned later by migration 0022. A typo status is accepted on write but `rows::card` only accepts `pending|answered|declined` (cards.rs:47,119,149), so the row becomes permanently unreadable/unanswerable while the run waits. No DB-level vocabulary means no fail-fast.
- **C2 (HIGH, CONFIRMED):** `cards.kind` is free text (0004:12) although the writer emits exactly `question|approval|purchase|takeover` (cards.rs:77–82). An unknown kind silently escapes any kind-scoped policy/index and still occupies the dedup slot.
- **C3 (HIGH, CONFIRMED):** `cards_thread_run_key` is `UNIQUE (thread_id, run_id) WHERE run_id IS NOT NULL AND status='pending'` (0004:19–21). Two failures: (a) any card with `run_id = NULL` is unguarded, so a run-less approval can be inserted twice; (b) the key has no call/kind component, yet `pending_tool_calls` is keyed by `(run_id, call_id)` (0021:18) — a run with two approval-requiring calls can hold only one pending card, the second `INSERT` maps to `Conflict` and its effect is never recorded. Fixing (b) requires *replacing* this index — **not additive**, needs an ADR (see Risks).
- **C4 (MED, CONFIRMED):** `status`/`answered_at`/`answer` are independent. `answer()` sets all three atomically (cards.rs:133–135), but a partial or buggy writer can produce `answered` with NULL `answered_at` or a `pending` row carrying an answer — inconsistent state that `rows::card` cannot repair.
- **C5 (MED, CONFIRMED):** `external_effects.kind` and `effect_key` are free text (0004:27–28) while keys are conventions (`approve:<card_id>`, `tool:<run_id>:<call_id>`). A mistyped key creates a ledger row no replay finds → the real effect can execute twice. PK `(run_id, effect_key)` is sound; the key *format* is unenforced.
- **C6 (MED, CONFIRMED):** `run_id ... ON DELETE CASCADE` on `external_effects` (0004:24) deletes the exactly-once marker when a run is deleted; a later replay of the same key is then free to execute. Ledger durability is coupled to run lifetime.

### Row-level security (L)

- **L1 (HIGH, CONFIRMED):** both policies compare only `space_id` (0004:42–43). Nothing enforces that `cards.space_id` equals its `threads.space_id`, or that `external_effects.space_id` equals its `runs.space_id`; a buggy writer can bind a card to another tenant's thread (the FK proves the thread exists, not that it belongs to the same space) — the card is then answerable in the wrong space. Both parent tables already carry `space_id` (0002:157,192), so a composite FK is possible.
- **L2 (MED, CONFIRMED):** `FORCE ROW LEVEL SECURITY` (0004:40) applies to the table owner too. Any DML in a hardening/backfill migration runs with an empty `boma.space_id` → `current_space_id()` is NULL (0002:27–31), the predicate is NULL, and the statement touches **0 rows silently**. A `VALIDATE CONSTRAINT` can then pass vacuously. The new migration must `SET LOCAL boma.system_reason` or `SET LOCAL boma.space_id` before any DML.
- **L3 (MED, GUESS):** `OR boma.is_system()` (0004:42–43) is a total bypass controlled only by a GUC string (0002:38–42); the closed `SystemReason` enum is enforced in Rust, not in the database (defect D-09). A non-enum value set by any future path crosses all tenants for these two tables.
- **L4 (LOW, CONFIRMED):** the `DO` block is not retry-safe: no `IF NOT EXISTS` on `CREATE POLICY`, no `CREATE TABLE IF NOT EXISTS`. A half-applied migration (tables created, loop aborted) fails on retry. `list_pending` filters `status='pending' ORDER BY created_at` (cards.rs:38–39) with no supporting index; with FORCE RLS this degrades to a scan as cards accumulate.

### State defaults & lifecycle (S)

- **S1 (HIGH, CONFIRMED):** `created_at` has no DB default (0004:15) — unlike `pending_tool_calls` (0021:16). The writer supplies a text-cast timestamp (cards.rs:94,104) while `answered_at = now()` uses the DB clock (cards.rs:134): two clocks, replica-dependent `ORDER BY`, and a malformed caller value fails only at write time.
- **S2 (HIGH, CONFIRMED):** `run_id ... ON DELETE SET NULL` (0004:11) leaves a `pending` card with no run after deletion: it is still listed and answerable (C3a), records no effect and resumes nothing — the member sees approval "succeed" with no action (also worker #334 C1).
- **S3 (MED, CONFIRMED):** no expiry/cancel state and no `answered_by`. Answered cards are never retired; S2 rows persist indefinitely. Adding a column is additive; consuming it requires a store change (out of this module's scope).
- **S4 (LOW, GUESS):** `payload jsonb DEFAULT '{}'` (0004:28) has no shape/kind coupling; an effect recorded with an empty payload is indistinguishable from a legitimately empty one.

## Concrete hardening (additive, prioritized)

All items land in a **new** migration `migrations/006X_cards_hardening.sql`, wrapped in `BEGIN/COMMIT`, with `SET LOCAL boma.system_reason` before DML and `CREATE … IF NOT EXISTS` / catalog-guarded policies so it is retry-safe (L2/L4).

- **P0 — pin the vocabulary (C1/C2/C5):** `ADD CONSTRAINT … CHECK (status IN ('pending','answered','declined'))`, `CHECK (kind IN ('question','approval','purchase','takeover'))`, and an effect-key shape CHECK (regex on `approve|tool|purchase:` prefix, GUESS pending enum review). Use `NOT VALID` then audit + `VALIDATE` so existing rows fail loudly, not silently, and the lock stays short.
- **P0 — seal the NULL-run gap (C3a/S2):** `ADD CONSTRAINT cards_approval_requires_run CHECK (kind = 'question' OR run_id IS NOT NULL) NOT VALID`. This makes run-less approval/purchase/takeover cards impossible without touching the existing index.
- **P0 — bind tenant identity (L1):** add `UNIQUE (id, space_id)` on `threads` and `runs`, then `cards (thread_id, space_id) REFERENCES threads (id, space_id)` and `external_effects (run_id, space_id) REFERENCES runs (id, space_id)`, both `NOT VALID`; reconcile mismatches, then `VALIDATE`. Cross-tenant mis-binding becomes a write error.
- **P1 — clock & consistency (S1/C4):** add a `BEFORE INSERT` trigger defaulting `created_at` to `now()` when omitted (pure addition; preferable to `ALTER COLUMN SET DEFAULT` if that counts as in-place), plus `CHECK (status = 'pending' OR answered_at IS NOT NULL)` after the status vocabulary is pinned.
- **P1 — preserve the ledger (C6):** add `external_effects_archive` mirroring the key and a `BEFORE DELETE` trigger that copies applied rows before cascade. Consumers can then consult the archive on replay; no existing FK is altered.
- **P1 — cover the hot read (L4):** `CREATE INDEX IF NOT EXISTS cards_space_status_created ON cards (space_id, status, created_at)` and `external_effects (space_id, run_id)` — RLS predicate plus read path, bounded scans.
- **P2 — system guard & lifecycle (L3/S3):** add `boma.system_reasons` lookup + a RESTRICTIVE policy requiring membership (GUESS: needs the closed enum values); add nullable `expires_at`/`answered_by` columns and document the intended consumer filter.

## Risks & mitigations

- The coarse `(thread_id, run_id)` index (C3b) **cannot be relaxed additively**. Replacing it with a call-scoped key is a behavior change → ADR + `docs/PARITY.md` + operator sign-off; do not attempt inside this additive migration.
- New CHECKs and composite FKs can reject pre-existing rows: always `NOT VALID` → audit query → `VALIDATE`; keep a rollback script that drops only the new objects.
- `FORCE RLS` means the migration's own DML is invisible without the GUC (L2) — fail the migration if `current_setting('boma.space_id', true)` and `boma.system_reason` are both unset rather than silently validating nothing.
- Trigger-based `created_at` changes stored values for callers that omitted it; those inserts currently fail, so this is a relaxation, but note it as a data-format observation if any client depends on the error.
- Archive trigger adds a write on run deletion; keep it `INSERT … ON CONFLICT DO NOTHING` and measure delete latency before enabling in production.

## Acceptance tests (suggested names)

- `req_017_invalid_status_and_kind_are_rejected_at_insert`
- `req_017_approval_without_run_cannot_be_created`
- `req_043_card_space_must_match_thread_space`
- `req_043_effect_space_must_match_run_space`
- `req_017_effect_ledger_survives_run_deletion`
- `req_007_card_created_at_defaults_to_db_clock`

## Decisions needed (GUESS → operator/spec)

1. Is a second pending card per run (two tool calls) legitimate? If yes, the dedup index must be replaced — non-additive ADR.
2. Exact closed `SystemReason` values for the DB-side guard (L3), and may 0002's `is_system()` be left unchanged?
3. Effect-key prefix list for C5 — confirm `approve|tool|purchase` is exhaustive.
4. On run deletion, decline/cascade pending cards (S2) or leave them and rely on TTL?
