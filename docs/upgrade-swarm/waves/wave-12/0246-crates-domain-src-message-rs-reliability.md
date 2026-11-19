# Upgrade proposal — `crates/domain/src/message.rs` (lens: reliability)

- Worker: 246
- Target version: v0.4.0
- Lens: reliability (representable invalid states, silent drift, broken ordering invariants)
- Evidence basis: `message.rs` read in full this session (234 lines); `error.rs`, `ids.rs`,
  `compaction.rs`, `app/usecases.rs`, `app/agui_provider.rs`, `infra/openai.rs`,
  `testkit/memory.rs` and spec `01`/`02`/`05` consulted. Code facts are **CONFIRMED** by those
  reads; spec-intent and runtime consequences are marked **GUESS**.

## Scope

Pure domain module: `MessageRole`, `Block`, `Message`, `Reaction`, `MessagePage`, text validation
and cursor codec. No IO, no async. Reliability here = "can a message that violates the chat
contract exist in memory, and can ordering be trusted?" Public contract (REQ-001/002/005,
spec `02-data-model.md:42`): unique `(thread, seq)`, monotonic per-thread `seq`, typed blocks
(text/image/file/tool/card), four roles, and `seq` as the only ordering a client may trust.

## Failure modes

**FM-1 — Malformed blocks are fully representable via serde (CONFIRMED, high).**
`Block` derives `Deserialize` with no validation. Every field is a free-form `String`/`bool`:
`Text{text}`, `Image{artifact_id}`, `File{artifact_id, name}`, `Tool{name, running}`,
`Card{card_id, kind}`. Empty/whitespace text, empty artifact ids, empty names/kinds and unbounded
lengths all deserialize cleanly. `validate_message_text` only guards the separate
`SendMessage.text` at the app boundary (`usecases.rs:4287`), never `Message.blocks`. Consumers
use `filter_map(Block::as_text)` (`openai.rs:128`, `agui_provider.rs:62`, `testkit/memory.rs:2708`),
so an empty `Text` block is silently forwarded as empty provider content.

**FM-2 — Empty block list and empty `Message` body (CONFIRMED, high).**
`blocks: Vec<Block>` has no non-empty invariant. `Message` with `blocks: vec![]` is valid Rust and
valid serde; it renders as a blank chat row and yields no provider content. `validate_message_text`
does not see it. There is no `Message::new`; all fields are `pub`, so no construction path can
enforce "a visible message carries at least one meaningful block".

**FM-3 — Role/provenance drift (CONFIRMED, high).**
Author fields are unconstrained `Option`s. `role: Bot` with no `bot_id`, `role: User` with no
`author_user_id`, `role: Tool` with no `run_id`, or *both* `bot_id` and `author_user_id` set are all
representable. Downstream code branches on `role` (`openai.rs:132-141`, `agui_provider.rs:53-56`,
`compaction.rs:69`, unread filter `testkit/memory.rs:2169`), so drift misattributes messages,
corrupts unread counts, and breaks REQ-010 group attribution (`groups.messages` attributed by
`botId`). `run_id`/`client_nonce` coherence (REQ-002) is likewise unchecked: `InvalidNonce` exists
in `error.rs` but no validator in this module.

**FM-4 — Role vocabulary drift across crates (CONFIRMED structural, GUESS impact).**
`MessageRole` is the canonical 4-value vocabulary with `as_str`/`parse`, but provider strings are
hand-mapped in other crates: `Bot => "assistant"` in both `agui_provider.rs:54` and `openai.rs:135`.
A fifth role or a mapping edit in one place silently diverges from the other; `parse("assistant")`
is deliberately rejected (test line 232). No single owned mapping exists.

**FM-5 — `seq` ordering is trusted but never validated (CONFIRMED, high).**
`seq: i64` is a plain pub field: negatives, zero, duplicates and gaps are representable. The spec
requires monotonic/unique per thread (`02-data-model.md:41-42`), but that lives in the DB; in the
domain nothing checks it. `MessagePage` carries `Vec<Message>` with no "newest-first, single
thread, strictly increasing" guarantee, and callers sort manually (`testkit/memory.rs:2202`).
`parse_cursor` accepts `seq:-5` and `seq:0` and has no upper/lower or thread bound. A malformed
page or cursor cannot be detected here and will drive pagination (REQ-005).

