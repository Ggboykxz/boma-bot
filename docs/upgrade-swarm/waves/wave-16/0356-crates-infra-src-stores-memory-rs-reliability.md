# Worker #356 — `crates/infra/src/stores/memory.rs` — LENS: reliability — v0.4.0 proposal

- **Status:** PROPOSAL (read-only review; no source changed). CONFIRMED = verified by reading `memory.rs`, `tenant.rs`, `rows.rs`, both memory migrations, `usecases.rs`, `ports.rs`, `domain/memory.rs`, `infra/tests/m1.rs` and `testkit/memory.rs` in this worktree; GUESS = needs a test/oracle run.
- **Scope:** `PostgresMemoryStore`'s `write`/`read`/`list`/`remove` and the append-only `memory_revisions` chain (REQ-022).

## Summary

Atomicity is sound: `tenant::with_tenant` wraps the upsert and the revision insert in one transaction (`tenant.rs:116,131-142`), so a failed second statement rolls back the first. Reliability risks are elsewhere: the revision chain grows without bound or dedup, `updated_at` is caller-supplied and can regress, there is no optimistic-concurrency check, and the store never reads back the history it writes. One integration test (`req_022_memory_revisions_are_append_only_in_postgres`) covers the happy path, cascade removal and revision count only.

## Failure modes

### Unbounded growth (U)

- **U1 (HIGH, CONFIRMED):** `write` unconditionally appends one row to `memory_revisions` and increments `revision` (`memory.rs:93-104`). No retention window, cap, TTL or compaction exists anywhere in the store or migrations. A caller in a write loop grows the table, its `memory_revisions_document_idx` index, WAL and backups without limit.
- **U2 (HIGH, CONFIRMED):** the store validates the *path* (`validate_memory_path`, 512-char cap) but never the *content*. `content text NOT NULL` (`0013_memory.sql:15`) has no length constraint, so a direct `MemoryStore::write` caller can store an arbitrarily large payload; U1 then multiplies it per write.
- **U3 (LOW, GUESS):** `revision` is `bigint`; `revision + 1` at `i64::MAX` overflows into the `CHECK (revision >= 1)` violation, turning a write into an error. Storage exhaustion (U1/U2) arrives first in practice, so this is a tail risk.

### Stale revisions and concurrency (S)

- **S1 (HIGH, CONFIRMED):** no expected-revision / CAS parameter exists on `NewMemoryDocument` (`ports.rs:1269`) or the port (`ports.rs:1282-1296`). Concurrent writers are last-writer-wins; a reader holding revision N can overwrite a newer revision N+1 with stale content — a lost update, with no way for the caller to detect it.
- **S2 (MED, CONFIRMED):** `updated_at` is passed in from `app.services.clock.now()` (`usecases.rs:4708`) and stored verbatim, not from the database. A slower writer with an earlier timestamp commits last, so `updated_at` can move *backwards* while `revision` moves forwards (`memory.rs:64-68,76-77`). Consumers that order by `updated_at` see stale data first.
- **S3 (MED, CONFIRMED):** `remove` deletes the document and cascades its whole chain; a later write to the same path recreates it at `revision = 1`. A client remembering `revision = N` sees a *lower* revision after recreation (`migrations/0013_memory.sql:24`, `memory.rs:63-67`) and may treat genuine new content as stale. There is no generation id to distinguish "recreated" from "older".
- **S4 (LOW, CONFIRMED):** `memory_revisions` is write-only — no store method reads it. The append-only invariant is asserted in a test by raw SQL (`m1.rs:1239-1245`), not verified by the adapter. `rows.rs:548` maps a negative `revision` to `u64::MAX` with `unwrap_or`, silently masking corruption instead of surfacing it.

### Dedup (D)

- **D1 (MED, CONFIRMED):** an identical write still bumps `revision` and appends a byte-for-byte duplicate revision. An agent that "refreshes" unchanged notes in a loop inflates both the counter and the table (compounds U1).
- **D2 (LOW, GUESS):** no rate limit or idempotency key applies at this layer, so the amplification factor of D1 depends entirely on upstream callers (likely a per-turn memory write — unverified).

