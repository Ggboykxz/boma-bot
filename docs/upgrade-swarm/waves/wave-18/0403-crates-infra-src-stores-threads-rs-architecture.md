# 0403 — `crates/infra/src/stores/threads.rs` — architecture lens — v0.4.0 proposal

Worker #403 · module `crates/infra/src/stores/threads.rs` (79 lines) · lens: architecture (ownership of
the `threads` table, projection duplication, invariant enforcement, tenant boundary, decoding) ·
READ-ONLY review of `rebuild-workspace/.worktrees/before-m5` (v0.3.0-rc5). Spec anchors: REQ-005,
REQ-010, REQ-043, REQ-048. CONFIRMED = read this session; likelihood/impact = GUESS. Proposal only.

## 1. Current structure (CONFIRMED)

- Thin read adapter: `PostgresThreadStore { pool: PgPool }`, `const fn new`, implements the two-method
  `ThreadStore` port (app/ports.rs:785-809). No writes and no transaction of its own here.
- `find` (:26-50) selects one thread by id; `for_bot` (:52-78) selects
  `WHERE bot_id=$1 AND kind='bot' ORDER BY created_at ASC LIMIT 1`. Both run one statement inside
  `tenant::with_tenant` (a transaction setting transaction-scoped `boma.space_id`, tenant.rs:51-63)
  and end with `row.map(rows::thread).transpose()`.
- The table's writers live elsewhere: bots.rs:59-70 and groups.rs:54-65 insert `kind='bot'` threads
  (the latter with `bot_id` = first group member), messaging.rs:439-455 inserts `kind='external'`
  threads via select-then-insert-`ON CONFLICT DO NOTHING`-reselect. The counters are bumped in
  messages.rs:51-58 (`next_message_seq`) and events.rs:45 (`next_event_seq`).
- The 11-column projection plus `boma.rfc3339(created_at)` appears verbatim four times: here at
  :35-38 and :61-63, and in messaging.rs:190-193 and :488-491 — all mapping through `rows::thread`
  (rows.rs:303-322). `messaging.rs::thread` is an exact duplicate of `find` by id.
- Schema (migrations/0002_m1.sql:155-166, 291; 0042_messaging.sql:13-21): `threads_bot_id_idx` is a
  plain (non-unique) index; the only uniqueness on identity is the partial index on external
  (platform, workspace, conversation) for `kind='external'`.

## 2. Architecture observations

### 2.1 Reader-only adapter, five owners (CONFIRMED shape, GUESS risk)
`ThreadStore` cannot create a thread, resolve an external identity, or advance a counter. The
invariants of one table are spread over five files with no place that states them. Drift risk is
real but diffuse: a future writer can insert a `kind='bot'` thread without the store noticing.

### 2.2 "One thread per bot" is assumed, not enforced (CONFIRMED gap, GUESS likelihood)
`for_bot` uses `ORDER BY created_at ASC LIMIT 1`, but nothing prevents several `kind='bot'` rows with
the same `bot_id`: bots.rs creates the bot's own thread while groups.rs creates a group thread with
the bot as `bot_id` (migrations/0053_groups.sql:10-11 says so explicitly). Usecases work around this
by checking `groups.find_for_bot` first (usecases.rs:4255-4267), so the store silently returns one of
several rows if that upstream check is skipped. `created_at` mixes app-provided strings (bots/groups)
with DB `now()` (messaging), so even the ordering is not on one clock. No tie-break on `id`.

### 2.3 Duplicated projection (CONFIRMED)
Four copies of the column list and three identical "select by id" queries. SQLx compile-time checking
catches a renamed column in every copy, but not semantic drift (one copy dropping a filter or a new
column being added in three of four places). `messaging.rs::thread` and `find` should be one loader.

### 2.4 Stringly-typed decoding (CONFIRMED fallbacks, GUESS impact)
`rows::thread` treats any `kind != "external"` as a bot thread, maps any unknown platform to
`Generic`, and substitutes empty strings for missing external ids. The DB CHECK (0042:14) makes this
unlikely today, but a corrupt row becomes a plausible-looking domain `Thread` instead of a surfaced
fault — invalid states are representable, and the failure looks transient (`Unavailable`).

