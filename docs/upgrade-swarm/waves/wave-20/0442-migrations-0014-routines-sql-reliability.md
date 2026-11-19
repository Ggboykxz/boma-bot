# v0.4.0 reliability proposal — `migrations/0014_routines.sql` (worker #442, lens: reliability)

- **Status:** PROPOSAL (read-only review; no source or migration changed). CONFIRMED = verified by reading files in this worktree; GUESS = needs a runtime/test/oracle run.
- **Scope:** the `routines` / `routine_runs` schema (0014) and the columns later added by 0015 (`runs.routine_id`), 0016 (`runs.not_before`), 0039 (`github_enabled`), 0041 (`email_senders`), cross-read with the Postgres adapter `crates/infra/src/stores/routines.rs`. Every hardening below is **additive** (new migration(s) only; no DROP/ALTER of existing objects).
- **Trace:** REQ-023 / REQ-024; spec/02-data-model.md:79-87 declares "an active routine has a scheduled wakeup" and the `@once` / failure-pause semantics.

## Summary

The schema is lean and mostly sound (PK/FKs with sensible cascades, row-level checks, partial due index, FORCE RLS). Its reliability gaps are of three kinds: the CHECKs validate *shape* but not *semantics* (bad timezone / malformed `@once` arrays are storable and only fail at dispatch time); the single permissive RLS policy lets a tenant mutate its own `routine_runs` audit rows and lets any non-empty `boma.system_reason` cross tenants; and the schedule has **no atomic claim** — `due()` is a bare SELECT (routines.rs:169-185) and `record_fired` an unconditional UPDATE (routines.rs:211-217), so overlapping sweeps or a restarted worker can double-fire one wake. Note also the pre-existing worker-side finding in STATE.md (M28): a routine-woken run once killed the worker; the schema offers no lease/claim primitive to make dispatch safe under retries.

## Failure modes

### Constraints (C)

- **C1 (HIGH, CONFIRMED):** `timezone text NOT NULL` (0014:17) has no validity check. An invalid IANA string is stored, then cron recomputation in the app fails or is skipped silently → the routine never wakes, with no DB-visible error.
- **C2 (MED, CONFIRMED):** `crons text[]` is guarded only by `cardinality(crons) >= 1` (0014:26). NULL elements, empty strings, and arrays mixing `@once` with real expressions all pass; `@once` is a magic string with no exclusivity rule. The app's special-casing (routines.rs:209-215, ports.rs:1483) then behaves ambiguously.
- **C3 (MED, GUESS):** no invariant ties `active` to `next_run_at`. `active=true, next_run_at=NULL` is storable despite spec/02:86 ("an active routine has a scheduled wakeup") → a permanently dormant schedule. Confirm no legitimate active+NULL rows exist before enforcing.
- **C4 (LOW, CONFIRMED):** `name <> ''` exists but there is no length bound on `name`/`prompt`/`crons`; a pathological value inflates rows and scan cost. `routine_runs.outcome` is a fixed CHECK enum (0014:38) — any new app-side outcome without a migration fails the INSERT and, because the audit insert shares the state-update transaction (routines.rs:235-245 / 291-302), rolls back the state transition too.
- **C5 (LOW, CONFIRMED):** `updated_at` is set only by app queries (`now()`) and the `create` bind (routines.rs:45-46); no DB trigger enforces it on direct writes.

### RLS / tenant isolation (R)

- **R1 (HIGH, CONFIRMED):** one `CREATE POLICY ... USING (... OR is_system()) WITH CHECK (...)` with no `FOR` defaults to ALL commands (0014:53-58), so a tenant can UPDATE or DELETE its own `routine_runs` history — the audit trail is mutable by the party it audits.
- **R2 (HIGH, GUESS):** `boma.is_system()` is true for *any* non-empty `boma.system_reason` (0002_m1.sql:38-42); the "closed enum" guarantee lives only in app code. A misconfigured/compromised connection that sets an arbitrary string crosses tenants.
- **R3 (MED, CONFIRMED):** `routine_runs.routine_id ... ON DELETE CASCADE` (0014:35) erases an entire run history when a routine is removed — inconsistent with 0015's deliberate `ON DELETE SET NULL` ("keeps old history readable", 0015:3-4).
- **R4 (LOW, CONFIRMED):** `routines` has `(next_run_at)` and `(thread_id)` indexes but none for the per-tenant `WHERE bot_id=$1 AND space_id=$2` list query (routines.rs:147-149) → sequential scan as routines grow.

### Schedule state (S)

