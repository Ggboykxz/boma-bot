# Worker #455 — `migrations/0021_pending_tool_calls.sql` — architecture (v0.4.0)

- Module: `migrations/0021_pending_tool_calls.sql` (36 lines, M2; REQ-016/REQ-017) — the parking table
  for tool calls that need human approval. Lens: architecture (structure, boundaries, coupling, debt).
- Mode: READ-ONLY proposal — no product code or migration changed, no tests run. CONFIRMED = verified by
  direct read this session; GUESS = needs a test/oracle run.
- Read for context (CONFIRMED): `0022_pending_tool_calls_status_check.sql`, `0004_cards.sql`
  (`external_effects` ledger), `0002_m1.sql:155-206` (`threads`, `runs`), `0055_signin_requests.sql`
  (later-house RLS convention), `crates/infra/src/stores/pending_tool_calls.rs:65-233`,
  `crates/app/src/ports.rs:1884-1978`, `spec/01-requirements.md:129-144`, `spec/golden/thread-answer.json`.
- No wire/API/data-format change is proposed. Every upgrade is a NEW forward-only, additive migration;
  anything externally visible is a DEVIATION for `docs/PARITY.md` + owner approval.

## 1. Current structure (CONFIRMED)

- One table, composite PK `(run_id, call_id)` (:18). Three FKs: `run_id`→runs, `space_id`→spaces,
  `thread_id`→threads, all `ON DELETE CASCADE` (:8-11) — denormalised so RLS can filter on `space_id`.
- Payload/state: `name`, `arguments jsonb DEFAULT '{}'`, `status text DEFAULT 'pending'`, `result jsonb`,
  `created_at DEFAULT now()`, `answered_at` (:12-17). The status vocabulary is added separately in 0022
  (`CHECK ... IN ('pending','applied','refused')`) — one column's invariant is split across two files.
- RLS via `DO $$ DECLARE … FOREACH tenant_table IN ARRAY ARRAY['pending_tool_calls'] … EXECUTE
  format(...) $$` (:21-36): a single-element template loop that computes the policy name at runtime,
  omits explicit `FOR ALL`, and is not greppable. Later house style (`0055:23-29`) is static
  `ENABLE`/`FORCE`/`CREATE POLICY … FOR ALL`.
- Queries (CONFIRMED, store :76-233): `create` uses `ON CONFLICT (run_id,call_id) DO NOTHING` →
  `AppError::Conflict` on replay; `find_pending` filters `run_id AND status='pending'`; `mark_applied`/
  `mark_refused` are guarded optimistic updates `WHERE status='pending'`. Exactly-once therefore rests on
  the PK, the status guard and the separate `external_effects` key `tool:<run>:<call>` (0004/REQ-017).

## 2. Architecture findings (CONFIRMED unless marked)

1. **Invariant split across files.** The default (0021), the CHECK (0022) and the Rust enum
   (`PendingToolCallStatus::parse`, store :36) are three owners of one column's vocabulary.
2. **Tenant/run consistency is not in the schema.** `space_id`/`thread_id` are independent FKs; nothing
   forces them to match the referenced run. A tenant whose RLS `WITH CHECK` passes on its own `space_id`
   could still point `run_id`/`thread_id` at another tenant's row. GUESS: unreachable via current usecases
   (`usecases.rs:6897+` builds all three from one `TenantContext`), but the DB does not enforce it.
3. **RLS governance is hidden.** The dynamic loop defeats grep, offline-metadata review and any
   `pg_policies` assertion; its single element reads like a copied multi-table loop.
4. **No index for RLS-scoped scans.** PK `(run_id, call_id)` serves the store's run-scoped reads, but a
   `space_id`-filtered scan (dashboard, cleanup, RLS filter) has no supporting index.
5. **State/timestamp link is unenforced.** `answered_at` is nullable with no rule tying it to `status`,
   and the effect key lives only in Rust/0004, so the exactly-once link is not introspectable here.
6. **Two clocks.** `created_at DEFAULT now()` is bypassed by the store, which binds a caller-supplied
   RFC3339 string (:91) — DB time and caller time coexist (same smell noted for cards).

