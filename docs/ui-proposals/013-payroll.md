# UI proposal 013 — payroll (payroll operations)

Status: PROPOSAL (GUESS where the spec is silent). Sector specialist #013. Read-only on product code.
Grounding: `docs/UI.md`, `spec/08-ui-ux.md`, and
`apps/web/components/{shell/bot-rail,thread/thread-composer}.tsx`. RPC names CONFIRMED against the
exports in `apps/web/lib/api/rpc.ts`; the sector mapping is GUESS (no payroll REQ exists in the spec).
Charter: Boma Bot only; no upstream/OpenBot brand, code, wording or assets; BoardUI tokens only,
merged with `cx()`, dark mode via tokens; one design system; self-hosted (no SaaS); keep the
robot-cat identity.

## 1. Persona and top jobs-to-be-done

**Persona — Nadia, payroll operations lead at a 600-employee manufacturer.** Runs a fortnightly cycle
against a fixed statutory deadline, supports a multilingual workforce (French, English, Arabic), and
reconciles a legacy time-and-attendance system with spreadsheets. Boma Bot is self-hosted on the
company's own machine: salary, bank and tax data never leave the premises. She is not a technologist;
a wrong approval is a pay dispute or a compliance breach, so ambiguity and hidden state are worse
than latency.

Top jobs:
1. **Prepare** — see on one screen which pay period is open, what is due and when, and which runs
   are still working.
2. **Verify** — confirm a proposed pay run or exception fix, knowing exactly what will change and
   for whom, before approving.
3. **Triage** — across a busy cycle, find paused exceptions and unread items, snooze non-urgent ones,
   and tell working runs from stalled ones.
4. **Protect** — keep salary, bank and tax identifiers out of logs, tooltips and other members' views;
   confirm the recipient before distributing payslips.
5. **Distribute & audit** — publish payslips to the right employees and leave an attributable record,
   including when a legacy system needs a supervised takeover.

## 2. Five concrete changes

All strings are new keys in `lib/i18n.ts` through `t(locale, key)` (English source; French/Hebrew
translations; a missing key falls back to English, never a blank or the key). All use BoardUI
components under `apps/web/components/` and the semantic tokens in `docs/UI.md`; classes merged with
`cx()` — no raw palette, hex or second design system.

**(a) Pay-period context strip — screen `/app/:botId` (thread header).**
A compact, read-only band above the transcript: chips for "period open", "statutory deadline" and
"runs working", each with a `Tooltip` carrying the detail. BoardUI `Badge`/`Chip`
(`components/base/badges/chip.tsx`), `Tooltip`, `IconButton`; surfaces `bg-background-secondary-default`,
hairline `border-separator-border`, text `text-text-secondary`/`text-text-primary`, icons
`text-foreground-icon-secondary`, type `text-caption-1-semibold`. RPCs already present: `botsGet`,
`threadsHead`, `runsList`. Content is bot-produced and untrusted — render as text, never HTML; keep
employee identifiers out of the DOM where possible.

**(b) Pay-run approval card — screen `/app/:botId` (pending card, REQ-007/REQ-017).**
Make the "what will happen" line dominant: name the pay period, headcount and gross total, with an
explicit confirm label ("Approve run 2026-11", not just "Approve"). High-impact cards use
`border-border-error-default` + `text-text-error-primary` **plus** an icon and the words
"High impact" — never colour alone. A failed run offers a named retry. BoardUI `Button`, `Badge`,
`Tooltip`; card surface `bg-background-primary-default`, `rounded-3xl`, `border-border-button-default`.
RPCs: `threadsAnswer`, `threadsReact`, `runsRetry`. The card is announced once through the polite live
region (spec §8.9); the transcript is not a live region.

**(c) Confidential-data composer and reply banner — screen `/app/:botId` (composer, REQ-001).**
Extend `ThreadComposer` with a "contains payroll data" `Switch` and a redaction hint; keep
Enter-to-send / Shift+Enter-newline and keep send disabled while an approval is pending. Salary,
IBAN/BIC, tax IDs and employee numbers stay LTR in `<bdi>`/`dir="auto"` inside the reply chip, never
in a hover-only tooltip. BoardUI `Switch`, `IconButton`, `Tooltip` from the installed base set; focus
`focus-visible:ring-2 focus-visible:ring-border-focus-ring`, 44 px `.boma-target`, icons
`text-foreground-icon-primary`. RPCs: `threadsSend`, `threadsReact` — text enters the existing
`value` state, so no new transport.

