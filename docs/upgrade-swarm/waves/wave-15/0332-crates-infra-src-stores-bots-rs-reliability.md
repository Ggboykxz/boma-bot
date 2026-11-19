# Worker #332 — `crates/infra/src/stores/bots.rs` — LENS: reliability — v0.4.0 proposal

- **Status:** PROPOSAL (read-only review; no source changed). CONFIRMED = verified by reading this worktree's file, `migrations/0002_m1.sql`, `migrations/0054_external_agents.sql`, `tenant.rs`, `rows.rs`, `ports.rs`, `usecases.rs`, `external_agents.rs`, `acp_agents.rs` and `spec/02-data-model.md` / `03-api-contract.md`. GUESS = needs an oracle/test run.
- **Scope:** the two `BotStore` methods `create_with_thread` (lines 29–102) and `find` (104–129), plus the invariants they own: the bot↔thread pair, the name, and the executor records that hang off the bot id.

## Summary

The transactional pair-write is the right shape (REQ-004/REQ-042), but the store trusts its inputs and its schema too much: names have no uniqueness, the bot↔thread link is unenforced in one direction, and executor state is neither freshness-checked nor cleaned on archive. Every failure is flattened into `AppError::Unavailable` by `tenant::unavailable` (`tenant.rs:153–156`), so retries cannot distinguish a conflict, a serialization abort, or a genuinely down database. The module has no local test (CONFIRMED: no `#[cfg(test)]` in `bots.rs`).

## Failure modes

### Duplicate and ambiguous names (N)

- **N1 (MED, CONFIRMED):** `bots` has no unique index on `(space_id, name)` (`0002_m1.sql:141–153`); `create_bot` only checks *format* via `Bot::validate_name` (`usecases.rs:359`), and the INSERT (lines 39–57) never checks for an existing name. Two concurrent creates with the same name both commit. A third request loses.
- **N2 (MED, CONFIRMED):** `@name` mention fan-out is exact (`spec/03-api-contract.md:74`). Two live bots with the same name in a space make mention routing ambiguous; the roster (`stores/roster.rs`) renders identical labels. Account import calls `create_bot` (`app/account_export.rs:570`) with fresh ids, so importing an archive into a space that already has that name is a silent duplicate generator.
- **N3 (LOW, GUESS):** `tenant::unavailable` hides `sqlx` unique violations; even if a `UNIQUE` index is added, the caller would see `Unavailable` and might retry forever. `tenant::is_unique_violation` (`tenant.rs:158–164`) exists but is unused by this module (CONFIRMED by grep).

### Delete cascade and pair atomicity (C)

- **C1 (HIGH, CONFIRMED):** `bots.thread_id` is deliberately *not* a foreign key (schema comment 148–149) to break the write-cycle. Consequence: deleting a thread directly leaves a live bot whose `thread_id` points at nothing. `find` then builds a `Bot` carrying a dangling id, while `ThreadStore::find`/`for_bot` return `None` — a half-created bot in the *delete* direction, exactly what the module header promises to prevent.
- **C2 (MED, CONFIRMED):** `rows::bot` uses `ThreadId::new(row.thread_id.unwrap_or_default())?` (`rows.rs:267`). A NULL `thread_id` (schema allows it) becomes `""`, and `OpaqueId::new` rejects blank ids (`domain/src/ids.rs:29–35`). So `find` returns an error for a structurally damaged bot instead of a usable value or a clean `NotFound`; the failure is misclassified as `Unavailable`.
- **C3 (MED, CONFIRMED):** the bot is the cascade root for a wide FK graph — threads (`0002_m1.sql:158`), memory, bot secrets, bot mail, messaging, notifications, ACP/AG-UI executors, groups, pages (`ON DELETE SET NULL`). `BotStore` exposes no delete, so removal only happens via space/user cascades or testkit cleanup (`tests/common/mod.rs:367`). A bot delete in one transaction can cascade over an arbitrarily long message history: unbounded lock time and a plausible statement-timeout rollback.
- **C4 (LOW, GUESS):** `create_with_thread` checks neither `bot.id != thread.id` nor id collisions; a duplicate PK surfaces as `Unavailable`. The returned structs are rebuilt from inputs, not read back, so a future DB trigger/default divergence would be invisible until a later `find`.

### Stale executors (E)

