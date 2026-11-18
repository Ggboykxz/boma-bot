# UI proposal 049 — social media (brand/agency account management)

Status: PROPOSAL (GUESS where the spec is silent). Sector specialist #049. Read-only on product code.
Grounding: `docs/UI.md`, `spec/08-ui-ux.md`, `spec/01-requirements.md`, `apps/web/lib/api/rpc.ts`, and
`apps/web/components/{thread/thread-composer,group/group-thread-screen}.tsx`.
Charter: Boma Bot only; no upstream/OpenBot brand, code, wording or assets; BoardUI tokens only,
`cx()` to merge, dark mode via tokens; one design system; self-hosted (no SaaS); keep the robot-cat
identity. CONFIRMED: RPC names from `rpc.ts`, REQ ids from `spec/01-requirements.md`.

## 1. Persona and top jobs-to-be-done

**Persona — Lina, social lead at a 12-person agency.** She runs eight client accounts across
Slack/Teams/Discord-class fronts (REQ-048) from one self-hosted Boma Bot instance. Drafts come from
bots; she decides what goes out. A mistaken publish is public and unrecoverable, so she needs to see
exactly which account, text and media a draft targets before a single click commits it. She works in
French, English and Arabic and is often on a phone between meetings.

Top jobs:
1. **Draft** — compose a post with media and a clear target account, without losing the draft to a
   stray Enter.
2. **Approve** — review a bot-proposed publish/reply and be certain what will happen before approving;
   never approve by colour alone.
3. **Triage** — find unread mentions, replies and reactions across accounts, snooze the non-urgent,
   and know which runs are still working.
4. **Coordinate** — run a campaign across several accounts/bots in one room and hand a reply task to
   the right bot with an attributable record.
5. **Track** — move a campaign item from idea to scheduled to published and see the bot runs attached
   to it.

## 2. Five concrete changes

All strings are new keys in `lib/i18n.ts` through `useTranslation()` → `t(locale, key)` (English
source; French/Hebrew translations; missing key falls back to English). No string in JSX. All use
installed BoardUI components and the semantic tokens in `docs/UI.md`; classes merged with `cx()`.

**(a) Post draft composer — screen `/app/:botId` (composer, REQ-001/REQ-036/REQ-048).**
Extend `ThreadComposer` with a target-account `Select`, an attachment `IconButton` (`RiAttachmentLine`)
and a named account `Chip`; keep Enter-to-send / Shift+Enter-newline and send-disabled-on-empty
exactly as today. BoardUI `TextArea`/`Button`/`IconButton`/`Select`/`Chip`/`Tooltip`; surfaces
`bg-background-primary-default`, border `border-border-button-default`, icons
`text-foreground-icon-primary`, focus `focus-visible:ring-2 focus-visible:ring-border-focus-ring`,
44 px `.boma-target`. RPCs already present: `threadsSend` (text), `artifactsGet` (media block),
`messagingList` (installed accounts). Media is bot/user-supplied and untrusted — render as text/link,
never HTML; keep tokens out of the DOM.

**(b) Publish-approval card — screen `/app/:botId` (pending card, REQ-007/REQ-017).**
Make the "what will happen" sentence dominant: name the account, the exact text, the media and the
schedule time. Give confirm an explicit label ("Publish to <account>", not "Approve"). High-impact or
destructive cards use `border-border-error-default` + `text-text-error-primary` **plus** an icon and
the word "High impact" — never colour alone. BoardUI `Button`/`Badge`/`Tooltip`; card
`bg-background-primary-default`, `rounded-3xl`, `border-border-button-default`. RPC: `threadsAnswer`.
The card is announced once through the polite live region (spec §8.9); the transcript is not a live
region. Send stays disabled while the approval is pending (`awaitingApproval`).

**(c) Mention & reaction triage in the rail — screen `/app` shell left rail
(REQ-008/REQ-009/REQ-064).** Replace the bare unread count with `Counter` + a named `Chip`
("Awaiting you", "Working", "Snoozed till 16:00") and expose reaction/mention detail through a
keyboard-focusable `Tooltip`. Group the snoozed shelf by due time using the installed `date-picker`.
BoardUI `Counter`/`Chip`/`IconButton`; tokens `text-text-primary`/`text-text-secondary`,
`bg-background-tertiary-default`, `animate-number-fade` for the count. RPCs: `botsList`, `threadsHead`,
`threadsMarkRead`, `threadsMarkUnread`, `threadsSnooze`, `threadsUnsnooze`, `threadsReact`,
`notificationsPolicyGet`/`notificationsPolicySet`.

