# UI proposal 087 — pet services (grooming, boarding and sitting)

Status: PROPOSAL (GUESS where the spec is silent). Sector specialist #087. Read-only on product code.
Grounding: `docs/UI.md`, `spec/08-ui-ux.md`, and
`apps/web/components/{shell/bot-rail,thread/thread-composer}.tsx`. Existing RPC names checked against
`apps/web/lib/api/rpc.ts`; every name below is CONFIRMED there.
Charter: Boma Bot only; no upstream brand, code, wording or assets; BoardUI semantic tokens only —
no raw palette classes or hex literals — `cx()` to merge; one design system, no second; no SaaS
(self-hosted on the salon's own machine); keep the owner-supplied robot-cat identity.

## 1. Persona and top jobs-to-be-done

**Persona — Maya, owner-operator of a two-chair grooming and boarding salon.** She works from a
shared front-desk machine, often with wet hands, a phone ringing, and a multilingual client base
(English, French, Arabic). Boma Bot runs self-hosted in the salon: no booking, photo or client data
leaves the building. A wrong client update or a missed temperament/vaccination note reaches an owner
or an animal, so an ambiguous confirmation is worse than a slow screen.

Top jobs:
1. **Triage** — see at a glance which bookings are checked in, which grooms are running, which boarding
   stays are due, and which owner messages wait on her, without hunting through the rail.
2. **Capture** — record intake notes (vaccination, temperament, allergies) reliably with unreliable,
   busy hands, without losing a draft to a stray Enter.
3. **Decide** — review a bot-proposed owner update or photo message and be certain what will be sent
   before approving; never confirm by colour alone.
4. **Hand over** — let the bot update the salon's booking/kennel software on-screen, and take over
   cleanly when it needs her hands.
5. **Follow up** — chase due rebookings, boarding check-outs and snoozed follow-ups on time.

## 2. Five concrete changes

All strings are new keys in `lib/i18n.ts` through `t(locale, key)`; no hardcoded copy. All reuse
installed BoardUI components under `apps/web/components/` and the semantic tokens in `docs/UI.md`.

**(a) Today's-bookings triage strip — screen `/app/:botId` (thread header).**
A compact, read-only band above the transcript: chips for "next appointment", "in grooming",
"boarding due", each with a `Tooltip` that expands detail. BoardUI `Chip`/`Badge`
(`components/base/badges/chip.tsx`), `Tooltip`, `IconButton`; surface `bg-background-secondary-default`,
hairline `border-separator-border`, text `text-text-secondary`/`text-text-primary`, icons
`text-foreground-icon-secondary`, type `text-caption-1-semibold`. RPCs already present: `botsGet`,
`threadsHead`, `threadsMessages`. Bot-produced content is untrusted — render as text, never HTML; keep
owner/pet identifiers out of the DOM where possible.

**(b) Owner-update / photo approval card — screen `/app/:botId` (pending card, REQ-007/REQ-017).**
Make the "what will happen" sentence the dominant line; add a consequence summary (which owner, which
photo) and an explicit confirm label ("Send photo update", not just "Approve"). High-impact cards use
`border-border-error-default` + `text-text-error-primary` **plus** an icon and the words
"High impact" — never colour alone. BoardUI `Button`, `Badge`, `Tooltip`; card surface
`bg-background-primary-default`, `rounded-3xl`, `border-border-button-default`. RPCs: `threadsAnswer`,
`threadsReact`. Announced once through the polite live region (spec §8.9); the transcript is not a
live region.

**(c) Intake quick-tags + dictation in the composer — screen `/app/:botId` (composer, REQ-001;
dictation path REQ-049).** Extend `ThreadComposer` with a mic `IconButton` (`RiMicLine`) and a visible
dictation-state chip, plus a row of catalogued quick-tags (vaccination, temperament, allergies) that
insert a stable token into the draft via the existing `value`/`onChange` props. Keep Enter-to-send /
Shift+Enter-newline exactly as today, and keep send disabled while an approval is pending. BoardUI
`IconButton` + `Tooltip` (`components/base/buttons/icon-button.tsx`); focus
`focus-visible:ring-2 focus-visible:ring-border-focus-ring`, 44 px `.boma-target` for wet hands, icon
`text-foreground-icon-primary`. RPC `threadsSend`; both dictation and tags enter the same `value` state,
so no new transport.

**(d) Booking/kennel takeover panel — screen `/app/:botId` (computer panel, REQ-026/028/029).**
Add a named status for each computer state (starting, running, awaiting takeover, released) and a clear
"Leaving this screen pauses nothing — the booking stays open" caption above takeover/release controls.
BoardUI `Button` + `Badge`, spinners from the installed states; surface
`bg-background-secondary-default`, text `text-text-secondary`, status text + icon never colour only.
RPCs already present: `computerStatus`, `computerTakeover`, `computerRelease`, `computerOpenScreen`.
Mirror drawer motion in RTL; keep physical screen coordinates untouched.

**(e) Rebooking / snooze shelf in the rail — screen `/app` shell left rail
(REQ-009/REQ-011/REQ-012).** Replace the unread count's bare number with a named `Chip` status
("Awaiting you", "Grooming", "Snoozed till 16:00") and expose due-time detail via a keyboard-focusable
`Tooltip`. Group the snoozed shelf by due time using the installed `date-picker`/`date-calendar` if a
picker is added. BoardUI `Chip`, `IconButton`; tokens `text-text-primary`, `text-text-secondary`,
`bg-background-tertiary-default`, `animate-number-fade` for the count per `docs/UI.md`. RPCs:
`threadsMarkRead`, `threadsMarkUnread`, `threadsSnooze`, `threadsUnsnooze`, `runsList`.

## 3. Accessibility, i18n and RTL

- New labels go in `lib/i18n.ts` (English source; French/Hebrew translations; missing key falls back
  to English, never a blank or the key). No string in JSX.
- `DirectionProvider` sets locale/direction; the boot script writes `lang`/`dir`. Use logical
  utilities (`ms`/`me`, `ps`/`pe`, `text-start`/`text-end`, `border-s`/`border-e`). Mirror the
  takeover/reassign arrows via `useDirection()`; keep appointment times, pet IDs, photo filenames and
  URLs LTR in `<bdi>`/`dir="auto"`.
- WCAG 2.2 AA gate: zero critical/serious axe findings in light, dark and RTL; 44 px targets via
  `.boma-target`; visible token-driven focus; full keyboard path (card decisions, takeover/release,
  quick-tags, snooze); status conveyed by text + icon, not colour; 360 px width;
  `prefers-reduced-motion` honoured.
- Privacy: error copy is one sentence of what happened plus one of what to do — no IDs, stack traces
  or provider text on screen (correlation id behind "details" only, spec §8.9). Owner/pet data and
  photos are never passed to a hover-only tooltip nor written to logs. The robot-cat mark stays unchanged.

## 4. Playwright tests (REQ ids where they exist)

- `REQ-001 send creates one optimistic message that is not duplicated`
- `REQ-007 pending approval card names the exact action, takes focus, and does not block reading`
- `REQ-009 unread divider moves and snooze shelf returns the booking on time`
- `REQ-012 every run status renders a distinct named chip with the right terminal action`
- `REQ-026 computer panel names starting, running, takeover and released states`
- `REQ-028 takeover and REQ-029 release are keyboard-operable and announced`
- `REQ-057 Hebrew RTL loads dir=rtl with a clean axe run`
- New behaviour above without a REQ yet (quick-tags, dictation chip, bookings strip) must be given a
  REQ id by the owner before implementation, per the traceability gate — do not invent one in test names.

## 5. Highest-impact first slice

**Change (b), the owner-update / photo approval card.** Smallest, highest-value step: one screen, no new
transport (`threadsAnswer`/`threadsReact` already exist), no new component (BoardUI `Button`/`Badge`/
`Tooltip` are installed), and it directly reduces the risk of an ambiguous owner-facing message about
an animal. Test-first: write `REQ-007 ... names the exact action` and extend
`spec/golden/ui/REQ-007-pending-card.json` first; ship only the dominant consequence sentence, the
explicit confirm label, and the text+icon high-impact marker. Defer (a), (c), (d), (e) until the owner
assigns REQ ids for the new controls and approves.
