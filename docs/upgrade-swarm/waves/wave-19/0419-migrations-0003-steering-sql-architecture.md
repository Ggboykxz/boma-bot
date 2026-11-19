# v0.4.0 architecture proposal — `migrations/0003_steering.sql` (steering schema / REQ-003)

Worker #419 · LENS=architecture · target v0.4.0 · READ-ONLY analysis; no code or schema changed.
Claim status: "current state" is CONFIRMED by direct reads of `0003_steering.sql`, `0001_init.sql`,
`0002_m1.sql`, `0012_attempt.sql`, `crates/domain/src/run.rs`, `crates/infra/src/stores/runs.rs`,
`crates/app/src/worker.rs`, `spec/02-data-model.md` and wave-10's `0179-...ports-rs-architecture.md`.
Forward-looking items are proposals (GUESS until implemented, tested and verified).

## Current structure (CONFIRMED)

- 9 lines, forward-only: two `ALTER TABLE runs ADD COLUMN` plus two `COMMENT ON COLUMN`. No down file,
  matching the sqlx `MIGRATOR` pipeline (`db.rs:20`) and the idempotence gate `req_053_...`.
- `steering_from_seq bigint` is NULLABLE with no default and no CHECK; `steering_consumed_seq bigint
  NOT NULL DEFAULT 0` has no CHECK either — unlike `0012_attempt.sql`, which adds `CHECK (attempt >= 1)`.
- Mixed "empty" representation: NULL (`from`) versus 0 (`consumed`). Sequences start at 1
  (`0002_m1.sql:162`), so 0 is a safe sentinel, but the two columns disagree on the convention.
- The window invariant lives only in Rust: `Run::steering_pending()` is
  `from.is_some_and(|f| f > consumed)` (`run.rs:349-352`). The DB expresses none of it: no generated
  column, no view, no CHECK tying the two columns together.
- `messages` is keyed by `(thread_id, seq)` UNIQUE (`0002_m1.sql:183`), but the steering columns carry
  no FK to it; a seq that was never written is representable.
- Writes are fenced only in application SQL: `set_steering_from` uses a `CASE ... MIN` update
  (`runs.rs:244-250`); `set_steering_consumed` uses `GREATEST` plus `lease_fence = $3` (`:273-279`).
  The migration itself enforces nothing about who may advance the window.
- The retry path silently clears the window (`steering_from_seq = NULL, steering_consumed_seq = 0`,
  `runs.rs:454-455`); completion (`set_status`, `:399-409`) leaves the old values in place. The
  four-pass bound is a code constant (`worker.rs:22`, `MAX_STEERING_PASSES`), so messages beyond it
  are neither recorded nor attributed: the window cannot answer "which messages did this run fold
  in, and what happened to the rest?".
- `spec/02-data-model.md:45` names a `SteeringMessage { runId, messageId, at }` entity. The migration
  implements it only as an implicit seq window on `runs`: no `messageId`, no `at`, no disposition.

## Guiding principle

Additive only: never edit `0003_steering.sql` (its checksum is recorded by sqlx) and never drop,
rename or rewrite a column. Every item below is a new migration, a new object, or a new test. Any
wire-visible change is a DEVIATION to record in `docs/PARITY.md` before shipping.

## Upgrades (5 concrete items)

1. **Add a derived `steering_pending` column in a new migration.**
   `ALTER TABLE runs ADD COLUMN steering_pending boolean GENERATED ALWAYS AS
   (COALESCE(steering_from_seq, 0) > steering_consumed_seq) STORED;` This makes the domain predicate
   (`run.rs:349`) queryable in SQL from one definition and needs no sync path; the worker's reload
   check (`worker.rs:140`) can read it. Additive — existing rows backfill automatically.
   *Risk:* generated columns require PG12+ (verify the supported server baseline) and any
   compile-time SQLx query selecting `*` must be regenerated offline. *Mitigation:* read it first
   through a runtime `query_scalar`; add a matrix test over (from, consumed) = (NULL,0), (1,0),
   (2,1), (2,2) asserting equality with `Run::steering_pending()`.

