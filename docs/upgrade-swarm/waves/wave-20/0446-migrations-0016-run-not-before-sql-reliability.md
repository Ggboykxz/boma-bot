# v0.4.0 reliability proposal — `migrations/0016_run_not_before.sql` (worker #446, lens: reliability)

Status: proposal only (read-only review). Claims below are CONFIRMED from the module,
`migrations/0002_m1.sql`, `0014_routines.sql`, `0015_run_routine_id.sql`,
`crates/infra/src/stores/runs.rs`, `crates/app/src/usecases.rs` and `crates/infra/src/db.rs`
unless marked GUESS. Additive only: every hardening below is a NEW migration file; none edit 0016.

## What this migration does (CONFIRMED)

`ALTER TABLE runs ADD COLUMN not_before timestamptz NULL;` — one nullable column, no DEFAULT
(existing rows read NULL = due now), no CHECK, no index, no backfill. It is the DB half of REQ-024
backoff: `auto_retry_routine_run` (`usecases.rs:5080-5095`) computes an absolute timestamp from the
app clock plus `retry_delay_seconds(attempt)` (60s doubling to 1800s) and passes it to
`RunStore::retry`, which writes `not_before = $3` on the failed → queued transition
(`runs.rs:431-472`). The claim CTE hides a queued run until `not_before <= now()`
(`runs.rs:342-353`). Interactive retries and fresh runs pass NULL and stay immediately due.

## Failure mode A — Constraints (CONFIRMED gaps)

1. **No bound at all.** `timestamptz` accepts `infinity` / `-infinity` and any far-future value.
   A single corrupt or hostile `not_before = 'infinity'` makes `not_before <= now()` false forever:
   the run sits `queued`, is never claimed, never fails, never alerts — a silent liveness hole.
   The 1800s cap lives only in domain code; nothing at the DB boundary enforces it.
2. **No state invariant.** The claim path sets `status='leased'` without clearing `not_before`
   (`runs.rs:355-360`), and `find_open_routine` treats any non-terminal run as open
   (`runs.rs:295-319`). A stale future `not_before` therefore survives on a leased/completed row
   and can reappear if that row is ever re-queued by a path that does not overwrite the column.
   Today `retry()` always rewrites it (NULL clears it), so this is latent, not active.
3. **A naive invariant would break claim.** `CHECK (status <> 'queued' OR not_before IS NULL)`
   is wrong: claim flips to `leased` while `not_before` is still set. Any such CHECK requires
   the claim UPDATE to clear `not_before` first — an app change, not just a migration.

## Failure mode B — Scheduling (CONFIRMED gaps)

1. **Coalescing coupling.** Because `find_open_routine` ignores `not_before`, a backoff-queued run
   counts as "open" and suppresses the next routine wake (correct: do not stampede). But a run
   stuck on a bad future timestamp suppresses that routine *forever* — backoff and coalescing
   turn one bad row into a permanent outage for the schedule.
2. **Clock split.** The deadline is rendered from `app.services.clock.now()` but compared to DB
   `now()`. App/DB clock skew shortens or lengthens the backoff; a fast-forwarded test clock
   (`testkit/scripts.rs` fast-forwards retries) can write a deadline far from DB time. Writing the
   deadline in SQL (`now() + $delay * interval '1 second'`) removes the split — cross-cutting,
   noted below.
3. **No wakeup.** When a window expires the run becomes claimable only on the next worker poll;
   claim latency = poll interval. Acceptable, but it should be documented, not assumed instant.
4. **Lease reclaim ignores `not_before`.** The `leased/running` expiry branch of the CTE
   (`runs.rs:348-350`) does not re-check the deadline; harmless for well-formed rows, but a
   corrupt leased row is reclaimed immediately.

## Failure mode C — RLS and migration mechanics (CONFIRMED)

`runs` is a FORCE-RLS tenant table (`0002_m1.sql:249-260`), policy
`space_id = boma.current_space_id() OR boma.is_system()`. Claim correctly runs as
`as_system(ScheduledJob)`; a plain added column needs no policy change and exposes no secret.
Migrations are embedded via `sqlx::migrate!` (`db.rs:20,72-74`) and each runs in a transaction,
so the Helm migrate job's `backoffLimit: 5` cannot leave a half-applied 0016. **Critical:** sqlx
validates checksums of applied migrations — editing 0016 (even adding `IF NOT EXISTS`) breaks
upgrades with `VersionMismatch`. All hardening must be a new file. Future migration backfills on
`runs` would be blocked by FORCE RLS even for the owner; this column needs no backfill.

## Hardening proposal (additive, new migration)

**Upgrade 1 — Reject non-finite deadlines.** New `0060_run_not_before_finite.sql`:
`ALTER TABLE runs ADD CONSTRAINT runs_not_before_finite CHECK (not_before IS NOT DISTINCT FROM NULL
OR (not_before <> 'infinity'::timestamptz AND not_before <> '-infinity'::timestamptz));`
IMMUTABLE, safe on existing rows (all NULL).
- **Risk:** low. A row already holding `infinity` (e.g. from a test clock) makes the migration
  fail; run a read-only audit query in pre-upgrade and decide the disposition first.

**Upgrade 2 — Cap the horizon in a BEFORE trigger.** New `0061_run_not_before_horizon.sql`: a
`BEFORE INSERT OR UPDATE OF not_before` trigger raising when `not_before > now() + interval '2
hours'` (above the 1800s domain cap, wide enough for clock drift). Triggers may use `now()`
where a CHECK cannot. Keep it invoker-rights and read no other rows so RLS is irrelevant.
- **Risk:** medium. Extra write cost on every `retry()`; a wrong constant or a legitimately long
  app-generated deadline causes failed retries. Make the limit configurable or generous, and test
  the fast-forward testkit path before enabling.

**Upgrade 3 — Partial index for the claim scan.** New `0062_runs_claim_idx.sql`:
`CREATE INDEX runs_queued_created_idx ON runs (created_at, id) WHERE status = 'queued';`
Shrinks the `ORDER BY created_at ... LIMIT 1` candidate scan when many future-dated queued rows
accumulate. (A predicate on `not_before <= now()` is impossible in an index — `now()` is not
IMMUTABLE.)
- **Risk:** low/medium. One more index to maintain on a hot write table; benchmark with a backlog
  before adopting, otherwise the existing `(status, created_at)` index suffices.

**Upgrade 4 — Cross-cutting fixes (NOT in a migration; list for the owning modules).**
(a) compute `not_before` in SQL to kill clock skew; (b) make `find_open_routine` ignore a queued
run whose `not_before` is beyond the max horizon, so one bad row cannot freeze a routine;
(c) clear `not_before` in the claim UPDATE if Upgrade's sibling invariant is ever added;
(d) document poll-bound claim latency.
- **Risk:** high/DEVIATION — changes REQ-024 dispatch behaviour; needs `docs/PARITY.md` and owner
  approval. Keep the client DTO (`not_before` field) unchanged.

## Traceability / verification (CONFIRMED where possible)

Keep REQ-024 test names verbatim (`crates/infra/tests/m1.rs:1357`,
`crates/testkit/src/build.rs::routine_retry_tests`, `crates/domain/src/run.rs`). Add an integration
test that seeds `not_before='infinity'` and asserts Upgrade 1 rejects it, and one that asserts a
future-deadline run is invisible to `claim_next_queued` until the window passes (already present at
`m1.rs:1357-1502`). Run `cargo fmt --check`, clippy `-D warnings` and the M3 integration suite.
