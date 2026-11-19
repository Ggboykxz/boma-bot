# Worker #487 — `migrations/0038_tickets.sql` — architecture (v0.4.0)

- Lens: architecture (structure, boundaries, coupling, debt) · Target: v0.4.0 · Status: PROPOSAL,
  READ-ONLY; no code or migration changed, no tests run. CONFIRMED = direct read this session;
  GUESS = inference not re-verified here.
- Module: `migrations/0038_tickets.sql` (66 lines, REQ-038) — feature-flagged board: `board_state`
  (per-space key counter), `tickets`, `ticket_comments`.
- Read (CONFIRMED): `spec/01:311-314` (REQ-038), `spec/02:46,76,122` (Task/"ticket work", `tickets`
  trigger, Board/Ticket/Comment), `crates/infra/src/stores/tickets.rs:1-260`, `0056:1-35` (later
  static-RLS style), `spec/05-known-defects.md` (no board defect). GUESS: `BOMA_BOARD` in `config.rs`.

## 1. Current structure (CONFIRMED)

- Three tables. `board_state(space_id PK→spaces, next_seq int DEFAULT 1)` (:7-10); `tickets(id PK,
  space_id→spaces, key, title, description, state, assignee_bot_id→bots ON DELETE SET NULL,
  created_by→users ON DELETE CASCADE, comment_count int DEFAULT 0, created_at, updated_at,
  UNIQUE(space_id,key))` (:18-31); `ticket_comments(id PK, ticket_id→tickets ON DELETE CASCADE,
  author_kind CHECK, author_id, body, created_at)` (:39-46). One index `ticket_comments_ticket
  (ticket_id, created_at)` (:48).
- RLS is already *static* style — three `ENABLE` + `FORCE` + `CREATE POLICY …_tenant_isolation` blocks
  (:12-16, :33-37, :50-66), no `DO/FOREACH/format()` loop as in 0021/0025. Predicates:
  `USING = WITH CHECK = (space_id = current_space_id() OR is_system())`. But policies omit `FOR ALL`
  (0056 has it), and `ticket_comments` has **no `space_id`**: its policy is a correlated `EXISTS`
  into `tickets` duplicated in `USING` and `WITH CHECK`.
- Four traits:
  1. **One invariant, three owners.** `BOT-<n>`, monotonic, allocate-and-insert in one txn is prose
     (:3-6) + Rust (`INSERT … ON CONFLICT DO UPDATE … RETURNING next_seq - 1`, store :95-104). No DB
     object ties `key` to `board_state.next_seq` or even to the `BOT-<int>` shape.
  2. **State vocabulary split.** CHECK (:24) and Rust string (`move_to(&str)`, store :137-152) both
     own `backlog|todo|in_progress|done`; an invalid value surfaces as a CHECK violation mapped via
     `tenant::unavailable` to a 5xx, not a typed error.
  3. **Denormalised count / missing link.** `comment_count` is maintained only in `comment()` (store
     :184-196); `list()` reads the cache (:70-78). Board↔run linkage (spec :46,:76) is absent — no
     `ticket_id` on runs/tasks, no link table (GUESS: app/audit layers only).
  4. **Two clocks + aggressive cascade.** Columns default `now()` but the store binds RFC3339
     `created_at` (:112-133,:183) while `updated_at` uses DB `now()`; `created_by … ON DELETE CASCADE`
     deletes a member's tickets and cascaded comments. `next_seq` is 32-bit (overflow at 2^31).

## 2. Guiding principle

0038 is immutable: sqlx verifies a checksum, so editing it breaks `boma migrate` on deployed DBs.
Every change is a NEW forward-only additive file (`0060_tickets_invariants.sql`, …) that only adds
objects, nullable/defaulted columns, `NOT VALID` constraints and indexes — never DROP / RENAME /
ALTER TYPE / DELETE or row rewrite. Keep the `BOT-<n>` format, `is_system()` predicate, FK actions and
RLS expression byte-compatible. Externally visible changes are DEVIATIONs for `docs/PARITY.md` with
owner approval (AGENTS.md).

## 3. Concrete upgrades (5), each with a risk

