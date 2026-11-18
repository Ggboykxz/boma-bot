# UI proposal 081 — beauty (self-hosted salon and spa desk)

Status: PROPOSAL (GUESS where the spec is silent). Sector specialist #081. Read-only on product code.
Grounding: `docs/UI.md`, `spec/08-ui-ux.md`, and
`apps/web/components/{thread/thread-composer,shell/bot-rail}.tsx` (two components read this session).
RPC names confirmed against `apps/web/lib/api/rpc.ts`.
Charter (CONFIRMED): Boma Bot only; fresh design (ADR-015) with the owner-supplied robot-cat as the
sole identity — no upstream/OpenBot brand, code, wording or assets; BoardUI semantic tokens only,
no raw palette classes or hex, classes merged with `cx()`; dark mode via tokens; one design system,
no second; self-hosted / on-prem, no SaaS (client photos, service notes and contact details never
leave the salon's own machine); product name and robot-cat mark unchanged.

## 1. Persona and top jobs-to-be-done

**Persona — Nadia, senior esthetician and owner of a two-chair salon.** Back-to-back appointments,
hands often wet or gloved, a shared workstation, and a multilingual clientele (French, English,
Hebrew). She runs Boma Bot on the salon's own machine: no cloud, no client photo or contact detail
leaving the premises, and consent for every image. A wrong click on a client-facing action is a
trust and privacy incident, so ambiguity is worse than latency. She is not a technologist.

Top jobs:
1. **Prepare** — before a client arrives, see the bot's one-screen brief (service history, product
   sensitivities, consent on file, open questions) without hunting through the rail.
2. **Capture** — write a service/treatment note and attach before/after photos quickly, with wet or
   gloved hands, without losing the draft to a stray Enter.
3. **Confirm** — review an action the bot proposes (a retail recommendation, a rebooking reminder,
   a before/after post) and be certain what will be sent to the client before approving; never
   approve on colour alone.
4. **Triage** — across a busy day, find unread client replies and paused follow-ups, snooze
   non-urgent ones, and know at a glance which reminders are still working.
5. **Hand over** — pass a client thread to a covering technician bot at the end of a shift with a
   clear, attributable record and no photo or contact detail leaking into logs or another view.

## 2. Five concrete changes

All strings are new keys in `lib/i18n.ts` through `t(locale, key)`; no hardcoded copy. All use
BoardUI components under `apps/web/components/` and the semantic tokens in `docs/UI.md`.

**(a) Client-brief strip — screen `/app/:botId` (thread header).**
A compact, read-only band above the transcript: chips for "next appointment", "service history",
"consent on file", "sensitivities", each with a `Tooltip` that expands detail. BoardUI `Badge`/`Chip`
(`components/base/badges/chip.tsx`), `Tooltip`, `IconButton`; surface `bg-background-secondary-default`,
hairline `border-separator-border`, text `text-text-secondary`/`text-text-primary`, icons
`text-foreground-icon-secondary`, type `text-caption-1-semibold`. RPCs already present: `botsGet`,
`threadsHead`, `threadsMessages`. Bot-produced and untrusted — render as text, never HTML; keep
client identifiers and photo URLs out of the DOM where possible.

**(b) Consent-and-recommendation approval card — screen `/app/:botId` (pending card, REQ-007/REQ-017).**
Make the "what will happen" sentence dominant (e.g. the exact message or photo to be sent); add an
explicit consequence line and a distinct confirm label ("Send rebooking reminder", not "Approve").
Sensitive actions (publishing a before/after photo, sending a promotional message) use
`border-border-error-default` + `text-text-error-primary` **plus** an icon and the word
"Sensitive" — never colour alone. BoardUI `Button`, `Badge`, `Tooltip`; card surface
`bg-background-primary-default`, `rounded-3xl`, `border-border-button-default`. RPCs: `threadsAnswer`,
`threadsReact`. Announced once through the polite live region (spec §8.9); the transcript is not a
live region.

**(c) Quick-note composer with photo attach — screen `/app/:botId` (composer, REQ-001).**
Extend `ThreadComposer` with a photo `IconButton` (`RiImageLine`) using the installed BoardUI
`file-upload` and a visible consent chip when a before/after image is attached; keep Enter-to-send /
Shift+Enter-newline exactly as today, and keep send disabled while an approval is pending. BoardUI
`IconButton` + `Tooltip` from `components/base/buttons/icon-button.tsx`; focus
`focus-visible:ring-2 focus-visible:ring-border-focus-ring`, 44 px `.boma-target` for gloved use,
icons `text-foreground-icon-primary`. RPC: `threadsSend`; the attached image enters the same `value`
state, so no new transport and no client photo leaves the machine.

