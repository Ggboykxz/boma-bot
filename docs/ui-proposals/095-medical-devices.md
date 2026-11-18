# UI proposal 095 — medical devices (manufacturers)

Status: PROPOSAL (GUESS where the spec is silent). Sector specialist #095. Read-only on product code.
Grounding: `docs/UI.md`, `spec/08-ui-ux.md`, `apps/web/lib/api/rpc.ts`, and the two components
`components/thread/message-blocks.tsx` (`CardBlock`) and `components/thread/thread-composer.tsx`
(`ThreadComposer`). No upstream brand, code, wording or assets consulted; nothing copied.
Charter: Boma Bot only, robot-cat identity kept; self-hosted, not SaaS; BoardUI tokens only, no
raw palette/hex, `cx()` to merge, dark mode via tokens; one design system; no new transport.

## 1. Persona and top jobs-to-be-done

**Persona — Nadia, quality and regulatory engineer at a mid-size medical-device manufacturer.**
She owns complaint intake, CAPA, design-control evidence and device-release sign-off for monitoring
and diagnostic devices under ISO 13485 / FDA 21 CFR 820. Boma Bot runs self-hosted on the company's
own machine; no complaint text leaves the building. She is not a technologist, and a wrong click on
a release or complaint action is a compliance and patient-safety event — ambiguity is worse than latency.

Top jobs:
1. **Intake** — log a customer complaint or adverse-event report quickly, with the device, event date
   and evidence attached, so nothing is lost in a busy queue.
2. **Decide** — review a proposed CAPA action or release hold and be certain what will happen before
   approving; never approve by colour alone, and keep the decision attributable.
3. **Trace** — find the design-output / verification evidence behind a requirement and follow an
   artifact's version chain and hash without leaving the thread.
4. **Triage** — see which complaints and service tickets are waiting, paused or overdue, with a status
   she can name out loud, not a bare dot.
5. **Escalate** — route a high-impact safety signal to quality and on-call with an accountable record,
   and place a regulatory hold on a run when evidence is incomplete.

## 2. Five concrete changes

All strings are new keys in `lib/i18n.ts` via `t(locale, key)`; no hardcoded copy. All use vendored
BoardUI components and the semantic tokens in `docs/UI.md`. Where a named component is not yet
vendored, install it through the BoardUI MCP — never hand-build a lookalike.

**(a) Structured complaint intake — `/app/:botId` (composer, REQ-001/REQ-002).**
Add a "Report complaint" `IconButton` + `Tooltip` to `ThreadComposer` that fills the existing
`value` state with a catalogue-templated intake line (device, event date, description). Enter still
sends, Shift+Enter still newlines, and send stays disabled while an approval is pending. Tokens:
`text-foreground-icon-primary`, `focus-visible:ring-2 focus-visible:ring-border-focus-ring`,
44 px `.boma-target`, surfaces `bg-background-primary-default`. RPC: `threadsSend` — no new
transport; evidence files remain artifacts already reachable through `artifactsListSpace`.

**(b) Safety-sign-off approval card — `/app/:botId` (pending card, REQ-007/REQ-017).**
Extend `CardBlock`: make the "what will happen" sentence the dominant line, give the confirm button
an explicit label ("Approve CAPA closure", not just "Approve"), and mark high-impact cards with
`border-border-error-default` + `text-text-error-primary` **plus** an icon and the words "High
impact" — never colour alone. BoardUI `Button`, `Badge`, `Tooltip`; card surface
`bg-background-primary-default`, `rounded-3xl`, `border-border-button-default`. RPC: `threadsAnswer`.
The card is announced once through the polite live region (spec §8.9); the transcript is not a live region.

**(c) Traceability evidence panel — `/app/:botId` (right context panel, REQ-036).**
A read-only "Evidence" list of the thread's artifacts: name, version, truncated hash, updated time,
with a version selector and a link back to the message. BoardUI `Table`/list, `Chip`, `IconButton`,
`Tooltip`; tokens `bg-background-secondary-default`, `border-separator-border`,
`text-text-secondary`/`text-text-primary`, `text-foreground-icon-secondary`. RPCs already present:
`artifactsListSpace`, `artifactsListVersions`, `artifactsGet`. Content is untrusted — render text
only, never HTML; keep hashes and lot numbers LTR in `<bdi>`.

