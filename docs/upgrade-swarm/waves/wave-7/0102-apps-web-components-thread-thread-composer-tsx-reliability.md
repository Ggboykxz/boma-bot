# 0102 — `apps/web/components/thread/thread-composer.tsx` — reliability lens (v0.4.0)

Target: `apps/web/components/thread/thread-composer.tsx` (106 lines, presentational composer) plus its
only production caller, `components/thread/thread-screen.tsx` (`send()`, lines 264–302; render 539–558).
Spec: `spec/08-ui-ux.md` §8.3 ("Centre — the thread"), REQ-001/REQ-008 (send a message), ADR-013.
Claims: CONFIRMED = read in this tree this session; GUESS = inferred, needs verification. Read-only
review; this document is the only write.

## Current behaviour (read, not run)

- Controlled component: `value`/`onChange` are owned by the parent (`draft`/`setDraft` in
  `thread-screen.tsx`). No draft persistence anywhere in this component (CONFIRMED).
- `canSend = value.trim().length > 0 && !sending && !awaitingApproval`. Two independent send
  paths call `onSend()`: form `onSubmit` (lines 54–59) and `TextArea.onKeyDown` on bare Enter
  (lines 88–96). Both are gated by the same `canSend`, which reads the `sending` *prop* (CONFIRMED).
- `onSend: () => void` carries no error/result; failure feedback lives in the caller as a separate
  `sendFailed` banner with a Retry button outside the composer (thread-screen 503–513) (CONFIRMED).
- `TextField isDisabled={awaitingApproval}`; the textarea itself is never disabled for `sending`,
  matching the docstring promise that a failed send keeps keyboard focus (CONFIRMED).
- Caller `send()`: guards `if (text.length === 0 || sending) return;` then `setSending(true)`;
  generates `clientNonce` fresh on every invocation with `Date.now()+Math.random()`; clears draft
  only on success; on error sets `sendFailed`, in `finally` sets `sending=false` (CONFIRMED).
- Caller wraps the composer in `<ComposerLoader active={runIsActive || sending}>` (CONFIRMED).
- No `isComposing` / `keyCode===229` check anywhere under `apps/web` (grep, CONFIRMED).
- No attachment/upload prop or state in this component; several `docs/ui-proposals/*` plan to add
  `file-upload` IconButtons to it (CONFIRMED).

## Failure modes

1. **Double-send race (high).** `sending` is React state/prop updated asynchronously. Two Enter
   keydowns, or Enter followed immediately by a submit click, in the same tick both observe
   `sending===false` and both call `onSend()`. Because the caller regenerates `clientNonce` per
   call, the server idempotency key cannot dedupe — two user messages and two agent runs land.
   The guard exists but is not synchronous (CONFIRMED by code path; race itself GUESS until a test).
2. **IME / RTL composition aborted (high).** `onKeyDown` treats any bare Enter as Send even when
   `event.nativeEvent.isComposing` is true (Japanese/Chinese/Korean candidate confirmation, and
   Arabic/Hebrew IME). It calls `preventDefault()` *before* checking `canSend`, so the composition
   commit is swallowed and a partial or trimmed-empty draft can be sent while text is still being
   composed. RTL also affects logical layout of the reply banner and button row; there is no
   mirroring concern in this component's flex row (it is symmetric), but the Enter rule is
   language-agnostic and must respect composition state (CONFIRMED code, impact GUESS for locales).
3. **Draft loss on navigation / remount (medium).** `draft` is component-local parent state with no
   persistence. Route change away from `/app/:botId`, reload, or returning to a different bot thread
   discards an in-progress message silently. `EmptyThread.onPick` writes `draft`, so a suggested
   prompt is lost the same way (CONFIRMED).
4. **Upload / attachment failure (medium, forward-looking).** The component has no notion of an
   attachment or its lifecycle. Proposals intend to bolt on `file-upload` buttons; with the current
   `onSend: () => void` and `canSend` that ignores uploads, a send can fire while an upload is still
   in flight or after it failed, sending a message with a missing/broken file and losing the local
   selection. No inline error channel or retry exists inside the composer (CONFIRMED).
5. **Nonce breaks idempotency on retry (medium).** A fresh `clientNonce` is minted each `send()` and
   each Retry. If the first POST timed out after the server committed, Retry posts a *new* nonce and
   duplicates the message instead of being deduped (CONFIRMED code; timeout duplicate GUESS).
6. **Approval gate aborts typing (low).** `isDisabled={awaitingApproval}` makes React Aria drop
   focus and can abort an in-flight IME composition when an approval card appears mid-typing;
   `canSend` already blocks the send, so disabling the field is redundant and harms the "keep the
   keyboard" promise (CONFIRMED code, UX impact GUESS).

## Concrete hardening (v0.4.0)

- **Synchronous send latch:** inside `ThreadComposer`, add `useRef(false)` guard around both send
  paths — a shared `submit()` that returns early if the latch is set, sets it synchronously, calls
  `onSend()`, and releases it when `sending` flips false/true via `useEffect`. Keep `canSend` for
  button styling; the latch is the correctness guard. Add a Playwright test: two rapid Enters and
  Enter+click produce exactly one `threadsSend`.
- **IME-safe Enter:** at the top of `onKeyDown`, `if (event.nativeEvent.isComposing || event.keyCode
  === 229) return;` before `preventDefault()`. Add a test dispatching a keydown with
  `isComposing: true` and asserting no send and no default prevention.
- **Draft persistence:** hoist a per-thread draft (`sessionStorage` keyed by thread/bot id) into the
  parent or a `useDraft(threadId)` hook; restore on mount, clear on successful send. Keep it out of
  `localStorage` and clear on send to limit PII exposure; document as an in-scope UX improvement.
- **Error + upload contract:** extend props with `sendError?: string | null`, `onRetry?: () => void`,
  `uploadState?: "idle" | "uploading" | "error"`; render an inline `role="alert"` with Retry inside
  the composer (or move the existing banner here), include `!uploading && uploadState!=="error"` in
  `canSend`, and keep focus in the textarea. This gives future attachment proposals a safe seam.
- **Stable nonce:** generate `clientNonce` once per draft attempt (store in a ref/state), reuse it
  across retries, and clear it only after a confirmed success; align with the server idempotency
  contract so a timed-out Retry dedupes.
- **Softer approval gate:** replace `isDisabled` with `isReadOnly` (or drop it and rely on
  `canSend`) so focus and composition survive while approvals are pending.
- Keep all copy through `t("…")` and the send button `disabled` only via `canSend`.

## Risks / trade-offs

- A latch released on a `sending` transition can stay stuck if the caller never flips `sending`
  (e.g. throws before `setSending`); mitigate with a timeout fallback and by fixing the caller's
  guard ordering (`setSending`/nonce before any early return).
- `isComposing` is well supported in modern engines; the `keyCode===229` fallback covers legacy
  IMEs but is deprecated — keep both, note in docs.
- `sessionStorage` drafts are per-tab and lose on tab close (acceptable); keying must be stable
  across bot/thread switches or drafts bleed between threads — needs a targeted test.
- New props change the public props interface of a reused primitive; all ~65 doc references and the
  single production caller must still typecheck. Keep them optional with current defaults.
- Nonce reuse changes externally visible retry behaviour (idempotency) — record in `docs/PARITY.md`
  as a DEVIATION if it alters wire semantics, and wait for approval before shipping.
- Inline error UI must use semantic tokens (`text-text-error-primary`, `border-border-error-default`)
  and keep zero critical/serious axe findings; Retry must be reachable by keyboard.