2. **Add the spec's `SteeringMessage` ledger table (additive); keep the seq window as the hot path.**
   `CREATE TABLE steering_messages (id text PK, space_id text, run_id text REFERENCES runs(id),
   message_id text REFERENCES messages(id), enqueued_at timestamptz NOT NULL, consumed_at
   timestamptz, disposition text NOT NULL, UNIQUE (run_id, message_id))` with the same RLS shape as
   `runs`. This supplies the `at` and `messageId` the spec names, an auditable "consumed once" marker,
   and a place to record messages dropped by the four-pass cap or cleared by retry.
   *Risk:* two sources of truth — if ledger and window drift, the audit is worse than none.
   *Mitigation:* one port call writes both in the transaction that currently sets the window
   (`usecases.rs` steering path); a reconciliation integration test asserts that every seq in
   `[from, consumed]` has a consumed ledger row and vice versa, after each pass and after retry.

3. **Encapsulate the window representation in a `run_steering` view.**
   Expose `run_id, first_pending_seq = COALESCE(steering_from_seq, 0), consumed_seq, pending,
   remaining = GREATEST(0, from - consumed), enqueued_at, consumed_at`. The view hides the NULL-vs-0
   split and the CASE/GREATEST arithmetic from every caller, so new consumers cannot re-derive it
   incorrectly. Leave both base columns untouched.
   *Risk:* semantics then live in three places (Rust domain, store SQL, view) and can diverge
   silently. *Mitigation:* a contract test compares view output with `Run::steering_pending()` and
   with the store's `GREATEST` result on the same rows; document the view as read-only for v0.4.0.

4. **Add timestamps to make steering latency observable.**
   `ALTER TABLE runs ADD COLUMN steering_first_enqueued_at timestamptz` and
   `steering_consumed_at timestamptz` (NULL for pre-upgrade rows), written by the same fenced update
   that advances `steering_consumed_seq`. Enables a real write-to-fold-in latency metric and detects
   starvation under the four-pass cap; supplies the `at` field the current migration cannot hold.
   *Risk:* every reader must tolerate NULL (no `expect` — the lints deny it outside tests), and a
   writer that sets the seq but forgets the timestamp reintroduces drift. *Mitigation:* fold both
   into the existing `set_steering_*` port signatures, add a not-null-when-nonzero assertion test,
   and gate on a before/after benchmark of the fenced UPDATE.

5. **Pin the schema contract with a REQ-tagged migration test (additive, no schema change).**
   Extend `crates/infra/tests/database.rs` next to `req_053_the_migrations_apply_and_are_recorded`
   with `req_003_steering_columns_and_comments_survive_migrations`: on a freshly migrated DB assert
   both column names, types, nullability and defaults, that both COMMENTs exist (stable prefix, not
   exact wording), and that the (from, consumed) matrix behaves as the domain expects. Also assert
   the new objects from items 1–4 exist so a later migration cannot silently remove them.
   *Risk:* low. Exact COMMENT text is fragile to wording edits, so match on a prefix. No new
   dependency is needed.

## Sequencing and gates

- Land 5 first (the contract test), then 1 → 3 → 4 → 2: prove the current shape, encode the
  predicate, hide it, add time, then add the ledger. Each item is one forward-only migration and one
  commit mentioning REQ-003.
- Keep `0003_steering.sql` byte-identical: sqlx records its checksum, so an edit breaks existing
  deployments before any new migration runs (CONFIRMED via `db.rs:20` and `req_053`).
- Nothing is dropped, renamed or destructively backfilled; no RLS policy is weakened. Replay the
  REQ-003 goldens and the trace check after each step. Wire-visible deltas go to `docs/PARITY.md`
  before shipping.
