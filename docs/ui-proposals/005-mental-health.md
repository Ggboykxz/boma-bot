# UI proposal 005 — mental-health support (private practice)

Status: PROPOSAL (GUESS where the spec is silent). Sector specialist #005. Read-only on product code.
Grounding: `docs/UI.md`, `spec/08-ui-ux.md` (§8.3, §8.5, §8.9), and two components under
`apps/web/components/`: `thread/thread-composer.tsx`, `thread/message-blocks.tsx`. RPC names
cross-checked against `apps/web/lib/api/rpc.ts` (CONFIRMED exports).
Charter: Boma Bot only; no upstream/OpenBot brand, code, wording or assets; BoardUI tokens only,
no raw palette/hex, `cx()` to merge; one design system; self-hosted, no SaaS; keep the robot-cat identity.

## 1. Persona and top jobs-to-be-done

**Persona — Yael, a licensed therapist in a small private practice.** She runs Boma Bot on the
practice's own machine: a bot for her own session prep and a client-facing support bot that offers
between-session reflection and grounding. Clients type in French, English, Arabic and Hebrew, often
from a phone, sometimes in distress. Nothing client-related may leave the building or reach a log.
Yael is not a technologist; an ambiguous safety cue or a leaked note is a harm, not a nuisance.

Top jobs:
1. **Notice risk** — see a client's crisis signal immediately, with a named action, never buried in a transcript.
2. **Check in** — let a client state how they feel in one tap, without a long form or a lost draft.
3. **Frame the boundary** — make plain that the bot is a support aid, not a clinician or a substitute for care.
4. **Protect confidentiality** — control what is stored, exported or shown on a shared screen/device.
5. **Continue between sessions** — keep a private journal of reflections the client can reopen in a thread.

## 2. Five concrete changes

All strings are new keys in `lib/i18n.ts` via `t(locale, key)` / `useTranslation()` — none in JSX.
All colours ride the semantic tokens in `docs/UI.md`; classes merge with `cx()` from `@/utils/cx`.

**(a) Crisis-support card — screen `/app/:botId`, pending card (REQ-007 pattern).**
When the bot emits a safety card, reuse the `CardBlock` machinery in `message-blocks.tsx`: a
read-only card with a dominant consequence sentence, a "Find help now" action and a "Talk to a
person" hand-off. Urgency is `border-border-error-default` + `text-text-error-primary` **plus** an
icon (`RiAlertLine`) and the word "Urgent" — never colour alone. It scrolls into view and takes
focus but does not block reading or other threads (spec §8.3). BoardUI `Button`, `Badge`, `Tooltip`;
surface `bg-background-primary-default`, `rounded-3xl`, `border-border-button-default`, type
`text-body-medium`. RPCs already exported: `threadsAnswer`, `threadsSubscribe`, `threadsMessages`.
Crisis resources are locally configured copy, not a client-side call to any third party.

**(b) Mood check-in strip — screen `/app/:botId`, composer (REQ-001).**
Extend `ThreadComposer` with a compact row of mood `IconButton`s above the text field. Choosing one
sends a short, catalogue-authored message through the existing `value`/`onSend` path — no new
transport. Keep Enter-to-send / Shift+Enter-newline exactly as today and keep send disabled while an
approval is pending. `IconButton` + `Tooltip` from `components/base/buttons/`; icon
`text-foreground-icon-primary`, focus `focus-visible:ring-2 focus-visible:ring-border-focus-ring`,
44 px `.boma-target`, pills `rounded-full`. RPC: `threadsSend`. Icon alone is insufficient: each
mood has a visible text label and an `aria-label` from the catalogue.

**(c) Session-boundary strip — screen `/app/:botId`, thread header.**
A read-only band above the transcript: a named chip "Support bot — not a clinician", a "Session
goal" chip and an "Open safety plan" chip, each with a `Tooltip` and a plain-language sentence.
Built from BoardUI `Badge`/`Chip`, `Tooltip`, `IconButton`; surface `bg-background-secondary-default`,
hairline `border-separator-border`, text `text-text-secondary`/`text-text-primary`, type
`text-caption-1-semibold`. RPCs: `botsGet`, `threadsHead`, `threadsMessages`. Bot-produced content
is untrusted — render as text, never HTML; keep client identifiers out of the DOM where possible.
The robot-cat mark is the only identity art; no new imagery is introduced.

