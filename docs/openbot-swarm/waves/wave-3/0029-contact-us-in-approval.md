# 0029 — contact-us-in × approval parity proposal (Boma Bot)

- Brief: id 29, area `contact-us-in`, lens `approval` (`docs/openbot-swarm/briefs-1000.jsonl`).
- Status: **PROPOSAL only**. No product code, spec, golden or catalogue was changed.
- Provenance: upstream consulted as **data** only. The named clone was absent this session, so the
  capability is restated generically from the area's role in brief 24 and our own product surface —
  **no upstream code, asset, wording, brand or Pro is used**. CONFIRMED = read this session.

## 1. Upstream capability, in my own words

The "contact us" area is the in-app route by which a person reaches the operator without leaving
the client: a low-key entry in the shell's secondary/footer area opens a way to send a support note
or feedback, usually carrying the current thread or run id so the operator can find the context.
As generic rules, not a copy of any implementation:

- It is a supporting, human-to-operator channel, never a primary agent action; by itself it
  approves nothing and triggers no agent side effect.
- It composes an outbound message plus, where recorded, the sender's contact identity and a
  correlation id tying the note back to the originating thread or run.
- Its labels, reply address and acknowledgement are user-visible and must localize and mirror
  (REQ-057 — the copy concern is brief 24's, not this one).
- It has unhappy paths (failed send, "we'll reply"), also user-visible strings.

Read through the **approval lens**: the channel is human-to-operator, but the moment the agent
acts *on* a contact — sending an external reply, opening or forwarding a support ticket, or
handing off to a human operator with privileges — that action is a side effect and must sit behind
REQ-017's gate. The invariant: a contact-originated effect runs only after an explicit member
decision, and exactly once. This is a permission/consent concern, unlike brief 24's copy concern.

## 2. Parity position for Boma Bot

- **Already ours (CONFIRMED):** the secondary "Support" nav entry (`RiCustomerServiceLine` in
  `apps/web/components/application/dashboard/dashboard-sidebar.tsx`). `CardBlock` in
  `apps/web/components/thread/message-blocks.tsx` already answers cards via `threadsAnswer`, owns
  the `CONFLICT` double-answer guard, renders explicit Approve/Deny `Button`s, a translated region
  name, effect/reversal copy (`thread.card.*`), a status line and an answered confirmation (M34 +
  swarm 2026-11-18-001 decision). REQ-017 backend gating and idempotent effects are done and
  golden-pinned. No new transport is needed.
- **Gap this lens exposes (CONFIRMED in code):** the free-text `question` branch submits
  `answer(draft.trim() || "approve")` from an always-enabled button labelled "Approve". An empty
  draft therefore silently records an approval — an *invisible approval* and a direct REQ-017
  breach ("no effect happens until a member approves"): no member approved, yet an effect is
  authorized. The card is also generic ("Card", "Pending decision"), not contextual to the
  contact/support effect it authorizes, so the member cannot tell what send/ticket/handoff they are
  approving.
- **Out of scope, proposal only:** a new contact transport/ticket backend, a structured effect
  payload on the wire, or durable answered state. Each changes behaviour or wire and needs a new
  REQ id, an ADR and owner approval (AGENTS.md; Queen's scope guard: no wire field, migration or
  golden-data change). No upstream wording, address or asset is imported for it.
- **In scope now:** remove the silent auto-approval and make the card's effect/reversal copy
  contextual to a contact/support relay, using only existing catalogue keys, tokens and RPC. No
  wire, data-format or permission change → **no DEVIATION**.

## 3. REQ-017 and the contextual `CardBlock` — no silent auto-approval

REQ-017 (`spec/01-requirements.md`, MUST): "no effect happens until a member approves" and "an
approval executes its effect once even if retried." `CardBlock` is that gate, so two things must
hold for a contact-originated action:

- **Explicit consent (the hard requirement).** Approval must be a deliberate act. Drop the
  `|| "approve"` fallback; keep the submit disabled until a non-empty answer exists; split the
  "Answer" label of a question from the "Approve" label of an approval so a free-text box never
  reads as a one-click approval. Preserve the `CONFLICT` guard and the exactly-once path, and keep
  the composer locked until a decision is recorded. An empty, defaulted or guessed input is never
  consent.
- **Contextual effect (copy/presentation only).** When a pending card authorizes a contact/support
  relay, its effect, reason, reversal and authorizer are named in one readable sentence through
  existing `thread.card.*` catalogue keys — e.g. what will be sent and by whom — so the member is
  not authorizing blind. Render args as inert text; show status, never the key/token/value
  (REQ-046 rule from the security vote). Mark high-impact with icon **and** word
  ("Contact relay — sends externally"), never colour alone.
- **BoardUI only:** semantic tokens (`text-text-*`, `bg-background-*`, `border-separator-border`),
  composite type utilities, `cx()`, installed `Button`/`Input`, icons by reference, visible focus
  ring. Dark mode rides `.dark` tokens; reduced-motion is already guarded (`globals.css:593`) — keep
  it, do not re-solve it. No raw palette, no `dark:` literals, no Pro, no second design system.

## 4. One REQ-named Playwright test

Add to `apps/web/tests/approval-card.spec.ts`, distinct from the existing
`REQ-007 …` and `REQ-017 …` names (uses the existing `seedRpc` / `seedApproval` pattern):

```ts
test("REQ-017 a contact-reply card never auto-approves an empty answer and records one explicit choice", async ({ page }) => {
  // GIVEN a seeded pending card for a contact/support relay, reached from the Support entry,
  // THEN the region's accessible name is a sentence naming effect, reason, reversal and
  //      authorizer; effect text is inert (no key/secret echoed); the high-impact marker is
  //      icon+word, not colour; the Message composer stays locked.
  // WHEN the free-text answer is left empty and submit is pressed,
  // THEN no approval is recorded: no threads/answer call, card still pending, submit disabled
  //      until a non-empty answer exists (no silent auto-approval).
  // WHEN an explicit Approve / non-empty Answer is given,
  // THEN exactly one attributable answer is recorded and a second answer is rejected
  //      by the existing CONFLICT guard.
});
```

Extend the `REQ-007-pending-card` golden with a seeded answered state (never a click-only path);
keep other goldens and axe unchanged apart from the known local RTL artefact (green on CI).

## 5. First slice (test-first, small)

1. Write the failing REQ-017 test above with the seeded contact-reply pending card.
2. In `CardBlock`'s `question` branch: remove `|| "approve"`, disable submit on an empty draft,
   and separate the "Answer" label from "Approve"; keep the `CONFLICT` guard and exactly-once path.
3. Add/adjust `thread.card.*` effect/reversal/authorizer keys for the contact/support relay in
   en/fr/he catalogues and the English fallback in the other eleven; identical key sets across all
   14 locales (`SUPPORTED_LOCALES`), no raw literal and no key echoed back.
4. Extend the golden with a seeded answered state; no wire, data-format or permission change.
5. Record "durable answered state" and "structured effect payload" in `docs/STATE.md` as proposals
   awaiting an ADR; implement none of them here.
6. Gates: web typecheck, lint, build, Playwright approval-card/shell/golden clusters, 360 px and
   dark mode. Mark CONFIRMED/GUESS in `docs/STATE.md` and update `docs/UI.md` for the component
   edit; touch no dependency (`docs/DEPENDENCIES.md` untouched). Keep trace and gate budgets strict.

## Guardrails

No upstream code, asset, wording, brand or Pro. One design system, one theme. No silent
auto-approval, no guessed or defaulted consent. No new REQ, wire field or behaviour change without
the owner. Every claim marked CONFIRMED only after a command actually ran.
