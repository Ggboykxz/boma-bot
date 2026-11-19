# v0.4.0 architecture proposal — `crates/infra/src/stores/memory.rs` (memory store / REQ-022)

Worker #355 · LENS=architecture · target v0.4.0 · READ-ONLY analysis (no code changed).

Status of claims: every "current state" statement below is CONFIRMED by direct reads of
`memory.rs`, `rows.rs`, `app/src/ports.rs` (1267-1324), `domain/src/memory.rs`, `tenant.rs`,
`0013_memory.sql` and `0024_memory_shared_bot_id_nullable.sql` in this worktree. Forward-looking
items are proposals (GUESS until implemented, tested and measured).

## Current structure (CONFIRMED)

- `PostgresMemoryStore` holds only a `PgPool` (`memory.rs:23-33`); it is a stateless adapter over
  `tenant::with_tenant`. All four port methods open their own tenant transaction.
- A document is keyed by `(space_id, scope, bot_id, path)`; `document_id()` (`:16-19`) builds
  `mem:{space}:{scope}:{bot}:{path}`. `scope` is re-derived from `bot_id.is_some()` as raw string
  literals independently in all four methods (`:45-49`, `:121`, `:151`, `:183`); the domain's
  `MemoryScope` (`domain/src/memory.rs:11-47`, with `as_str()`/`FromStr`) is never used by the store.
- `write` (`:37-110`) validates the path in Rust, then runs two statements in one tenant transaction:
  the UPSERT on the projection (`:62-91`) and an INSERT into `memory_revisions` (`:93-104`). Two
  conflict arbiters exist — the table UNIQUE for bot docs and the partial index
  `memory_documents_shared_key` for shared docs (`0024:19-21`) — hence two near-identical SQL
  strings (`:63-80`). The history row reuses `row.revision` returned by the UPSERT, so under the
  row lock the projection and the append-only revision cannot disagree.
- `read`/`list`/`remove` (`:112-202`) select by `space_id`, `scope`, `bot_id IS NOT DISTINCT FROM $3`
  and `path`, relying on RLS alone for tenant isolation (no explicit `space_id = context...`
  predicate, unlike `messages.rs:count_above`). `remove` deletes the projection; revisions follow by
  `ON DELETE CASCADE` (`0013:24`).
- Every driver error becomes `AppError::Unavailable` via `tenant::unavailable` (`:91,104,137,167,197`),
  which logs the raw sqlx error at `warn`; `tenant::is_unique_violation` exists but is unused here.
- `updated_at` is caller-supplied (`NewMemoryDocument.updated_at`, `ports.rs:1276-1277`) and bound as
  a string cast to `timestamptz` (`:88`, `:101`); `content` has no size bound (only `path` is checked,
  `domain/src/memory.rs:49-85`). A System context is refused with `NotFound` in all four methods.

## Guiding principle for v0.4.0

Keep the REQ-022 wire contract and the append-only revision invariant byte-for-byte; make the
store's internal structure explicit: one derivation per key, one SQL shape, one tenant posture, one
failure taxonomy, and a bounded revision lifecycle. Any externally visible change is a DEVIATION
to record in `docs/PARITY.md` before shipping.

## Upgrades (5 concrete items)

1. **One key type, one scope derivation.** Build a private `MemoryKey { id, space_id, scope, bot_id }`
   once per call from `(context, bot_id, path)` using the domain's `MemoryScope`, then pass it to
   every query; delete the four repeated `scope`/`bot_key` blocks and the `unwrap_or("")` trick in
   `document_id`. Test that all four methods derive the same key for the same inputs and that shared
   and bot keys cannot collide.
   *Risk:* `id` is a persisted PK — changing its format would orphan existing rows. *Mitigation:*
   keep `document_id()` byte-identical, make the type a pure refactor with an id golden test, and
   never re-derive the id inside SQL.

2. **Factor the two UPSERT strings into one definition.** Keep both arbiters but build the statements
   from shared `COLUMNS`/`VALUES`/`SET`/`RETURNING` fragments, or move to one unique expression index
   on `(space_id, scope, coalesce(bot_id, ''), path)` and a single `ON CONFLICT` target (needs a
   migration replacing the partial index).
   *Risk:* an expression index changes plan choice and needs a data migration for shared rows; a
   format refactor can silently swap bind order. *Mitigation:* ship the const-fragment refactor in
   v0.4.0 with a byte diff of both emitted SQL strings plus a shared+bot concurrency test; carry the
   index unification as a separate ADR.

3. **One tenant posture, including the System decision.** Add the explicit `space_id = context_space`
   predicate used elsewhere to `read`/`list`/`remove`, mirroring the RLS `OR boma.is_system()` branch,
   or centralize in one helper the fact that memory has no system path and System yields `NotFound`.
   *Risk:* a predicate that forgets the system branch hides documents from future export paths, and
   extra predicates can change plans. *Mitigation:* Member and System integration tests per method
   and `EXPLAIN` against `memory_documents_shared_key` and the table UNIQUE.

4. **Classify failures instead of mapping everything to `Unavailable`.** Distinguish a unique/check/
   FK violation (bad `bot_id`, scope/bot mismatch, arbiter collision) from a real outage; return
   `Rule`/`NotFound` for the former and reserve `Unavailable` for connection errors. Check that
   `NewMemoryDocument.bot_id` belongs to `context.space_id()` before writing (or add a composite FK),
   and validate the timestamp format at the boundary.
   *Risk:* error codes are wire-visible (DEVIATION), and the ownership check is a GUESS — RLS on
   `bots` may already block cross-space references. *Mitigation:* confirm with a differential test
   before claiming a defect; add negative tests for a foreign bot, a bad timestamp and a scope
   mismatch, and replay the REQ-022 goldens.

5. **Make the revision lifecycle explicit and bounded.** Each write appends a full `content` copy
   with no cap or retention (`0013:22-32`); propose a configurable retention/prune policy applied in
   the write transaction, plus an additive `history(path, since_revision)` port so pruning is not
   silent, and a content-size limit beside `MAX_MEMORY_PATH_LENGTH`.
   *Risk:* pruning and size limits are externally visible (DEVIATION) and could break a bot that
   stores large documents; the history port is a feature outside REQ-022. *Mitigation:* instrument
   current content sizes first (no pruning in v0.4.0), gate any cap behind an ADR, and benchmark
   writes per document before and after any retention change.

## Cross-cutting risks / sequencing

- Sequence 1 → 2 → 3 → 4 → 5: lock the key and SQL definitions with tests first, then tenant
  posture, then failure taxonomy, then lifecycle. Items 1 and 2 are internal refactors; 3-5 touch
  behavior and need DEVIATION review before shipping.
- Do not duplicate `validate_memory_path` or row→domain conversion here — they belong to
  `domain/src/memory.rs` and `rows.rs`. No new dependency is required.
- Gate each step with `cargo fmt --check`, `clippy -D warnings`, REQ-022 integration/golden tests
  and `tools/trace_check.py`; mark a claim CONFIRMED only after rerunning the verifying command.

## Out of scope for this module

The `MemoryStore` port shape (`app/src/ports.rs`), row mapping (`rows.rs`), HTTP DTOs
(`api/src/wire.rs`), path validation and `MemoryScope` (`domain/src/memory.rs`), tenant transactions
(`tenant.rs`) and schema/RLS (`0013_memory.sql`, `0024_memory_shared_bot_id_nullable.sql`) belong to
other workers.
