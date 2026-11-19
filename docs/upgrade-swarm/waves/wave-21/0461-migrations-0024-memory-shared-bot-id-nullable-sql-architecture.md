# 0461 — migrations/0024_memory_shared_bot_id_nullable.sql — architecture (v0.4.0)

Worker #461 · module `migrations/0024_memory_shared_bot_id_nullable.sql` · lens: architecture ·
READ-ONLY review of the `before-m5` worktree at v0.3.0-era source. Proposals only; this change
writes no code and edits no migration.

Status of claims: "current state" below is CONFIRMED by direct reads of this file, `0013_memory.sql`,
`docs/STATE.md` (1058-1077) and the companion wave-16 memory-store review (0355). Forward-looking
items are GUESS until implemented, tested and measured. Everything here is additive-only: no
recorded migration is edited or reordered; any v0.4.0 schema work ships as a new file.

## What the file is — and the invariant it restores

21 lines, one concern: make "shared" memory writable. `0013` encoded shared as `bot_id = ''` with
`bot_id NOT NULL ... REFERENCES bots(id)`; the FK rejects `''`, so no shared row could ever exist —
a defect the `Option<BotId>` in-memory adapter masked until the real Postgres run (STATE.md 1065-1077).
0024 moves the shared marker to NULL, which a FK ignores, in six statements: `DROP NOT NULL`,
`DROP DEFAULT`, `DROP CONSTRAINT IF EXISTS memory_documents_check`, `SET DEFAULT NULL`, add the
NULL-shaped `memory_documents_scope_bot_id_check` CHECK, and a partial unique index
`memory_documents_shared_key (space_id, path) WHERE scope = 'shared' AND bot_id IS NULL` as the upsert
arbiter. Existing rows are untouched (only bot rows could exist). No RLS/policy statement is present,
so `0013`'s tenant isolation is left intact — a clean, minimal blast radius (CONFIRMED).

Architecturally the file is three coupled decisions in one place: the **sentinel** (NULL vs `''`), the
**validation boundary** (renamed CHECK, still defensively requiring `bot_id <> ''` for scope `bot`),
and the **uniqueness arbiter** (partial index because a plain UNIQUE treats NULLs as distinct). The
Rust store consumes all three (wave-16: two near-identical UPSERT strings, one per arbiter), so this
migration is a schema-level contract, not an isolated DDL tweak.

## Guiding principle for v0.4.0

Keep 0013 and 0024 byte-for-byte immutable (checksum = contract), keep the REQ-022 wire contract and
the append-only revision invariant unchanged, and express any further change as a *new* migration.
Prefer making the NULL contract explicit and test-checked over clever schema. Any externally visible
change is a DEVIATION for `docs/PARITY.md` before shipping.

## Proposed upgrades for v0.4.0 (additive; in dependency order)

### U1. Enforce "new schema, new migration" as a hard architecture rule
Never edit 0013/0024 to tidy them. Record their checksums and add any follow-up as a higher-numbered
file (e.g. `0060_memory_shared_arbiter_*.sql`) so recorded DBs do not hit `VersionMismatch`. This is
the exact lesson from the 0025 incident (STATE.md 1058-1063).
*Risk:* an in-place "fix" to a comment or statement changes the checksum and breaks the whole shared
suite at once. *Mitigation:* CI gate comparing recorded vs on-disk checksums; a docs entry marking
0013/0024 frozen; owner sign-off for any exception.

### U2. Make the NULL-vs-empty-string sentinel a single, test-checked contract
0024 keeps `bot_id <> ''` inside the new CHECK for scope `bot` (line 15) even though the bug made
`''` rows impossible — harmless but a second, implicit sentinel. Add an additive migration and tests
that assert the invariant instead of relying on a comment: no `''` values, bot scope ⇒ non-NULL FK-
valid id, shared scope ⇒ NULL, and both PG and in-memory adapters derive the same `MemoryScope`.
*Risk:* tightening the CHECK further could reject legacy rows if any `''` ever slipped in under a
deferred FK; that is GUESS, not CONFIRMED. *Mitigation:* a pre-flight `SELECT` assertion in the new
migration before any constraint change; differential adapter test + REQ-022 golden replay.

### U3. Harden the DDL for scale without giving up atomicity
The file has no `-- no-transaction` directive (CONFIRMED); under SQLx's default (GUESS until runner
config is checked) it runs as one transaction — good for atomicity, but `DROP NOT NULL`,
`ADD CONSTRAINT` (no `NOT VALID`) and `CREATE UNIQUE INDEX` (not `CONCURRENTLY`) take locks that block
writes on a large table. Define a documented threshold and a recipe: beyond it, use `ADD CONSTRAINT
... NOT VALID` + `VALIDATE CONSTRAINT` and `CREATE UNIQUE INDEX CONCURRENTLY` in a separate,
deliberately non-transactional additive migration.
*Risk:* `CONCURRENTLY` cannot run inside a transaction, so opting into it trades atomicity for
availability, and a partial failure would need manual cleanup. *Mitigation:* keep the current single
transaction while the table is small; gate the switch on measured row counts, never by default.

### U4. Document and, later, consolidate the two-conflict-arbiter design
The table UNIQUE `(space_id, scope, bot_id, path)` cannot discriminate shared rows (NULLs are
distinct), so the partial index is load-bearing, not redundant. Keep both arbiters and record *why*
in schema docs. A future additive migration could replace them with one expression index on
`(space_id, scope, coalesce(bot_id, ''), path)` and a single `ON CONFLICT` target, collapsing the two
UPSERT strings in the store (wave-16 U2).
*Risk:* an expression index changes plan choice, needs a data migration for shared rows, and a bind-
order slip in the UPSERT refactor is silent. *Mitigation:* ship only the documentation now; carry any
unification as its own ADR + new migration with `EXPLAIN` and a shared+bot concurrency test.

### U5. Give the migration an explicit idempotency and rollback posture
Statement guards are inconsistent: `DROP CONSTRAINT IF EXISTS` and `CREATE INDEX IF NOT EXISTS` are
guarded, but `DROP NOT NULL`, `DROP DEFAULT`, `SET DEFAULT NULL` and `ADD CONSTRAINT` are not — safe
only because the runner applies a migration once. Also, reverting to `NOT NULL` would fail once any
shared row exists. Add, additively, a recorded down migration and a rollback note that a reverse must
first delete/convert shared rows.
*Risk:* writing a down migration that silently drops shared documents is data loss; a
"just re-run it" assumption breaks if the file is ever applied twice out of band. *Mitigation:*
document the one-shot contract, make the down path fail loudly on non-NULL-shared rows, and keep
checksums locked (U1).

## Sequencing & gate

U1 → U2 lock immutability and the sentinel contract first; U3 and U4 are scale/consolidation work
gated on measurements and an ADR; U5 is documentation-plus-down-migration and can land alongside U1.
No recorded migration is edited. Every step: `cargo fmt --check`, `clippy -D warnings`, the REQ-022
integration/golden suite, `tools/trace_check.py`, and a checksums check; mark a claim CONFIRMED only
after rerunning the verifying command.

## What not to touch

0013/0024 bytes and checksums; the NULL sentinel and the partial index as currently defined; the
append-only `memory_revisions` cascade; the 0013 RLS policy; the REQ-022 wire contract; the Rust
store internals owned by wave-16. No new dependency is required.

CONFIRMED by reading the tree at v0.3.0-era source; all risk, timing and scale-threshold notes are
GUESS until the candidate build is migrated, tested and measured.
