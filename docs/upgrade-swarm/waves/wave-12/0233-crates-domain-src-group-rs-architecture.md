# Group domain model — architecture proposal (v0.4.0)

Module: `crates/domain/src/group.rs` (REQ-010 group chats).
Lens: architecture. Scope: proposal only — no code was changed.

## Current shape (CONFIRMED — read this session)
- `MemberKind { Person, Bot }` with `as_str()`, `#[serde(rename_all = "snake_case")]`.
- Hand-rolled `GroupId(String)`: `new` rejects blank with `DomainError::InvalidName("empty group id")`,
  `as_str()`; no `Display`, `AsRef<str>`, `Ord`, `#[serde(transparent)]`.
- `Group { id, space_id, name, thread_id, created_at: String, bot_ids: Vec<BotId> }`,
  `#[serde(deny_unknown_fields)]`; only domain logic is `Group::validate_name`.
- Constants `MAX_GROUP_NAME_LENGTH = 64`, `MAX_GROUP_BOTS = 8` (UX rationale in doc comment).
- Invariants are enforced *outside* the model: `app/usecases.rs::create_group` checks
  non-empty, ≤8, dedups by `HashSet`, rejects archived bots; `infra/stores/groups.rs` writes
  the thread, group, N bot memberships and one `person` membership in one tenant transaction;
  `testkit/memory.rs` has its own private `GroupMemberKind`.
- Mention resolution (`usecases.rs::mentioned_bots`) is an async O(bots) ASCII-lowercase
  substring scan for `@<bot name>`; it depends on bot names fetched from the store.
- Zero in-file tests; `lib.rs` re-exports `Group, GroupId, MAX_GROUP_BOTS, MAX_GROUP_NAME_LENGTH,
  MemberKind`.

## Guiding principles
Keep the domain pure (no IO/clock/async). Make invalid states unrepresentable: a `Group` that
exists should already satisfy its own rules. Keep DB column names, wire field names, `as_str()`
values, error messages used by clients and the public re-exports byte-stable — anything else is
a DEVIATION requiring `docs/PARITY.md` and approval before shipping.

## Upgrade 1 — Retire the hand-rolled `GroupId` in favour of `opaque_id!`
- Current: duplicated id boilerplate with a different error variant from every other id.
- Proposal: declare `GroupId` via the `opaque_id!` macro in `ids.rs` (it already generates
  `Display`, `AsRef<str>`, `Ord`, `#[serde(transparent)]`, `DomainError::EmptyId`) and re-export
  as before; keep `GroupId::new` / `as_str` call sites unchanged (CONFIRMED: all consumers use
  only those two methods).
- Why: one id implementation, consistent error taxonomy, no drift as ids evolve.
- Risks: the error text changes from `InvalidName("empty group id")` to
  `EmptyId("GroupId")`; check API error mapping tests before merging. Purely internal refactor
  with no wire/DB change. Effort: low.

## Upgrade 2 — Add a typed membership model so the domain matches the store
- Current: the store persists `group_memberships(group_id, member_kind, member_id)` for both
  persons and bots, and the spec lists `ChatGroupMember` (data-model §2), but the domain only
  carries `bot_ids` plus an implicit owner. `MemberKind` is unused in the domain.
- Proposal: add `GroupMember { kind: MemberKind, id: MembershipId }` (or a `BotMember`/
  `PersonMember` pair) with `MemberKind::try_from(&str)` as the inverse of `as_str()`; add
  `Group::members()` and `Group::owner_id()` accessors. Keep `bot_ids` as the stored field and
  the `groups.get` / `groups.list` wire shape unchanged in this release.
- Why: one vocabulary for membership kind across domain, store and testkit; removes the private
  duplicate enum and makes row decoding testable without a database.
- Risks: spec data-model line "a group thread has one owner bot per member" is ambiguous about
  whether each *person* needs an owner bot — GUESS, record as a decision in `docs/STATE.md`
  before encoding it as an invariant. Do not change persistence or wire fields yet.

## Upgrade 3 — Construct `Group` through a validating constructor
- Current: `Group` is a public-fields data bag; every store/testkit constructor can build an
  empty, oversized or duplicate-member group. The real rules live in `create_group`.
- Proposal: add `Group::new(...)` that enforces the pure rules (non-empty name via
  `validate_name`, 1..=`MAX_GROUP_BOTS` bots, no duplicates, no empty member list) and returns
  a typed `DomainError`; keep fields public for serde/row decoding but mark the constructor as
  the only path used by app/infra. Move the non-empty / ≤8 / dedup checks out of `create_group`.
- Why: invalid aggregates become hard to construct; rules get unit/property tests in `domain`.
- Risks: archived-bot and tenant-ownership checks need IO and must stay in the app layer —
  do not pull them down. Duplicate suppression currently happens *before* the ≤8 check
  (CONFIRMED); preserve that order or an accepted request becomes a rejection (DEVIATION).

## Upgrade 4 — Give the mention grammar a pure home with defined collision rules
- Current: case-insensitive `@name` substring matching runs in the async use case; no handling
  of overlapping names ("Ana" vs "Analyst"), punctuation or non-ASCII case folding.
- Proposal: a domain-level `Mention<'a> / mention_targets(&[(BotId, &str)], &str)` pure function
  with documented collision policy (longest matching name wins, ties deterministic by
  creation order). `mentioned_bots` becomes name lookup + one call to this function.
- Why: the addressing rule for REQ-010 is business logic; it belongs next to the model and can be
  property-tested without a store.
- Risks: any change to who is addressed is externally visible — default to today's exact
  substring semantics and keep the new collision policy behind explicit tests; run the
  original as oracle (differential tests) and record any difference in `docs/PARITY.md` before
  shipping.

## Upgrade 5 — Lock the contract with in-file unit and property tests
- Current: no tests in this module; `MAX_GROUP_NAME_LENGTH` is counted with `chars()`, which is
  not grapheme clusters, so emoji/combining marks are untested; serde shape is untested.
- Proposal: unit tests for empty/over-long/whitespace/Unicode names, all `MemberKind` string
  round-trips, `GroupId` serde and `Display`, and `Group::new` boundary cases (0, 1, 8, 9 bots,
  duplicates); a `proptest` for name validation and constructor invariants. Name tests with the
  REQ id (`req_010_...`) per the traceability gate.
- Why: protects the wire/DB contract while upgrades 1–4 move code around.
- Risks: if tests reveal that `chars()` counts differently from the original's length rule,
  treat it as a parity finding, not a silent fix; confirm against the original and `docs/STATE.md`.

## Suggested order
1 (id consistency) → 5 (tests) → 3 (constructor) → 2 (membership types) → 4 (mentions, last
because it is the only behavior-visible change). Each step: `cargo fmt --check`, clippy, tests.