**FM-6 — `created_at` is an unvalidated `String` (CONFIRMED, medium).**
Any garbage/empty string is representable; no canonical UTC/RFC3339 parse, no timezone, no
comparison helper. Clients must ignore it for ordering (per spec), but it is user-visible and a
non-canonical value breaks any future lexicographic use. Reply metadata (`reply_to_message_id`,
`reply_quote`, `client_nonce`) is likewise unbounded and unchecked for shape.

## Concrete hardening for v0.4.0

1. **H-1 (FM-1, FM-2): validate on construction and deserialization.** Add
   `Message::try_new(...)` returning `Result<Message, DomainError>` that rejects empty `blocks`,
   empty `Text{text}`, empty artifact/card ids and names, and caps text at `MAX_MESSAGE_LENGTH`
   (reuse `EmptyMessage`/`MessageTooLong`; add `EmptyBlock`/`InvalidBlock` variants as needed).
   Replace derived `Deserialize` on `Block`/`Message` with `#[serde(try_from = "...")]` (or a manual
   impl) calling that validator so serde is not an unvalidated back door. Wire JSON shape is
   unchanged; risk: legacy rows holding empty blocks would be rejected on read — check
   `05-known-defects.md` and golden files; backfill or file a DEVIATION if needed.
2. **H-2 (FM-3): role↔author coherence.** In `try_new`, enforce `Bot ⇒ bot_id.is_some()`,
   `User ⇒ author_user_id.is_some()`, `Tool ⇒ run_id.is_some()`, `System ⇒ neither`, and reject
   dual attribution. Add a typed `DomainError::RoleAuthorMismatch` (safe message, no ids quoted).
   Risk: rejects previously-stored contradictory combos; verify against real data/goldens first.
3. **H-3 (FM-5): ordering invariants in the domain.** Add `#[must_use] fn validate_page(&MessagePage)`
   checking non-empty, single `thread_id`, strictly increasing `seq`, and newest-first order; make
   `parse_cursor` reject `seq <= 0` (or the project's documented floor) so only cursors this product
   writes are accepted. Risk: tightening `parse_cursor` changes acceptance for malformed input only
   (same error class), low risk; stricter page validation may reject legacy pages — sequence after
   H-1.
4. **H-4 (FM-6): typed timestamp + bounded metadata.** Add a canonical UTC `Timestamp` newtype
   (RFC3339 parse/display) for `created_at`, and cap `reply_quote`/`client_nonce` lengths with
   `InvalidNonce` reused. Risk: serde shape changes only if stored strings are non-canonical
   (**GUESS**: DB stores canonical text) — verify against `spec/openapi.yaml`; DEVIATION if changed.
5. **H-5 (FM-4): one owned role mapping.** Add `provider_role(self) -> &'static str` on
   `MessageRole` and have `agui_provider.rs`/`openai.rs` call it instead of local `match` arms; add a
   round-trip test over `ALL`. Internal refactor, no wire change; coordinate across crates.
6. **H-6 (FM-2, FM-3): make invalid states unrepresentable long-term.** Make `Message` fields
   private with accessors once `app`/`infra`/`testkit` construction sites are migrated; keep H-1/H-2
   validators as the gate. Higher churn — do in its own commit after H-1..H-5 are green.

## Risks & ordering

- H-2/H-4 are behavior-tightening (reject previously representable data): check known defects,
  goldens and real rows before merging; file DEVIATIONs if externally visible.
- H-1/H-3/H-5 are behavior-preserving for valid data — do first, each with `cargo fmt --check`,
  `clippy -D warnings` and tests. Guard H-4's timestamp shape with an `openapi.yaml` contract check.

## Tests to add (REQ ids in names)

- `req_001_message_rejects_empty_blocks_and_empty_text_block`
- `req_001_bot_role_requires_bot_id` / `req_010_group_message_has_single_author`
- `req_005_page_requires_strictly_increasing_seq_and_single_thread`
- `req_005_parse_cursor_rejects_nonpositive_seq`
- `req_002_client_nonce_is_bounded_in_domain`
- `req_005_all_roles_round_trip_through_provider_mapping`

Line count: ~105 (<120). Status: proposal only, no code changed (READ-ONLY worker).
