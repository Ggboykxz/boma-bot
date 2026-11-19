# 0221 — `crates/domain/src/compaction.rs` — architecture (v0.4.0)

- Worker: 221 · Lens: architecture · Target: v0.4.0 · Read-only review; no code edited.
- Module: REQ-069 (window-aware context compaction), ~196 lines, pure domain (std only, no IO/async).
  Public surface: `CompactionReport`, `estimate_tokens`, `message_chars`, `compact_history`; 4 unit tests.
- Spec anchor (REQ-069, SHOULD): compact older history lossily before the window is exceeded, mark
  elisions, tell the person, and never die of "prompt is too long" when compaction can save the turn.
- Read here: `compaction.rs`, `message.rs` (`Block`/`Message`/`MessageRole`), `worker.rs:295-338`
  (the only caller), `spec/01-requirements.md:556-562`, `wave-11/0199-…-architecture.md` (format).
- CONFIRMED = read this session; GUESS = inferred.

## Structure today (CONFIRMED)

Two-pass greedy compressor: pass 1 elides the text of oldest `Tool` messages in place; pass 2 drops
oldest whole messages. Returns `(Vec<Message>, Option<CompactionReport>)`; `None` means "fit as-is".
The caller (`worker.rs`) sets `budget = window_tokens * 3 / 4`, then turns `Some(report)` into a
hardcoded-English `System` note written back to the thread. Four structural seams stand out:

1. **The documented retention policy is not implemented.** The doc comment (lines 46-47) promises
   "system-role messages coming first are kept over user messages at the same age", but `drop_index`
   is `let drop_index = 0usize` (line 85), never reassigned — pass 2 always removes index 0, so a
   leading `System` prompt is dropped *first*. Contract and behavior disagree.
2. **Budget math and elision accounting use different units.** `budget_chars = budget_tokens * 4`
   and `message_chars`/`estimate_tokens` count **chars**; `elide_tool_blocks` and `elided_bytes`
   count **bytes** (`text.len()`, `freed`, the marker), and `if before <= 24` mixes the two. With
   14 locales / RTL / CJK, bytes ≫ chars, so the byte figure shown to the person cannot reconcile
   with the char budget. Size rules are magic numbers (4 chars/token, +1/msg, 512/64/32 per block)
   duplicated across `estimate_tokens` and `compact_history`.
3. **The hot path is quadratic on the exact case REQ-069 targets.** Both loops recompute
   `compacted.iter().map(message_chars).sum()` each iteration (lines 66, 87), and `history.to_vec()`
   clones every payload byte before eliding. Long threads — the motivating input — pay the most.
4. **Failure is invisible and policy is untestable.** If the budget is unsatisfiable, the function
   silently returns an over-budget history with a report; the caller cannot tell the "never dies"
   guarantee failed. Retention order, elision granularity and size constants are hardcoded with no
   policy seam, and there are no property tests over the invariants.

## Concrete upgrades for v0.4.0 (each with a risk)

1. **Implement a real, explicit retention policy.** Replace the frozen `drop_index = 0` with an
   ordered victim selection that protects the newest message and leading `System`/pinned messages,
   then drops oldest droppable messages; make the doc comment and the algorithm agree. Risk: changing
   which messages survive changes the model's visible context and the note's `dropped_messages`
   count — externally visible, so freeze it in a golden/differential test and record any shift in
   `docs/PARITY.md` before shipping.

2. **Unify units behind one typed size model.** Introduce a `TokenBudget`/`SizeEstimate` newtype and
   a single estimator used by both `compact_history` and `estimate_tokens`: pick one unit (chars for
   budget, name the report field `elided_chars` or convert deliberately), centralise the 4-char and
   per-block constants, and fix the `before <= marker_len` guard. Risk: renumbering the report's byte
   count changes the user-facing note text (`worker.rs:317`); treat as a DEVIATION, keep a byte-valued
   field if the wire note is frozen, and add a non-ASCII test asserting budget vs reported accounting.

3. **Make compaction linear.** Carry a running `total_chars`, decrement it as blocks are elided and
   messages dropped, and stop recomputing the sum; consider taking the `Vec<Message>` by value to
   avoid the full pre-elision clone. Risk: a stale running total could undershoot and over-elide —
   guard with a debug assertion / property test that the incremental total equals a fresh sum, and
   benchmark a 50-message, 40k-char thread before/after (criterion) to prove the win.

4. **Model the outcome explicitly.** Replace the `(Vec, Option<Report>)` sentinel with a typed
   `CompactionOutcome` carrying the history, report and a `target_met: bool` (or a `Result` with a
   `BudgetUnsatisfiable` variant), so `worker.rs` can log/act when the window cannot be met. Risk:
   this is a public API change across the domain/app boundary; migrate the single call site first,
   keep the REQ-069 test names intact for `tools/trace_check.py`, and decide logging-vs-failure with
   the owner if "never dies" cannot be honoured — do not silently downgrade to an error the run hits.

5. **Add a policy seam and property tests.** Parameterise retention (protected roles, elision order)
   via an injectable `CompactionPolicy` with a documented default, and add `proptest` invariants:
   result ≤ budget when satisfiable, history never grows, newest and protected roles always survive,
   idempotence, and report counts match the diff. Risk: a naive "always ≤ budget" property is false
   for unsatisfiable budgets and for per-message overhead the estimator ignores — encode the exact
   satisfiability precondition, and use non-ASCII generators so unit-mismatch regressions are caught.

## Explicit non-goals

No new dependency (no real tokenizer) without an ADR and licence check; no change to the note's wire
text, the `BOMA_*` budget config, or the `System`-message notice shape without a `docs/PARITY.md`
entry and owner approval. Keep the module pure (no IO, no async, no tracing) and every REQ id in test
names. GUESS: upstream likely elides more aggressively than our two-pass skeleton — verify against the
original as oracle before claiming parity, do not copy its code or wording.