**(d) Quality and service triage board — `/app/board` (REQ-038, feature-flagged).**
Use the existing columns for complaint / CAPA / service tickets, with named status chips ("Backlog",
"In review", "On hold", "Closed") and a detail panel showing attached runs. BoardUI `Table`,
`Badge`/`Chip`, `Button`; tokens `text-text-primary`, `text-text-secondary`, `border-border-table`,
safe status colours only alongside text + icon. RPCs: `boardsGet`, `ticketsCreate`, `ticketsMove`,
`ticketsComment`. Moving a ticket by keyboard mirrors for RTL; when the flag is off the route is absent.

**(e) Regulatory hold and safety escalation — `/app/:botId` banner + settings panel (REQ-012/REQ-064/REQ-067).**
Add a "Regulatory hold" banner with a named status chip and pause/resume controls, plus notification
switches for waiting/completed reviews and a daily cap. BoardUI `Switch`, `Badge`, `Button`, `Tooltip`;
error tokens `text-text-error-primary`/`border-border-error-default` paired with text + icon. RPCs:
`runsList`, `runsPause`, `runsResume`, `notificationsPolicyGet`, `notificationsPolicySet`,
`notificationsDestinationsList`, `notificationsDestinationsCreate`. Status changes go through the
polite live region; no run id or correlation id is shown on the banner.

## 3. Accessibility, i18n and RTL

- New labels go in `lib/i18n.ts` (English source; French/Hebrew translations; missing key falls back
  to English, never a blank or the key). No string in JSX.
- `DirectionProvider` sets locale/direction; the boot script writes `lang`/`dir`. Use logical utilities
  (`ms`/`me`, `ps`/`pe`, `text-start`/`text-end`, `border-s`/`border-e`). Mirror the escalation and
  approve/back arrows via `useDirection()`; keep device identifiers (UDI/DI), lot numbers, hashes,
  dates, codes and URLs LTR in `<bdi>`/`dir="auto"`.
- WCAG 2.2 AA gate: zero critical/serious axe findings in light, dark and RTL; 44 px targets via
  `.boma-target`; visible token-driven focus; a full keyboard path (approval, version selector, ticket
  move, hold, switches); status conveyed by text + icon, not colour; 360 px width; `prefers-reduced-motion`.
- Regulated-record discipline: error copy is one sentence of what happened plus one of what to do —
  no ids, stack traces or provider text on screen (correlation id behind "details" only, spec §8.9).
  Every decision keeps its bot/member attribution; complaint text and PHI are never written to logs or
  exposed to a tooltip that requires hover alone.

## 4. Playwright tests (REQ ids where they exist)

- `REQ-001 complaint template sends exactly one message that is not duplicated`
- `REQ-007 safety approval card names the exact CAPA action, takes focus, and does not block reading`
- `REQ-017 answering the card unlocks the composer once and only once`
- `REQ-036 evidence panel lists versions and hashes as LTR text, never HTML`
- `REQ-038 board moves a ticket by keyboard and names every status`
- `REQ-012 every run status renders a distinct named chip with the right terminal action`
- `REQ-067 regulatory hold pauses and resumes with a named status`
- `REQ-064 notification policy toggles announce through the polite live region`
- `REQ-057 Hebrew RTL loads dir=rtl with a clean axe run`
- New behaviour above without a REQ yet (complaint template, evidence panel, hold banner) needs a REQ
  id from the owner before implementation, per the traceability gate — do not invent one in test names.

## 5. Highest-impact first slice

**Change (b), the safety-sign-off approval card.** It is the smallest, highest-value step: one screen,
no new transport (`threadsAnswer` already exists), no new component (BoardUI `Button`/`Badge`/`Tooltip`
are vendored), and it directly reduces the risk of an ambiguous release or CAPA decision. Test-first:
write `REQ-007 safety approval card names the exact CAPA action...` and extend
`spec/golden/ui/REQ-007-pending-card.json` first; ship only the dominant consequence sentence, the
explicit confirm label, and the text+icon high-impact marker. Defer (a), (c), (d), (e) until the owner
assigns REQ ids and approves.
