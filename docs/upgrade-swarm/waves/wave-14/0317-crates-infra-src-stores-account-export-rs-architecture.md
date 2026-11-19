# v0.4.0 architecture proposal — `crates/infra/src/stores/account_export.rs`

Worker #317 · LENS=architecture · target v0.4.0 · READ-ONLY analysis (no source changed).

Status of claims: every "current state" statement below is CONFIRMED by direct reads of this
file, `crates/app/src/ports.rs:651-754`, `crates/app/src/account_export.rs:260-345`,
`crates/infra/src/tenant.rs:41-143`, `migrations/0046_account_exports.sql`, and the store's call
sites (`crates/bin/src/commands.rs:212`, `crates/infra/tests/m1.rs`, `crates/testkit/src/memory.rs`).
Forward-looking items are proposals (GUESS until implemented and verified). English per repo disk
convention; no gate weakened.

## Current structure (CONFIRMED)

- One thin adapter, `PostgresAccountExportStore { pool: PgPool }` (45-55), implements the four-method
  `AccountExportStore` port (ports.rs:718-754): `bundle`, `create`, `find`, `mark_ready`.
- **Every method opens its own full system transaction.** `as_system` → `scope` (tenant.rs:95-143)
  does `pool.begin()`, two `set_config`, the body, then `commit`/`rollback`. So `find`, `bundle` and
  `mark_ready` are three independent transactions, not one lifecycle.
- `bundle` (60-209) is a single consistent snapshot of **seven sequential queries** under one
  `#[allow(clippy::too_many_lines)]` (57). Dependencies are chained in Rust: bots → `bot_ids` →
  threads + bot_memory; threads → `thread_ids` → messages; plus profile, routines, artifacts.
  Batching uses `= ANY($1)` (103, 125, 143) — no N+1 (good). Roughly 140 lines of glue.
- Mapping is split: six row types reuse `crate::stores::rows::*`; two are local raw structs
  (`ExportRowRaw` 17-26, `ArtifactMetaRow` 28-41) mapped inline (182-193).
- **Authorization is split between layers.** `find` enforces ownership in SQL
  (`WHERE id = $1 AND user_id = $2`, 246-248), but `mark_ready` updates by `id` alone (291) and
  `bundle` filters by `user_id`/`bot_id`/`thread_id`; the use case calls `find` first
  (account_export.rs:317-322) and relies on that earlier check.
- **Schema/port mismatch:** the table CHECK admits `pending | ready | failed` (migration:8), but no
  method ever writes `failed`, and `ExportRow.status` / the SQL column are free `String`s.
- All driver errors collapse to `AppError::Unavailable` via `tenant::unavailable`; `fetch_one` on a
  missing profile (73) therefore surfaces as `Unavailable`, not `NotFound`.
- Reads are unbounded: messages, memory documents and artifacts have no LIMIT/cap in SQL. `skipped`
  is only populated later at render time (account_export.rs:326-332). No `#[cfg(test)]` module in
  this file; coverage lives in `crates/infra/tests/` and the memory store (`testkit/src/memory.rs`).

## Guiding principle for v0.4.0

Keep the adapter a system-context boundary, but make the *lifecycle* explicit: one consistent read,
one ownership-checked status transition, typed states, bounded memory. Split the monolith without
adding transactions or weakening REQ-044's cross-tenant justification.

## Upgrades (5 concrete items)

1. **Decompose `bundle` into typed per-entity helpers behind one transaction.** Introduce a private
   `AccountExportReader<'c> { conn: &'c mut PgConnection }` with `profile/bots/threads/messages/
   memory/routines/artifacts` methods; keep the single `as_system` closure and the current chaining,
   then drop the `too_many_lines` allowance. *Risk:* higher-ranked/boxed-future lifetimes (tenant.rs:
   39,57) resist borrowing `&mut PgConnection` in many small futures. *Mitigation:* keep all helpers
   in one closure, pass `&mut *conn` as today, extract only mapping + query construction first;
   assert the seven-query order is unchanged. No wire change.