## 3. Concrete upgrades for v0.4.0 (additive only), each with a risk

**U1 — Freeze 0021 and define the aggregate contract (do first).** Leave 0021 and 0022 byte-identical;
add a new migration with a header stating the boundary (parking state machine; dedup ledger is 0004's
`external_effects`) plus a fresh-PG canary asserting the table shape and the 0022 constraint. *Risk:*
sqlx checksum immutability forbids editing 0021/0022; a shape assertion fails on purpose when a later
additive column lands — update the assertion in the same commit.

**U2 — Enforce tenant/run consistency in the schema.** New migration: `CREATE UNIQUE INDEX runs_id_space
ON runs (id, space_id)` and `threads_id_space ON threads (id, space_id)` (both additive), then
`ALTER TABLE pending_tool_calls ADD CONSTRAINT … FOREIGN KEY (run_id, space_id) REFERENCES runs (id,
space_id)` and likewise `(thread_id, space_id) → threads`. *Risk:* needs new unique indexes (none exist
today, CONFIRMED from 0002:155-206); any pre-existing mismatched row aborts the migration (GUESS: none —
run a pre-flight audit first); the extra FKs duplicate cascade paths, so re-verify delete cascades and
the REQ-017 goldens after landing.

**U3 — Make RLS governance introspectable.** In every NEW tenant migration use static
`ENABLE`/`FORCE`/`CREATE POLICY … FOR ALL` (0055 style); add a test querying `pg_policies` /
`pg_class.relrowsecurity` for `pending_tool_calls_tenant_isolation` with `using = with check`, derived
from one shared expected-policy constant. *Risk:* the legacy loop in 0021 stays — the test must not
mirror it or it will drift; pin to the embedded PG version and freeze the RLS expression.

**U4 — Add RLS/lifecycle indexes.** `CREATE INDEX pending_tool_calls_space_status ON pending_tool_calls
(space_id, status)` and optionally `(thread_id, status)`. *Risk:* extra write amplification on an already
small table; confirm with `EXPLAIN` under a tenant session that the RLS predicate uses the index. If it
does not, drop the index via a further additive migration rather than editing this one.

**U5 — Make the exactly-once link explicit and the state machine total.** Add nullable `effect_key text`
plus a CHECK tying `answered_at` to `status` (NULL while `pending`; applied rows carry a result) and a
COMMENT that `tool:<run>:<call>` stays authoritative in `external_effects`. *Risk:* a second dedup column
invites divergence and a wrong backfill could mark rows applied and skip execution — default the column
to NULL, never backfill a value, keep 0004's key authoritative, and never hold the DB transaction across
the tool call (coordinate with the `PendingToolCallStore` owner).

## 4. Ordering, invariants, non-goals

Order U1 → U2 → U3 → U4 → U5 (contract, consistency, governance, performance, state model). Invariants:
0021/0022 stay byte-for-byte unchanged; no DROP / RENAME / ALTER TYPE / DELETE of existing objects; PK
`(run_id, call_id)`, effect keys, RLS semantics and error mapping (`Conflict`/`NotFound`) unchanged;
`.sqlx` offline metadata refreshed; REQ-017 goldens (`thread-answer.json`) and the contract suite replayed.
Non-goals: no edit to 0004's ledger, no change to `PendingToolCallStore`/usecase orchestration (other
workers' scope), no new dependency, no externally visible behavior change.

## 5. Evidence & tests to add (suggested names)

- CONFIRMED — this session's reads: 0021 (:1-36), 0022 (:1-7), 0004 (ledger), 0002:155-206, 0055:23-29,
  store :65-233, ports :1884-1978, spec :129-144, thread-answer.json.
- GUESS — no cross-tenant FK misuse today; no pre-flight mismatches; embedded PG version for U3.
- Names: `req_017_pending_tool_call_shape_stable` (U1), `pending_tool_calls_tenant_consistency` (U2),
  `req_017_pending_tool_calls_rls_is_introspectable` (U3).

Line budget: <120. Status: proposal only; READ-ONLY worker, no code changed and no tests run.
