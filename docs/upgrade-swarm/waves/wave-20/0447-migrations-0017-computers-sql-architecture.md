# 0447 — `migrations/0017_computers.sql` — architecture (v0.4.0)

- Worker #447 · Lens: architecture (structure, boundaries, coupling, debt) · Target: v0.4.0.
- Status: PROPOSAL (read-only review; no migration, code or doc outside this file changed).
  CONFIRMED = verified by direct read of this worktree; GUESS = needs a test/oracle run.
- Module: `migrations/0017_computers.sql` (94 lines): the M5 schema for a computer per bot, its run
  holder / execution lease, and the per-space home and browser-profile storage (REQ-026..031).
- Read for context (CONFIRMED): `0018_screen_capabilities.sql`, `0019_computer_takeover.sql`,
  `0020_team_computers.sql`, `0022_pending_tool_calls_status_check.sql`,
  `crates/domain/src/computer.rs`, `crates/infra/src/stores/computer.rs`,
  `crates/infra/src/stores/computer_row.rs`, `spec/01-requirements.md:213–259`,
  `spec/02-data-model.md:100–113`, `spec/04-nonfunctional.md:134–135`, and the wave-19/0421
  proposal for house style.

## Current structure (CONFIRMED)

One file owns five tables with two distinct lifetimes: the **live computer aggregate**
(`computers`, `computer_runs`, `computer_execution_leases`) and the **durable storage artifacts**
(`agent_homes`, `browser_profiles`). Follow-up migrations already patch it: 0018 adds
`screen_capabilities`, 0019 adds `takeover_by`/`takeover_at` to `computers`, 0020 turns `private`
into an explicit flag and adds `computers_team_per_space`.

1. **Vocabulary is pinned, but in three places.** The `status` CHECK (:20), the six-variant Rust
   `ComputerStatus`, and the spec text must agree byte-for-byte. Unlike `0004_cards.sql`, the DB
   *does* reject unknown states — keep that property; a new variant is a paired Rust+SQL step.
2. **The sweeper index does not match the sweeper query.** `computers_idle_idx` is on
   `last_used_at` (:35–36), but `running_idle` filters
   `last_used_at <= $1 - (idle_minutes || ' minutes')::interval`, a per-row bound that is not
   sargable; the index can only narrow `status='running'`.
3. **Two widths disagree.** DB `screen_generation integer` (:21) vs domain `u64`; both store casts
   saturate (`unwrap_or(i32::MAX)` / `unwrap_or(u64::MAX)`). At INT_MAX the generation stops moving,
   which would let a stale REQ-030 capability look current again.
4. **Isolation is hidden in dynamic SQL.** The `DO $$ … FOREACH … EXECUTE format(...)` loop (:79–94)
   enables/forces RLS for five tables; the effective policy is not greppable and is invisible to
   SQLx offline metadata, the same debt 0421 flagged in `0004`.
5. **Partial model.** `spec/02-data-model.md:107,110` names `ComputerUpdate` and
   `HomeDeletionNotice`; neither table exists yet. `takeover_at` has no pair constraint and no FK,
   and `computer_execution_leases` carries no `computer_id`, a `fence` with no sign check, and no
   expiry index.

## Guiding principle for v0.4.0

`0017` (and 0018–0020) are immutable: every change ships as a NEW, additive migration that only
adds objects, defaulted/nullable columns, indexes or constraints, never drops or rewrites existing
rows. Preserve wire-visible status names, RLS expressions and the effect of the existing unique
indexes; anything externally visible is a DEVIATION for `docs/PARITY.md`. Spec 04 §4.9 requires a
documented downgrade story (`DROP CONSTRAINT` / `DROP INDEX` / `DROP TABLE`) alongside each file.

## Concrete upgrades (5), each with a risk

1. **Make RLS introspectable without removing the loop.** In a new `0060_computers_rls_assert.sql`,
   add a migration-time `DO $$` that raises unless `pg_policies` shows
   `computers_tenant_isolation` and the four siblings with matching `using`/`with check`, and a test
   (name carries REQ-043) asserting `pg_class.relrowsecurity`/`relforcerowsecurity`; give every
   *new* tenant table an explicit static `ENABLE`/`FORCE`/`CREATE POLICY` block. *Risk:* `pg_policies`
   columns vary by Postgres version and a test mirroring the loop can drift from it. *Mitigation:*
   pin to the embedded PG 17 used by the packs, assert only policy name + enforced flag, and derive
   both sides from one shared expected-policy constant.

