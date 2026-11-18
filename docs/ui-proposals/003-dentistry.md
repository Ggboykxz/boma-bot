# UI proposal 003 — dental practices

Status: PROPOSAL (GUESS where the spec is silent; component, token and RPC names CONFIRMED by
reading the files below). Sector specialist #003. Strictly read-only on product code.
Grounding: `docs/UI.md`, `docs/IMPROVEMENTS.md`, `spec/08-ui-ux.md`, and
`apps/web/components/{shell/app-shell,board/board-screen,activity/activity-screen,thread/thread-composer}.tsx`,
plus `apps/web/lib/api/rpc.ts` and the test names under `apps/web/tests/`.
Charter: Boma Bot only; no upstream/OpenBot brand, code, wording or assets; BoardUI tokens only,
no raw palette or hex, `cx()` to merge, dark mode via tokens; one design system; self-hosted only,
no hosted/SaaS service; keep the robot-cat identity.

## 1. Persona and top jobs-to-be-done

**Persona — Dr. Luis, a dentist in a two-chair practice.** He shares a workstation with a hygienist
and a front-desk assistant, moves between chairs every 20–30 minutes, and wears gloves or holds
instruments between entries. The assistant handles recalls, lab cases and insurance follow-ups;
Luis owns treatment-plan and prescription decisions. Boma Bot runs on the practice's own machine —
patient information never leaves the building, and no cloud or hosted service is involved. He is
not a technologist: an ambiguous confirmation is worse than a slow screen.

Top jobs:
1. **Prepare** — at chairside, see the open plan, pending lab case and unanswered patient question
   in one place instead of hunting through the rail.
2. **Decide safely** — accept a treatment plan or ok a prescription only after reading exactly what
   the bot will do; never infer impact from a colour.
3. **Track cases** — follow a crown/impression from "sent to lab" to "back from lab" and know which
   chair it blocks, without reading a log.
4. **Recall** — triage hygiene/recall replies, defer the ones that are not due yet, and never lose a
   patient who answered after hours.
5. **Hand over** — pass an open chart from hygienist to dentist (and back) with an attributable
   record, and capture a note with hands that are not free for a keyboard.

## 2. Five concrete UI/UX changes

All copy is a new key in `lib/i18n.ts` via `t()`; no hardcoded string in JSX. Components named below
are the installed BoardUI ones under `apps/web/components/`; tokens are the semantic ones mapped in
`docs/UI.md`. Patient identifiers are bot-produced untrusted text: render as text, never HTML.

**(a) Treatment-plan approval card — `/app/:botId` (pending card, REQ-007/REQ-017).**
Make the "what will happen" sentence dominant: "Send this treatment plan and estimate to <patient>".
Give the confirm button a named label ("Accept plan", not "Approve"). Prescription and
high-value-plan cards add a text+icon "High impact" marker using `text-text-error-primary` and
`border-border-error-default` — never colour alone. Build with `Button` and `Chip` from
`components/base/badges/chip.tsx` and `Tooltip`; surface `bg-background-primary-default`,
`rounded-3xl`, `border-border-button-default`; type `text-headline-medium` / `text-body-medium`.
RPCs already exported: `threadsAnswer`, `threadsReact`. The card takes focus once and is announced
through the polite live region; the transcript stays a plain `role="log"`, not a live region.

**(b) Lab-case and chair-turnover board — `/app/board` (REQ-038).**
Re-purpose the existing four columns for a dental practice: `backlog` = "Lab cases to send",
`todo` = "Due today", `in_progress` = "In chair", `done` = "Returned / seated". A ticket card
names the case, the patient identifier (isolated in `<bdi>`), the assignee bot and the blocking
chair. Reuse
BoardUI `Button`, `Chip` and `Tooltip`; tokens `bg-background-secondary-default` for column
surfaces, `border-separator-border` hairlines, `text-text-tertiary` for keys, merged with `cx()`.
RPCs: `boardsGet`, `botsList`, `ticketsCreate`, `ticketsMove`, `ticketsComment`. Flag-gated as today:
a 404 keeps the absent route, not a locked screen.