**(d) Technician handover banner — screen `/app/g/:groupId` (group thread, REQ-010/REQ-067/REQ-068).**
Add a dismissal-free banner naming outgoing and incoming technician bot with a named status chip,
reusing the existing reassign row. BoardUI `Button`, `Badge`, `Avatar`; tokens
`bg-background-secondary-default`, `border-separator-border`, `text-text-secondary`, and
`text-text-error-primary` plus text+icon for a failed handover. RPCs already present: `groupsGet`,
`groupsMessages`, `groupsReassign` (`runsPause`/`runsResume` if a run must pause). Every message
keeps its bot attribution so the client record stays accountable.

**(e) Booking and follow-up triage in the rail — screen `/app` shell left rail (REQ-009/REQ-011/REQ-012).**
Replace the unread count's bare number with `Counter` + a named `Chip` status ("Awaiting you",
"Reminder due", "Snoozed till 16:00"), with due-time detail behind a keyboard-focusable `Tooltip`.
Group the snoozed shelf by due time using the installed `date-picker`/`date-calendar` if a picker is
added. BoardUI `Counter`, `Chip`, `IconButton`; tokens `text-text-primary`, `text-text-secondary`,
`bg-background-tertiary-default`, `animate-number-fade` for the count per `docs/UI.md`. RPCs:
`threadsMarkRead`, `threadsMarkUnread`, `threadsSnooze`, `threadsUnsnooze`, `runsList`.

## 3. Accessibility, i18n and RTL

- New labels go in `lib/i18n.ts` (English source; French/Hebrew translations; a missing key falls
  back to English, never a blank or the key). No string in JSX.
- `DirectionProvider` sets locale/direction; the boot script writes `lang`/`dir`. Use logical
  utilities (`ms`/`me`, `ps`/`pe`, `text-start`/`text-end`, `border-s`/`border-e`). Mirror the
  reassign/back arrows via `useDirection()`; keep appointment times, phone numbers, client IDs and
  URLs LTR in `<bdi>` / `dir="auto"`.
- WCAG 2.2 AA gate: zero critical/serious axe findings in light, dark and RTL; 44 px targets via
  `.boma-target`; visible token-driven focus; full keyboard path (card decisions, photo attach,
  handover, snooze); status and consent conveyed by text + icon, not colour; 360 px width;
  `prefers-reduced-motion` honoured.
- Client privacy: photo consent and sensitivities are explicit text, never a hover-only tooltip;
  error copy is one sentence of what happened plus one of what to do — no client detail, ID, stack
  trace or provider text on screen (correlation id behind "details" only, spec §8.9). No client
  photo, contact detail or service note is written to logs. The robot-cat mark stays unchanged.

## 4. Playwright tests (REQ ids where they exist)

- `REQ-001 send creates one optimistic message that is not duplicated`
- `REQ-007 pending approval card names the exact action, takes focus, and does not block reading`
- `REQ-009 unread divider moves and snooze shelf returns the bot on time`
- `REQ-012 every run status renders a distinct named chip with the right terminal action`
- `REQ-010 group thread attributes each message and reassigns by keyboard alone`
- `REQ-067 run pause and resume announce their status through the polite live region`
- `REQ-057 Hebrew RTL loads dir=rtl with a clean axe run`
- New behaviour above without a REQ yet (client-brief strip, photo-consent chip) must be given a REQ
  id by the owner before implementation, per the traceability gate — do not invent one in test names.

## 5. Highest-impact first slice

**Change (b), the consent-and-recommendation approval card.** It is the smallest, highest-value
step: one screen, no new transport (`threadsAnswer` already exists), no new component (BoardUI
`Button`/`Badge`/`Tooltip` are installed), and it directly reduces the risk of an ambiguous,
client-facing privacy decision. Test first: write `REQ-007 ... names the exact action` and extend
`spec/golden/ui/REQ-007-pending-card.json`; ship only the dominant consequence sentence, the
explicit confirm label, and the text+icon sensitive marker. Defer (a), (c), (d), (e) until the owner
assigns REQ ids and approves.
