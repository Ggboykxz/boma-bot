# UI proposal 019 — edtech (school teaching assistant)

Status: PROPOSAL (GUESS where the spec is silent). Sector specialist #019. Read-only on product code.
Grounding: `docs/UI.md`, `spec/08-ui-ux.md`, and
`apps/web/components/{thread/thread-screen,artifacts/artifacts-grid}.tsx`.
Charter: Boma Bot only; no upstream/OpenBot brand, code, wording or assets; BoardUI semantic tokens
only (never raw palette/hex), classes merged with `cx()`; one design system; self-hosted, no SaaS/cloud;
keep the robot-cat identity in the mark and empty states.

## 1. Persona and top jobs-to-be-done

**Persona — Ms. Leila, a secondary-school teacher.** She runs a multilingual class (French, English,
Arabic) and uses Boma Bot installed on the school's own machine: lesson drafts, student work and grades
never leave the building. She is not a technologist; a grade or answer released to the wrong class, or
approved on a colour cue alone, is a real harm, so ambiguity is worse than latency.

Top jobs:
1. **Prepare** — before a lesson, see one compact band of context (class, unit, open questions) without
   hunting through the rail.
2. **Release** — review a bot-drafted grade or feedback and be certain exactly what will reach students
   before approving; never decide by colour alone.
3. **Tutor** — ask the bot a question in her own language and reuse the answer as a cited study note.
4. **Track** — across a week, find materials (plans, worksheets, quizzes), filter by type and bot, and
   see which student threads still need her.
5. **Publish and hand over** — share a compiled study guide with the class without giving students a
   login, and pass an open thread to a colleague bot with an attributable record.

## 2. Five concrete changes

All strings are new keys in `lib/i18n.ts` through `t(locale, key)`; no hardcoded copy. All use BoardUI
components under `apps/web/components/` and tokens from `docs/UI.md`.

**(a) Lesson-prep context strip — screen `/app/:botId` (thread header).**
A compact, read-only band above the transcript: chips for "class", "unit", "open questions", each with
a `Tooltip` for detail. BoardUI `Chip`/`Badge`, `Tooltip`, `IconButton` (`components/base/badges/chip.tsx`,
`components/base/buttons/icon-button.tsx`); surface `bg-background-secondary-default`, hairline
`border-separator-border`, text `text-text-secondary`/`text-text-primary`, icons
`text-foreground-icon-secondary`, type `text-caption-1-semibold`. RPCs already present: `botsGet`,
`threadsHead`, `threadsMessages`. Bot-produced content is untrusted — render as text, never HTML.

**(b) Grade-release approval card — screen `/app/:botId` (pending card, REQ-007/REQ-017).**
Make the "what will happen" sentence dominant; add an explicit consequence summary and a distinct
confirm label ("Release to Class 4B", not just "Approve"). High-impact releases use
`border-border-error-default` + `text-text-error-primary` **plus** an icon and the word "High impact"
— never colour alone. BoardUI `Button`, `Badge`, `Tooltip`; card `bg-background-primary-default`,
`rounded-3xl`, `border-border-button-default`. RPCs: `threadsAnswer`, `threadsReact`. Announce once
through the polite live region (spec §8.9); the transcript is not a live region.

**(c) Materials library — screen `/app/artifacts` (REQ-036).**
Reuse `ArtifactsGrid`: name lesson plans, worksheets and quizzes by type with a type/bot filter and
name search. Add a named type chip per card and show the version chain clearly. BoardUI `Input`,
`Select`, `Chip`; tokens `bg-background-primary-default`, `border-border-button-default`,
`text-text-primary`/`text-text-secondary`/`text-text-tertiary`, `text-foreground-icon-secondary`,
focus `focus-visible:ring-2 focus-visible:ring-border-focus-ring`. RPCs: `artifactsListSpace`,
`botsList`. Keep the robot-cat empty state when no materials exist.

**(d) Assignment board — screen `/app/board` (feature-flagged, REQ-038).**
Columns by state (to do / in progress / submitted / graded), ticket cards with key, title, assignee bot
and last comment; creating a ticket for a bot queues work visible here, not in the chat. Status is text
+ icon (`Chip` via `components/base/badges/chip.tsx`), never colour alone. Tokens
`bg-background-secondary-default`, `border-separator-border`, `text-text-primary`. RPCs: `boardsGet`,
`ticketsCreate`, `ticketsMove`, `ticketsComment`. When the flag is off the route is absent, not locked.

**(e) Shared study guide — screen `/p/:slugToken` (REQ-037).**
Publish a compiled guide for the class with no session or product chrome beyond a small footer
carrying the robot-cat mark. Unknown or revoked tokens render one identical "not available" screen,
same wording and timing; `noindex`, `no-referrer`, its own CSP. Tokens `bg-background-full`,
`text-text-primary`, `bg-background-secondary-default`; never render artifact HTML in the document.
Existing endpoint: `GET /api/v1/pages/shared/{token}` (as used by `shared-page-screen.tsx`).

## 3. Accessibility, i18n and RTL

- New labels go in `lib/i18n.ts` (English source; French/Hebrew translations; a missing key falls back
  to English, never a blank or the key). No string in JSX.
- `DirectionProvider` sets locale/direction; the boot script writes `lang`/`dir`. Use logical utilities
  (`ms`/`me`, `ps`/`pe`, `text-start`/`text-end`, `border-s`/`border-e`). Mirror the board's column and
  back arrows via `useDirection()`; keep class codes, URLs, grades and dates LTR in `<bdi>`/`dir="auto"`.
- WCAG 2.2 AA gate: zero critical/serious axe findings in light, dark and RTL; 44 px targets via
  `.boma-target`; visible token-driven focus; full keyboard path (approval, filters, board moves);
  status by text + icon, not colour; 360 px width; `prefers-reduced-motion` honoured.
- Student privacy: error copy is one sentence of what happened plus one of what to do — no student IDs,
  grades, stack traces or provider text on screen (correlation id behind "details" only, spec §8.9).

## 4. Playwright tests (REQ ids where they exist)

- `REQ-001 send creates one optimistic message that is not duplicated`
- `REQ-007 grade-release card names the exact action, takes focus, and does not block reading`
- `REQ-017 answering the card reopens the composer without duplicating the message`
- `REQ-036 materials grid filters by type and bot and shows the version chain`
- `REQ-038 assignment board moves a ticket by keyboard alone and hides when the flag is off`
- `REQ-037 shared study guide renders without chrome; invalid token shows the uniform screen`
- `REQ-009 unread divider moves after a student thread is read`
- `REQ-012 every run status renders a distinct named chip with the right terminal action`
- `REQ-057 Hebrew RTL loads dir=rtl with a clean axe run`
- New behaviour above without a REQ yet (lesson-prep strip, high-impact marker) must be given a REQ id
  by the owner before implementation, per the traceability gate — do not invent one in test names.

## 5. Highest-impact first slice

**Change (b), the grade-release approval card.** Smallest, highest-value step: one screen, no new
transport (`threadsAnswer` exists), no new component (BoardUI `Button`/`Badge`/`Tooltip` are installed),
and it directly prevents a grade or answer reaching the wrong class through an ambiguous confirmation.
Test-first: write `REQ-007 ... names the exact action` and extend
`spec/golden/ui/REQ-007-pending-card.json` first; ship only the dominant consequence sentence, the
explicit release label, and the text+icon high-impact marker. Defer (a), (c), (d), (e) until the owner
assigns REQ ids and approves.