**(d) Exception triage in the rail — screen `/app` shell left rail (REQ-009/REQ-011/REQ-012).**
Replace the bare unread number with a `Counter` plus a named `Chip` status ("Awaiting you", "Working",
"Snoozed till 16:00"), with due-time detail on a keyboard-focusable `Tooltip`. Group the snoozed shelf
by due time via the installed `date-picker`/`date-calendar`. BoardUI `Counter`, `Chip`, `IconButton`;
tokens `text-text-primary`, `text-text-secondary`, `bg-background-tertiary-default`,
`animate-number-fade` per `docs/UI.md`. RPCs: `threadsMarkRead`, `threadsMarkUnread`, `threadsSnooze`,
`threadsUnsnooze`, `runsList`.

**(e) Payslip distribution and supervised takeover — screen `/app/:botId` right panel
(REQ-026/REQ-028/REQ-029).**
A distribution panel lists generated payslip artifacts newest first, each with a named status and a
"confirm recipients" gate; for the legacy system, raise the computer panel with takeover/release
controls. BoardUI `drawer`, `spinner`, `skeleton`, `table`, `Button`; surfaces
`bg-background-primary-default`/`bg-background-secondary-default`, hairline `border-separator-border`,
status `text-text-error-primary` plus text+icon. RPCs: `artifactsListSpace`, `artifactsGet`,
`computerStatus`, `computerTakeover`, `computerRelease`, `computerOpenScreen`. Takeover is an
explicit, attributable action; artifact content is untrusted — never render its HTML in the document.

## 3. Accessibility, i18n and RTL

- New labels go in `lib/i18n.ts` (English source; French/Hebrew translations; a missing key falls back
  to English, never blank or key). No string in JSX.
- `DirectionProvider` (explicit BCP 47 locale) sets direction; the boot script writes `lang`/`dir`.
  Use logical utilities (`ms`/`me`, `ps`/`pe`, `text-start`/`text-end`, `border-s`/`border-e`) and
  `useDirection()` to mirror arrows and drawer motion in RTL. Employee IDs, IBANs, tax IDs and URLs
  stay LTR in `<bdi>`/`dir="auto"`.
- WCAG 2.2 AA gate: zero critical/serious axe findings in light, dark and RTL; 44 px targets via
  `.boma-target`; visible token-driven focus; a full keyboard path (approval, snooze, takeover,
  recipient gate); status by text + icon, not colour; 360 px width; `prefers-reduced-motion` honoured.
- Payroll privacy: error copy is one sentence of what happened plus one of what to do — no employee
  IDs, amounts, stack traces or provider text on screen (correlation id behind "details" only,
  spec §8.9). Nothing confidential lives in a hover-only tooltip; no salary data is written to logs.

## 4. Playwright tests (REQ ids where they exist)

- `REQ-001 send creates one optimistic message that is not duplicated`
- `REQ-007 pending approval card names the pay period, takes focus, and does not block reading`
- `REQ-009 unread divider moves and the snooze shelf returns the bot on time`
- `REQ-012 every run status renders a distinct named chip with the right terminal action`
- `REQ-026 computer panel moves from provisioning to takeover with named states`
- `REQ-028 takeover and release announce their status through the polite live region`
- `REQ-057 Hebrew RTL loads dir=rtl with a clean axe run`
- New behaviour without a REQ yet (pay-period strip, payroll-data switch, recipient gate) must be
  given a REQ id by the owner before implementation — do not invent one in test names per the
  traceability gate.

## 5. Highest-impact first slice

**Change (b), the pay-run approval card.** It is the smallest, highest-value step: one screen, no new
transport (`threadsAnswer`/`runsRetry` already exist), no new component (BoardUI `Button`/`Badge`/
`Tooltip` are installed), and it directly reduces the risk of an ambiguous, irreversible pay decision.
Test-first: write `REQ-007 ... names the pay period` and extend
`spec/golden/ui/REQ-007-pending-card.json` first; ship only the dominant consequence sentence, the
explicit confirm label and the text+icon high-impact marker. Defer (a), (c), (d), (e) until the owner
assigns REQ ids and approves.
