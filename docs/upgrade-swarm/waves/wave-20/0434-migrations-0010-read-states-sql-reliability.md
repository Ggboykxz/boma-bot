# Worker 0434 — `migrations/0010_read_states.sql` — reliability lens (v0.4.0)

- Status: PROPOSAL (read-only review; no source or migration changed). Target: v0.4.0.
- Evidence: **CONFIRMED** = read this session from `0010_read_states.sql`, `0001_init.sql`,
  `0002_m1.sql`, `0009_reactions.sql`, `0011_snoozes.sql`, `spec/02-data-model.md`,
  `spec/01-requirements.md`, and the sibling proposal `wave-17/0382` (read-states store).
  **GUESS** = runtime consequence inferred, not reproduced by a test this session.

## Scope and current structure

`0010` creates `thread_read_states` (one row per `(thread_id, user_id)`, `last_seq >= 0`, `updated_at`
NOT NULL, PK `(thread_id, user_id)`) and enables tenant RLS through a dynamic `DO $$ … FOREACH …
EXECUTE format` loop (:16-31). It is the schema substrate of REQ-009's unread divider; writers are
the `PostgresReadStateStore` analysed in #382. `0010` is immutable: every change below ships as a
NEW, additive migration (e.g. `0060_read_states_hardening.sql`) that only adds objects/constraints/
defaults, never drops or rewrites a row. RLS expression, PK, FK cascades and absent-row-means-zero
stay byte-compatible; visible changes are DEVIATIONs for `docs/PARITY.md`.

## Failure modes

### Constraints
- **C1 (HIGH, CONFIRMED schema / GUESS impact):** `space_id` is an independent FK to `spaces` (:9);
  nothing ties it to `thread_id`'s space (`threads.space_id`, `0002_m1.sql:157`). A row for a thread
  in space A tagged space B passes every FK yet is invisible to A's RLS and visible to B: the
  legitimate reader loses their divider (unread from start) and a wrong-tenant row can carry A's
  `last_seq`. The store's `EXISTS (thread in caller space)` guard (#382 L12-13) covers its own write
  path only; a backfill, repair script or other writer can still mislabel.
