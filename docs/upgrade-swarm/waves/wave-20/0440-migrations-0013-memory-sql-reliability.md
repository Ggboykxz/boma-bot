# v0.4.0 reliability proposal — `migrations/0013_memory.sql` (worker #440, lens: reliability)

- **Status:** PROPOSAL (read-only review; no source changed). CONFIRMED = verified by reading this worktree; GUESS = needs a runtime/test or oracle run.
- **Scope:** `0013_memory.sql` — the `memory_documents` / `memory_revisions` schema, their constraints, the per-table RLS block, and the append-only revision chain (REQ-022). Read together with the follow-up `0024_memory_shared_bot_id_nullable.sql` and the actual writer `crates/infra/src/stores/memory.rs`.
- **Constraint:** additive only. Every hardening below is a *new* migration that adds objects (indexes, `ADD CONSTRAINT`, columns, triggers); nothing rewrites 0013 itself. The shipped shared-memory defect in 0013 is already compensated by 0024 (CONFIRMED) — this proposal targets the residual gaps.

## Summary

0013 creates a sound-looking schema whose one fatal flaw (the empty-string shared marker behind a bot foreign key) is patched later by 0024. What remains is a set of *convention-only* invariants: the document is "a projection of the latest revision" but nothing in the database enforces that; a bot-scoped document is not tied to a bot in its own space; and the revision chain has no idempotency or size bound. RLS is the standard, well-formed `space_id`-only policy reused across the codebase — correct, but it scopes by space and not by bot, and it rests entirely on two session GUCs. The reliability posture is therefore "correct by caller discipline", with the database providing no defence in depth beyond tenant isolation (CONFIRMED).

## Failure modes

### Constraints (C)

- **C1 (HIGH, CONFIRMED):** as shipped, `bot_id text NOT NULL DEFAULT '' REFERENCES bots(id)` plus `CHECK (... scope='shared' AND bot_id='')` makes shared memory unwritable — `''` is not a bot id, so the FK rejects it. 0024 fixes this by moving the marker to NULL, dropping the NOT NULL/DEFAULT, replacing the CHECK, and adding a partial unique index `memory_documents_shared_key` for shared rows. Any v0.4.0 baseline that runs 0013 without 0024 is broken; keep the pair inseparable in migration ordering and in test fixtures.
- **C2 (HIGH, CONFIRMED structural / GUESS exploitability):** `bot_id` references `bots(id)` only; there is no composite FK tying `(space_id, bot_id)` to the bot's own space, and RLS checks only the document's `space_id`. A bot-scoped document in space A can therefore name a bot that lives in space B. Today the store sets both from `context.space_id()` (memory.rs:43-50), which masks it, but no DB rule prevents drift or a system-path bug from creating the cross-space row.
- **C3 (MED, CONFIRMED):** nothing enforces that `memory_documents.revision` equals the newest `memory_revisions.revision`, nor that revision numbers advance monotonically. The writer does both inserts in one `with_tenant` transaction (memory.rs:54-104), so they are atomic under normal operation, but a future writer or a system-path repair can desynchronise the projection from the chain with no constraint firing.
- **C4 (LOW, CONFIRMED):** `content text NOT NULL` has no length/byte cap (and neither does `path` beyond `validate_memory_path` in the app). A large or attacker-influenced memory entry bloats both tables and inflates RLS-scoped `list` reads.

### RLS (R)

- **R1 (LOW, CONFIRMED):** the `DO` block enables + forces RLS and creates `<table>_tenant_isolation` with the standard `space_id = boma.current_space_id() OR boma.is_system()` predicate, matching 0002/0004/0010 etc. It is well formed; enabling RLS before the policy means a failed migration leaves deny-all (fail-closed, acceptable). Policy creation is not idempotent, so a re-run after a partial failure errors — fine for versioned migrations, note it.
- **R2 (MED, GUESS):** `boma.current_space_id()` / `boma.is_system()` are `LANGUAGE sql STABLE` functions reading the GUCs `boma.space_id` / `boma.system_reason` via `current_setting(..., true)` (0002_m1.sql:27-42). Any code able to `SET` those GUCs in the app session bypasses isolation entirely. The store compensates with explicit `space_id = $1` predicates on every read/write (memory.rs:129,159,189); keep that defence in depth. Can't be fixed inside this migration.
- **R3 (MED, CONFIRMED structural / GUESS intent):** the policy scopes only by space, not by `scope` or `bot_id`. Within a space any caller can read/write any bot's memory and the `WITH CHECK` likewise permits inserting rows under another bot's id. This may be intentional (space teammates share memory) but must be confirmed; today only the RPC layer enforces bot ownership.