2. **Make the lifecycle coherent: one ownership-checked status transition.** Give `mark_ready` a
   `user_id` and a `WHERE id = $1 AND user_id = $2 AND status = 'pending'` predicate, returning rows
   affected; have the use case re-check ownership in the same transaction that flips status. Keep zip
   rendering **outside** the DB transaction (artifact I/O/`spawn_blocking` must not pin a connection).
   *Risk:* a longer-lived transaction raises lock/pool pressure, and a `pending→ready` guard makes a
   concurrent second download a no-op — client-visible timing, so log as a DEVIATION candidate in
   `docs/PARITY.md`. *Mitigation:* never hold the txn across rendering; test the two-concurrent-
   downloads case; keep bytes returned even if the status update is best-effort.

3. **Close the ownership perimeter of the read.** The threads query filters only on `bot_id = ANY`
   (103) and messages only on `thread_id = ANY` (125); add explicit owner predicates (threads
   `AND user_id = $user`, messages joined to owner-verified threads) so a co-owned thread cannot leak
   into one person's archive. *Risk:* if REQ-044 intends "all messages on my bots" regardless of
   author, a filter would *drop* legitimate rows — an externally visible DEVIATION. *Mitigation:*
   confirm intent in `spec/01-requirements.md` REQ-044 and golden fixtures first; prefer a
   differential test against `MemoryAccountExportStore` before changing the SQL.

4. **Align schema and port with a typed status.** Replace the free `status: String` with an
   `ExportStatus` enum (`Pending | Ready | Failed`) at the port boundary and add `mark_failed(id,
   reason)` (plus a bounded `error` column if the reason must persist), so the DB's `failed` state is
   reachable. *Risk:* status strings are public API values. *Mitigation:* keep JSON serialization
   byte-identical (`pending`/`ready`/`failed`) with a documented fallback for unknown values; make
   `mark_failed` additive and preserve `ExportRow`'s `PartialEq` for fixtures.

5. **Bound large-account reads and surface truncation.** Add keyset pagination or configurable caps
   to messages, bot_memory and artifacts, and record any truncation in the existing `skipped` array
   so the archive stays honest. *Risk:* truncation is silent data loss and a DEVIATION if a golden
   expects full content. *Mitigation:* default cap generous/no-op in v0.4.0, make it explicit config
   (document in `docs/STATE.md`), and assert each skipped path appears in `skipped`.

## Cross-cutting risks / sequencing

- Sequence 1 → 3 → 2/4 → 5. Step 1 is mechanical and low risk; step 3 changes exported content and
  must be validated against the spec/oracle first; 2 and 4 are independent afterwards; 5 depends on
  the `skipped` plumbing from 2.
- All four methods legitimately use `as_system` (REQ-044 crosses every space). Never replace it with
  `with_tenant`, and keep `SystemReason::AccountExport` as the closed-enum justification.
- Deduplicate the three clone-then-close prologues (`id.to_owned()` etc.) via one private adapter,
  but keep error mapping in `tenant::unavailable` so no driver detail reaches clients (REQ-054).
- Verification gate per step: `cargo fmt --check`, `clippy -D warnings`, `tests/` integration,
  golden + contract suites; add a `#[cfg(test)]` module here for status parsing and cap logic.

## Out of scope for this module

Zip rendering, artifact byte loading and `skipped` caps live in `crates/app/src/account_export.rs`;
the API download route and the in-memory test double belong to other modules. This proposal only
reshapes the store's transactions, ownership checks, status typing and read bounds.

## Evidence

CONFIRMED — read this session: `account_export.rs:1-306`; `ports.rs:651-754`;
`account_export.rs:260-345`; `tenant.rs:41-143`; `migrations/0046_account_exports.sql:1-21`; grep of
the `PostgresAccountExportStore` call sites in `crates/{bin,infra,testkit}`.
GUESS — that co-owned threads/messages actually exist in seeded data (verify with a query before 3);
that any golden asserts full unbounded message content (verify before 5).