**U1 — Freeze 0038 + additive shape/contract test (do first).** Never edit 0038; add a fresh-PG canary
asserting shape (3 tables, PK/FKs + cascade actions, CHECKs, `UNIQUE(space_id,key)`, one index, all
policies enforced with `USING = WITH CHECK`) and a header stating the boundary (run scheduling out of
scope). *Risk:* checksum immutability forbids edits; a shape assertion fails on purpose when a later
column lands. *Mitigation:* one shared expected-shape constant, updated in the same commit; regenerate
`.sqlx` metadata.

**U2 — One owner for the state vocabulary.** New migration: `ticket_states(code text PRIMARY KEY)`
seeded with the four values, then `ADD CONSTRAINT tickets_state_fk … NOT VALID`, audit, `VALIDATE`.
Leave 0038's CHECK untouched; document the table as future source of truth. *Risk:* two owners
overlap; a legacy unknown state aborts VALIDATE; a new state still needs the frozen CHECK edited.
*Mitigation:* keep the CHECK binding, use the table for introspection only; pair with a typed state
error so invalid `move_to` is a 4xx (GUESS: port change owned by another brief).

**U3 — Row-local, cheap comment isolation.** New migration: `tickets_space_id_id(space_id,id)` index
for today's `EXISTS`; optionally nullable `ticket_comments.space_id`, composite UNIQUE `(id,space_id)`
on tickets + FK `(ticket_id,space_id)→tickets(id,space_id)`, and a direct-predicate comment policy in
a new file (0038 untouched); add `FOR ALL` to every new policy. *Risk:* nullable `space_id` weakens
isolation until backfilled, the backfill is a data change, and the composite FK repeats the cascade —
legacy mismatches fail. *Mitigation:* land the index first; gate the column behind an ADR, a
pre-flight mismatch audit and a cross-space-rejection RLS test (GUESS: skip backfill in v0.4.0).

**U4 — Assert the key/counter invariant without a trigger.** New migration:
`ADD CONSTRAINT tickets_key_format CHECK (key ~ '^BOT-[1-9][0-9]*$') NOT VALID`, audit + VALIDATE, a
COMMENT, and a read-only view `board_key_state(space_id,last_key,next_seq)`; widen `next_seq` to
`bigint` later. *Risk:* the regex duplicates Rust's `format!` and can drift; a legacy key blocks
VALIDATE; a true monotonicity trigger would change behaviour. *Mitigation:* share one regex constant,
keep the view diagnostic-only, treat enforcement as a separately approved DEVIATION.

**U5 — Count and work-linkage structure without row rewrites.** New migration: read-only
`ticket_summaries` view computing `COUNT(ticket_comments)` with a parity test against `comment_count`,
plus nullable, unused `tasks.ticket_id` (or `ticket_work` link table) + index so "which run works on
ticket X" is queryable. *Risk:* view is a second source of truth; the run-side column touches 0002's
table and is a DEVIATION if exposed. *Mitigation:* view diagnostic-only, do not switch `list()` in
v0.4.0; keep the link nullable/unused and coordinate with the run/task brief before any write path.

## 4. Cross-cutting risks / sequencing

Order **U1→U2→U3→U4→U5**: freeze, vocabulary, RLS, key invariant, read models; U3-U5 need ADR/parity
approval. Coordinate with the tickets.rs and run/task owners — this brief owns migration structure
only. Regenerate `.sqlx`; CI runs `boma migrate` on fresh PG and replays REQ-038 tests. Freeze the key
format, `is_system()` predicate and cascades; no new column may change a public DTO. Mark claims
CONFIRMED/GUESS; gate with `cargo fmt --check`, clippy `-D warnings`, golden and contract suites.

## 5. Out of scope

Port/RPC changes, the `BOMA_BOARD` flag, typed state errors, run/task scheduling, purge jobs, and any
in-place edit of 0038 — another brief or forbidden by the checksum invariant.

## 6. Evidence & suggested tests

- CONFIRMED: `0038` (:1-66), `tickets.rs:1-260`, `spec/01:311-314`, `spec/02:46,76,122`, `0056:1-35`.
- GUESS (re-verify): flag wiring in `config.rs`, absent run↔ticket link, embedded-PG regex/VALIDATE.
- Suggested names: `req_038_migration_0038_is_immutable_and_shape_stable` (U1),
  `req_038_ticket_comments_rls_rejects_cross_space` (U3),
  `req_038_tickets_key_matches_board_state_format` (U4).

Line budget: <120. Status: proposal only; READ-ONLY worker, no code changed and no tests run.