- **S1 (HIGH, CONFIRMED from schema):** `due()` is a plain `SELECT ... WHERE active AND next_run_at <= $1` (routines.rs:178-180) with **no lease, claim token, `FOR UPDATE SKIP LOCKED`, or idempotency key**. Two workers, or one worker retried after an ambiguous failure, can both read the same wake and both call `record_fired` → duplicate run + duplicate `routine_runs` row. The due index (0014:29-30) does not change this.
- **S2 (HIGH, CONFIRMED):** a routine paused by failures is only `active=false` with `account_failures >= threshold` (routines.rs:271); it is indistinguishable from a user-paused or `@once`-spent routine (all set `active=false`). Manual resume re-enables it with the counter still at threshold → it re-pauses on the next failure (resume/pause loop). No reason column exists.
- **S3 (MED, GUESS):** missed wakes are not caught up (past `next_run_at` fires once and recomputes from now). Good for avoiding a thundering herd, but after downtime schedules are silently skipped; there is no "missed" outcome to surface this.
- **S4 (LOW, CONFIRMED):** dispatch correctness depends entirely on app-side timezone arithmetic; the DB stores only the resulting `timestamptz`, so a bad computation (C1) or clock skew is invisible here.

## Concrete hardening (prioritized, additive only)

1. **P0 — atomic claim (closes S1):** new migration adds `lease_owner text NULL`, `lease_expires_at timestamptz NULL`, index `(lease_expires_at) WHERE lease_expires_at IS NOT NULL`, and `boma.claim_due_routines(as_of, owner, ttl_seconds)` that selects due rows `FOR UPDATE SKIP LOCKED` and sets the lease in one transaction; dispatcher uses it instead of `due()`, and `record_fired` becomes conditional on still owning an unexpired lease. Stale leases reclaimed after TTL → worker-death recovery.
2. **P0 — immutable audit trail (closes R1):** add an `AS RESTRICTIVE` policy on `routine_runs` denying UPDATE/DELETE unless `boma.is_system()` (additive; the existing PERMISSIVE ALL policy cannot be dropped under additive-only). Pair with a BEFORE DELETE trigger that copies `routine_runs` to a new `routine_run_archive` table (closes R3) before the cascade.
3. **P1 — semantic field validation (closes C1/C2):** additive BEFORE INSERT OR UPDATE trigger `boma.validate_routine_fields()` that rejects unknown timezones (against `pg_timezone_names`), NULL/empty cron elements, and `@once` mixed with other expressions. Add equivalent `NOT VALID` CHECKs (`array_position(crons, NULL) IS NULL`) then `VALIDATE CONSTRAINT` to cover rows inserted outside the trigger path.
4. **P1 — pause reason (closes S2):** additive column `pause_reason text NULL` (`'failures' | 'user' | 'once'`) set by the existing state transitions, backfilled as GUESS for historical `active=false` rows; resume clears it and resets `account_failures` only when the reason was `'failures'`.
5. **P2 — invariants + indexes (closes C3/C4/C5/R4):** add `CHECK (NOT active OR next_run_at IS NOT NULL) NOT VALID` (validate only after confirming no bad rows), a length CHECK on `name`/`prompt`, an `updated_at` trigger, and `routines(space_id, bot_id)` index. For C4, adopt a rule: every new `RoutineOutcome` variant ships a migration widening the CHECK in the same change.

## Risks / trade-offs

- Claim leases add a state machine (owner/TTL/reclaim) and require an app change; clock skew or too-short TTL can double-fire, too-long TTL delays recovery after a worker dies. Choose TTL > worst-case sweep interval and test both edges.
- A RESTRICTIVE immutability policy plus archive trigger changes observable behavior (tenant can no longer edit/delete run history) → ADR + `docs/PARITY.md` entry; archive grows without bound and needs its own RLS/retention policy.
- Backfilling `pause_reason` is guesswork for existing rows; a wrong guess could let a failures-paused routine resume incorrectly. Prefer `NULL` = "unknown, treat as user-paused" (safest).
- Enforcing `active ⇒ next_run_at IS NOT NULL` could reject legacy rows; validate in staging first. Timezone validation via a trigger queries a catalog view — keep it a trigger, not a CHECK (CHECKs cannot subquery), and benchmark on bulk imports.
- R2 (arbitrary `system_reason`) cannot be fully fixed additively: tightening the existing `is_system()` function is a non-additive change needing its own ADR. Add a `boma.system_reason_valid()` helper for new policies now; schedule the function change separately.

## Verification to add

Real-Postgres integration tests (RLS enabled, `--locked`): two concurrent `claim_due_routines` calls return disjoint sets (S1); expired lease reclaimed (S1); restrictive policy blocks tenant UPDATE/DELETE of `routine_runs` (R1); routine delete preserves archived history (R3); invalid timezone / malformed `@once` arrays rejected (C1/C2); `active=true, next_run_at=NULL` rejected (C3); resume-after-failure clears the counter (S2). All claims above are CONFIRMED from source reading except items explicitly marked GUESS.
