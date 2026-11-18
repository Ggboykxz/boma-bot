# UI proposal 018 — higher education (university / college)

Status: PROPOSAL (GUESS where the spec is silent). Sector specialist #018. Read-only on product code.
Grounding: `docs/UI.md`, `spec/08-ui-ux.md`, and
`apps/web/components/{thread/thread-screen,shell/app-shell}.tsx` (read this session). RPC names are
CONFIRMED from `@/lib/api/rpc` exports and from imports in the two screens read; component paths,
tokens and i18n/RTL rules are CONFIRMED from `apps/web/components/`, `docs/UI.md` and `AGENTS.md`.
Persona, jobs and screen treatments are GUESS. No test below was run (proposed, not CONFIRMED).
Charter: Boma Bot only; no upstream/OpenBot brand, code, wording or assets; BoardUI tokens only,
no raw palette or hex, `cx()` to merge, dark mode via tokens; one design system; self-hosted only —
no hosted/SaaS tier, pricing or multi-tenant upsell surface; keep the robot-cat identity unchanged.

## 1. Persona and top jobs-to-be-done

**Persona — Dr. Nadia Okoye, a lecturer running a 300-student undergraduate course with two TAs.**
Boma Bot runs on the department's own machine: syllabus, reading list, draft announcements and
student questions never leave it. Her course panel is multilingual (English, French, Arabic) and her
course codes, dates and URLs must stay LTR. She is not a technologist; an unchecked bot post to the
whole cohort is a reputational and fairness event, so ambiguity is worse than latency. The bot is a
teaching assistant, never a substitute for her judgement.

Top jobs:
1. **Set up** a course assistant grounded in the syllabus in one short pass, without inventing copy.
2. **Curate** the reading list: keep one versioned source of truth, see what changed and who changed it.
3. **Guard** a proposed cohort-wide action (announcement, deadline change, mass email) and be certain
   of the exact effect and audience before approving — never by colour alone.
4. **Plan office hours** by seeing which topics generate the most unresolved student questions.
5. **Coordinate** the two TAs in one reading room and publish one vetted answer as a read-only page.

## 2. Five concrete changes

All copy is a new key in `lib/i18n.ts` via `useTranslation()` / `t(locale, key)`; no JSX string.
Components named below are existing BoardUI ones under `apps/web/components/`, tokens are the
semantic ones in `docs/UI.md`. Bot- or student-produced content is untrusted: render as text, never
as HTML. Class names joined with `cx()`.

**(a) Course-assistant onboarding — `/onboarding/:step?` (REQ-042).**
Relabel the existing three steps with catalogue keys: name the course space, create the "course
assistant" bot with a course code and one sentence of purpose, connect a model provider/key. The
owned robot-cat head (`assets/brand/boma-bot-head.svg`) greets on step 1 — no new asset. BoardUI
`input`, `button`, `select`, `Tooltip`; surface `bg-background-full`, text `text-text-primary`/
`text-text-secondary`, type `text-title-2-medium`/`text-body-medium`; progress is existing local
state. RPCs already available: `spacesCreate`, `botsCreate`, `modelsCatalog`, `modelsConnect`.

**(b) Reading-list shelf and versioned reader — thread right panel + `/app/artifacts/:artifactId`
(REQ-036).** In the context panel, group course materials (syllabus, reading list, past exams) by
type with name, owner bot, version and updated time; search and filter by type; reader shows the
version chain, download, "open in thread" and copy, images bounded, content as text only. BoardUI
`card`, `input`, `select`/`Chip`, `divider`, `ScrollFade`; `rounded-3xl` +
`border-border-button-default`, `bg-background-secondary-default`, icons
`text-foreground-icon-primary` from `@remixicon/react`. RPCs: `artifactsListSpace`, `artifactsGet`,
`artifactsListVersions`, `artifactsRemove`.

**(c) Announcement approval queue — `/app/:botId` pending card + Activity (REQ-007, REQ-011).**
Make the dominant line the explicit effect sentence ("Post <text> to <cohort> at <time>"), with the
acting bot and audience named, plus a distinct "Post announcement" confirm label instead of a bare
"Approve". High-impact cards carry `border-border-error-default` + `text-text-error-primary`
**plus** a leading icon and the words "Cohort-wide" — never colour alone. BoardUI `Button`, `Badge`,
`Tooltip`, `Drawer`; card `bg-background-primary-default`. RPCs: `threadsAnswer`, `threadsMessages`,
`threadsSubscribe`, `runsRetry`. The card is announced once via the polite live region; the
transcript itself is not a live region (spec §8.9).

