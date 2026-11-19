# v0.4.0 reliability proposal — `crates/infra/src/stores/computer.rs`

- Worker: 336 (upgrade swarm, wave 15). Lens: reliability. Target: v0.4.0.
- Scope: read-only review; this document is the only file written.
- Claim markers: **CONFIRMED** = read from source this session; **GUESS** = inference, verify before shipping.

## 1. Current behaviour (CONFIRMED)

`PostgresComputerStore` implements the five `ComputerStore` methods (REQ-026) over one `PgPool`.
Every read/write runs through `tenant::with_tenant` (member/system scope) or `tenant::as_system`
(`running_idle`), which opens a transaction, sets `boma.space_id` / `boma.system_reason`, then
commits or rolls back. All driver errors collapse to `AppError::Unavailable` via
`tenant::unavailable`, which also logs the raw driver error at `warn`. Rows are projected by
`ComputerRow` / `computer()` in `computer_row.rs`. The schema (migrations 0017/0019/0020) enforces
three uniqueness guarantees: PK `id`, `UNIQUE (space_id, bot_id)`, and a partial unique index for
one team computer per space. `takeover_by` / `takeover_at` are plain text/timestamptz with no expiry
or FK to users. Callers in `usecases.rs` implement takeover, release and the idle sweep.

## 2. Failure modes

### 2.1 Leaked sessions
- **CONFIRMED**: takeover is read → check-holder → full-row `update` in the app layer
  (`computer_takeover`). The store's UPDATE is unconditional (no version/CAS/row lock) and the hold
  has no expiry: if a holder disconnects or crashes before `computer_release`, `takeover_by` stays
  set forever and all bot-driven input is refused — a leaked human-control session.
- **CONFIRMED**: `sweep_idle_computers` calls `runtime.stop(external_id)` *then* `update`. A crash or
  DB error between them leaves a stopped provider container referenced as `status='running'` with a
  dead `external_id`; the `?` on `update` also aborts the whole sweep, stranding later rows.
- **CONFIRMED**: a `status='running'` row with `external_id IS NULL` is skipped forever by the
  sweeper's `let Some(external_id) = ... else { continue }` — an unreapable stale row.
- **GUESS**: a provider container can die (host vanishes) while the row still says `running`; this
  store has no reconciliation path for that divergence.

### 2.2 Takeover races
- **CONFIRMED**: two concurrent `computer_takeover` calls both read `takeover_by = NULL`, both pass
  the exclusivity check, both `update` the full row; last writer wins and *both* callers get `Ok`.
  The exclusive-hold invariant (REQ-029) breaks silently at every layer.
- **CONFIRMED**: the same lost-update pattern applies to every field `update` writes — status
  transitions, `screen_generation` bumps (REQ-030), `external_id`, timestamps. A stale reader can
  clobber a newer writer; `updated_at` is client-supplied, not DB time, so it is not a version.
- **CONFIRMED**: `running_idle` is a plain `SELECT ... fetch_all` with no `FOR UPDATE SKIP LOCKED`;
  two overlapping sweeper runs receive the same rows and both stop the same sessions.

### 2.3 Orphan rows / misclassified errors
- **CONFIRMED**: `create` uses `ON CONFLICT (id) DO NOTHING ... fetch_one`. An id collision yields
  zero rows → `RowNotFound` → `Unavailable`. Collisions on `(space_id, bot_id)` or the team-per-space
  index are not covered by `ON CONFLICT (id)` and surface as a raw unique violation → also
  `Unavailable`. `tenant::is_unique_violation` exists but is unused here, so "already exists" is
  indistinguishable from "DB down": retries spin and the port's documented error is not honoured.
- **CONFIRMED**: `i32::try_from(u64).unwrap_or(i32::MAX)` silently clamps oversized limits; i32::MAX
  then violates the CHECK bounds → `Unavailable`. The read path maps negative i32 to `u64::MAX`,
  masking corruption instead of surfacing it.
- **CONFIRMED**: `computer_runs` / `computer_execution_leases` ("one open per computer") are never
  touched by this store, so a computer can flip running/stopped while a run or lease row stays open.
- **CONFIRMED**: `running_idle` has no `LIMIT`/`ORDER BY` and holds a system-context (RLS-lifted)
  transaction open for an unbounded `fetch_all`.

## 3. Concrete hardening (v0.4.0)

H1. Add boundary-level CAS store methods: acquire with `WHERE id=$1 AND takeover_by IS NULL`,
    release with `WHERE id=$1 AND takeover_by=$2`, and check `rows_affected == 1`; map 0 rows to
    `ScreenTaken`/`Conflict`. Add a `takeover_at` lease so abandoned holds self-clear.
    *Risk*: changes port signatures and takeover behaviour → DEVIATION; lease duration from spec/measurement.
H2. Add optimistic concurrency to `update`: a `version` column (or `WHERE updated_at = $expected`)
    bumped with DB `now()`; a stale write returns a typed `AppError::Conflict`.
    *Risk*: needs a migration and every caller touched; DEVIATION if callers now see conflicts.
H3. Make the idle sweep claim atomically: `running_idle` selects `FOR UPDATE SKIP LOCKED` with a
    bounded `LIMIT`, or move claim + stop + update into one store transaction.
    *Risk*: lock contention and partial batches; the sweep must be re-run to completion.
H4. Reconcile provider state: clear `external_id` on stop, reap `running` rows with no handle, make
    provider stop idempotent, and queue a retry before updating the row.
    *Risk*: needs runtime idempotency; extras may exceed spec scope — propose, do not ship unasked.
H5. Classify errors: use `tenant::is_unique_violation` in `create` to return a typed already-exists
    error; stop clamping (validate limits in the domain, return `Rule` on overflow); stop mapping
    `RowNotFound` to `Unavailable`. *Risk*: new error variants ripple through callers/metrics.
H6. Remove silent corruption: reject negative/out-of-range stored values in `computer()` instead of
    `unwrap_or(MAX)`, and let the DB own `updated_at`. *Risk*: legacy bad rows fail to load; clean up
    in a migration first.
H7. Observability: add a span with computer id, status transition and a hashed takeover user; ensure
    `tenant::unavailable` never logs a connection string. *Risk*: slightly higher log volume.

## 4. Test plan (REQ-tagged)

- `req_029_concurrent_takeover_has_one_winner`: two racers → one `Ok`, one `ScreenTaken`.
- `req_029_takeover_lease_expires`: expired `takeover_at` lets a new holder acquire.
- `req_026_create_conflict_is_typed`: duplicate id and duplicate (space,bot) → already-exists, not Unavailable.
- `req_026_update_stale_write_conflicts`: older snapshot rejected, newer value preserved.
- `req_026_sweep_claims_rows_once`: two concurrent `running_idle` calls return disjoint sets.
- `req_026_running_row_without_handle_is_reaped`: sweeper does not skip it forever.

## 5. Deviations / questions to owner

- H1/H2/H4 change observable API/takeover behaviour — record in `docs/PARITY.md` and wait for approval.
- Confirm whether takeover should have a lease (and its duration) or rely on explicit release only.
- Confirm whether the idle sweep can run concurrently in deployment; this sets H3's urgency.
