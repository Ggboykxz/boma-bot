# Wave 15 — worker 0348 — `crates/infra/src/stores/groups.rs` (reliability), v0.4.0

Scope: reliability hardening of the Postgres `GroupStore` adapter only. Read-only review; no code changed.
Every claim is tagged CONFIRMED (verified by reading the file/migration this session) or GUESS (inferred).

## What the code guarantees today (baseline)

- CONFIRMED: `create` writes the thread, the group row and every membership inside ONE
  `tenant::in_space` transaction (`groups.rs:51-114`; `tenant.rs:107-143` commits/rolls back as one
  unit, with transaction-scoped RLS tenant setting). Partial create is therefore not a drift source.
- CONFIRMED: all read paths scope by `space_id` (`find`, `find_by_thread`, `find_for_bot`,
  `list_for_member`); `group_memberships` RLS derives its tenant through the group
  (`0053_groups.sql:33-43`). Cross-tenant leakage is not the reliability gap here.
- CONFIRMED: `bots_for` (`groups.rs:30-45`) is the single membership→`bot_ids` hydrator; every read
  calls it, and `list_for_member` calls it once per group (N+1).

## Failure modes

### FM-1 — Membership drift (CONFIRMED; high)
CONFIRMED: `create` inserts every bot membership with the SAME `created_at` (`groups.rs:87`), while
`bots_for` orders by `created_at, member_id` (`groups.rs:33-34`). Creation order therefore degrades
to lexicographic `member_id` order. The port/domain promise "bot members, in creation order"
(`ports.rs:479`, `domain/group.rs:77`) is broken whenever the caller's `bot_ids` order differs from
the id sort order. Concretely: `GroupsCreate.member_bot_ids` returned at create time can differ from
`Group.bot_ids` returned by `find`/`list_for_member`, silently changing mention/fan-out order.
CONFIRMED: `create` echoes the caller's `group.bot_ids` (`groups.rs:109`) instead of re-reading, so
the inconsistency is invisible until a later read.
CONFIRMED: `find_for_bot` uses `ORDER BY g.created_at LIMIT 1` with no `g.id` tiebreaker
(`groups.rs:191-192`): a bot in two groups minted in the same tick resolves nondeterministically.

### FM-2 — Empty / malformed groups (CONFIRMED for code; GUESS for schema intent)
CONFIRMED: the store accepts a bare `NewGroup` with empty `bot_ids`; `.first()` then maps to
`AppError::Unavailable` (`groups.rs:53`), mislabeling a caller/data error as an infrastructure
outage. The use case rejects empty lists (`usecases.rs:9279-9283`), but the adapter is a public
port reachable from other callers — defense in depth is missing.
CONFIRMED: the schema has no invariant requiring >=1 bot and >=1 person membership per group
(`0053_groups.sql:18-24`); empty or person-only groups are storable by any non-use-case path. A
person-only group never appears in `list_for_member`'s inverse? No — it appears for its owner but
has no bot to fan out to; a bot-only group is invisible to every `list_for_member`.
CONFIRMED: a single corrupt membership row makes the ENTIRE group unreadable — `bots_for` maps an
invalid `BotId` to `AppError::Unavailable` (`groups.rs:41-43`) and aborts hydration. One bad row
blacks out a whole room.