- **C2 (MED, CONFIRMED schema / GUESS impact):** `CHECK (last_seq >= 0)` (:11) has no ceiling. The
  store accepts any non-negative value (#382); a stale/replayed head above the thread's real max
  makes `count_above` return 0, so unread silently vanishes. A CHECK cannot reference `messages`,
  so a bound is either a trigger or the store-side clamp (#382 H2).
- **C3 (MED, CONFIRMED mechanism):** `last_seq` may regress. A CHECK is per-row, not per-transition,
  so it cannot enforce monotonicity. `mark_unread` legitimately rewinds to 0 (REQ-009,
  `rpc.ts:414`), so a naive DB `GREATEST` trigger would break it — coordinate with #382 H1
  (split `advance`/`reset`).
- **C4 (LOW, CONFIRMED):** `updated_at timestamptz NOT NULL` with no default (:12) is an outlier vs
  the house `DEFAULT now()` convention (`0055`, `0059`, `0001`). The store sets it explicitly today;
  any future writer omitting it fails the insert, and mixed app/DB clocks make ordering unreliable.
- **C5 (LOW, GUESS):** no membership constraint — a row can exist for a user who is not a member of
  a group thread; the schema only checks the user exists and the space string matches.
- **C6 (INFO, CONFIRMED):** widths align (`last_seq` :11 ↔ `messages.seq` `0002:172` ↔
  `threads.next_message_seq` `0002:162`, all `bigint`). Preserve; `i64::MAX` handling is store-side.

### RLS
- **R1 (MED, CONFIRMED):** the policy is built in dynamic SQL (:16-31); the effective expression is
  not greppable and invisible to SQLx offline metadata (also raised in wave-19/0421). There is no
  migration-time assertion that `thread_read_states_tenant_isolation` exists with symmetric
  `USING`/`WITH CHECK`.
- **R2 (MED, CONFIRMED mechanism / GUESS runtime):** `USING (space_id = boma.current_space_id() OR
  boma.is_system())` (:24-25). If the transaction GUC is unset (background job, migration, admin
  session not via `with_tenant`), `current_space_id()` is NULL, the predicate is never true for a
  non-system role, and reads return empty → unread badge 0, silently. `FORCE RLS` (:22) also binds
  the table owner, so repairs must run as `is_system` or `BYPASSRLS` or they no-op.
- **R3 (LOW, CONFIRMED):** re-run safety — `CREATE POLICY` has no `DROP POLICY IF EXISTS` guard; a
  checksum-recovery manual re-apply fails on "policy already exists". Additive follow-ups must be
  idempotent.

### Ordering
- **O1 (INFO, CONFIRMED):** `0010` correctly follows `0001_init` (tables) and `0002_m1` (functions
  `current_space_id` at :27, `is_system` at :38). New migrations must keep numeric order and run
  after `0010`.
- **O2 (LOW, CONFIRMED):** `CREATE INDEX CONCURRENTLY` cannot run in a transaction block; the
  tenant index below must be its own migration with the runner's transaction disabled, or use a
  plain `CREATE INDEX` inside the transaction and accept a brief write lock on the hot mark path.
- **O3 (LOW, CONFIRMED):** FKs `ON DELETE CASCADE` (:8-10) prevent orphan rows — preserve. A
  mislabeled `space_id` still cascades via `thread_id`, but the wrong-tenant row lingers until the
  thread is deleted.

## Concrete hardening (additive only, each with a risk)

1. **Reconcile `space_id` with the thread's space (C1).** Add a `BEFORE INSERT OR UPDATE` trigger
   asserting/setting `NEW.space_id = (SELECT space_id FROM threads WHERE id = NEW.thread_id)`.
   *Risk:* a validating `CHECK`/FK-style constraint would fail on any existing mislabeled row, and
   there is no trigger precedent in this repo. *Mitigation:* run a pre-flight audit first; keep the
   trigger write-only (new writes); treat it as a shared tenant-integrity pattern with an ADR, not a
   one-off; coordinate with #382's write guard.
2. **DB ceiling / monotonicity (C2, C3) — deliberately deferred to the store.** A DB `LEAST/
   GREATEST` trigger risks silently changing `mark_unread` (reset to 0) or regressing when messages
   are hard-deleted — an externally visible DEVIATION. *Recommendation:* keep clamp + advance/reset
   in #382 H1/H2; at DB level retain only `CHECK (last_seq >= 0)` plus metrics on backward writes.
3. **Tenant index (O4/C1 scale).** Add `thread_read_states_space_user_idx ON thread_read_states
   (space_id, user_id)`, concurrently in its own migration (O2). *Risk:* write amplification on the
   hot mark path; choose column order from real `EXPLAIN` output, not guesswork.
4. **`updated_at` default (C4).** `ALTER COLUMN updated_at SET DEFAULT now()` — additive, matches
   house style. *Risk:* none for existing rows; the store must still set it on update (it does).
   A `BEFORE UPDATE` auto-set is optional but adds a new trigger pattern and per-write overhead.
5. **RLS assertion in a new migration (R1).** A `DO $$` block that raises unless `pg_policies` shows
   `thread_read_states_tenant_isolation` with equal `using`/`withcheck`, and `pg_class` shows
   `relrowsecurity`/`relforcerowsecurity` true. *Risk:* catalog columns vary by PG version.
   *Mitigation:* pin to the embedded PG 17, assert only policy name + symmetry + enforced flags,
   derive both checks from one expected-policy constant, and back it with a catalog test.
6. **Idempotent additive migrations (R3).** Use `CREATE INDEX IF NOT EXISTS`, `DROP POLICY IF
   EXISTS …; CREATE POLICY`, `CREATE OR REPLACE FUNCTION`. *Risk:* a `DROP POLICY` briefly widens
   access if `CREATE` fails — keep both in one transaction so failure rolls back; byte-identical
   expressions.
7. **Preserve the sound parts.** PK `(thread_id,user_id)`, `ON DELETE CASCADE` FKs, aligned bigint
   widths, the RLS expression, and absent-row-means-zero must remain unchanged.

## Verification (REQ-009 ids in test names; testcontainers Postgres)

- `req_009_read_state_space_id_matches_thread_space` — C1 mislabel rejected.
- `req_009_read_state_rls_policy_present_and_symmetric` — R1 catalog assertion.
- `req_009_read_state_unset_guc_yields_no_rows` — R2 documents fail-closed behaviour.
- `req_009_read_state_updated_at_defaults_to_db_now` — C4.
- `req_009_read_state_additive_migration_is_re_runnable` — R3.
- Replay golden `spec/golden/ui/REQ-009-unread-divider.json` as the regression baseline; keep RLS
  expressions and PK/constraints frozen.

## Coordination and open questions (GUESS)

- #382 owns monotonicity, ceiling and head coherence in Rust; this worker owns schema/RLS/index
  safety. Do not add a DB clamp that contradicts #382 H1/H2.
- Is `messages.seq` strictly append-only (no hard deletes)? Decides whether any DB ceiling is safe.
- Are read states ever written outside `with_tenant` (jobs/backfills)? Decides R2 severity.
- Accept a repo-wide trigger pattern for tenant integrity in v0.4.0, or keep invariants in Rust only?
