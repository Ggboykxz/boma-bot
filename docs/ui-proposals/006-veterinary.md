# UI proposal 006 — veterinary practice (companion animals)

Status: PROPOSAL (GUESS where the spec is silent). Sector specialist #006. Read-only on product code.
Grounding: `docs/UI.md`, `spec/08-ui-ux.md`, and
`apps/web/components/{shell/bot-rail,thread/thread-composer}.tsx`. Existing RPC names checked against
`apps/web/lib/api/rpc.ts`.
Charter: Boma Bot only; no upstream brand, code, wording or assets; BoardUI semantic tokens only —
no raw palette classes or hex literals — `cx()` to merge; one design system, no second; no SaaS
(self-hosted on the clinic's own machine); keep the owner-supplied robot-cat identity.

## 1. Persona and top jobs-to-be-done

**Persona — Dr. Lena, veterinarian in a three-vet small-animal practice.** 20-minute consults,
worn gloves, a shared workstation, and a multilingual client base (English, French, Arabic). She
runs Boma Bot self-hosted on the clinic machine — no cloud, no client or patient data leaving the
building. A wrong click on a care action reaches a client or a medical record, so an ambiguous
confirmation is worse than a slow screen.

Top jobs:
1. **Triage** — at a glance see which cases are still working, which wait on her, and which results
   are open, without hunting through the rail.
2. **Capture** — jot a consult note or dictate with unreliable, gloved hands without losing the draft
   to a stray Enter.
3. **Decide** — review a bot-proposed care plan or client message and be certain what will happen
   before approving; never confirm by colour alone.
4. **Record** — let the bot update the practice-management system on-screen, and take over cleanly
   when it needs her hands.
5. **Hand over / follow up** — pass an open case to a covering vet at shift change and chase due
   recalls and snoozed follow-ups on time.

## 2. Five concrete changes

All strings are new keys in `lib/i18n.ts` through `t(locale, key)`; no hardcoded copy. All reuse
BoardUI components under `apps/web/components/` and the semantic tokens in `docs/UI.md`.

**(a) Case-triage context strip — screen `/app/:botId` (thread header).**
A compact, read-only band above the transcript: chips for "next appointment", "open cases",
"pending results", each with a `Tooltip` that expands detail. BoardUI `Chip`/`Badge`
(`components/base/badges/chip.tsx`), `Tooltip`, `IconButton`; surface `bg-background-secondary-default`,
hairline `border-separator-border`, text `text-text-secondary`/`text-text-primary`, icons
`text-foreground-icon-secondary`, type `text-caption-1-semibold`. RPCs already present: `botsGet`,
`threadsHead`, `threadsMessages`. Bot-produced content is untrusted — render as text, never HTML;
keep patient/client identifiers out of the DOM where possible.

**(b) Care-plan / client-message approval card — screen `/app/:botId` (pending card,
REQ-007/REQ-017).** Make the "what will happen" sentence the dominant line; add a consequence
summary and an explicit confirm label ("Send client message", not just "Approve"). High-impact
cards use `border-border-error-default` + `text-text-error-primary` **plus** an icon and the words
"High impact" — never colour alone. BoardUI `Button`, `Badge`, `Tooltip`; card surface
`bg-background-primary-default`, `rounded-3xl`, `border-border-button-default`. RPCs: `threadsAnswer`,
`threadsReact`. Announced once through the polite live region (spec §8.9); the transcript is not a
live region.

**(c) Gloved composer — screen `/app/:botId` (composer, REQ-001; dictation path REQ-049).**
Extend `ThreadComposer` with a mic `IconButton` (`RiMicLine`) and a visible dictation-state chip;
keep Enter-to-send / Shift+Enter-newline exactly as today, and keep send disabled while an approval
is pending. BoardUI `IconButton` + `Tooltip` (`components/base/buttons/icon-button.tsx`); focus
`focus-visible:ring-2 focus-visible:ring-border-focus-ring`, 44 px `.boma-target` for gloved use,
icon `text-foreground-icon-primary`. RPC `threadsSend`; dictated text enters the same `value` state,
so no new transport.

**(d) Practice-record takeover panel — screen `/app/:botId` (computer panel, REQ-026/REQ-028/REQ-029).**
Add a named status for each computer state (starting, running, awaiting takeover, released) and a
clear "Leaving this screen pauses nothing — the record stays open" caption above takeover/release
controls. BoardUI `Button` + `Badge`, spinners from the installed states; surface
`bg-background-secondary-default`, text `text-text-secondary`, status text + icon never colour only.
RPCs already present: `computerStatus`, `computerTakeover`, `computerRelease`, `computerOpenScreen`.
Mirror drawer motion in RTL; keep physical screen coordinates untouched.

**(e) Recall / follow-up triage in the rail — screen `/app` shell left rail
(REQ-009/REQ-011/REQ-012).** Replace the unread count's bare number with `Counter` + a named `Chip`
status ("Awaiting you", "Working", "Snoozed till 16:00") and expose due-time detail via a
keyboard-focusable `Tooltip`. Group the snoozed shelf by due time using the installed
`date-picker`/`date-calendar` if a picker is added. BoardUI `Counter`, `Chip`, `IconButton`; tokens
`text-text-primary`, `text-text-secondary`, `bg-background-tertiary-default`, `animate-number-fade`
for the count per `docs/UI.md`. RPCs: `threadsMarkRead`, `threadsMarkUnread`, `threadsSnooze`,
`threadsUnsnooze`, `runsList`.

## 3. Accessibility, i18n and RTL

- New labels go in `lib/i18n.ts` (English source; French/Hebrew translations; missing key falls back
  to English, never a blank or the key). No string in JSX.
- `DirectionProvider` sets locale/direction; the boot script writes `lang`/`dir`. Use logical
  utilities (`ms`/`me`, `ps`/`pe`, `text-start`/`text-end`, `border-s`/`border-e`). Mirror the
  takeover/reassign arrows via `useDirection()`; keep appointment times, patient IDs and URLs LTR in
  `<bdi>`/`dir="auto"`.
- WCAG 2.2 AA gate: zero critical/serious axe findings in light, dark and RTL; 44 px targets via
  `.boma-target`; visible token-driven focus; full keyboard path (card decisions, takeover/release,
  snooze); status conveyed by text + icon, not colour; 360 px width; `prefers-reduced-motion` honoured.
- Privacy: error copy is one sentence of what happened plus one of what to do — no IDs, stack traces
  or provider text on screen (correlation id behind "details" only, spec §8.9). Client/patient data
  is never passed to a hover-only tooltip nor written to logs. The robot-cat mark stays unchanged.

## 4. Playwright tests (REQ ids where they exist)

- `REQ-001 send creates one optimistic message that is not duplicated`
- `REQ-007 pending approval card names the exact action, takes focus, and does not block reading`
- `REQ-009 unread divider moves and snooze shelf returns the case on time`
- `REQ-012 every run status renders a distinct named chip with the right terminal action`
- `REQ-026 computer panel names starting, running, takeover and released states`
- `REQ-028 takeover and REQ-029 release are keyboard-operable and announced`
- `REQ-057 Hebrew RTL loads dir=rtl with a clean axe run`
- New behaviour above without a REQ yet (dictation chip, case-triage strip) must be given a REQ id by
  the owner before implementation, per the traceability gate — do not invent one in test names.

## 5. Highest-impact first slice

**Change (b), the care-plan / client-message approval card.** Smallest, highest-value step: one
screen, no new transport (`threadsAnswer`/`threadsReact` already exist), no new component (BoardUI
`Button`/`Badge`/`Tooltip` are installed), and it directly reduces the risk of an ambiguous
client-facing decision. Test-first: write `REQ-007 ... names the exact action` and extend
`spec/golden/ui/REQ-007-pending-card.json` first; ship only the dominant consequence sentence, the
explicit confirm label, and the text+icon high-impact marker. Defer (a), (c), (d), (e) until the
owner assigns REQ ids and approves.