**(c) Recall triage in the left rail — `/app`, `/app/:botId` (REQ-009/REQ-011/REQ-012).**
Replace the bare unread number with a `Counter` plus a named `Chip` ("Awaiting you", "Working",
"Snoozed till 16:00"); due-time detail appears in a keyboard-focusable `Tooltip`. Group the snoozed
shelf by due date with the installed `date-picker`/`date-calendar` if a picker is added. Components:
`Counter`, `Chip`, `IconButton`; tokens `text-text-primary`, `text-text-secondary`,
`bg-background-tertiary-default`, focus `border-border-focus-ring`. RPCs: `threadsMarkRead`,
`threadsMarkUnread`, `threadsSnooze`, `threadsUnsnooze`, `runsList`.

**(d) Imaging and lab-scan viewer — `/app/artifacts/:artifactId` (REQ-036).**
Radiographs and lab scans are artifacts: show a bounded frame (never wider than the panel), the
version chain, download and "open in thread". Filter the grid by type (`Claim`, `Radiograph`,
`Lab scan`) through the existing search/filter controls. Components `Button`, `Chip`, `Tooltip`;
tokens `bg-background-secondary-default`, `border-separator-border`, `text-text-secondary`;
composite `text-caption-1-semibold`. RPCs: `artifactsListSpace`, `artifactsGet`,
`artifactsListVersions`. Content stays untrusted — no artifact HTML in the app document.

**(e) Chairside composer — `/app/:botId` (REQ-001; dictation path REQ-049).**
Extend `ThreadComposer` with a mic `IconButton` (`RiMicLine`) and a visible state `Chip`. Keep
Enter-to-send, Shift+Enter-newline, and send disabled while an approval is pending. Components:
`IconButton` from `components/base/buttons/icon-button.tsx`, `Tooltip`, `Chip`; tokens
`text-foreground-icon-primary`, `focus-visible:ring-border-focus-ring`, 44 px `.boma-target`.
RPC: `threadsSend` — dictated text enters the same `value` state, so no new transport. A shipped
dictation control is new behaviour: the owner must assign a REQ id before implementation.

## 3. Accessibility, i18n and RTL

- Every new label is a key in `lib/i18n.ts` (English source, French/Hebrew translations; missing key
  falls back to English, never blank or the raw key). No string in JSX.
- `DirectionProvider` sets locale and direction; the boot script writes matching `lang`/`dir`. Use
  logical utilities (`ms`/`me`, `ps`/`pe`, `text-start`/`text-end`, `border-s`/`border-e`); mirror the
  handover arrow with `useDirection()`. Keep patient IDs, dates and URLs LTR in `<bdi>`/`dir="auto"`.
- WCAG 2.2 AA gate: zero critical/serious axe findings in light, dark and RTL; 44 px targets;
  token-driven visible focus; full keyboard path (card decision, ticket move, snooze, mic);
  status named in text + icon, not colour; 360 px width; `prefers-reduced-motion` honoured.
- Clinical privacy: error copy is one sentence of what happened plus one of what to do — no chart
  IDs, stack traces or provider text on screen (correlation id behind "details" only, spec §8.9).
  Nothing PHI-like is conveyed by hover alone; self-hosted only, no hosted recall or messaging SaaS.

## 4. Playwright tests (REQ ids where they exist)

- `REQ-038 the board shows lab-case columns and moves a case by keyboard alone`
- `REQ-038 the board renders the not-found shape when the flag is off`
- `REQ-007 the treatment-plan approval names the exact consequence, takes focus, and does not block reading`
- `REQ-009 the recall shelf shows a named status and snoozes until the chosen date`
- `REQ-012 each run status renders a distinct named chip with the right terminal action`
- `REQ-036 the imaging viewer bounds the frame and exposes the version chain`
- `REQ-001 chairside send creates one optimistic message; Enter sends and Shift+Enter inserts a newline`
- `REQ-057 Hebrew RTL and dark mode keep the dental labels legible with a clean axe run`
- New behaviour without a REQ (mic chip, dental column labels) is not named in a test until the owner
  assigns an id — traceability counts declaration lines, not comments.

## 5. Highest-impact first slice

**Change (b), the lab-case / chair-turnover board.** It is the smallest safe step: one screen,
already flag-gated, no new transport (`boardsGet`, `botsList`, `ticketsCreate`, `ticketsMove`,
`ticketsComment` exist), no new component, and no REQ id to invent — REQ-038 already has tests and
the absent-route golden. Test-first: extend `REQ-038 ... moves a case by keyboard alone`, then add
the dental column labels and case card metadata using only `Chip`/`Tooltip` and semantic tokens via
`cx()`. Ship without the approval card, rail, viewer or composer changes; take those up only after
the owner approves their REQ ids and this slice stays green in light, dark and RTL.