### 2.5 Tenant boundary is implicit (CONFIRMED shape, GUESS impact)
`find`/`for_bot` accept `TenantContext::System` and add no `space_id` predicate; they rely on RLS
alone. Callers already build a Member fallback for system paths (usecases.rs:4246-4254), and
`account_export.rs:656` calls `for_bot` with the import context, so the fallback looks intentional —
but the adapter cannot distinguish a cross-tenant read from a member read, unlike `count_above` in
messages.rs:217-226, which binds `space_id` as defence in fact. A future system caller gets an
arbitrary space's thread with no compile-time signal.

## 3. Concrete upgrades for v0.4.0 (ordered by value/risk)

- **A. One thread projection (low, internal).** Extract a `pub(crate) const THREAD_COLUMNS` and a
  `load_thread(conn, ...)` helper in threads.rs; use it in both `find`/`for_bot` and in
  messaging.rs:182 and :477. Keep `kind='external'` filters in the list query. *Risk:* none
  externally visible; a cross-module refactor that must preserve the list endpoint's filter.
- **B. Deterministic `for_bot` (low-medium).** Add `ORDER BY created_at ASC, id ASC` and exclude
  group threads with `AND NOT EXISTS (SELECT 1 FROM groups g WHERE g.thread_id = threads.id)`;
  add an index on `groups (thread_id)` (only `groups_space` exists today). *Risk:* one extra lookup
  per call and a migration; confirm no caller intentionally wants a group thread from `for_bot`.
- **C. Fail loudly on corrupt rows (low-medium).** In `rows::thread`, return a typed permanent error
  for unknown `kind`/platform and for `kind='external'` with missing ids, instead of the silent
  bot/`Generic`/empty-string fallbacks. *Risk:* a new `AppError` variant ripples into API mapping;
  legacy corrupt rows would start erroring (DB CHECK makes this unlikely — GUESS).
- **D. Make the tenant boundary explicit (medium).** Bind `space_id` in `find`/`for_bot` when a Member
  context supplies one (defence in fact), or reject `System` with a typed error; audit all call sites
  first (realtime, account export, usecases). *Risk:* a system path may legitimately need a cross-space
  read — do not guess; if observable, ADR + DEVIATION.
- **E. Enforce the invariant in the schema (medium-high).** Add an explicit thread subtype (e.g.
  `is_group boolean`) alongside `kind`, a partial unique index for one primary bot thread per
  `(space_id, bot_id)`, and update bots.rs/groups.rs/messaging.rs inserts plus `rows::thread`.
  *Risk:* migration across three writers, domain/wire ripple; group threads must remain legal. Needs
  ADR, `STATE.md` question, `PARITY.md` if visible, and owner sign-off.

## 4. Risks / gates / non-goals

- Sequence allocation and external-identity resolution stay where they are in v0.4.0 (no write-path
  rewrite); only E touches schema. Keep every read in `tenant::with_tenant`; never move
  `boma.space_id` out of tenant.rs (ADR-002). `unsafe_code` forbidden, clippy `-D warnings`, no
  unwrap/expect/panic outside tests, rustdoc on public items.
- B/C/D/E change externally visible behavior or add migrations: record in `docs/STATE.md` and
  `docs/PARITY.md`, wait for owner approval. A is internal and reversible. Sequence A -> B -> C -> D -> E.
- Group-vs-bot thread semantics are partly implicit in the original; if the spec is silent, write a
  GUESS + "Decisions needed" entry rather than choosing silently. All likelihood/impact stays GUESS
  until the integration tests below run. `tools/trace_check.py` counts declaration lines, so REQ ids
  belong in test names, not comments.

## 5. Suggested tests (integration, real PG)

- `req_005_for_bot_ignores_group_threads_and_is_deterministic`
- `req_005_find_round_trips_both_thread_kinds` and `req_048_external_identity_survives_reload`
- `req_043_for_bot_under_member_context_cannot_read_another_space`
- `req_005_corrupt_kind_is_a_permanent_error_not_a_bot_thread` (with C)
- `req_005_two_primary_bot_threads_are_rejected` (with E)

## Bottom line

The adapter is small and correct for the happy path, but it is a reader over a table with five
writers, it assumes an unenforced one-thread-per-bot invariant, it duplicates its projection four
times, and it trusts a RLS policy plus silent decoding fallbacks for tenant and kind correctness.
A/B/C are local, high-value hardening; D/E are genuine architectural fixes that need ADRs and owner
approval. Runtime impact is GUESS until the proposed tests run.
