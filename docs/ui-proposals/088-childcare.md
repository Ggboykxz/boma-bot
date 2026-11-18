# UI proposal 088 — childcare (early-years setting)

Status: PROPOSAL (GUESS where the spec is silent). Sector specialist #088. Read-only on product code.
Grounding (CONFIRMED, read this session): `docs/UI.md`, `spec/08-ui-ux.md`, and
`apps/web/components/{thread/thread-composer,board/board-screen}.tsx`. RPC names are CONFIRMED from
`@/lib/api/rpc` exports; component paths and tokens are CONFIRMED from `apps/web/components/` and
`docs/UI.md`. Persona, screen treatments and new behaviours are GUESS.
Charter (CONFIRMED): Boma Bot only — no upstream/OpenBot brand, code, wording or assets; fresh design
(ADR-015) with the owner-supplied robot-cat as the sole identity; BoardUI tokens only (no raw palette
or hex), merge with `cx()`; one design system; self-hosted/local-first, never SaaS.

## 1. Persona and top jobs-to-be-done

**Persona — Ms. Nora, nursery manager in a 40-place daycare.** She runs Boma Bot self-hosted on the
setting's own machine: no child record, parent message or medication note leaves the building; there
is no SaaS account. Staff and parents are multilingual (English, French, Arabic). She is not a
technologist; an unchecked bot reply to a parent, or a missed allergy/medication note, is a
safeguarding event, so ambiguity is worse than latency. The bot is an assistant, never a substitute
for her judgement or the required staff-to-child ratio.

Top jobs:
1. **Prepare** — before a session, see one-screen context (children present, open parent messages,
   medication/allergies due, staff on shift) without hunting the rail.
2. **Draft** — produce a parent message or activity note fast, without losing the draft to a stray
   Enter or a rerender.
3. **Guard** — review a proposed parent-facing reply or record and be certain what will be sent, to
   whom, and with what consequence; never approve by colour alone.
4. **Triage** — across rooms, find unread parent messages and paused follow-ups, snooze non-urgent
   ones, and know at a glance which runs are still working.
5. **Hand over** — pass an open room thread to a covering practitioner/bot at shift change with a
   clear, attributable record and no child data in logs or another room's view.

## 2. Five concrete changes

All strings are new keys in `lib/i18n.ts` through `useTranslation()`/`t(locale, key)`; no hardcoded
copy. All use BoardUI components under `apps/web/components/` and the semantic tokens in `docs/UI.md`.
Bot- and parent-produced content is untrusted: render as text, never HTML (spec §8.5).

**(a) Session-prep context strip — screen `/app/:botId` (thread header).**
A compact, read-only band above the transcript: chips for "children present", "parent messages open",
"medication due", "staff on shift", each with a `Tooltip` (`components/base/tooltip/tooltip.tsx`) that
expands detail. BoardUI `Chip`/`Badge` (`components/base/badges/chip.tsx`), `Tooltip`, `IconButton`;
surfaces `bg-background-secondary-default`, hairline `border-separator-border`, text
`text-text-secondary`/`text-text-primary`, icons `text-foreground-icon-secondary`, type
`text-caption-1-semibold`. RPCs already available: `botsGet`, `threadsHead`, `threadsMessages`. Keep
child names and identifiers out of the DOM where possible. New behaviour — needs a REQ id from the
owner before implementation.

**(b) Safeguarding approval card — screen `/app/:botId` (pending card, REQ-007/REQ-017).**
Make "what will be sent, and to whom" the dominant line; add an explicit consequence summary and a
distinct confirm label ("Send to parent", not just "Approve"). Parent-facing / high-impact cards use
`border-border-error-default` + `text-text-error-primary` **plus** an icon and the words
"Safeguarding — check before sending" — never colour alone. BoardUI `Button`, `Badge`, `Tooltip`;
card surface `bg-background-primary-default`, `rounded-3xl`, `border-border-button-default`. RPCs:
`threadsAnswer`, `threadsReact`. Announce the card once through the polite live region (spec §8.9);
the transcript is not a live region.

**(c) Guardrail composer — screen `/app/:botId` (composer, REQ-001).**
Extend `ThreadComposer` with a visible "safeguarding on" chip and a shield `IconButton` with a
`Tooltip` explaining the mode. Keep Enter-to-send / Shift+Enter-newline exactly as today and keep
send disabled while an approval is pending (`awaitingApproval`). BoardUI `IconButton`
(`components/base/buttons/icon-button.tsx`) + `Tooltip`; icon reference from `@remixicon/react`
(e.g. `RiShieldCheckLine`); focus `focus-visible:ring-2 focus-visible:ring-border-focus-ring`, 44 px
`.boma-target`, `text-foreground-icon-primary`. RPCs: `threadsSend`; the chip is local state, no new
transport.

