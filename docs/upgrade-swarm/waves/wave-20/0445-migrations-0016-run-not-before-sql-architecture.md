# Wave 20 / Worker 0445 — `migrations/0016_run_not_before.sql` (LENS: architecture)

Proposal for v0.4.0. Additive only: every change below *adds* a column, constraint, index, table or
comment; nothing already applied is rewritten, dropped, renamed or destructively back-filled.

## Current structure (CONFIRMED — read this session)

`0016_run_not_before.sql` is a 7-line, one-way migration: a header citing REQ-024, a three-line
comment defining the sentinel, and a single statement:

```sql
ALTER TABLE runs ADD COLUMN not_before timestamptz NULL;
```

Semantics (CONFIRMED, from the header comment and `crates/domain/src/run.rs:316-321`): `NULL` means
"due as soon as queued" — interactive retries (REQ-012) and freshly created runs leave it `NULL`;
only the automatic retry after a failed routine wake carries a delay. The column is nullable with
no default, no `CHECK`, no FK and no index.

Related surface read this session (CONFIRMED):
- `0012_attempt.sql` added `attempt integer NOT NULL DEFAULT 1 CHECK (attempt >= 1)` to `runs`.
- `0015_run_routine_id.sql` added `routine_id text REFERENCES routines(id) ON DELETE SET NULL`;
  only routine-woken runs carry it.
- `0002_m1.sql` created `runs_status_created_at_idx ON runs (status, created_at)` and a unique
  `(thread_id, client_nonce)` key on `runs`.
- `crates/infra/src/stores/runs.rs:330-357` — `claim_next_queued` selects a claimable row with
  `(status = 'queued' AND (not_before IS NULL OR not_before <= now())) OR (status IN ('leased',
  'running') AND lease_expires_at <= now())`, ordered by `created_at, id`, `FOR UPDATE SKIP LOCKED`.
- `crates/infra/tests/m1.rs:1467-1483` exercises the predicate directly (`not_before IS NULL OR
  not_before <= now()`), then advances `not_before` by `UPDATE` to re-enable the claim.
- `crates/domain/src/run.rs:324-344` — backoff policy lives in **code**: `MAX_ATTEMPTS = 3`,
  `RETRY_BASE_SECONDS = 60`, `RETRY_CAP_SECONDS = 1800`, `retry_delay_seconds()` exponential curve.

## Architectural assessment

The migration is minimal and parity-correct, but it encodes the whole retry contract in a comment
and in Rust constants. The database stores a timestamp with no guard, no index that matches the
claim predicate, and no representation of the policy that produced it. That is a structural split:
the sentinel's meaning, its enforcing query and its tuning knobs live in three different layers, so
a reader of the schema cannot tell what `not_before` means or prove the invariant. All upgrades below
close that gap additively without changing observable behaviour (no new DEVIATION expected; final
parity sign-off is the operator's).

## Concrete upgrades (additive only)

### 1. Partial index aligned with the claim predicate
Add `CREATE INDEX runs_claimable_idx ON runs (created_at, id) WHERE status = 'queued'` and a
second partial index `... (not_before, created_at, id) WHERE status = 'queued' AND not_before IS
NOT NULL`. The existing `(status, created_at)` index cannot serve the `IS NULL OR <= now()` branch
cheaply; these let the worker find due and backoff-deferred runs without a filter over all runs.
Down/concurrent-safe form: `CREATE INDEX CONCURRENTLY` in an additive follow-up migration.
- **Risk**: medium. The claim predicate is an `OR`; the planner may still choose a seq scan on a
  small `runs` table, so verify with `EXPLAIN` on a seeded table before claiming a win. Index churn
  adds write cost on a hot `INSERT`/`UPDATE` table. Mitigation: measure, keep only indexes that
  the plan actually uses.

### 2. Generated effective-time column to remove the `OR`
Add `not_before_effective timestamptz GENERATED ALWAYS AS (COALESCE(not_before, '-infinity'
::timestamptz)) STORED` (PG 12+). Consumers can then express "due" as
`not_before_effective <= now()` — one sargable predicate — and a single index on
`(status, not_before_effective, created_at)` can back it. The raw `not_before` column and the
`NULL` sentinel stay exactly as they are.
- **Risk**: medium. PG-version floor must hold; SQLx compile-time queries and `RunRow` shrink/
  change, so the upgrade touches consumer code (a schema change, not a data rewrite). GUESS:
  `-infinity` comparisons behave as expected for `timestamptz` — confirm with a unit test.

### 3. DB-enforced invariant tying the delay to automatic retries
Add `ALTER TABLE runs ADD CONSTRAINT runs_not_before_auto_retry CHECK (not_before IS NULL OR
(routine_id IS NOT NULL AND attempt > 1))`. This turns the header comment into a guard: only a
routine-woken run past its first attempt may be backoff-deferred; interactive retries (REQ-012)
and fresh runs stay `NULL`.
- **Risk**: medium-high. Adding a validated `CHECK` scans and locks the table; use `NOT VALID`
  then `VALIDATE CONSTRAINT` in a follow-up to avoid a long lock. Must first CONFIRM no existing
  path sets `not_before` on a non-routine or `attempt = 1` run — `auto_retry_routine_run` does not,
  but `clients.rs`/test fixtures update `not_before` directly (m1.rs:1479) and may violate it.
  A violation would break runtime writes, so verify every writer before shipping.

### 4. Self-describing schema via `COMMENT ON`
Add `COMMENT ON COLUMN runs.not_before` spelling out the sentinel ("NULL = due immediately;
non-NULL = earliest claim instant for an automatic retry of a failed routine wake, REQ-024") and
that the value comes from the exponential curve in `retry_delay_seconds()`. Comments are metadata.
- **Risk**: low. No behaviour change; only risk is drift if the policy changes. Mitigation: name
  REQ ids in the text, matching the sibling proposals' convention.

### 5. Reference table for the backoff policy (single source of truth)
Additively create `retry_policy (id, max_attempts, base_seconds, cap_seconds)` seeded with
`3 / 60 / 1800` (values CONFIRMED in `run.rs`). The Rust constants remain authoritative at runtime;
the table is the schema-visible record and a future migration can retune policy without code
changes.
- **Risk**: medium. Duplicated policy invites drift — require a test asserting table and constants
  agree. Seeding is new data, still additive; any eventual switch of the runtime to read the table
  would be externally visible and must go through the DEVIATION/approval gate first.

## Summary

| # | Upgrade | Additive surface | Risk |
|---|---|---|---|
| 1 | Claim-path partial indexes | 2 indexes | medium (verify plan) |
| 2 | Generated effective-time column | 1 generated column | medium (consumer change) |
| 3 | Auto-retry `CHECK` | 1 constraint | medium-high (audit writers) |
| 4 | Schema comments | metadata | low |
| 5 | Policy reference table | 1 table + seed | medium (drift) |

Recommended order for v0.4.0: 4 and 1 first (zero/low risk, measurable), then 3 after the writer
audit, then 2 and 5 as a coordinated consumer-facing change. Every item is additive; none alters
the historical meaning of `NULL` or of existing `not_before` values.