### FM-3 — Cascade deletes (CONFIRMED chain; GUESS on trigger frequency)
CONFIRMED FK chain: `group_memberships.group_id -> groups(id) ON DELETE CASCADE` and
`groups.thread_id -> threads(id) ON DELETE CASCADE` (`0053_groups.sql:11,19`);
`threads.bot_id -> bots(id) ON DELETE CASCADE` (`0002_m1.sql:158`); and many thread children cascade
(messages/events, cards `0004_cards.sql:10`, read_states, snoozes, reactions, routines, pending tool
calls). Therefore deleting the FIRST bot (the group thread's owner, `groups.rs:53,60`) or deleting
the thread/space silently destroys the group, all memberships and the whole conversation history.
CONFIRMED: there is no `delete`/`update` method on the store port (`ports.rs:489-546`), so group
removal happens only via these invisible DB cascades — no Rust code observes or audits the blast
radius. GUESS: app paths soft-archive bots (`archived_at`) rather than hard-delete, so the trigger is
most likely a space deletion or a maintenance/direct-DB operation; verify before prioritizing.
GUESS: no tombstone/soft-delete exists, so a cascaded group cannot be distinguished from "never
existed" by `find` (returns `None`).

### FM-4 — Secondary reliability gaps (CONFIRMED)
CONFIRMED: every failure is `Unavailable`, including unique violations on create
(`tenant::unavailable` at `groups.rs:65,78,90,99`; `tenant.rs:153-156`). A retried `create` with the
same minted ids gets a misleading 503-style error instead of a conflict/idempotent result.
CONFIRMED: no `tracing` in this module — drift, corrupt rows and cascades are undiagnosable from
logs. CONFIRMED: `list_for_member` is N+1 (`groups.rs:234-238`).

## Concrete hardening for v0.4.0 (prioritized)

- **H1 (FM-1):** add `position int NOT NULL` to `group_memberships` via a versioned migration;
  backfill with the intended bot order (GUESS: derive from `created_at` then id; document any
  unavoidable ordering loss in `docs/PARITY.md`). Insert with the index of each `bot_id` and order
  `bots_for` by `position, member_id`. Add `g.id` as a tiebreaker to `find_for_bot`. *Risk:* backfill
  of existing rows is best-effort and may not recover lost intent — record as a one-time migration
  caveat, do not claim it fixes already-drifted data.
- **H2 (FM-2):** validate `NewGroup` at the adapter boundary (non-empty/unique `bot_ids`, non-empty
  owner, id well-formed) with a typed `AppError::Rule`/`DomainError`, not `Unavailable`; distinct
  conflict error for unique violations (idempotent retry). Make `bots_for` skip-and-log invalid
  membership rows at `warn!` with ids (no PII) instead of failing the whole read; add a drift metric
  for skipped rows. *Risk:* changing `Unavailable`→`Rule`/conflict is externally visible — ADR +
  `docs/PARITY.md` DEVIATION entry + owner approval before shipping.
- **H3 (FM-2):** add a deferred constraint or trigger enforcing >=1 `'bot'` and >=1 `'person'`
  membership per group at commit (plus the existing `groups_name_not_empty`). *Risk:* triggers add
  write cost and interact with RLS/FORCE RLS; deferred constraint errors surface only at commit, so
  map them to a typed rule error. Consider a maintained `member_count` column only if reads need it.
- **H4 (FM-3):** add a guard so a thread backing a `groups` row cannot be hard-deleted (FK
  `RESTRICT` for that relationship or a BEFORE DELETE trigger raising an exception), and re-point
  the shared thread off any single bot owner if that bot can be hard-deleted. Add a DELETE audit
  trigger/log recording cascade magnitude for space/group deletion. *Risk:* changing `CASCADE`→
  `RESTRICT` can break legitimate space-deletion flows — audit all delete paths first, and prefer a
  soft-delete/tombstone plus an explicit `delete_group` use case that deletes in a controlled order.
- **H5 (FM-4):** batch membership hydration in `list_for_member` with one
  `WHERE group_id = ANY($1)` query grouped in Rust (removes N+1, matches project standard). Add
  structured `tracing::warn!` at every degraded branch. *Risk:* none behavioral; adds a small
  mapping layer — cover with a differential test against the per-group path.

## Tests (names carry REQ id; real Postgres via testcontainers)

- `REQ_010_bot_order_survives_reopen_when_ids_are_not_sorted` — regression for FM-1 (proptest over
  shuffled `bot_ids`).
- `REQ_010_group_without_bot_is_rejected_by_store` and
  `REQ_010_one_corrupt_membership_does_not_hide_group` — FM-2.
- `REQ_010_deleting_owner_bot_does_not_silently_erase_group` and
  `REQ_010_deleting_thread_guard_raises_typed_error` — FM-3 (needs H4).
- `REQ_010_create_with_duplicate_ids_is_idempotent_conflict` — FM-4; plus an N+1 query-count test
  for `list_for_member` (H5) and an original-vs-new differential parity run recorded in `docs/PARITY.md`.

## Decisions needed / acceptance

- GUESS → CONFIRM: does any runtime path hard-delete a bot/thread/space, or are all soft? (drives H4
  urgency). Record the answer in `docs/STATE.md`.
- Ship H1+H5+H2 as reliability-only; H3 and H4 need an ADR (schema + externally visible behavior)
  and maintainer veto before release.
- Exit criteria for v0.4.0: no ordering drift on reopen, empty/malformed groups rejected at the
  boundary with typed errors, cascades guarded and observable, and all new tests green against a
  real database.