- **E1 (HIGH, CONFIRMED):** both executor records are keyed by `bot_id` with `ON DELETE CASCADE` (`0054_external_agents.sql:5`, ACP migration likewise), so *deleting* a bot cleans them — but *archiving* only sets `archived_at` and hides the bot from lists (spec 02:38). Executor rows survive archive; unarchive resurrects a possibly rotated `base_url`/`executor` with no cutoff.
- **E2 (MED, CONFIRMED):** `ExternalAgentStore::connect` upserts on `bot_id` alone and writes `updated_at`, but nothing reads `updated_at` for freshness; there is no generation/fence. `disconnect` (`external_agents.rs:88–108`) is a bare `DELETE` that returns `Ok(())` when zero rows match, so a stale or mistyped bot_id reports success. It also requires `context.space_id()` (line 94): a system cleanup context gets `NotFound`, so no automated stale-row sweeper is possible.
- **E3 (LOW, GUESS):** in-flight runs that captured an executor config before a reconnect keep using the old endpoint; without a fence token a late run can answer from a revoked executor. No test covers archive/delete/disconnect against a live executor.

## Concrete hardening (prioritized)

- **P0 — classify errors:** add a small `map_write_error` helper in `bots.rs` that calls `tenant::is_unique_violation` *before* `tenant::unavailable` and returns a typed `AppError::Conflict`/`AppError::Rule("bot name already in use")` for name/id conflicts; log SQLSTATE at debug. `find` should map a bad `thread_id` to `AppError::NotFound` (corrupt row = unusable bot) rather than a domain parse error.
- **P0 — atomic pair:** make `bots.thread_id` a `DEFERRABLE INITIALLY DEFERRED` FK to `threads(id) ON DELETE SET NULL`, then add a deferred constraint trigger asserting `bot.thread_id` is non-null and points back to a thread with `bot_id = bot.id` at commit. This preserves the insert order (both rows land in one transaction) while forbidding an orphan bot on a direct thread delete. Add an integration test `REQ-004_bot_and_thread_cannot_be_half_deleted` for both directions.
- **P1 — name uniqueness:** migration adding `UNIQUE (space_id, lower(btrim(name))) WHERE archived_at IS NULL`, so archived names remain reusable (matches "archive keeps history"). Add a preflight `SELECT` in `create_bot` for a friendly conflict, keeping the index as the race-safe backstop. Treat the new rejection as an externally visible behaviour change: log a DEVIATION in `docs/PARITY.md` and a decision in `docs/STATE.md` — the spec's Bot table lists no name-unique constraint, so this needs the operator's veto before shipping.
- **P1 — executor lifecycle on archive/delete:** in the same transaction that archives a bot, delete its `bot_external_agents` and `bot_acp_agents` rows (new store method or use-case change); add a `joined` executor lookup that filters `bots.archived_at IS NULL` so an archived bot never resolves. Add `updated_at`-based fencing (or a generation column) and make `disconnect` report zero-row deletes as `NotFound`; allow a registered `SystemReason` for sweeper cleanup. Tests: `REQ-059_archive_removes_executor`, `REQ-061_disconnect_is_not_silently_idempotent`, `REQ-004_delete_cascades_executor_rows`.
- **P2 — bounded cascade & observability:** for large spaces, replace an unbounded single `DELETE` with a batched purge (child rows first, with a configurable batch size and a deadline) inside one transaction, or at minimum log cascade row counts and statement duration with `tracing`. Compare `bot.id` and `thread.id` before insert and reject equality. Add a concurrency test for two simultaneous creates with the same name asserting exactly one commit and one typed conflict.

## Risks / trade-offs

- A deferred FK plus trigger changes the schema's documented "no cycle" decision and adds hidden write-time work; migration must backfill existing bots first, and the trigger must not recurse on the bot-delete cascade. Verify on the pinned Postgres before rollout.
- Enforcing name uniqueness can reject archive imports and duplicate-name workflows that the reference allowed; the partial index and a documented DEVIATION mitigate this, but the operator must accept the behaviour change.
- Deleting executor rows on archive means unarchiving loses configuration; decide whether to retain them in an `archived` table rather than delete, and document the resurrect-vs-lose trade-off.
- System-context cleanup needs a new closed `SystemReason` variant (deliberate code change per ADR-002/D-09); do not widen `disconnect` to free-form tenant escape.
- Fencing executors adds a column and a version check on every run start; keep it opt-in per bot so existing AG-UI/ACP behaviour is unchanged until the operator approves the DEVIATION.
- Batched cascade changes lock duration and failure granularity; measure with a large seeded space before choosing between it and a single transactional delete.
