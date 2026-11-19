# 0097 — apps/web/components/thread/message-blocks.tsx — architecture (v0.4.0)

- Target: `before-m5` (v0.3.0-rc5), module `apps/web/components/thread/message-blocks.tsx` (262 lines).
- Read-only review; no code edited. Sister files read for the renderer contract:
  `lib/api/rpc.ts` (`Block` union :22–27, `threadAnswer`/`RpcError` :461),
  `components/thread/thread-screen.tsx` (:384–389), `tests/approval-card.spec.ts`,
  `spec/golden/ui/REQ-007-pending-card.json`, `spec/08-ui-ux.md` §§8.3/8.10,
  `spec/01-requirements.md` REQ-007/017, `docs/adr/033-no-silent-approval-on-empty-answer.md`.

## What this module owns today

Two layers in one file: `MessageBlocks` is a stateless queue that maps `Block[]` to `BlockView` keyed
by `type-index`; `BlockView` is a `switch` over the five `Block` variants; `CardBlock` is the only
stateful piece — a local, per-card answer machine (`busy`, `answerGiven`, `failure`, `draft`) that
calls `threads.answer` once and reports success through `onCardAnswered` so the screen lifts the
composer gate (`thread-screen.tsx:386–388`). This split is sound and should survive v0.4.0: a dumb
renderer, a variant switch, one isolated stateful leaf. The architecture debt is that the leaf owns
both policy (which kinds are blocking) and transport (optimistic state + conflict handling) with no
typed contract to the backend and no single source of truth for "is this card answered".

ADR-033 / REQ-017 are honoured in the current code: the `question` submit path
(`:207–212`) trims the draft and only calls `answer()` when length > 0, and the submit `Button` is
disabled while the draft is empty (`:222`) — no `draft || "approve"` fallback remains. Approval and
purchase kinds keep the explicit Approve/Deny buttons (`:187–201`); there is no Approve affordance on
a question card. Keep these exact semantics in every proposal below; they are pinned by the
`REQ-017 a question card never records an empty answer as approval` Playwright test and the
`REQ-007-pending-card` golden (disabled "Send answer", absent "Approve").

## Block renderer structure in v0.4.0

Keep `MessageBlocks` → `BlockView` → `CardBlock` as the three tiers. Give each tier one job:
`MessageBlocks` = list/keying only; `BlockView` = exhaustive, *compile-time-checked* variant dispatch;
`CardBlock` = a typed answer controller with a narrow props contract. Add an `assertNever` default
arm so a new `Block` variant fails the build instead of silently rendering `null` (`:97–99` today).
The `onCardAnswered` callback currently discards `answer` at the call site — either consume it
(e.g. gate state keyed by card) or narrow the callback type to `(cardId: string) => void` so the API
matches its use and the doc comment at `:30–31`.

## Proposals (concrete upgrades, with risks)

1. **Narrow the card contract to the wire types.** Import a `CardKind` union and a discriminated
   `CardBlock` from `rpc.ts` instead of `{ kind: string }` (`:27`), and type `answer` as
   `"approve" | "deny" | { text: string }`. This makes `explainKey`, `isApproval` and `answeredLabel`
   exhaustive and stops an unknown kind being mislabelled as a question (`:124–129`). Risks: (a) the
   wire shape is hand-written pending codegen (ADR-003) — regenerate or keep a single typed mirror,
   never two; (b) a server sending a new kind must not crash the renderer, so keep a safe fallback
   branch even after narrowing; (c) changing the answer literal shape is a DEVIATION if it reaches
   `threads.answer` — keep `{cardId, answer: string}` on the wire.

2. **Extract the answer state machine into a testable hook** (`useCardAnswer`): own
   `busy/answerGiven/failure/draft`, the conflict branch (`RpcError.code === "CONFLICT"` → honest
   "answered" unlock, `:142–147`) and the no-empty guard, with `CardBlock` reduced to markup.
   Risks: (a) over-abstraction for three call sites — keep it one hook/file, not a class tree;
   (b) moving the guard risks reintroducing the implicit approval if the hook is reused loosely —
   the REQ-017 test must still pin "no RPC on empty"; (c) conflict handling currently treats *any*
   CONFLICT as answered-elsewhere — verify that is the only CONFLICT the endpoint emits (GUESS,
   check `crates/api`), or the UI will lie about a rejected answer.

3. **Add request cancellation and a bounded timeout.** Give `threadsAnswer` an `AbortSignal`
   (abort on unmount / card replacement) and cap slow calls; today a late response can set state on
   an unmounted card. Risks: (a) a fixed timeout punishes poor networks — pick ~10–15 s and surface
   an honest "still trying"/failed state; (b) aborting on re-render must not cancel a legitimate
   in-flight answer when the list re-keys — key cancellation on `cardId`, not component identity;
   (c) an aborted request may have reached the server effect (idempotency keys per REQ-017 are the
   safety net), so the UI must never claim "denied" after an abort.

4. **Typed transport errors with retry affordance.** Replace the `failure: "failed" | "already"`
   booleans with the `RpcError.code` plus a message from the error envelope; map 429/5xx to a retry
   that preserves the draft, keep CONFLICT as the already-answered path. Risks: (a) retrying an
   ambiguous answer is safe only because the server rejects a second answer (REQ-007/017) — confirm
   before showing Retry; (b) never derive card copy from a raw server string (leaks internals, breaks
   i18n) — translate through a `thread.card.*` catalogue key; (c) more states enlarge the golden/axe
   surface — re-run `approval-card.spec.ts`, `golden.spec.ts` and `a11y.spec.ts`.

5. **Preserve BoardUI tokens and a11y while extending the card.** Keep every semantic token
   (`text-text-*`, `bg-background-secondary-default`, `border-separator-border`,
   `text-foreground-icon-secondary`, `text-text-error-primary`) and composite type utility in the
   card; do not introduce raw palette classes or `dark:` literals. Keep `role="region"` with the
   sentence-style `aria-label`, `role="status"`/`role="alert"` for state, `boma-focus` on the
   disclosure summary, and the `boma-card-in` entrance guarded by reduced-motion (spec §8.10, a11y
   gate). Risks: (a) any markup change can break the golden's exact accessible names — run the REQ-007
   golden; (b) the animated check SVG must remain `aria-hidden` and decoration-only, never the sole
   answer signal; (c) RTL: keep the answer form and button group on logical `gap`/flex, no physical
   left/right assumptions.

## Explicit non-goals

No change to the `{cardId, answer}` wire contract, no implicit approval by any path, no removal of
the explicit Approve/Deny buttons, no second design system or raw colour, no new BoardUI Pro
component. The server remains the authority on gates and effect idempotency; this module only
collects an explicit decision and reports it honestly.

## Evidence

CONFIRMED — read this session: `message-blocks.tsx` :25–262; `rpc.ts` :22–27; `thread-screen.tsx`
:384–389; `approval-card.spec.ts` :36–108; `REQ-007-pending-card.json`; ADR-033; REQ-007/017 text.
GUESS — whether `CONFLICT` has meanings besides "already answered" on `threads.answer` (verify against
`crates/api` before proposal 2/4); whether the `onCardAnswered` answer argument is used anywhere
outside `thread-screen.tsx`.
