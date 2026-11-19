# Worker #338 — `crates/infra/src/stores/computer_row.rs` — LENS: reliability — v0.4.0 proposal

- **Status:** PROPOSAL (read-only review; no source changed). CONFIRMED = verified by reading `computer_row.rs`, `stores/computer.rs`, `crates/domain/src/computer.rs`, `crates/domain/src/ids.rs`, `crates/domain/src/error.rs`, `crates/app/src/error.rs`, `crates/infra/src/tenant.rs`, `migrations/0017_computers.sql`, `migrations/0019_computer_takeover.sql`, `migrations/0002_m1.sql` and sqlx 0.9.0 `row.rs`/`connection/resolve.rs` in this worktree; GUESS = needs a test/oracle run.
- **Scope:** the `ComputerRow` decode contract (`COMPUTER_COLUMNS`, `sqlx::FromRow`) and the `computer()` projection used by every `PostgresComputerStore` method (`find`, `find_team`, `find_by_id`, `create`, `update`, `running_idle`), covering REQ-026..031.

## Summary

The mapper is a pure, 22-line projection with no validation: it trusts the DB CHECKs, trusts three silent `u64::MAX` clamps, and trusts hand-maintained column strings. Reliability breaks at the decode seam: a bad row is either a retryable "database unavailable" (poisoning the whole idle sweep) or a client-fault 4xx, never an actionable corruption signal. One concrete defect is already present: `takeover_at` is selected twice.

## Failure modes

### Row decode errors (D)

- **D1 (MED, CONFIRMED):** `COMPUTER_COLUMNS:40` lists raw `takeover_at` and `:42` lists `boma.rfc3339(takeover_at) AS takeover_at` — 20 columns for a 19-field struct. sqlx 0.9.0 resolves name lookups via a `HashMap` built in ordinal order (`resolve.rs:109` `column_names.insert`), so the *last* duplicate wins and decoding happens to succeed; the raw `timestamptz` column is wasted. The binding is positional-fragile: any reordering that makes the raw column last breaks `Option<String>` decode with a type mismatch.
- **D2 (HIGH, CONFIRMED):** `FromRow`/`query_as` failures are raised at fetch time and mapped by `tenant::unavailable` (`tenant.rs:153–156`) to `AppError::Unavailable` — the *only* retryable variant (`app/src/error.rs:49–51`). A single corrupt value (bad type, unexpected NULL in a NOT NULL column) is indistinguishable from an outage. `running_idle` (`computer.rs:221`) maps *all* rows in one batch, so one bad row fails the entire sweep and the retry re-hits the same poison row forever.
- **D3 (HIGH, CONFIRMED):** failures from `computer()` itself (`?` on `ComputerId/SpaceId/BotId::new` at 47–49, `ComputerStatus::from_str` at 52, `UserId::new` at 55) become `AppError::Rule`, which `is_client_fault()` classifies as the caller's fault (4xx). A blank id or unknown status in the DB is server-side corruption: no retry, no alert, wrong owner.
- **D4 (LOW, GUESS):** `boma.rfc3339` uses `to_char(..., 'HH24:MI:SS')` (`0002_m1.sql:49–53`): second precision, so sub-second input is truncated on read-back and in-memory string ordering may diverge from DB ordering. Infinity/out-of-range timestamp rendering not verified.

### Null handling (N)

- **N1 (HIGH, CONFIRMED):** `takeover_by`/`takeover_at` (migration 0019:6–8) have no CHECK that both are NULL or both set. A half-populated pair is projected verbatim (54–56) and drives `ScreenTaken` decisions with no audit time.
- **N2 (HIGH, CONFIRMED):** `screen_generation`, `memory_limit_mb`, `idle_minutes` use `unwrap_or(u64::MAX)` (53, 58, 60). A negative/corrupt `i32` silently becomes `u64::MAX`: `screen_generation = MAX` invalidates every screen capability indefinitely; memory/idle `MAX` are clamped back to `i32::MAX` on the next `update()` (`computer.rs:142,145,147`), violating the DB CHECKs `64..4096` / `1..1440` (`0017:21,24,26`) → UPDATE error → `Unavailable` → retry storm. Silent read corruption becomes a write failure.
- **N3 (MED, CONFIRMED):** `cpu_limit: f64` (59) is unvalidated; NaN or out-of-range values survive into the domain and back into `update()` (`computer.rs:146,183`). The CHECK (`0017:25`) guards writes only, not reads.
- **N4 (MED, CONFIRMED):** no status↔timestamp invariant: `running` with `stopped_at` set, or `stopped` with `started_at` but no `stopped_at`, is returned as-is. `running_idle` (`computer.rs:213–215`) keys only on status + `last_used_at`.

### State drift / write-back (S)