### Boundary and duplication (O)

- **O1 (MED, CONFIRMED):** `write` derives `scope` from `document.bot_id` and ignores the caller's `scope` argument (`usecases.rs:4697-4698`, `memory.rs:45-49`); `read`/`list`/`remove` skip path validation entirely. Malformed paths are accepted on non-write paths, so a legacy invalid-path row could be removed/overwritten without validation.
- **O2 (LOW, CONFIRMED):** the bot and shared SQL strings are duplicated (`memory.rs:62-80`) with only the arbiter differing; a future change to one (e.g. adding a guard or a column) can silently drift from the other.

## Concrete hardening (prioritized)

- **P0 — bound the blast radius:** add a `MAX_MEMORY_CONTENT_BYTES` constant and check `document.content.len()` in `write` before the transaction, mirroring `validate_memory_path` (U2). Add a migration-backed retention policy (keep last N revisions per `document_id`) with a scheduled job, or a periodic prune query, and expose the chosen N in typed config (U1). Document that this drops history — it is a policy decision, not a silent default.
- **P1 — concurrency safety:** extend `NewMemoryDocument` with `expected_revision: Option<u64>` and add `AND revision = $expected` to the upsert (or a `SELECT ... FOR UPDATE` guard), returning `AppError::Conflict` on mismatch (S1). Source `updated_at` from `boma.now()`/`CLOCK_TIMESTAMP()` inside the same transaction so it is monotonic with `revision` (S2). These two are behavior changes — see Risks.
- **P2 — dedup and integrity:** skip the insert when `content` equals the current row's content, or coalesce consecutive identical revisions during pruning (D1/D2). Add a `store::verify_revision_chain` test helper that checks `max(revision)` in `memory_revisions` equals the document's `revision`, and replace `unwrap_or(u64::MAX)` with a typed error (S4). Add a `generation`/`uuid` column if S3 must be distinguishable to clients.
- **P3 — consistency and tests:** validate paths in `read`/`list`/`remove` via `validate_memory_path` (O1); extract one `upsert_sql(bot: bool)` helper to remove the duplicated strings (O2); add CONFIRMED-level integration tests for U1/U2/S1/S2/D1 with `testcontainers`.

## Risks & mitigations

- **DEVIATION exposure (AGENTS.md):** P1's CAS and P2's dedup change externally visible revision numbers and conflict behavior; record in `docs/PARITY.md` and obtain operator approval before shipping. Keep the unchanged paths byte-identical and land tests first.
- P0's retention policy deletes historical data: it needs an operator decision on N and an ADR; never enable pruning by default without it. A content cap may reject writes the current build accepts (also a DEVIATION if any spec path relies on oversized content).
- Sourcing `updated_at` from the DB changes the returned timestamp from the app clock by milliseconds-to-seconds; add a differential check against the original before adopting.
- The CAS guard adds a write-read round trip under contention; measure with a benchmark and cap retries to avoid a hot loop.
- Migrations (retention column, generation id) must be additive and backwards-compatible with existing chains; backfill in batches, then enforce.

## Acceptance tests (suggested names)

- `req_022_memory_content_over_cap_is_refused` (U2)
- `req_022_revision_chain_is_bounded_after_prune` (U1)
- `req_022_stale_revision_write_returns_conflict` (S1)
- `req_022_updated_at_never_regresses_with_revision` (S2)
- `req_022_identical_write_does_not_append_revision` (D1)
- `req_022_revision_chain_matches_document_projection` (S4)
- `req_022_recreated_document_exposes_generation_change` (S3)

## Decisions needed (GUESS → operator/spec)

1. Retention policy: keep last N revisions (what N?), TTL, or unlimited with only a content cap (U1)?
2. What content-size cap, and is rejecting larger writes acceptable as a DEVIATION (U2)?
3. Should identical writes be deduplicated, given the port contract "grows by one per write" (D1 / DEVIATION)?
4. Is optimistic concurrency required, and should `updated_at` come from the DB clock (S1/S2 / DEVIATION)?
5. Do clients need to distinguish a recreated document (S3), i.e. is a generation column worth a migration?
