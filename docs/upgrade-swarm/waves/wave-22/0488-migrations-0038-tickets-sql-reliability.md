# 0488 — `migrations/0038_tickets.sql` — reliability lens (v0.4.0)

Worker #488 · module `migrations/0038_tickets.sql` · lens: reliability · READ-ONLY review of
`rebuild-workspace/.worktrees/before-m5`. Proposals only; no code changed here. Claims:
**CONFIRMED** = read in this tree this session; **GUESS** = inferred, needs a test.
Scope: `board_state`, `tickets`, `ticket_comments`, their RLS policies and one index (66 lines).
Spec anchor: REQ-038 (ticket board, gated by `BOMA_BOARD=true`); env flag `BOMA_BOARD`.

## What the file already does right (keep, do not re-propose)

- Per-space `board_state.next_seq` with a single `UNIQUE (space_id, key)` makes keys monotonic and
  collision-rejecting at the DB layer — duplicate keys fail loudly rather than corrupt the board.
- `state`, `author_kind` are CHECK-constrained; FKs use deliberate cascades (`ON DELETE CASCADE` /
  `SET NULL`), so referential orphans cannot form for tickets/comments/assignee (CONFIRMED).
- RLS is ENABLE + FORCE on all three tables with matching USING/WITH CHECK predicates; comments
  derive tenancy from the parent ticket via EXISTS, so there is no spoofable `space_id` on a comment.
- `created_at`/`updated_at` default to `now()`; `comment_count` defaults to 0 (CONFIRMED).

## Failure modes

### State

- **F1. `updated_at` is insert-only (high).** It has `DEFAULT now()` but no `ON UPDATE` trigger and
  no app contract visible here, so it never advances on UPDATE (CONFIRMED structure). Any consumer
  sorting by `updated_at` or using it as an incremental-sync watermark silently misses edits
  (GUESS that the API does so). `board_state` has no timestamp at all.
- **F2. `comment_count` can drift (high).** It is denormalized with no trigger, FK-derived view, or
  constraint tying it to `ticket_comments`. Concurrent inserts/deletes or a failed app-side update
  leave a wrong count in list/pagination UIs; there is no reconciliation path (CONFIRMED absence).
- **F3. No state-transition or lifecycle guard (medium).** CHECK admits any of the 4 values in any
  order (`backlog`→`done`, `done`→`backlog`); nothing records who/when a state changed. History cannot
  be reconstructed and "done with open comments" is allowed (CONFIRMED). A restrictive CHECK would
  be a behavior change, so it is out of scope.
- **F4. Key allocation is app-trust only (medium).** "Race-free under the same transaction" is a
  comment, not a DB guarantee: `next_seq` is plain `integer` with no `CHECK (> 0)` and no mandated
  `SELECT … FOR UPDATE`. A lost update yields a duplicate `BOT-<n>` → UNIQUE violation → failed
  create (500). int4 also caps at 2^31-1 keys per space (CONFIRMED structure, GUESS on locking).

### Expiry

- **F5. RLS context expiry fails quiet on reads (high).** `boma.current_space_id()` is presumed a
  transaction-local GUC (GUESS). When it is unset/expired it likely returns NULL; `space_id = NULL`
  is NULL, so USING is false and reads return **zero rows** — fail-closed but silent, so a job or
  health check sees an "empty board" instead of an error. WITH CHECK failures on writes are loud.
- **F6. No retention/expiry and weak time index (medium).** Tickets/comments are immortal; the only
  index is `(ticket_id, created_at)`. There is no `created_at`-leading index for time-sliced cleanup,
  so unbounded growth degrades autovacuum and latency over time (CONFIRMED structure).
- **F7. Cascades destroy history (medium).** `created_by … ON DELETE CASCADE` means offboarding one
  user deletes every ticket they opened and, via `ticket_id … ON DELETE CASCADE`, all comments —
  irreversible, with no archive (CONFIRMED).

### RLS

- **F8. FORCE RLS also binds the owner (medium).** Migration, backup and maintenance jobs running as
  the table owner without `boma.is_system()` see nothing / fail writes (CONFIRMED FORCE; GUESS on
  job identity).
