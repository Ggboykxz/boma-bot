# Worker #457 — `migrations/0022_pending_tool_calls_status_check.sql` — architecture (v0.4.0)

- Module: `migrations/0022_pending_tool_calls_status_check.sql` (7 lines, REQ-017) — the DB-layer
  enforcement of the pending-tool-call status vocabulary. Lens: architecture (structure, boundaries,
  coupling, debt), not reliability (sibling #458).
- Mode: READ-ONLY proposal — no product code or migration changed, no tests run. CONFIRMED = verified
  by direct read this session; GUESS = design inference needing a test/oracle run.
- Read for context (CONFIRMED): `0021_pending_tool_calls.sql:7-19` (column + default), `0004_cards.sql`
  (ledger), `0045_audit_cors.sql:12-24`, `0052/0049` (DROP+ADD precedent), `0055_signin_requests.sql:16`
  (inline named CHECK), `crates/domain/src/card.rs:102-137` (enum `as_str`/`parse`),
  `crates/infra/src/stores/pending_tool_calls.rs:36,119,148,184` (store literals),
  `crates/infra/src/stores/rows.rs:422-424` (card vocabulary), `spec/01-requirements.md:137-144`,
  sibling #455/#456 and `docs/upgrade-swarm/README.md`.
- **Additive only:** all upgrades are NEW migration files, tests or comments. 0022 stays byte-identical
  (sqlx checksum immutability, #455 U1). No DROP/REPLACE of the existing constraint, no wire/API/data
  change. Anything externally visible is a DEVIATION for `docs/PARITY.md` + owner approval.

## 1. Current structure (CONFIRMED)

- The whole file is one statement: `ALTER TABLE pending_tool_calls ADD CONSTRAINT
  pending_tool_calls_status_check CHECK (status IN ('pending','applied','refused'))` (:5-7). Named CHECK,
  matching the house style of 0045/0052/0054 rather than the inline unnamed CHECKs of 0034/0037/0043.
- No `NOT VALID`: PostgreSQL takes ACCESS EXCLUSIVE and scans the whole table during the ADD, blocking
  writes until validation completes. Fine on an empty new table; a risk on a large installed base.
- The column's DEFAULT (`'pending'`, 0021:14), this CHECK, and the Rust enum (`card.rs:105-112`) are
  three owners of one vocabulary. The store adds a fourth copy as raw SQL literals (:119, :148, :184).
- The CHECK pins the *domain* only. It says nothing about transitions, timestamps or terminal payloads:
  `mark_applied`/`mark_refused` rely solely on `WHERE status='pending'` optimistic updates (:147-149,
  :183-185) for exactly-once, with the `external_effects` key in 0004 as the durable dedup ledger.
- Migration 0044+ never revisits it; the constraint has no `COMMENT`, so its three values and their link
  to `PendingToolCallStatus` exist only in this header comment (:1-4) and the enum's rustdoc.
- Neighbours differ deliberately: `signin_requests.status` is `pending|granted|refused` (0055:16) and
  `cards.status` is `pending|answered|declined` (`rows.rs:422-424`, still unconstrained — #0422 C1).
  The three sets must **not** be unified into one type or copied across tables.

## 2. Architecture findings (CONFIRMED unless marked)

1. **Vocabulary drift is unguarded.** Adding a fourth state (e.g. the `expired`/TTL idea in #0372 E1)
   needs a coordinated change in 0022's successor, the enum's `ALL`/`as_str`/`parse`, the store SQL and
   the memory twin — nothing in CI detects a mismatch today.
2. **Constraint ≠ state machine.** A closed IN-list rejects junk but permits illegal terminal states
   (`applied` with NULL result, `refused` with a result, terminal rows without `answered_at`). The
   REQ-017 exactly-once guarantee is therefore unenforceable and unintrospectable at the DB layer.
3. **Deploy-time lock shape is implicit.** The file reads as a tiny, safe migration; its ACCESS
   EXCLUSIVE + full-scan cost is invisible. A large `pending_tool_calls` table turns a 7-line migration
   into a write outage.
4. **Immutability is a boundary, not a note.** Once shipped, this file cannot be edited; all hardening
   must be forward-only. That boundary is stated only in sibling proposals, not in the file itself.
5. **Discovery is by grep-failure.** There is no canary asserting that `pg_constraint` still carries
   `pending_tool_calls_status_check`, so a later DROP (e.g. a careless 0045-style widening) would be
   silent until a bad value is written. GUESS: no test currently introspects constraints.

## 3. Concrete upgrades for v0.4.0 (additive only), each with a risk

**U1 — Freeze 0022 and add a constraint contract canary (do first).** Keep the file byte-identical;
add a fresh-PG test that reads `pg_constraint`/`pg_get_constraintdef` for `pending_tool_calls_status_check`
and asserts the IN-list matches `PendingToolCallStatus::ALL` (new const in `card.rs`). Add a `COMMENT ON
CONSTRAINT` in a NEW migration linking the DB values to that enum. *Risk:* parsing constraintdef is
brittle across PG versions — pin to the embedded PG and assert set membership, not raw text; a future
fourth status must update enum + canary + a new widening migration in one commit, or the canary fails
on purpose.

**U2 — Single-source the vocabulary in Rust.** Introduce `PendingToolCallStatus::ALL` and a proptest
 asserting `parse(as_str(v)) == v` for every variant and `as_str` collisions are absent; have the store
 build its three status literals from that const (wire strings unchanged). The DB CHECK stays a
 deliberate duplicate — SQL cannot import Rust — but CI now proves the copies agree. *Risk:* the store
 is another worker's scope (#371/#372); coordinate and keep exact `Rule`/`Conflict` error strings and
 the memory twin in lockstep. Never accept a new wire value before its migration lands, or an old DB
 rejects it at write time.

**U3 — Add complementary state-integrity CHECKs in a new migration.** Add named, additive constraints:
terminal rows require `answered_at IS NOT NULL`; `pending` rows require `answered_at IS NULL`; `applied`
requires `result IS NOT NULL`; `refused` requires `result IS NULL`; `btrim(call_id) <> ''` and
`btrim(name) <> ''`. This makes the REQ-017 state machine partly introspectable without touching the
existing CHECK. *Risk:* a pre-flight audit must prove no stored row violates the new rules, or the
migration aborts; these tighten writes (not the wire format) but a legacy writer that forced a terminal
status would now fail — flag as a DEVIATION if observable. Do not encode the `external_effects` key
here; 0004 stays authoritative.

**U4 — Adopt a lock-safe CHECK pattern for future vocabulary migrations.** In every NEW migration that
adds a CHECK to a populated table, use `ADD CONSTRAINT ... NOT VALID` then a separate `VALIDATE
CONSTRAINT` (SHARE UPDATE EXCLUSIVE, writes continue), preceded by a `SELECT` proving no row falls
outside the new set. Document this beside 0022 so the cheap-looking statement is not copied blindly.
*Risk:* NOT VALID leaves a window where old rows are unchecked — safe only because 0021 seeded clean
data; verify no writer can insert an out-of-set value during the window, and confirm the sqlx migrator's
transaction behaviour before splitting the statements across one or two files.

**U5 — Record the boundary and cross-reference, don't unify.** Add a header comment / DATA-MODEL note
stating that this constraint owns *only* the parking-state domain, that dedup lives in `external_effects`
(0004) and transitions in `PendingToolCallStore`, and that `cards.status` and `signin_requests.status`
are different vocabularies (do NOT copy `pending|applied|refused` into `cards`, whose values are
`pending|answered|declined`). *Risk:* documentation drift is invisible — bind the note to U1's canary so
a vocabulary change forces the note to be updated; any future `cards` CHECK is #0422's scope.

## 4. Ordering, invariants, non-goals

Order U1 → U2 → U3 → U4 → U5 (contract, single source, integrity, deploy safety, documentation).
Invariants: 0021/0022 byte-for-byte unchanged; no DROP/RENAME/ALTER TYPE on existing objects; PK
`(run_id, call_id)`, RLS semantics, error mapping and REQ-017 goldens (`thread-answer.json`) unchanged;
`.sqlx` offline metadata refreshed after any additive migration. Non-goals: no TTL/expiry status, no
port/store orchestration rewrite (other workers), no constraint on `cards`, no new dependency.

## 5. Evidence & suggested test names

- CONFIRMED — this session's reads: 0022 (:1-7), 0021 (:7-19), 0045 (:12-24), 0055 (:16),
  `card.rs:102-137`, store (:36,119,148,184), `rows.rs:422-424`, spec :137-144, #455/#456/#0422.
- GUESS — no existing constraint-introspection test; impact of a large table at 0022's ADD; appetite
  for the new CHECKs in U3.
- Names: `req_017_status_check_constraint_matches_enum`, `req_017_status_values_round_trip`,
  `req_017_pending_tool_call_state_is_db_integral`, `req_017_additive_check_is_lock_safe`.

Line budget: <120. Status: proposal only; READ-ONLY worker, no code changed and no tests run.
