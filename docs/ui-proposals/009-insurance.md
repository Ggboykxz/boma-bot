# UI proposal 009 — insurance (claims handling)

Status: PROPOSAL (GUESS where the spec is silent). Sector specialist #009. Read-only on product code.
Grounding: `docs/UI.md`, `spec/08-ui-ux.md`, and
`apps/web/components/{shell/app-shell,board/board-screen}.tsx`. Existing RPC names checked against
`apps/web/lib/api/rpc.ts`. No upstream brand, code, wording or assets consulted or copied.
Charter: Boma Bot only; no upstream brand/code/wording/assets; BoardUI semantic tokens only — no raw
palette classes or hex literals — `cx()` to merge; one design system, no second; no SaaS (self-hosted
on the insurer's own machine, no claimant data leaving the building); keep the robot-cat identity
unchanged.

## 1. Persona and top jobs-to-be-done

**Persona — Yusuf, a claims examiner at a mid-size mutual insurer.** He handles 30–60 open claims a
day, works a shared adjudication workstation, and serves a multilingual client base (English, French,
Arabic). Boma Bot runs self-hosted on the insurer's own machine: no cloud, no claimant PII leaving the
building. A claim decision is financial and regulated, so "what will happen" must be explicit and
attributable — an ambiguous confirmation is worse than a slow screen.

Top jobs:
1. **Triage** — see at a glance which claims are working, which await him, and which are due today,
   without hunting through the rail.
2. **Capture** — jot a loss note or dictate it quickly and reliably, without losing the draft to a
   stray Enter.
3. **Decide** — review a bot-proposed claim action (reserve change, payout, claimant message) and be
   certain of the consequence before approving; never confirm by colour alone.
4. **Record** — let the bot work the policy/claims system on-screen, and take over cleanly when it
   needs his hands, with an accountable audit trail.
5. **Move / hand over** — advance a claim through the pipeline and pass it to a covering examiner with
   a clear, attributable record.

## 2. Five concrete changes

All strings are new keys in `lib/i18n.ts` through `t(locale, key)`; no hardcoded copy. All reuse
BoardUI components under `apps/web/components/` and the semantic tokens in `docs/UI.md`; classes
merged with `cx()`.

**(a) Claim-triage context strip — screen `/app/:botId` (thread header).**
A compact, read-only band above the transcript: chips for "open claims", "awaiting you", "due today",
each with a `Tooltip` that expands detail. BoardUI `Chip`/`Badge` (`components/base/badges/chip.tsx`),
`Tooltip`, `IconButton`; surface `bg-background-secondary-default`, hairline `border-separator-border`,
text `text-text-secondary`/`text-text-primary`, icons `text-foreground-icon-secondary`, type
`text-caption-1-semibold`. RPCs already present: `botsGet`, `threadsHead`, `threadsMessages`.
Bot-produced content is untrusted — render as text, never HTML; keep claimant identifiers out of the
DOM where possible.

**(b) Claim-decision approval card — screen `/app/:botId` (pending card, REQ-007/REQ-017).**
Make the "what will happen" sentence the dominant line; add an explicit consequence summary and a
distinct confirm label ("Approve payout", not just "Approve"), plus who is authorising it. High-impact
cards use `border-border-error-default` + `text-text-error-primary` **plus** an icon and the words
"High impact" — never colour alone. BoardUI `Button`, `Badge`, `Tooltip`; card surface
`bg-background-primary-default`, `rounded-3xl`, `border-border-button-default`. RPCs: `threadsAnswer`,
`threadsReact`. Announced once through the polite live region (spec §8.9); the transcript is not a
live region.

**(c) Loss-note composer — screen `/app/:botId` (composer, REQ-001; dictation path REQ-049).**
Extend `ThreadComposer` with a mic `IconButton` (`RiMicLine`) and a visible dictation-state chip; keep
Enter-to-send / Shift+Enter-newline exactly as today, and keep send disabled while an approval is
pending. BoardUI `IconButton` + `Tooltip` (`components/base/buttons/icon-button.tsx`); focus
`focus-visible:ring-2 focus-visible:ring-border-focus-ring`, 44 px `.boma-target`, icon
`text-foreground-icon-primary`. RPC `threadsSend`; dictated text enters the same `value` state, so no
new transport. Claim numbers and dates stay LTR in `<bdi>`.

**(d) Claims-system takeover panel — screen `/app/:botId` (computer panel, REQ-026/REQ-028/REQ-029).**
Add a named status for each computer state (starting, running, awaiting takeover, released) and a
clear "Leaving this screen pauses nothing — the record stays open" caption above takeover/release
controls. BoardUI `Button` + `Badge` and the installed spinner/skeleton states; surface
`bg-background-secondary-default`, text `text-text-secondary`, status as text + icon, never colour
only. RPCs: `computerStatus`, `computerTakeover`, `computerRelease`, `computerOpenScreen`. Mirror
drawer motion in RTL; keep physical screen coordinates untouched; no claimant PII in logs.

**(e) Claims pipeline on the board — screen `/app/board` (REQ-038).**
Retitle columns to claim states (intake, in review, awaiting documents, settled) using the existing
flag-gated board; each card shows claim key, title, assigned bot/examiner and last comment. Reuse the
installed `table`/`chip`, `Button`, `input`, `select`, and `Badge`; surfaces `bg-background-primary-default`
and `bg-background-secondary-default`, hairline `border-separator-border`, status text + icon.
RPCs already present: `boardsGet`, `ticketsCreate`, `ticketsMove`, `ticketsComment`. Working (ticket)
runs appear in Activity, not in the transcript; when the flag is off the route is absent, not locked.

## 3. Accessibility, i18n and RTL

- New labels go in `lib/i18n.ts` (English source; French/Hebrew translations; missing key falls back
  to English, never a blank or the key). No string in JSX.
- `DirectionProvider` sets locale/direction; the boot script writes `lang`/`dir`. Use logical utilities
  (`ms`/`me`, `ps`/`pe`, `text-start`/`text-end`, `border-s`/`border-e`). Mirror takeover and pipeline
  arrows via `useDirection()`; keep claim numbers, dates, currency and URLs LTR in `<bdi>`/`dir="auto"`.
- WCAG 2.2 AA gate: zero critical/serious axe findings in light, dark and RTL; 44 px targets via
  `.boma-target`; visible token-driven focus; full keyboard path (card decisions, takeover/release,
  ticket move, dictate); status conveyed by text + icon, not colour (spec §8.9 run-status vocabulary);
  360 px width; `prefers-reduced-motion` honoured; no animation on streaming content.
- Privacy: error copy is one sentence of what happened plus one of what to do — no claimant IDs, stack
  traces or provider text on screen (correlation id behind "details" only). Claimant data is never
  passed to a hover-only tooltip nor written to logs. The robot-cat mark stays unchanged.

## 4. Playwright tests (REQ ids where they exist)

- `REQ-001 send creates one optimistic message that is not duplicated`
- `REQ-007 pending approval card names the exact action and consequence, takes focus, and does not block reading`
- `REQ-009 unread divider moves and the snooze shelf returns the claim on time`
- `REQ-012 every run status renders a distinct named chip with the right terminal action`
- `REQ-026 computer panel names starting, running, takeover and released states`
- `REQ-028 takeover and REQ-029 release are keyboard-operable and announced`
- `REQ-038 board shows claim-state columns and a keyboard-only ticket move`
- `REQ-057 Hebrew RTL loads dir=rtl with a clean axe run`
- New behaviour without a REQ yet (dictation chip, claim-triage strip) must be assigned a REQ id by
  the owner before implementation, per the traceability gate — do not invent one in test names.

## 5. Highest-impact first slice

**Change (b), the claim-decision approval card.** Smallest, highest-value step: one screen, no new
transport (`threadsAnswer`/`threadsReact` already exist), no new component (BoardUI `Button`/`Badge`/
`Tooltip` are installed), and it directly reduces the risk of an ambiguous, regulated financial
decision. Test-first: write `REQ-007 ... names the exact action and consequence` and extend
`spec/golden/ui/REQ-007-pending-card.json` first; ship only the dominant consequence sentence, the
explicit confirm label, the authorisation line, and the text+icon high-impact marker. Defer (a), (c),
(d), (e) until the owner assigns REQ ids and approves.