- **F9. Comment authors are unvalidated (medium).** `author_id text NOT NULL` has no FK and
  `author_kind` is not checked against it, unlike `assignee_bot_id (SET NULL)`; deleted users/bots
  leave dangling comment authors (CONFIRMED).
- **F10. `OR boma.is_system()` is a wide bypass (low).** If `is_system()` is ever true for an
  ordinary request role (misconfig), it becomes a global cross-tenant read with no second guard
  (GUESS on how the helper is set).

## Concrete hardening (v0.4.0, additive only — new objects, no destructive DDL)

- **P1.** Add `trg_tickets_touch_updated_at` BEFORE UPDATE trigger: `NEW.updated_at := now()` unless
  the writer explicitly set it. Fixes F1 without changing columns.
- **P2.** Add BEFORE/AFTER INSERT/DELETE/UPDATE triggers on `ticket_comments` maintaining
  `tickets.comment_count` transactionally; add a read-only `boma.reconcile_comment_counts()` to
  detect/fix drift. Fixes F2.
- **P3.** Add append-only `ticket_state_audit(ticket_id, from_state, to_state, actor, at)` plus a
  trigger writing a row on every state change. Fixes F3 observability without restricting moves.
- **P4.** Add `CHECK (next_seq > 0)` and a `boma.allocate_ticket_key(p_space)` SECURITY DEFINER
  function doing atomic `UPDATE … RETURNING` under the tenant context; document mandatory use.
  Fixes F4's drift; consider bigint in a later, non-additive migration.
- **P5.** Add `boma.require_space_id()` that RAISEs when the GUC is NULL, and use it in maintenance
  entry points; add a metric/alert on RLS policy-violation errors so quiet empty reads are detected.
  Addresses F5 without altering existing policies.
- **P6.** Add `CREATE INDEX tickets_created_at_idx ON tickets (space_id, created_at)` and a matching
  one on `ticket_comments` to enable time-sliced retention reporting (F6). Partitioning is deferred.
- **P7.** Add BEFORE DELETE archive triggers + append-only `tickets_archive` / `ticket_comments_archive`
  tables capturing user-cascade deletes (F7), keeping the live-table behavior unchanged.
- **P8.** Add a documented `boma.is_maintenance()`/SECURITY DEFINER read helper for owner-run jobs
  (F8), and a validation trigger on comment INSERT/UPDATE checking `author_id` exists in `users` or
  `bots` per `author_kind` (F9). Add a metric on `is_system()` usage outside system contexts (F10).

## Risks / trade-offs

- R1. P2's trigger will overwrite any app-managed `comment_count` logic; audit call sites first or
  counts double-update. P1 likewise overrides writers that set `updated_at` for imports — hence the
  "unless explicitly set" guard.
- R2. P4's function runs SECURITY DEFINER under FORCE RLS: a bug could mint keys for the wrong
  space. Keep it `SET search_path` and tenant-checked; the CHECK alone does not prevent duplicates.
- R3. P8's author-existence trigger rejects comments whose author was deleted — a new write failure.
  Gate on a backfill reconciling existing orphans, or make it a warning-only audit trigger.
- R4. P7 archives add storage and a write on every cascade delete; size the archive and define a
  deletion policy before enabling.
- R5. Every new trigger/function must be RLS- and `search_path`-safe and covered by a REQ-038 test;
  none may change the existing public API shape (would be a DEVIATION in `docs/PARITY.md`).

## Explicit non-goals / rejected options

- Do not change existing columns, policies, CHECKs or cascade actions (destructive, not additive).
- Do not add restrictive state-transition CHECKs or a `space_id` to `ticket_comments` in v0.4.0.
- Do not introduce partitioning or a new expiry column without an ADR and owner approval.

## Suggested rollout

Wave A (invisible, low risk): P1, P4-CHECK, P6, RLS-context metric (P5 observability half).
Wave B (needs tests/backfill): P2, P3, P7, P8; verify under REQ-038 tests and two-tenant RLS cases.
Nothing was executed in this pass; all line/module claims are from source read this session.
