# v0.4.0 reliability proposal — `crates/domain/src/group.rs` (REQ-010)

Worker #234, lens: reliability. Read-only review; no product code changed.
Evidence tags: **CONFIRMED** = read from this worktree this session; **GUESS** = inferred.

## Scope / what the type guarantees today

The module is a passive record: `Group` (lines 66-79) exposes every field as `pub`, including `bot_ids: Vec<BotId>`; `GroupId::new` (48-54) rejects only a trimmed-empty string; `validate_name` (87-98) checks only empty/length; `MAX_GROUP_BOTS = 8` (15) is **not referenced anywhere in the domain crate**; `MemberKind` (20-25) is never used by `Group`. **CONFIRMED**
All real membership rules live in `create_group` (`crates/app/src/usecases.rs:9276-9324`): non-empty list, ≤8, dedup, bot exists in tenant, bot not archived. `crates/infra/src/stores/rows.rs:166-177` rebuilds `Group` by struct literal straight from DB rows with **no invariant check**. **CONFIRMED**
Creation itself is one tenant transaction (thread + group + memberships): a good property to keep. **CONFIRMED** (`crates/infra/src/stores/groups.rs:44-112`)

## Failure modes

### 1. Invalid membership
- **Panic on empty room.** `send_message` falls back to `group.bot_ids[0]` (`usecases.rs:4349`) and `crates/api/src/rpc.rs:3925-3926` indexes `bot_ids[0]` for `owner_bot_id`/`member_count`. Any `Group` rehydrated with zero membership rows (bad import, manual DB edit, partial migration) panics in non-test code with indexing — forbidden by the workspace lints. **CONFIRMED** (source reads + grep)
- **Duplicates and overflow are unrepresentable only by convention.** The domain type accepts 0, 9, or repeated `BotId`s; dedup happens in the app layer, so a second caller (store rehydration, future `groups.add_member`) can reintroduce them silently. Duplicate ids would double-fanout runs/mentions. **CONFIRMED** (domain has no check; `mentioned_bots` at `usecases.rs:4469` iterates every entry)
- **Wrong error class.** Structural violations ("needs at least one bot", ">8 bots", "archived bot") are all returned as `AppError::Rule(DomainError::InvalidName(..))`, conflating a name problem with a membership problem; callers cannot branch on the reason. **CONFIRMED**
- **Opaque ids stay opaque by design** (`crates/domain/src/ids.rs:1-6`, test `req_043…`: `ThreadId::new("bot_123")` is valid). Do **not** add a `grp_` prefix check here — that would break the REQ-043 contract. **CONFIRMED**

### 2. Channel drift
- `Group.thread_id` is the single channel anchor: `room_thread_for_bot` (`usecases.rs:4245-4268`) redirects every message from every member bot into that thread, and `reassign_delegation` gates on `old.thread_id == group.thread_id` (`usecases.rs:9962`). A stale, orphaned or cross-space `thread_id` therefore yields `NotFound` on send or a silent wrong-room reply. **CONFIRMED**
- `find_by_thread` (`groups.rs:130-160`) resolves the inverse mapping but neither it nor `rows::group` verifies that the thread's owner bot is still a member or that the thread exists in the same space; referential integrity is left entirely to DB constraints. **GUESS** (no FK definition was inspected)
- Group routing is unconditional, so a REQ-048 external-channel installation bound to a bot's own thread would be redirected into the group room when that bot joins a group — replies stop coming back to the channel the user wrote in. **GUESS** (inbound wiring not inspected; the unconditional redirect in `room_thread_for_bot` is CONFIRMED)
- The thread owner is `group.bot_ids.first()` at creation (`groups.rs:53`). If that bot is later archived or its row removed, no domain or app code re-anchors the channel — the group keeps pointing at a thread whose owner is no longer a member. **GUESS** (no add/remove-member flow was found by grep, so this arises from archival or out-of-band edits)

### 3. State transitions
- `Group` has no lifecycle field: create is the only modeled transition; member add/remove/archive/deletion are absent, so "state" changes implicitly through the membership table and bot archival. **CONFIRMED**
- `create_group` rejects archived bots at creation, but `mentioned_bots` (`usecases.rs:4469-4487`) and delegation target selection (`usecases.rs:8136-8153`) do **not** filter archived members: a bot archived after creation still receives mentions and delegations. **CONFIRMED**
- `groups.create` has no idempotency key: `NewGroup` carries no nonce (`ports.rs:472`) and ids are freshly generated each call, so a client retry after a timeout creates a second room. REQ-002 solves this for messages with `clientNonce`; groups lack the equivalent. **CONFIRMED**

## Concrete hardening (v0.4.0)

1. **Checked constructor + accessors.** Add `Group::try_new(..)` validating name, `1..=MAX_GROUP_BOTS` bots, uniqueness, non-empty ids; make fields private with `owner_bot_id() -> Option<&BotId>` and `bot_ids() -> &[BotId]`. Replace all three `[0]` reads with `owner_bot_id().ok_or(AppError::Unavailable)?`. Call `try_new` in `rows::group` and at the end of `create_group`. **Risk:** cross-crate field reads need migration; rejected legacy rows could make a room unavailable — mitigate by logging + metric on read and keeping an explicitly named unchecked path for one release. Purely internal API: no wire change.
2. **Membership newtype.** Introduce `MemberSet` built via `TryFrom<Vec<BotId>>` that enforces count/uniqueness once, used by both `create_group` and `rows::group`; retire the dead `MemberKind` confusion by documenting that person membership is the owner row. **Risk:** additive type is cheap; making `create_group` consume it changes only the app layer. Add typed `DomainError::InvalidMembership` / `GroupChannelDrift` variants while keeping today's HTTP status mapping, so no external DEVIATION is needed.
3. **Channel-coherence check.** Add pure `Group::is_channel_coherent(thread_owner_bot_id)` and call it in `room_thread_for_bot`: if false, return `AppError::Unavailable` + `tracing::warn!` (ids only, no names/PII) instead of routing to a suspect room. Add a metric counter for orphan/degraded groups and revalidate `find_by_thread` against the same space. **Risk:** refusing a degraded room is externally visible → needs an ADR/DEVIATION entry and a metric-first rollout, not a silent flip.
4. **Archive-aware fanout.** Filter archived bots in `mentioned_bots` and delegation target lookup; if every mentioned bot is archived, return `Rule` instead of starting runs. **Risk:** behavior change for an edge case that currently "works" (runs an archived bot); record in `docs/PARITY.md` and confirm it matches spec intent before shipping.
5. **Creation idempotency.** Extend `NewGroup` with a client nonce (REQ-002 pattern) and a unique `(space_id, nonce)` constraint; on replay return the existing `GroupsCreated`. **Risk:** needs a migration and a new request field (potential DEVIATION); scope to v0.4.0 only if the wire contract can reuse an idempotency header.
6. **Tests.** Domain unit tests named `req_010_…` for constructor invariants (proptest over counts/duplicates/unicode names); an integration test for an empty/orphan membership row asserting no panic; a test that an archived member is not mentioned; a replay of the existing `req_010_*` goldens to catch routing regressions.

## Rollout / open decisions

- Order: (1)+(2) first (no wire change, removes the panic class), then (3)+(4) behind metrics + ADR, then (5) with a migration.
- Needs owner/ADR: whether degraded groups should fail closed (recommended) or serve stale rooms; whether idempotency is a new field or an idempotency header.
- All claims above are source-read only; no tests were run in this worker session.