**(d) Confidentiality and data controls — screen `/app/:botId` header overflow menu.**
Use the installed `Dropdown` + `Switch` + `Button` to expose "Private session" (hide previews on
this device), "Export this journal" and a clear note on what is stored locally. Role-gated like
other header actions, so a client-facing bot cannot silently expose practitioner views. Tokens:
`bg-background-secondary-default`, `border-separator-border`, `text-text-secondary`, errors via
`text-text-error-primary` **plus** text. RPCs that exist today: `botsGet`, `threadsMarkRead`,
`threadsMarkUnread`, `threadsSnooze`, `threadsUnsnooze`; a real delete/export procedure does not
yet exist — do not fake one with a snooze. Every destructive control needs a REQ id first.

**(e) Private journal library — screen `/app/artifacts` (spec §8.5).**
Reuse the artifact grid as a journal: cards for name, type, owner bot and updated time, filtered by
bot, searchable by name; the viewer offers version selector, download and "open in thread".
BoardUI `Table`/`Card`, `Badge`, `IconButton` with `bg-background-tertiary-default`,
`border-separator-border`, `text-text-primary`. RPCs: `artifactsListSpace`, `artifactsGet`,
`artifactsListVersions`. Content is untrusted: never render artifact HTML in the app document,
and images stay inside a bounded frame.

## 3. Accessibility, i18n and RTL

- Every string in `lib/i18n.ts` (English source; French/Arabic/Hebrew translations; a missing key
  falls back to English, never a blank or the raw key). No string in JSX.
- `DirectionProvider` + boot script set `lang`/`dir` together; use logical utilities (`ms`/`me`,
  `ps`/`pe`, `text-start`/`text-end`, `border-s`/`border-e`). Mirror the check-in row and the
  disclosure arrow via `useDirection()`; keep clinician names, phone numbers, URLs and any crisis
  hotline number LTR in `<bdi>`/`dir="auto"`.
- WCAG 2.2 AA gate: zero critical/serious axe findings in light, dark and RTL; 44 px `.boma-target`;
  visible token-driven focus; full keyboard path (card decisions, mood row, menu controls, export);
  status by text + icon, not colour; 360 px width; `prefers-reduced-motion` honoured; no animation
  on streaming content (spec §8.9).
- Announce a safety card, mood confirmation and a failed hand-off once through the polite live
  region; the transcript itself is not a live region (spec §8.9).
- Privacy: error copy is one sentence of what happened plus one of what to do — no names, ids,
  stack traces or provider text on screen (correlation id behind "details" only). Nothing
  client-related is passed to a hover-only tooltip, and no journal content reaches logs.

## 4. Playwright tests (REQ ids where they exist)

- `REQ-007 pending safety card names the exact action, takes focus, and does not block reading`
- `REQ-001 mood check-in sends one optimistic message that is replaced, not duplicated`
- `REQ-009 unread divider and snooze shelf keep a paused follow-up visible and on time`
- `REQ-012 every run status renders a distinct named chip with the right terminal action`
- `REQ-021 a failed run shows a plain-language cause and a working retry action`
- `REQ-057 Hebrew RTL loads dir=rtl with a clean axe run`
- Safety card, mood row, boundary strip, confidentiality controls and journal library above have
  no REQ yet: the owner must assign REQ ids before implementation; do not invent them in test names
  (traceability gate).

## 5. Highest-impact first slice

**Change (a), the crisis-support card.** It is the smallest, highest-value step: one screen, no new
transport (`threadsAnswer`/`threadsSubscribe` already exist), no new component (`CardBlock`,
`Button`, `Badge`, `Tooltip` are installed), and it directly reduces the worst harm in this sector —
an ambiguous risk signal. Test-first: write `REQ-007 pending safety card names the exact action...`
and extend `spec/golden/ui/REQ-007-pending-card.json` first; ship only the dominant consequence
sentence, the explicit "Find help now"/"Talk to a person" labels and the text+icon urgent marker.
Defer (b)–(e) until the owner assigns REQ ids, confirms the locally configured crisis resources and
approves.
