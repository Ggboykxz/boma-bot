# 0347 — `crates/infra/src/stores/groups.rs` — architecture proposal (v0.4.0)

- Worker #347 · Lens: architecture · Target: v0.4.0 · Read-only review; no source was changed.
- Module: `crates/infra/src/stores/groups.rs` (244 lines) at `before-m5` (v0.3.0-rc5): the
  Postgres adapter for the `GroupStore` port.
- Read for context (CONFIRMED by direct reads): `spec/01-requirements.md` REQ-010 / REQ-043,
  `spec/golden/group-create.json`, `crates/app/src/ports.rs` :468–546 (`NewGroup`, `GroupStore`),
  `crates/infra/src/stores/rows.rs` :152–176 (`GroupRow`, `group`), `crates/infra/src/tenant.rs`
  (`in_space`, `scope`, `unavailable`, `is_unique_violation`), `migrations/0053_groups.sql`, `AGENTS.md`.

## Current structure (CONFIRMED)

One struct `PostgresGroupStore { pool: PgPool }` with a `const fn new` (:15–27). The `GroupStore`
impl has five methods: `create` (:49–114) and four reads (`find` :116–144, `find_by_thread`
:146–174, `find_for_bot` :176–207, `list_for_member` :209–243). A private `bots_for` (:29–45)
hydrates bot members from `group_memberships` via `BotId::new`.

Three structural facts drive this proposal:

- **Every read is an N+1.** `find`, `find_by_thread` and `find_for_bot` run one group SELECT plus one
  `bots_for` query; `list_for_member` runs one group SELECT and then one `bots_for` **per group**
  (:234–238). This conflicts with AGENTS.md's "no N+1 queries" standard and scales linearly with a
  person's group count.
- **Four near-identical tenant wrappers.** Each read repeats
  `context.space_id().cloned().ok_or(AppError::NotFound)?`, clones the space id twice, converts the
  key to an owned `String`, and opens `tenant::in_space` with a boxed future. The group-row SELECT is
  also duplicated verbatim in `find`/`find_by_thread` and again (with `g.` prefixes) in the other two.
  A column rename must be fixed in four places — the opposite of what `rows.rs` :1–5 promises.
- **"Creation order" is not actually preserved.** All bot memberships are inserted with the same
  `group.created_at` (:87), so `bots_for` orders by `created_at, member_id` (:34) and the DB returns
  lexicographic id order. `NewGroup.bot_ids` is documented as creation order, with the first bot
  owning the shared thread's `bot_id` handle (`ports.rs` :479, migration `0053` :10). `create` echoes
  the input vector (:109), but every subsequent read can return a different first bot (GUESS:
  reasoned from code, not executed against Postgres).

## Guiding principle for v0.4.0

Make hydration, tenant scope and ordering single-sourced without changing the `GroupStore` port, the
`boma.rfc3339(created_at)` alias, the `ORDER BY` ties or any error text. Port signatures and the
`group-create.json` golden are load-bearing; any externally visible change (especially bot order) is
a DEVIATION for `docs/PARITY.md` and needs owner approval before shipping (AGENTS.md).

## Concrete upgrades (5), each with a risk

1. **Set-based hydration: kill the N+1.** Replace the per-row `bots_for` loop with one query over the
   whole id set: `SELECT group_id, member_id FROM group_memberships WHERE group_id = ANY($1) AND
   member_kind = 'bot' ORDER BY group_id, created_at, member_id`, then bucket into a
   `HashMap<GroupId, Vec<BotId>>` and zip with the group rows in `list_for_member`; use the same
   helper (single id) in the other three reads. *Risk:* RLS on `group_memberships` uses a correlated
   subselect on `groups` (`0053` :40–43); `ANY($1)` must stay index-shaped on
   `group_memberships_bot`. *Mitigation:* keep `member_kind` as the leading predicate, add a
   `req_010_list_for_member_is_single_query` test (count statements via a tracing layer), and replay
   the group golden before/after to confirm identical output.

2. **Centralise the four wrappers.** Add private `fn member_space(context) -> Result<SpaceId,
   AppError>` (one `Member` match instead of four) and a `const GROUP_COLUMNS: &str` plus a small
   `load_groups(conn, predicate_sql, params)` helper that owns the FROM/WHERE and maps via
   `rows::group`. Keep predicates as fixed string constants with **bound** parameters, never
   interpolated ids. *Risk:* a generic helper over `&mut PgConnection` with the higher-ranked
   boxed-future signature in `tenant.rs` is easy to over-engineer. *Mitigation:* land only the two
   trivial extractors first; do not abstract the boxed future itself unless the diff shrinks.

3. **Make bot order real, not accidental.** Add an `ordinal` column to `group_memberships` (or store
   per-bot insertion offsets) and order by it, so the first member is deterministically the thread's
   bot owner. *Risk:* a migration + backfill is required and the resulting order can differ from
   today's lexicographic one — externally visible, so DEVIATION material. *Mitigation:* for v0.4.0
   first add a test asserting `create` input order == read order on ids chosen to fail lexicographic
   sorting; if it fails, ship the column behind owner approval rather than silently re-ordering.

4. **Classify errors instead of blanket `Unavailable`.** `create` maps a duplicate-id PK violation to
   `AppError::Unavailable` (:63–65, :78), and `bots_for`/`create` turn a bad stored/new id into
   `Unavailable` (:42–43, :104) — an outage is conflated with a conflict or corrupt data. Use the
   existing `tenant::is_unique_violation` to return a typed conflict for a repeated group id, and log
   id-parse failures at debug (as `rows.rs` :326–329 does) before returning a domain error. *Risk:*
   changing the error variant shifts the HTTP mapping — record in `docs/PARITY.md`; do not log ids or
   names at warn (REQ-054). *Mitigation:* add `req_010_duplicate_group_is_conflict` and a corrupted-row
   unit test; keep `Unavailable` as the fallback for every unknown driver error.

5. **Resolve the `find_for_bot` LIMIT 1 ambiguity.** The schema allows a bot in many groups yet the
   port promises "the group a bot is a member of" and the query silently takes `LIMIT 1`
   (:191–192, `ports.rs` :522). Either enforce the invariant with a partial UNIQUE index on
   `(member_kind, member_id) WHERE member_kind = 'bot'`, or change the port to return `Vec<Group>`.
   *Risk:* both options are cross-worker: the index is a migration + a create-path failure mode (a bot
   already grouped), and a port change ripples into `usecases.rs` and the API. *Mitigation:* GUESS —
   spec is silent on one-group-per-bot; raise it in `docs/STATE.md` under "Decisions needed", keep
   `LIMIT 1` and its `ORDER BY g.created_at` unchanged until answered, and add a test documenting
   current behaviour.

## Cross-cutting risks / sequencing

- Sequence 2 → 1 → 3 → 4 → 5: shrink the wrappers, then change hydration, then order, then errors,
  then the invariant. Each step leaves the port and goldens intact until item 3.
- `GroupStore`, `NewGroup`, migration `0053` and the RLS policies are other workers' scope: coordinate
  items 3/5, do not duplicate those edits. No new dependency is needed.
- Gate each step with `cargo fmt --check`, `clippy -D warnings`, a Postgres/testcontainers integration
  test with `REQ-010`/`REQ-043` in its name, the group golden, and `tools/trace_check.py`.

## Out of scope for this module

The mention/fan-out and bot-to-bot handoff logic (`usecases.rs`), the HTTP DTOs, `rows.rs` mapping and
migration `0053` itself belong to other workers. This proposal only restructures the group store adapter.