2. **Give the idle sweeper a sargable deadline.** Add a nullable `idle_deadline_at timestamptz`
   maintained by a `BEFORE INSERT OR UPDATE` trigger as
   `last_used_at + idle_minutes * interval '1 minute'`, backfill existing rows, then build
   `CREATE INDEX CONCURRENTLY … (idle_deadline_at) WHERE status='running'` and point `running_idle`
   at it (coordinated with the infra owner). *Risk:* a STORED generated column would rewrite and
   lock `computers`, and trigger-based maintenance can drift from the app clock. *Mitigation:* use
   the nullable trigger column (metadata-only DDL + bounded backfill), derive it from the same DB
   `now()` family as `boma.rfc3339`, and prove equivalence against the old predicate with a golden.

3. **Fail closed on generation wrap, then widen deliberately.** Add
   `CHECK (screen_generation < 2147483647)` plus a COMMENT and an alert threshold, so a boot that
   would wrap fails instead of silently reviving stale capabilities (REQ-030). Plan a separate ADR'd
   migration to widen to `bigint` (or add a `bigint` companion) once a consumer is ready. *Risk:* a
   guard alone makes a very long-lived, frequently restarted computer unavailable, and a widening
   ALTER is only "additive" if the owner accepts a rewrite-free type change. *Mitigation:* monitor
   `max(screen_generation)`, treat exhaustion as a capacity alarm, and keep the domain's `u64` as
   the target width so no future cast saturates again.

4. **Add the two spec-named tables additively.** Create `computer_updates(computer_id, update_id,
   status, …)` and `home_deletion_notices(home_id, requested_at, completed_at, …)` with FKs to
   `computers`/`agent_homes`, CHECK-constrained status, and explicit static RLS policies. *Risk:*
   unused tables add schema surface and a wrong retention backfill could enqueue premature home
   deletions (REQ-026 "home preserved per retention policy"). *Mitigation:* land only with a named
   consumer, default new rows to "observed"/"pending", keep deletion a system-path job under
   `SystemReason::ScheduledJob`, and freeze all cleanup behind an ADR.

5. **Tighten lease and takeover invariants additively.** Add
   `CHECK ((takeover_by IS NULL) = (takeover_at IS NULL))`, `takeover_by` FK to `users(id) ON
   DELETE SET NULL`, `CHECK (fence >= 0)`, a nullable `computer_id` on
   `computer_execution_leases` with an index, and a partial unique index on live lease owners
   (`WHERE expires_at > now()`). *Risk:* existing half-set takeovers or dangling member ids make
   the migration fail, and a team computer's `bot_id` is a sentinel that can collide with a real
   bot. *Mitigation:* pre-flight audits
   (`SELECT … WHERE takeover_by IS NULL <> takeover_at IS NULL`, and an anti-join on `users`),
   backfill before adding constraints, and document the sentinel in the 0020 team-computer comment.

## Cross-cutting risks / sequencing

- Sequence 1 → 3 → 5 → 2 → 4: settle policy visibility and the generation seam first, then
  invariants, then the sargable index (needs the store change), then the new tables (needs a
  consumer). Each step is a separate additive migration.
- Regenerate SQLx offline metadata after every column/index addition; CI must run `migrate` on a
  fresh DB and replay `computer-status.json`, `computer-takeover-conflict.json` and
  `ui/REQ-026-computer-panel.json`. Freeze status names, RLS expressions and unique semantics.
- No new dependency. Mark every claim CONFIRMED/GUESS per `AGENTS.md` and gate each step with
  `cargo fmt --check`, clippy `-D warnings`, `cargo test -p boma-infra`, golden and contract suites.

## Out of scope

The six-variant `ComputerStatus` domain type, `ComputerStore`/its in-memory doubles, takeover
orchestration in `usecases.rs`, `screen_capabilities` (0018), and any in-place edit of 0017–0020
belong to other briefs in this swarm. The duplicate `takeover_at` projection in
`computer_row.rs:35,40` is noted for the infra owner, not fixed here.