**(d) Campaign war-room — screen `/app/g/:groupId` (group thread, REQ-010/REQ-068).**
A room where bots draft for several accounts; add a dismissal-free handover banner naming outgoing
and incoming bot with a named status chip, reusing the existing reassign row and a retry on failure.
BoardUI `Avatar`/`Button`/`Badge`; `bg-background-secondary-default`, `border-separator-border`,
`text-text-error-primary` for a failed handover plus text+icon. RPCs already present: `groupsGet`,
`groupsMessages`, `groupsReassign`, `threadsSend` (plus `runsPause`/`runsResume` if a run must pause).
Every message keeps its bot attribution so the record stays accountable.

**(e) Campaign board — screen `/app/board` (feature-flagged, REQ-038).**
Columns idea → draft → approved → scheduled → published; ticket cards show key, title, assignee bot
and last comment; a detail panel shows the bot runs attached with their status chips. BoardUI
`table`/`chip`; tokens `border-border-table`, `text-text-primary`/`text-text-secondary`. RPCs:
`boardsGet`, `ticketsCreate`, `ticketsMove`, `ticketsComment`, `runsList`. When the flag is off the
route is absent, not a locked screen (spec §8.6).

## 3. Accessibility, i18n and RTL

- New labels go in `lib/i18n.ts` (English source; French/Hebrew translations; a missing key falls back
  to English, never a blank or the raw key). `DirectionProvider` sets locale/direction; the boot
  script writes `lang`/`dir`. No string in JSX.
- Use logical utilities (`ms`/`me`, `ps`/`pe`, `text-start`/`text-end`, `border-s`/`border-e`). Mirror
  the reassign/back arrows via `useDirection()`; keep handles, URLs, schedule times and digits LTR in
  `<bdi>`/`dir="auto"`. Drawer motion reverses in RTL (spec §8.9).
- WCAG 2.2 AA gate: zero critical/serious axe findings in light, dark and RTL; 44 px targets via
  `.boma-target`; token-driven visible focus; full keyboard path (account select, approval, reassign,
  snooze, board moves); status conveyed by text + icon, not colour; 360 px width;
  `prefers-reduced-motion` honoured.
- Publishing copy is one sentence of what happened plus one of what to do; account tokens, webhooks,
  ids and stack traces never reach the screen (correlation id behind "details" only). A high-impact
  publish is never gated on hover alone. Nothing account-related is written to logs.

## 4. Playwright tests (REQ ids where they exist)

- `REQ-001 send creates one optimistic post draft that is not duplicated`
- `REQ-017 publish approval card names account, text and media and does not block reading`
- `REQ-007 pending card takes focus and announces once through the polite live region`
- `REQ-008 reply, quote and reaction render with text + icon, never colour alone`
- `REQ-009 unread divider moves and the snooze shelf returns the account on time`
- `REQ-010 group room attributes each message and reassigns by keyboard alone`
- `REQ-068 handover to another bot announces its status and retries on failure`
- `REQ-048 messaging installation states render without leaking tokens`
- `REQ-064 notification policy drives the rail's named status chips`
- `REQ-036 media in a post renders as safe text/link, never HTML`
- `REQ-057 Hebrew RTL loads dir=rtl with a clean axe run`
- New behaviour above without a REQ yet (target-account chip, campaign columns) must be given a REQ id
  by the owner before implementation, per the traceability gate — do not invent one in test names.

## 5. Highest-impact first slice

**Change (b), the publish-approval card.** It is the smallest, highest-value step: one screen, no new
transport (`threadsAnswer` already exists), no new component (BoardUI `Button`/`Badge`/`Tooltip` are
installed), and it directly removes the risk of an unintended public publish. Test-first: write
`REQ-017 publish approval card names account, text and media` and extend
`spec/golden/ui/REQ-007-pending-card.json` first; ship only the dominant consequence sentence, the
explicit "Publish to <account>" label, and the text+icon high-impact marker. Defer (a), (c), (d) and
(e) until the owner assigns REQ ids and approves.
