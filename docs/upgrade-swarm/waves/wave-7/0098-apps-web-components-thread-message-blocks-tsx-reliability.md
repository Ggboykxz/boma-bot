# 0098 · apps/web/components/thread/message-blocks.tsx · LENS=reliability · v0.4.0 proposal

## Scope note

Only `CardBlock` is stateful (an `answer` RPC plus `busy`/`answerGiven`/`failure`/`draft`). The
unread divider is owned by `thread-screen.tsx` (l.304-308, 479-490), so this covers only how block
rendering and card answers destabilise it. **[C]** = verified by reading this file, `rpc.ts`,
`thread-screen.tsx`, `spec/08-ui-ux.md` §8.3 and ADR-033; **[G]** = inferred, needs a test.
Proposal only — no code changed; no claim asserts a passing test.

## 1. Failure modes

### 1.1 Card answer races

1. **[C] Double-submit window.** The guard `if (busy || answerGiven !== null) return` (l.132-134)
   reads render-time state, and `setBusy(true)` (l.135) is async. Two events in one React batch
   (double-click, Enter + click, repeat keypress) both pass, firing two `threadsAnswer` calls. The
   server is idempotent by effect key (migration 0004), but the UI can show the second reply's
   `CONFLICT` as "already answered" over a genuine first success and call `onCardAnswered` twice.
2. **[C] No abort, no client timeout.** `threadsAnswer({ cardId, answer })` (l.138) omits the
   optional `signal` that `rpc.ts` (l.460-465) accepts; there is no `AbortController`. On unmount or
   navigation, late `setAnswerGiven`/`setBusy`/`onCardAnswered` fire on a dead tree, and a hung link
   pins `busy = true` forever — the card can never be acted on again.
3. **[C] Index-key remount.** `MessageBlocks` keys rows by `` `${block.type}-${index}` `` (l.36). A
   streamed update or an inserted/reordered block shifts a card's index, React remounts it, and
   `answerGiven`/`failure` reset to null: the card reverts to "Waiting" with live buttons while the
   parent's `answeredCards` set has already lifted the composer gate — card and gate disagree, and a
   second answer is offered (server then returns `CONFLICT`).
4. **[C] One-way, thin callback.** `onCardAnswered` is optional and carries only `(cardId, answer)`,
   no outcome or kind; if omitted the local card looks answered but the gate stays blocked. The parent
   (l.386-388) only adds the id to a set — no persistence — so reload re-offers answered cards.
5. **[G] Split-screen drift.** Another device answering yields `CONFLICT`; handled honestly
   (l.142-146), but with no subscription the card converges only on the next refetch or click.

### 1.2 Empty-answer / implicit-approval regression (REQ-017, ADR-033)

6. **[C] The guard is correct today but fragile.** `draft.trim()` is submitted only when non-empty
   (l.208-211) and submit is disabled while blank (l.222). ADR-033 removed a former
   `draft.trim() || "approve"` fallback; any refactor restoring a default inside `answer()` recreates
   the safety defect. Treat this as a regression-test invariant, not a one-off fix.
7. **[C] Answer string is overloaded as UI state.** The Approve button calls `answer("approve")`
   (l.189) and the checkmark/label key off `answerGiven === "approve"` / `"deny"` / `"answered"`
   (l.155-160, 233). A `question` answered with the literal text "approve" renders the green check
   and "Approved" — a free-text answer mislabelled as an authorisation. The `CONFLICT` sentinel
   "answered" is similarly indistinguishable from real text. This does not authorise an effect by
   itself, but it corrupts the audit-facing story REQ-017 relies on.
8. **[C] Failure path is retryable by design.** A non-`CONFLICT` error leaves `answerGiven` null and
   shows "failed" (l.148, 181-186), so submit re-enables — correct, but with no dedup a slow success
   plus a retry can resolve twice without a generation check (compounds mode 1).

### 1.3 Unread divider

9. **[C] Answer is not locally inert.** `AnswerReply.runId` (rpc.ts l.451-457) means an answer resumes
   the held run, which streams new messages. `thread-screen` recomputes `dividerIndex` from `readSeq`
   and `unreadCount` (l.306-308); `onCardAnswered` advances neither, so immediately after answering
   the divider can sit stale. The fix belongs in `thread-screen` (refetch `threadsHead` / advance
   read position); this module must not assume a stable block list across the answer.