### Dedup / revision chain (D)

- **D1 (MED, CONFIRMED):** `memory_revisions.space_id` is a denormalised copy of the parent's `space_id`, enforced only by convention. There is no FK tying `(document_id, space_id)` to the document, so a mismatched pair could make a revision visible under a different tenant than its parent — the very thing the redundant column is meant to help isolate.
- **D2 (MED, CONFIRMED):** memory writes are not idempotent. The golden (`memory-write.json`) carries no idempotency key and the store unconditionally does `ON CONFLICT ... revision = revision + 1` (memory.rs:65-77). A retried request after an ambiguous timeout appends a fresh revision every time; `UNIQUE(document_id, revision)` only stops duplicate *numbers*, not duplicate logical writes. AGENTS.md asks for idempotency keys where retries exist.
- **D3 (LOW, CONFIRMED):** the chain is append-only with no cap, TTL, or archival. Unbounded revision growth feeds C4 and disk pressure. The `bigserial id` itself is fine; `UNIQUE(document_id, revision)` is a useful integrity guard worth keeping.

## Concrete hardening (prioritized, all additive)

1. **P0 — close C2:** new migration adds `CREATE UNIQUE INDEX bots_space_id_id_key ON bots (space_id, id);` (prerequisite: GUESS, verify no duplicate) then `ALTER TABLE memory_documents ADD CONSTRAINT memory_documents_bot_space_fk FOREIGN KEY (space_id, bot_id) REFERENCES bots (space_id, id);` scoped to bot rows. This makes cross-space references unrepresentable. Deferrable if needed for existing-data backfills.
2. **P0 — close D1:** `CREATE UNIQUE INDEX memory_documents_id_space_key ON memory_documents (id, space_id);` then `ALTER TABLE memory_revisions ADD CONSTRAINT memory_revisions_doc_space_fk FOREIGN KEY (document_id, space_id) REFERENCES memory_documents (id, space_id);`. Guarantees a revision can never be tagged with a tenant other than its document's.
3. **P1 — close D2:** add a `write_key text` column (nullable) + `CREATE UNIQUE INDEX memory_documents_write_key_key ON memory_documents (space_id, write_key) WHERE write_key IS NOT NULL;`, and have the writer pass a caller-generated key so a retry becomes `DO NOTHING` / returns the existing row instead of incrementing. API/behavior change → ADR + `docs/PARITY.md` entry.
4. **P1 — close C3:** add a trigger (or a deferred constraint trigger) on `memory_revisions` asserting the inserted `revision` equals `memory_documents.revision` after the doc upsert, and rejecting backwards movement of the document's `revision`. Cheaper alternative: a periodic consistency check query rather than a per-write trigger.
5. **P2 — close C4/D3:** `ALTER TABLE memory_documents ADD CONSTRAINT memory_documents_content_size_chk CHECK (octet_length(content) <= <cap>);` (and the same on `memory_revisions`), plus a documented retention/archival policy for old revisions. Keep caps configurable in code, not hard-coded magic numbers.
6. **P2 — observability (R2/R3):** a test/assertion that every read/write passes an explicit `space_id` predicate alongside RLS; document that `is_system()` is a trusted path and that bot-ownership enforcement stays in the RPC layer. No change to the policy text itself.

## Risks / trade-offs

- Composite FKs (items 1-2) need unique indexes first; adding them to `bots`/`memory_documents` costs storage and can fail on pre-existing duplicate data — verify with a read-only query before shipping, keep constraints `NOT VALID` then `VALIDATE` to avoid a long lock.
- Idempotency keys (item 3) change the RPC contract and revision semantics (a retry no longer advances the counter); needs the golden updated, an ADR, and parity approval — this is the highest-friction item.
- A revision-consistency trigger (item 4) adds write cost and a new failure mode: a buggy trigger can block all memory writes. Prefer the periodic check unless the atomicity guarantee is proven insufficient.
- Content size caps (item 5) can reject legitimately large memory entries — a behavior change; calibrate from observed data and record any rejection in `docs/PARITY.md`.
- None of the RLS items can fully defend against a caller that can `SET` the GUCs; that is a connection-role concern outside this file and must be tracked separately.

## Verification to add

Real-Postgres integration tests (RLS forced): cross-space bot reference rejected (C2); revision with mismatched space rejected (D1); duplicate write key is idempotent (D2); oversized content rejected (C4); projection/chain consistency check passes after concurrent writes (C3); cross-tenant and unscoped reads return empty (R1/R2). Existing coverage: `crates/infra/tests/m1.rs:1177` asserts revisions are append-only — extend it rather than duplicate. All claims above are CONFIRMED from source reading except those explicitly marked GUESS.