**(d) Shift-handover banner — screen `/app/g/:groupId` (room group, REQ-010/REQ-067/REQ-068).**
Add a dismissal-free banner naming the outgoing and incoming practitioner/bot with a named status
chip, reusing the existing reassign row. BoardUI `Button`, `Badge`, `Avatar`
(`components/base/avatar/avatar.tsx`); tokens `bg-background-secondary-default`,
`border-separator-border`, `text-text-secondary`, `text-text-error-primary` for a failed handover
plus text and icon. RPCs already present: `groupsGet`, `groupsMessages`, `groupsReassign` (and
`runsPause`/`runsResume` if a run must pause). Every message keeps its bot attribution so the record
stays accountable.

**(e) Parent-message triage in the rail — screen `/app` shell left rail (REQ-009/REQ-011/REQ-012).**
Replace the unread count's bare number with `Counter` + a named `Chip` status (e.g. "Needs review",
"Working", "Snoozed till 16:00"), and expose due-time detail via a keyboard-focusable `Tooltip`.
Group the snoozed shelf by due time using the installed `date-picker`/`date-calendar` if a picker is
added. BoardUI `Counter` (`components/base/badges/counter.tsx`), `Chip`, `IconButton`; tokens
`text-text-primary`, `text-text-secondary`, `bg-background-tertiary-default`, `animate-number-fade`
per `docs/UI.md`. RPCs: `threadsMarkRead`, `threadsMarkUnread`, `threadsSnooze`, `threadsUnsnooze`,
`runsList`.

## 3. Accessibility, i18n and RTL

- New labels go in `lib/i18n.ts` (English source; French/Hebrew translations; a missing key falls back
  to English, never a blank or the key). No string in JSX.
- `DirectionProvider` sets locale/direction; the boot script writes `lang`/`dir`. Use logical utilities
  (`ms`/`me`, `ps`/`pe`, `start`/`end`, `text-start`/`text-end`, `border-s`/`border-e`,
  `rounded-s`/`rounded-e`). Mirror the reassign/back arrows via `useDirection()`; keep session times,
  child IDs, URLs and phone numbers LTR in `<bdi>`/`dir="auto"`.
- WCAG 2.2 AA gate: zero critical/serious axe findings in light, dark and RTL; 44 px targets via
  `.boma-target`; visible token-driven focus; full keyboard path (card decisions, reassign, snooze);
  status conveyed by text + icon, not colour; 360 px width; `prefers-reduced-motion` honoured.
- Child privacy: error copy is one sentence of what happened plus one of what to do — no child name,
  ID, allergy, medication, stack trace or provider text on screen (correlation id behind "details"
  only, spec §8.9). Nothing a practitioner must act on lives in a hover-only tooltip; no child data
  is written to logs, analytics or another room's view. The robot-cat mark stays unchanged.

## 4. Playwright tests (REQ ids where they exist)

- `REQ-001 send creates one optimistic message that is not duplicated`
- `REQ-007 pending safeguarding card names the exact action, takes focus, and does not block reading`
- `REQ-017 approval names the exact parent recipient and effect before it commits, idempotently`
- `REQ-009 unread divider moves and the snooze shelf returns the room on time`
- `REQ-012 every run status renders a distinct named chip with the right terminal action`
- `REQ-010 group room attributes each message and reassigns by keyboard alone`
- `REQ-067 pause and resume announce their status through the polite live region`
- `REQ-057 Hebrew RTL loads dir=rtl with a clean axe run`
- New behaviour without a REQ yet (session-prep strip, guardrail chip) must be given a REQ id by the
  owner before implementation, per the traceability gate — do not invent one in test names.

## 5. Highest-impact first slice

**Change (b), the safeguarding approval card.** Smallest, highest-value step: one screen, no new
transport (`threadsAnswer` already exists), no new component (BoardUI `Button`/`Badge`/`Tooltip` are
installed), and it directly reduces the risk of a parent-facing message or child record being sent
unchecked. Test-first: write `REQ-007 ... names the exact action` and extend
`spec/golden/ui/REQ-007-pending-card.json` first; ship only the dominant consequence sentence, the
explicit "Send to parent" label, and the text+icon safeguarding marker. Defer (a), (c), (d), (e)
until the owner assigns REQ ids and approves.
