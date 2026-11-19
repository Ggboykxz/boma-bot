# Upgrade proposal — `crates/domain/src/compaction.rs` (lens: reliability)

- Worker: 222
- Target version: v0.4.0
- Lens: reliability (silent information loss, broken invariants, best-effort-without-signal)
- Evidence basis: `compaction.rs` (196 lines) and `message.rs` read in full this session, plus REQ-069 in `spec/01-requirements.md`. Code facts are **CONFIRMED** by that read; runtime/provider effects are **GUESS**.

## Scope

Pure domain function: `compact_history` + helpers, no IO/async. Contract (REQ-069, CONFIRMED spec lines 556-562): under window pressure, oldest tool payloads/bulky messages are elided **with a note marker**, the turn runs, and a turn "never dies of 'prompt is too long' when compaction can save it". Reliability here = does compaction preserve what the model still needs, cut on safe boundaries, and fail loudly when it cannot meet the budget?

## Failure modes

**FM-1 — Lost context: whole messages vanish with no in-band note (CONFIRMED, high).**
Pass 2 `compacted.remove(drop_index)` deletes messages outright; nothing is inserted into the returned history to tell the model a gap exists. The only marker is the tool-text elision string; dropped user/bot messages leave no trace. The module doc claims "the caller tells the person what happened", but the API only returns a `CompactionReport` — there is no enforced mechanism, so a caller that forgets it silently loses context. Elided tool text is also gone irrecoverably with no relevance check (**GUESS**: oldest tool output may still be the only record of a decision).

**FM-2 — Lost context: the budget guarantee is best-effort and can still kill the turn (CONFIRMED, high).**
`while compacted.len() > 1` stops at a single message. One oversized newest message (max user text is 16_000 chars = `MAX_MESSAGE_LENGTH`, CONFIRMED in `message.rs`) can exceed `budget_tokens*4` alone; the function then returns a still-over-budget history with `Some(report)`, i.e. "success". REQ-069's never-dies promise is violated with no flag distinguishing "fit" from "gave up". Elision can also fail to help: `message_chars` charges fixed 512/name+64/name+32 for Image/File/Card/Tool blocks (CONFIRMED), so non-text bulk cannot be shrunk, and a 25-35 char Text block is replaced by a *longer* marker (net growth) despite the `before <= 24` guard.

**FM-3 — Bad summary boundaries: the system prompt is dropped first (CONFIRMED, high).**
`let drop_index = 0usize` is fixed; `remove(0)` always deletes the oldest element first. The doc explicitly promises "system-role messages coming first are kept over user messages at the same age" — the code does the opposite. A leading `System` message is discarded before older user turns, silently changing assistant behaviour. This is a doc/implementation contradiction, not a corner case.

**FM-4 — Bad summary boundaries: eviction can split a tool call from its result (CONFIRMED structural; GUESS provider effect).**
`message.rs` defines `Block::Tool` (call) on bot messages and `MessageRole::Tool` (output) separately. Dropping index 0 one message at a time can remove a Tool result while its invoking block survives (or vice versa), leaving an orphaned call/result. Many providers reject unmatched tool calls; the cut has no exchange/pairing awareness and no `seq` check (ordering is assumed, not verified — GUESS that caller pre-sorts).

**FM-5 — Unlocalized, misleading system note (CONFIRMED).**
The sole marker `"[elided: {bytes} bytes of tool output]"` is hardcoded English and reports raw UTF-8 byte count, which is meaningless to the model and inconsistent with `message_chars`' char-based accounting. It is untranslated and direction-agnostic despite the project's i18n/RTL policy. No localized marker is emitted for dropped messages at all, so the "tells the person" requirement is unmet in-band.

**FM-6 — Report and token model are internally inconsistent (CONFIRMED, medium/low).**
`elide_tool_blocks` credits `text.len().saturating_sub(24)` (bytes, fixed marker guess), but the real marker length varies with digit count (~35 chars for 40k bytes), so `elided_bytes` is inaccurate. `estimate_tokens` uses `count()/4 + 1` (floor+1), contradicting its "rounded up" doc and the `budget_tokens*4` conversion used by compaction — two token models. Re-running compaction can re-elide an existing marker (`marker > 24` chars → nested marker); no idempotency guard.

## Concrete hardening for v0.4.0

1. **H-1 (FM-3, FM-4): boundary-aware eviction.** Never drop `System` role messages; compact only the mutable non-system window. Evict in complete exchanges (a bot tool-call block and its `Tool` result together), using adjacency/`run_id`/reply metadata; add `debug_assert` that input is ordered by `seq` or sort a working copy. Preserves newest message. Risk: if system messages alone exceed budget, H-4 must handle it — sequence accordingly.
2. **H-2 (FM-5, FM-1): localized note marker.** Insert one explicit `System`-role note at the cut point stating how many messages/payloads were elided, with text from the i18n layer (no hardcoded English, RTL-safe), and mark it so re-compaction recognizes and merges it instead of re-eliding. Satisfies REQ-069's "note marker" and "tells the person" in-band. Risk: note must be in-context only — confirm (GUESS) it is never persisted to `messages`; otherwise this is a data-format DEVIATION requiring `docs/PARITY.md`.
3. **H-3 (FM-2): hard budget guarantee, no silent best-effort.** Add `within_budget`/`remaining_tokens` to `CompactionReport`; if a single message still exceeds budget after elision, truncate its Text blocks (head+tail kept) or return a typed "cannot fit" signal so the caller aborts gracefully instead of sending an oversized prompt. Highest-value fix for the REQ-069 promise.
4. **H-4 (FM-6): one token model and honest accounting.** Make `estimate_tokens` canonical, fix the doc ("floor+1", not "rounded up"), measure elision in chars with the exact replacement length, and only replace when the marker is actually shorter (prevents growth). Rename/redocument `elided_bytes` if it becomes chars — DEVIATION check if any caller reads it.
5. **H-5 (FM-1): relevance guard (optional, GUESS cost/benefit).** Before dropping a whole message, keep a short local summary line rather than nothing. Only if it fits the "lossy but honest" intent; otherwise document that loss is by design.

## Risks & ordering

- H-1 and H-3 are low-churn and behavior-preserving except for the buggy cases — do first, each with `cargo fmt --check`, `clippy -D warnings`, tests.
- H-2 changes in-context content (new note message): inspect `spec/openapi.yaml` and persistence path before merging; file a DEVIATION only if it reaches the wire/disk.
- H-4 touches a public field/API (`elided_bytes`, `estimate_tokens` semantics); check callers and `docs/PARITY.md` first.
- H-5 is a judgement call; leave as proposal pending owner input, not a v0.4.0 blocker.

## Tests to add (REQ ids in names)

- `req_069_system_message_survives_compaction` (FM-3).
- `req_069_tool_call_and_result_are_not_split` (FM-4).
- `req_069_compaction_inserts_localized_note_marker` (FM-5).
- `req_069_single_oversized_message_reports_within_budget_false` (FM-2).
- `req_069_second_compaction_is_idempotent` (FM-6).
- `req_069_elision_never_increases_message_size` (FM-2/FM-6).

Line budget: ~95. Status: proposal only, no code changed (READ-ONLY worker).
