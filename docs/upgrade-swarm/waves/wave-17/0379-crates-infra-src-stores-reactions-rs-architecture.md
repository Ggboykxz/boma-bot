# v0.4.0 architecture proposal — `crates/infra/src/stores/reactions.rs`

Lens: architecture. Scope: REQ-008 reactions store. Worker #379.

Status: statements marked **CONFIRMED** were read from source this session; the upgrades and their
risks are **GUESS** proposals to be validated by the owning agent before implementation. English.

## Current structure (CONFIRMED)

- `PostgresReactionStore { pool: PgPool }` is a thin adapter implementing the `ReactionStore` port
  (`crates/app/src/ports.rs`, `react` + `list_for_messages`). Both methods run inside
  `tenant::with_tenant`, which opens one transaction, sets `boma.space_id` and
  `boma.system_reason` as transaction-local settings, then commits or rolls back (REQ-043, D-09).
- `react` performs three statements in that transaction: (1) `SELECT 1 FROM messages WHERE id AND
  space_id` → `AppError::NotFound`; (2) `INSERT ... ON CONFLICT DO NOTHING` or `DELETE`;
  (3) `SELECT emoji, count(*), bool_or(user_id = $2)` aggregated in SQL, mapped to `Reaction`
  with `u32::try_from(row.count).unwrap_or(u32::MAX)` and `ORDER BY emoji`.
- `list_for_messages` early-returns on an empty input, builds a `Vec<String>` of keys, runs one
  `SELECT message_id, emoji, user_id ... WHERE message_id = ANY($1) AND space_id = $2`, then groups
  in Rust into `HashMap<String, BTreeMap<String, (u32, bool)>>` with `entry.0 += 1` and caller
  attribution via `caller.as_deref()`. Keys are re-parsed with `MessageId::new`.
- Two row mappings exist in `rows.rs`: `ReactionRow { emoji, count: i64, reacted: bool }` and
  `ReactionAttributionRow { message_id, emoji, user_id }`.
- Schema (migration `0009_reactions.sql`): PK `(message_id, user_id, emoji)`, FK cascades, index on
  `message_id`, RLS forced with the space predicate; `thread_id` and `created_at` are written on
  insert but never read.
- `valid_emoji` trims, rejects empty, more than 16 `chars()`, and control characters. No Unicode
  normalization and no grapheme awareness.
- The actor is passed explicitly to `react`; the use case (`react_to_message`) derives it from
  `context.user_id()`. The port itself does not enforce that invariant.

## Architectural tensions

- T1: "the truth for a message" is computed two ways — SQL aggregate in `react`, Rust aggregation
  in `list_for_messages`. Same domain type, two implementations; they can diverge.
- T2: count conversion is inconsistent — saturating `try_from` on one path, unchecked `u32 += 1`
  on the other (debug overflow panics; release wraps). Clippy already forbids explicit `panic`.
- T3: `react` pays three round trips inside its transaction; no row lock serializes concurrent
  reactors, so count correctness relies entirely on the final re-read.
- T4: emoji is an unvalidated `String` at the boundary; the "16 chars" rule counts code points,
  so a valid ZWJ/skin-tone/flag emoji can be rejected and a plain word like `hello` accepted.
- T5: `list_for_messages` accepts an unbounded, unduplicated ID list and exposes an ordering
  (alphabetical via `BTreeMap`) that is an implicit API contract.

## Concrete upgrades (highest value first)

### U1 — One projection for reaction truth
Move the aggregation into a single reusable query/mapper used by both methods: return, per message,
`emoji`, `count`, and `bool_or(user_id = $caller) AS reacted` from the database. `react` reuses it
after the write; `list_for_messages` passes the same caller id through a grouped aggregate instead
of pulling raw rows. Delete `ReactionAttributionRow` and the Rust grouping loop; keep `ReactionRow`
extended with `message_id`. This removes T1 and most of T2 in one change.
*Risk:* SQL `bool_or` with a nullable caller must yield `false`, not `NULL`, or `reacted` changes.
Preserve the `space_id` predicate and RLS on every path. Bulk aggregation changes the query plan —
benchmark at realistic page sizes. Effort: M. Test: one shared REQ-008 integration test asserting
both methods return identical `Vec<Reaction>` for the same seeded state.

### U2 — A total, single count policy
Introduce a `ReactionCount` newtype in `boma_domain` with one documented `TryFrom<i64>` /
`checked_add` policy (saturate at `u32::MAX`, matching today's `react` behavior) and use it in both
row mappings. Removes the remaining T2 divergence.
*Risk:* The `list_for_messages` path changes from wrap/panic to saturation; reaching `u32::MAX`
reactions is unrealistic but technically externally visible — record in `docs/PARITY.md`, do not
ship as a silent change. Effort: S. Test: proptest on boundary values plus a REQ-008-named test.

### U3 — Collapse `react` to one atomic statement
Use a single `WITH` CTE: resolve the message in the tenant, apply add/remove, then return the
aggregate, all in one round trip inside `with_tenant`. Take a row lock on the target message
(`SELECT ... FOR UPDATE`) so concurrent reactors to a popular message serialize cleanly.
*Risk:* `NotFound` semantics must survive the rewrite (currently produced by the separate `SELECT`),
and `FOR UPDATE` adds contention on hot messages — measure before committing. Readability drops
versus the current explicit three steps, so keep the SQL commented. Effort: M. Test: concurrent
add/remove stress test against a real DB, asserting final counts.

### U4 — Emoji as a domain newtype
Replace `valid_emoji(&str)` with a `boma_domain::Emoji` newtype built from a grapheme-aware,
normalized string (Unicode NFC, grapheme count, explicit allowed length), validated once at the
port boundary. The store then accepts `&Emoji`, making invalid states unrepresentable downstream.
*Risk:* stricter validation is a DEVIATION from the current permissive rule and possibly from the
original; must be justified in `docs/PARITY.md` and the exact rule confirmed against the spec or the
original as oracle. Adds a dependency (`unicode-segmentation`) with a license check. Effort: M.
Test: property/fuzz test for the newtype plus REQ-008 acceptance cases (ZWJ, flag, skin tone, word).

### U5 — Harden bulk hydration and document ordering
Deduplicate and cap `message_ids` (cap ≥ a page size), reject non-empty-but-invalid keys explicitly,
and document that reaction order is alphabetical. If a different order is desired, treat it as a
DEVIATION rather than a cleanup.
*Risk:* a cap can drop reactions for oversized batches; choose a safe floor and record it. Avoid
changing output order without parity approval. Effort: S. Test: REQ-008-named test with duplicate
IDs and an over-large batch.

## Cross-cutting note (not a full upgrade)

The store trusts the caller-supplied `user_id`; the invariant that it equals `context.user_id()` is
currently enforced only in the use case. Either encode the actor into the port signature or add an
integration test that pins the invariant, so a future caller cannot attribute a reaction to another
member. No port change this wave — it touches `app`, `api`, and `testkit`.

## Sequencing and gates

1. U1 + U2 together (single projection and single count policy) — largest consistency win.
2. U3 (atomic write/read) only after U1 tests are green, so the CTE has a proven reference result.
3. U4 last: it is the only upgrade that changes accepted input, so it needs the parity decision.
4. U5 is independent and can land any time.

For each step: `cargo fmt --check`, `cargo clippy -- -D warnings`, and the REQ-008 integration tests
against a real database before commit. Claims in `docs/STATE.md` stay GUESS until a command above
is actually run by the owning agent.
