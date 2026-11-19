# 0245 — `crates/domain/src/message.rs` — architecture (v0.4.0)

- Worker: 245 · Lens: architecture · Target: v0.4.0 · Read-only review; no product code edited.
- Module: 234-line pure-domain module — `MessageRole`, `Block` (5 variants), `Message` (15
  fields), `Reaction`, `MessagePage`, `MAX_MESSAGE_LENGTH` + `validate_message_text`, cursor
  helpers, 3 unit tests. Spec anchors: REQ-001 (send → one message, one run), REQ-002 (nonce
  idempotency), REQ-005 (ordered history/cursor), REQ-008 (replies/quotes/reactions/mentions),
  REQ-010 (bot mentions), `AGENTS.md` "make invalid states unrepresentable", "strict
  deserialization", "no externally visible change without ADR / docs/PARITY.md".
- Read here: `crates/domain/src/{message,error,ids,compaction,page}.rs`; `crates/infra/src/
  openai.rs:124–154`, `stores/rows.rs:334–357`, `stores/messages.rs:169–205`; `crates/app/src/
  {agui_provider.rs:53–62, acp_provider.rs:230–239}`; `crates/api/src/wire.rs:1756+`; ~40
  `Block::*` matches in `crates/testkit/src/{build,memory}.rs`; `spec/01,02,03,08`;
  `spec/golden/thread-messages.json`; brief 0169 (format).
- CONFIRMED = read in this session; GUESS = inferred.

## Structure today (CONFIRMED)

`MessageRole`'s wire vocabulary lives in four places: `#[serde(rename_all = "lowercase")]`,
`as_str()`, `parse()`, and `ALL`. Four independent edits keep them in sync today.

`Block` is internally tagged (`type`, lowercase) with five variants; only `Text` carries
content in M1. It has **no** `deny_unknown_fields`, unlike `PageBlock` in `page.rs:49`. Its
`artifact_id`, `card_id` and `kind` are raw `String`, even though `ids.rs` defines twelve
opaque newtypes (no `ArtifactId`/`CardId` yet). Only `as_text()` exists as a helper: every other
inspection is a hand-written `match` repeated across five crates. Model-context builders
(`openai.rs`, both ACP/AG-UI providers), compaction and testkit all collapse blocks to joined
`as_text()` output — so an Image/File/Tool/Card block contributes **zero** content to what the
model sees. That is the central REQ-001 seam: `threads.send` accepts "text, attachments and
mentions" (`spec/03:41`), but the persisted model has no mention representation and attachments
are inert payload; mention resolution is a send-time concern only.

`Message` is a flat struct with six optional identity fields plus the reply pair and nonce,
with no invariant tying them to `role`: a `Bot` message may carry `author_user_id`, a `Tool`
message may lack `run_id`, and `blocks` may be empty. All construction is public struct
literals across infra/testkit/conformance/api tests. `DomainError::InvalidNonce` exists but
this module never validates a nonce; `validate_message_text` counts Unicode scalar values via
`.chars()`, and there is no whole-message validation (empty blocks, sum across text blocks).

`Reaction` is `emoji/count/reacted` — no reactor identity, so per-user reaction dedup cannot be
expressed in the pure domain; `emoji` is unvalidated. `parse_cursor` accepts `seq:007`, negative
seqs and unbounded input, with no thread or tenant binding. `created_at` is an untyped RFC3339
string (goldens). `spec/02:42` lists `searchTokens`, absent from both domain model and
`MessageRow` (an earlier brief already logged it as out of scope).

## Concrete upgrades for v0.4.0 (each with a risk)

1. **Single-source the role vocabulary.** Derive `ALL`, `as_str()` and `parse()` from one
   `[(MessageRole, &str); 4]` table (macro or const), and add a test round-tripping all four
   values through serde as well. Risk: switching serde to `try_from`/`into` changes the error
   text for an unknown role (wire-visible); keep the derived tagging in v0.4.0 and assert
   serde/as_str/parse parity in `req_001_role_vocabulary_has_one_source` first.

2. **Gate whole-message invariants at the boundary.** Add
   `validate_message(&Message) -> Result<(), DomainError>` (non-empty blocks; exactly one
   authorship id consistent with `role`; reply fields paired; nonce length ≤ fixed cap using
   the existing `InvalidNonce`; each text block ≤ `MAX_MESSAGE_LENGTH`) plus a `Message::new`
   constructor, called from the send use case before persistence. Keep fields public initially.
   Risk: import/export (REQ-044) and golden fixtures may hold odd combinations — run conformance
   and all goldens before enabling; rejecting any previously accepted input is a DEVIATION.

3. **Make block→context projection explicit in the domain.** Add `Message::text_parts()` and a
   documented visitor stating which block kinds become model text, attachments, or metadata,
   and have openai/agui/acp/compaction call it instead of hand-matching. Risk: this decides
   whether a future Image/File block reaches the model — externally visible; freeze today's
   text-only behavior in a golden/proptest and treat any change as ADR + DEVIATION. Land the
   refactor behavior-identically.

4. **Type the loose identifiers — not the timestamp yet.** Add `ArtifactId`/`CardId` via
   `opaque_id!` in `ids.rs` and use them in `Block`; `#[serde(transparent)]` keeps JSON identical.
   Risk: many testkit/infra literals must move to `ArtifactId::new(..)?` — migrate mechanically
   and let the compiler find them. Do **not** bundle `created_at: DateTime<Utc>`: RFC3339 string
   is wire-visible (goldens) and deserves its own ADR. `searchTokens` stays separate scope.

5. **Tighten reaction and cursor validation without wire churn.** Validate `Reaction.emoji` as
   one grapheme cluster (unicode-segmentation, so ZWJ/flags still pass) with a new
   `DomainError::InvalidEmoji`; bound cursor length and normalize `seq:007`/negatives by parsing
   to `i64` and re-emitting canonical `seq:7` rather than rejecting; add `MessagePage::has_more()`
   and `#[must_use]` on cursor helpers. Risk: rejecting v0.3-issued `seq:007` would break
   "opaque" cursors — accept-and-normalize avoids that; tenant/thread binding (REQ-043) stays an
   infra decision so bytes do not change.

## Explicit non-goals

No `Mention` block or wire-shape change to `Message`/`Block` in this brief: mentions are a
`threads.send` input resolved at send time, and persisting them changes golden JSON (new REQ +
ADR + PARITY entry + owner approval). No nested `ClientMessage`/`AgentMessage` enum replacing
the flat struct — it would ripple through ~40 literals and DTOs for little gain; prefer #2. No
`deny_unknown_fields` on `Message`/`Block` despite the `PageBlock` inconsistency until a golden
sweep proves no client sends extra fields (otherwise it is a wire-visible rejection). No
behavior change shipped without the ADR/PARITY gate; this worker edited no product code.