**(d) Office-hours insight — Activity view, reached from the rail (REQ-011, REQ-012).**
Group runs by day with the prompt snippet, bot, duration and a named status chip; add a small bar
sparkline of unresolved-question volume per topic using `chart-1`…`chart-5` tokens **and** a text
label beside each bar. BoardUI `table`, `Chip`, `divider`, `Tooltip`; `border-separator-border`,
`text-text-secondary`, `text-caption-1-semibold`. RPCs: `runsList`, `botsList`, `threadsHead`.
Working runs sort to the top and update in place; no decorative spinner outlives its work.

**(e) TA reading room and vetted-answer page — `/app/g/:groupId` + `/p/:slugToken`
(REQ-010, REQ-037, REQ-058).** A group thread where only mentioned bots run, with reassign by
keyboard and an access panel listing who may approve; a published answer renders as a read-only page
with no product chrome, `noindex`/`no-referrer`, and one identical "not available" screen for any
unknown or revoked token (no wording or timing difference). BoardUI `Avatar`, `Badge`, `Button`,
`table`; tokens `bg-background-secondary-default`, `border-separator-border`,
`text-foreground-icon-secondary`, focus `border-border-focus-ring`. RPCs: `groupsCreate`,
`groupsList`, `groupsGet`, `groupsMessages`, `groupsReassign`, `adminMembersList`,
`adminGrantsList`, `adminGrantsCreate`; the page uses the existing anonymous
`GET /api/v1/pages/shared/{token}` fetch (credentials omitted).

## 3. Accessibility, i18n and RTL

- Every new label is a catalogue key (English source; French/Hebrew translations; a missing key
  falls back to English, never a blank or the key). No hardcoded student-facing string.
- `DirectionProvider` sets locale/direction; the boot script writes `lang`/`dir` on `<html>`. Use
  logical utilities (`ms`/`me`, `ps`/`pe`, `start`/`end`, `text-start`/`text-end`, `border-s`/`border-e`,
  `rounded-s`/`rounded-e`). Mirror back/reassign arrows and drawer motion via `useDirection()`; keep
  course codes, dates, emails, URLs and OTP digits LTR in `<bdi>`/`dir="auto"`.
- WCAG 2.2 AA gate: zero critical/serious axe findings in light, dark and `dir=rtl`; 44 px targets
  via `.boma-target`; visible token-driven focus (`.boma-focus`, `focus-visible:ring-border-focus-ring`);
  full keyboard path (card decision, reassign, snooze, reader version switch); status by text + icon,
  never colour; 360 px width; `prefers-reduced-motion` honoured.
- Error copy is one sentence of what happened plus one of what to do — no student name, grade,
  stack trace or provider text on screen (correlation id behind "details" only, spec §8.9). Nothing
  a lecturer must act on is hover-only; no student data in logs or another course's view.

## 4. Playwright tests (REQ ids where they exist)

- `REQ-042 course-assistant onboarding completes three steps and resumes on reload`
- `REQ-036 reading-list shelf lists versions and opens a bounded reader`
- `REQ-007 cohort-wide approval names the exact effect and audience, takes focus, and does not block reading`
- `REQ-011 activity groups runs by day for office-hour planning`
- `REQ-012 every run status renders a distinct named chip with the right terminal action`
- `REQ-010 TA group thread attributes each message and reassigns by keyboard alone`
- `REQ-037 vetted-answer page renders without chrome and shows one uniform unavailable screen`
- `REQ-057 Hebrew RTL loads dir=rtl with a clean axe run`
- New behaviour without a REQ yet (reading-list shelf, insight sparkline) must be assigned a REQ id
  by the owner before implementation, per the traceability gate — do not invent ids in test names.

## 5. Highest-impact first slice

**Change (c), the cohort-wide approval queue.** It is the smallest, highest-value step: one screen,
no new transport (`threadsAnswer`/`threadsMessages` already exist), no new component (BoardUI
`Button`/`Badge`/`Tooltip`/`Drawer` are installed), and it directly reduces the risk of an unchecked
announcement reaching 300 students. Test-first: write `REQ-007 cohort-wide approval names the exact
effect` and extend `spec/golden/ui/REQ-007-pending-card.json` first; ship only the dominant effect
sentence, the explicit "Post announcement" label and the text+icon cohort-wide marker. Defer (a),
(b), (d), (e) until owner REQ ids and ADR approvals are in place.