10. **[C] Index keys corrupt the "last read" boundary visually.** When the server replaces a card
    block with an answered text block, following blocks change index and React reuses the wrong
    instance — text/image/file content can briefly render under the wrong block before settling,
    right where the divider sits.
11. **[G] Modal card may be off-screen.** §8.3 (l.112) says an unanswered modal card scrolls into
    view and takes focus; nothing here or in the parent does that, so a card below the divider can be
    answered blind. Focus/scroll is a parent concern; note it for coordination.

### 1.4 Malformed blocks

12. **[C] Empty cardId is actionable.** `normalizeBlock` (rpc.ts l.323-335) yields `cardId: ""` when
    both spellings are missing. `CardBlock` still renders working buttons, so clicking calls
    `threadsAnswer({ cardId: "" })` → server error shown as generic "failed", and `onCardAnswered`
    poisons the parent set with `""`. Text/image/file blocks with `""` fields render empty elements.
13. **[C] Unknown card kinds become questions.** `kind` is any string. Only `approval`/`purchase`
    and `question` are handled (l.121-122); `takeover` — a card kind in §8.3 (l.90) — and any
    unknown kind fall into the free-text form, inviting a user to "answer" a takeover card.
14. **[C] `default: return null` hides data.** `BlockView` (l.97-99) silently drops block types
    outside the union (e.g. a new streamed type before `normalizeBlock` catches up): content
    vanishes with no signal. React escapes `text`/`name`, so this is loss, not XSS.

## 2. Concrete hardening (prioritized)

**P0 — close the answer races**
1. **[C] Ref-based in-flight lock + `AbortController`.** Keep `useRef<Set<string>>` (or one cardId
   key) checked and set *before* the async call, with a single `busy` map for rendering; store the
   controller, pass `signal` to `threadsAnswer`, abort on unmount, release in `finally`. *Risk:*
   aborting a request the server applied looks like local failure — reconcile on refetch / `CONFLICT`,
   never claim the effect did not happen.
2. **[C] Stable keys.** Use `cardId` for cards and `artifactId` for image/file, index only as last
   resort. *Risk:* cardId must be globally unique; if not, prefix with the message id (new prop).
3. **[C] Separate outcome from answer text.** Replace `answerGiven: string | null` with an explicit
   `{ status: "approve" | "deny" | "text" | "answered"; value?: string }`; keep the wire `{cardId,
   answer}` unchanged. *Risk:* touches label rendering and goldens — keep i18n keys stable.
4. **[C] Pin ADR-033 with a test inside `answer()` too**, not only in the submit handler: reject
   empty/`"approve"` defaults regardless of call site. *Risk:* none; protects the REQ-017 gate.

**P1 — honest, robust rendering**
5. **[C] Guard malformed cards:** render empty-`cardId` and unknown/`takeover` kinds as a read-only
   "card unavailable" line, never an answer form. *Risk:* new i18n keys; align takeover copy with
   the computer-panel work.
6. **[C] Fallback for unknown block types** ("unsupported content" line, dev warning) instead of
   `null`. *Risk:* minor visual noise until the union catches up.
7. **[C] Refetch/advance on answer via the parent**, passing outcome+kind through `onCardAnswered`
   so gate and read state match after a reload. *Risk:* `thread-screen.tsx` change (internal, not a
   wire DEVIATION); one extra `threadsHead` call per answer.

**P2 — polish**
8. **[G] Scroll/focus the first unanswered modal card on arrival**, once, respecting
   `prefers-reduced-motion`. *Risk:* scroll jank mid-stream; debounce.
9. **[C] `aria-live="polite"` on the status line** so answers/failures are announced once; keep
   status in words (§8.3). *Risk:* screen-reader chatter if polled — throttle.

## 3. Verification (test-first, REQ-tagged Playwright)

- **REQ-017:** double-click fires one `threadsAnswer`; unmount mid-flight causes no state update;
  delayed success + retry resolves once; literal "approve" as a question renders "answered", not
  "Approved"; empty question answer sends no call (ADR-033).
- **REQ-007 / goldens:** injecting a block before a card keeps its answered state (stable keys);
  empty-`cardId` and `takeover` cards are read-only; unknown type shows the fallback line.
- **REQ-009:** after an answer streams new messages, the divider matches a refetched head;
  keyboard/360 px/RTL/reduced-motion and axe pass with the new live region.
