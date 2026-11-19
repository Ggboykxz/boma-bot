# Brief 0003 — "always allow on" through an accessibility lens

Provenance: `docs/openbot-swarm/briefs-1000.jsonl` id 3 (area `always-allow-on`, lens `a11y`).
Upstream clone read as **data only** (`/tmp/opencode/openbot-parity`, HEAD `386085ff`):
`CHANGELOG.md` lines 1362, 1670, 7192, 7751, 8379 and the approval-card/agent-access
components. **No upstream code, asset, wording or brand is reproduced here.** Product code is
read-only; this file is the only artefact written. Claims below are CONFIRMED by those reads unless
marked GUESS. Language: English (AGENTS.md disk convention).

## 1. The upstream capability, in my own words (inspiration only)

When an assistant asks permission for a tool call, one of the available answers authorises the
whole *class* of call from then on instead of just this one occurrence. Taking that answer:

- is offered only while the agent is in the ask-each-time mode (CONFIRMED, CHANGELOG 8379);
- changes the agent's standing to one that no longer pauses for those calls (auto-approve /
  "full access"), a widening that the UI confirms first because it removes a safety gate
  (CONFIRMED, 7751, agent-access confirmation dialog);
- resolves approval prompts that were already waiting, so they do not linger (CONFIRMED, 7751);
- is recorded on the host, so a policy change from another device or administrator is reflected
  on reconnect (CONFIRMED, 7751, `staleTime: 0` re-read in the host access fields);
- can be locked so it cannot be narrowed (CONFIRMED, 1670, "auto-approve locked");
- is scoped by modality: on a small companion surface the persistent answer is deliberately
  absent, so the user cannot authorise text they cannot see (CONFIRMED, 1362, partial approvals
  offer only Deny).

Accessibility interest: this is a *high-consequence widening decision* rendered as a card control.
It is exactly the kind of action that must be reachable by keyboard, named in plain language,
announced to a screen reader, confirmed before taking effect, and never signalled by colour alone.

## 2. What Boma Bot actually has (the parity target)

Boma Bot deliberately has no agent "power levels" and no persistent "always allow" button — adding
one would be a new feature and a DEVIATION requiring owner approval (AGENTS.md). Parity here is
**behavioural and accessibility parity of the decision surface**, not feature-for-feature copying:

- REQ-007 / REQ-017 — a pending approval or purchase card gates a side effect until answered
  (CONFIRMED, `spec/01-requirements.md`, `apps/web/components/thread/message-blocks.tsx`).
- The card is a named `region` ("Pending decision"), carries a polite `status` ("Waiting for your
  decision"), an effect sentence and a reversal sentence, and offers Approve / Deny buttons
  (CONFIRMED, `message-blocks.tsx:154-214`, `apps/web/lib/i18n.ts:114-128`).
- Real RPCs: `threads.answer` with `{cardId, answer}` resolves a card exactly once;
  `threads.updatePolicy` and the `approvalRules` namespace are the in-spec home of "which tool
  calls need approval" (CONFIRMED, `spec/03-api-contract.md:22,46`). Any future persistent
  auto-approve control must write through `threads.updatePolicy`, never a bespoke endpoint.

## 3. Accessibility parity changes (grounded in our own sources)

1. **Focus and state on arrival.** `spec/08-ui-ux.md` §8.3 requires a waiting card to "scroll into
   view, take focus". Today the region has no `tabIndex`/focus effect (CONFIRMED, no focus call in
   `components/thread/`). First slice: move focus to the region (`tabIndex={-1}` + one-shot focus on
   card arrival) so a keyboard/screen-reader user is placed at the decision, without trapping them —
   the transcript stays readable and other threads reachable (§8.3, "Approvals"). WCAG 2.4.3, 2.4.11.
2. **Polite announcement, not a live transcript.** The card's `status` names the waiting state;
   the outcome ("Approved"/"Denied") must land in a polite live region, never by turning the
   transcript into one (§8.9 "Announcements"). Keep `role="status"` on the waiting line and route
   the result through the same region after `threads.answer` resolves. WCAG 4.1.3.
3. **No colour-only state.** The effect/reversal and the answered state are sentences plus an
   icon, never a coloured chip alone (§8.9 "run status vocabulary", docs/UI.md map). The ongoing
   "waiting" indicator must carry text, not just a token colour. WCAG 1.4.1, 1.4.11.
4. **Explicit, confirmed widening — only if approved.** The upstream persistent choice maps to a
   *policy* change in Boma. If the owner approves such a control, place it beside Approve/Deny as a
   separately labelled action, gate it behind a confirmation that states what will no longer be
   asked, write through `threads.updatePolicy`, and expose its on/off state as text plus icon.
   Keep it out of this first slice and list it in `docs/STATE.md` as a proposal.

## 4. Tokens, type and direction (BoardUI only)

- Surfaces/text: `bg-background-secondary-default`, `border-separator-border`,
  `text-text-primary`, `text-text-secondary`, error `text-text-error-primary`; icon
  `text-foreground-icon-primary`. Composite type: `text-body-medium`, `text-body-2-regular`,
  `text-caption-1-semibold`. Merge with `cx()`; focus `focus-visible:ring-2
  focus-visible:ring-border-focus-ring`. `prefers-reduced-motion` keeps the fade, drops movement
  (existing `boma-card-in` → `boma-fade-in` path). Targets ≥44 px via `.boma-target` (WCAG 2.5.8).
- RTL: locale drives `DirectionProvider`; use logical `ps/pe`, `ms/me`, `start/end`, mirror the
  drawer; keep paths, code and URLs LTR in `<bdi>`/`dir="auto"` (§8.9 "RTL", docs/UI.md
  "i18n and RTL", BoardUI mechanics). All card copy stays in the i18n catalogue (REQ-057).
- No Pro components, no second design system, no vendored-source edits.

## 5. One REQ-named Playwright test (first gate)

Add to `apps/web/tests/approval-card.spec.ts` (pattern from the existing REQ-007/017 tests and
`a11y.spec.ts`), named so the REQ id appears in the title:

```
test("REQ-017 the approval decision is keyboard-only, announced, RTL-mirrored and motion-safe", ...)
```

Steps: seed a pending approval via `seedRpc` + `threads/answer` spy; load `/app/bot_golden/`;
assert focus lands on `getByRole("region", { name: "Pending decision" })`; Tab to Deny and
activate with Enter (no pointer), assert the `{cardId, answer}` payload and that
`getByRole("status")` announces the recorded outcome; set `boma-locale=he` in an init script and
re-run to assert `dir="rtl"` and a clean axe run; set `reducedMotion: "reduce"` and assert the card
uses `boma-fade-in`. Zero critical/serious axe findings throughout.

## 6. First slice

Keep it in-spec and small: (a) focus management on card arrival, (b) the polite outcome announcement,
(c) the test above. No new RPC, no new control, no policy change. Land it, run the existing
REQ-007/REQ-017, `a11y`, `golden` and `motion` specs, then propose the persistent auto-approve
control separately for owner approval.