- **S1 (HIGH, CONFIRMED):** `create()` uses `ON CONFLICT (id) DO NOTHING ... RETURNING` + `fetch_one` (`computer.rs:106–126`). An at-least-once replay with the same id returns no row → `RowNotFound` → `Unavailable` (retryable) → permanent retry instead of returning the existing computer; a `(space_id, bot_id)` conflict is swallowed identically.
- **S2 (MED, CONFIRMED):** `update()` ignores `rows_affected` (`computer.rs:195–201`): a stale/deleted id returns `Ok(())` while nothing was written.
- **S3 (MED, CONFIRMED):** `COMPUTER_COLUMNS` is a hand-maintained string duplicated across four queries and fed through `AssertSqlSafe(format!(...))`, so there is no compile-time column/type check against `ComputerRow`. Field/projection drift is caught only at runtime as `Unavailable` (D2).

## Concrete hardening (prioritized)

- **P0 — D1/S3:** delete raw `takeover_at` from the projection (keep the `rfc3339` alias); add a unit test asserting the projection's column names list is unique and has the same names/count as `ComputerRow`'s fields, derived from one shared `const` array joined at use sites.
- **P0 — D2/D3:** split fetch errors from decode/validation errors. Introduce an internal `AppError::CorruptRow { table, id }` (non-retryable, 5xx, log at `error!` with the raw value and column) instead of funnelling both through `Unavailable`/`Rule`; keep the wire error envelope stable. Requires an ADR (new internal variant) — do not change the public error shape without operator sign-off.
- **P0 — N2/N3:** replace the three `unwrap_or(u64::MAX)` sentinels with checked conversions that return `CorruptRow`; validate `cpu_limit.is_finite()` and the known bounds. Before enforcing, run a one-shot audit query for negative/out-of-range values and backfill or quarantine offenders.
- **P1 — D2 (sweep):** in `running_idle`, map rows individually: log+count bad rows and continue the batch, emitting a metric and a bounded warning; never let one row abort all idle releases. Pair with a quarantine/dead-letter record so skipping is visible, not silent.
- **P1 — S1/S2:** make `create()` replay-safe (`ON CONFLICT (id) DO UPDATE SET id = EXCLUDED.id RETURNING ...` or a follow-up `SELECT` on no row), and map a `(space_id, bot_id)`-only conflict to `Conflict`; check `rows_affected == 1` in `update()` and return `NotFound` otherwise. Both change observable port behavior: ADR + `docs/PARITY.md` entry.
- **P1 — N1/N4:** validate cross-field invariants in `computer()` — `takeover_by.is_some() == takeover_at.is_some()`, status↔timestamp coherence, `created_at <= updated_at` — first in log-only mode with a metric, then enforce after a data audit. Add the matching two-column CHECK in a migration so new writes cannot create the pair.
- **P2 — D4:** decide explicitly whether second precision is part of the format contract; if so, document it and normalise on write, otherwise select `to_char` with microseconds and treat it as a data-format DEVIATION.

## Risks & mitigations

- Tightening decode rejects rows that currently "work" via sentinels; stage as audit → log-only → enforce, with a rollback flag, so historic rows do not take down reads on deploy.
- A new error variant touches all `AppError` matches (`crates/api/src/error.rs`, `crates/app/src/service.rs`) and test mocks; land together and keep the client envelope unchanged.
- `create()` returning an existing row on replay can mask a payload mismatch: compare provider/space/bot before returning, else `Conflict`.
- Skipping bad rows in the sweeper could hide drift; the metric/alert is mandatory, not optional.
- Adding the takeover-pair CHECK will fail on half-populated historic rows; backfill before the migration.

## Acceptance tests (suggested names)

- `req_030_computer_columns_are_unique_and_match_row_fields`
- `req_026_corrupt_row_is_not_reported_as_database_unavailable`
- `req_026_unknown_status_is_a_server_error_not_a_client_fault`
- `req_027_negative_limits_and_nonfinite_cpu_are_rejected`
- `req_029_takeover_pair_is_atomic_and_null_consistent`
- `req_028_create_replay_returns_existing_computer`
- `req_028_update_missing_row_returns_not_found`
- `req_026_one_bad_row_does_not_abort_idle_sweep`

## Decisions needed (GUESS → operator/spec)

1. Is an internal `CorruptRow` error variant acceptable, or must the existing `Unavailable`/`Rule` envelope hold unchanged (D2/D3)?
2. Should bad rows in the idle sweep be skipped-and-alerted or fail-closed (P1 sweep)?
3. Is the silent `u64::MAX` sentinel relied on by any caller, or can it become a hard error (N2)?
4. On `create()` replay with a different payload: return existing, or `Conflict` (S1)?
5. Is second-precision RFC3339 the intended wire/data-format contract (D4)?
