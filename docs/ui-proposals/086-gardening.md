# UI proposal 086 — gardening (self-hosted market-garden desk)

Status: PROPOSAL. Sector framing and the new interactions are GUESS (a sector lens, not in
`spec/`); file, component, token and RPC references are CONFIRMED (read this session). Read-only
on product code; this is the only file written. Sources: `docs/UI.md`, `spec/08-ui-ux.md`
(§8.3, §8.5, §8.6, §8.9), two components — `apps/web/components/thread/thread-composer.tsx`,
`apps/web/components/board/board-screen.tsx` — and `apps/web/lib/api/rpc.ts` (exports confirmed).
Test names aligned to `apps/web/tests/{thread-ui,board,artifacts,snooze,a11y}.spec.ts` and
`spec/golden/ui/REQ-*.json`.

Charter (CONFIRMED): Boma Bot only; fresh design (ADR-015) with the owner-supplied robot-cat as
the sole identity mark — no upstream/OpenBot brand, code, wording or assets; no borrowed garden
clip-art, foliage seal or stock photography. BoardUI semantic tokens only, never raw palette
classes or hex; classes merged with `cx()` from `@/utils/cx`; dark mode via tokens; one design
system, no second library, no hand-built lookalike. Self-hosted on the grower's own machine: no
SaaS, no plot data, photo or contact detail leaving it. This bot is a record-keeping aid — no
agronomic, pesticide-safety or weather guarantee in any string.

## 1. Persona and top jobs-to-be-done

**Persona — Ines, a market gardener running a few beds and a polytunnel.** Boma Bot runs on the
shed's own machine; the work log, plant photos and supplier details never leave it. She works
from a phone with dirty or gloved hands, in French, English and Hebrew, often in poor light, and
is not a technologist. A wrong tap on a treatment or a posted garden update is a trust and safety
incident, so an ambiguous state is worse than a slow one. The robot-cat mark stays unchanged.

Top jobs:
1. **Log care** — record water / feed / prune / harvest in one tap, with an optional photo, before
   the thought is lost and without a stray Enter sending a half-typed note.
2. **Know what is due** — see today's and this week's tasks (sow, plant, tend, harvest) and a frost
   watch at a glance, not buried in a transcript.
3. **Approve safely** — review an intervention the bot proposes (a treatment with a dose, a seed
   order, a garden post) and be certain what will happen before confirming; never on colour alone.
4. **Track growth** — reopen a plant's photo journal with its version chain and owner, showing only
   what a real RPC stores.
5. **Hand over** — pass a bed to a covering grower bot at shift change with an attributable record
   and no photo or supplier detail leaking into another view or a log.

## 2. Five concrete UI changes

All new strings are keys in `lib/i18n.ts` via `t(locale, key)` / `useTranslation()`; none in JSX.
All colours ride semantic tokens; classes merge with `cx()`. Bot-produced text is untrusted —
render as text, never HTML.

**(a) Care-log quick strip — `/app/:botId`, composer (REQ-001).** Extend `ThreadComposer` with a
compact row of Water / Feed / Prune / Harvest `IconButton`s above the real React Aria `TextField`,
reusing its `value`/`onSend` path: a tap sends one short, catalogue-authored message via the
existing `threadsSend` transport. Add a photo `IconButton` (`RiImageLine`) using the installed
`file-upload`, with the image entering the same `value` state — no new transport, no photo off the
machine. Keep Enter-to-send / Shift+Enter-newline and the `awaitingApproval` gate exactly as today;
send stays disabled while an approval is pending. BoardUI `IconButton`, `Tooltip`, `Badge`; tokens
`text-foreground-icon-primary`, `bg-background-primary-default`, `border-border-button-default`,
`rounded-full`, `focus-visible:ring-2 focus-visible:ring-border-focus-ring`, 44 px `.boma-target`.
RPC (CONFIRMED): `threadsSend`.

**(b) Intervention approval card — `/app/:botId`, pending card (REQ-007).** Make the "what will
happen" sentence dominant (the exact treatment + dose, seed order or garden post), add an explicit
consequence line and a distinct confirm label ("Apply treatment", not "Approve"). Sensitive actions
(pesticide application, publishing a plant photo) use `border-border-error-default` +
`text-text-error-primary` **plus** `RiAlertLine` and the word "Sensitive" — never colour alone.
BoardUI `Button`, `Badge`, `Tooltip`; card surface `bg-background-primary-default`, `rounded-3xl`,
`border-border-button-default`. RPCs (CONFIRMED): `threadsAnswer`, `threadsReact`. Announced once
through the polite live region (§8.9); the transcript is not a live region.

**(c) Seasonal task board — `/app/board`, feature-flagged (REQ-038).** Reuse `BoardScreen` as the
care calendar: relabel the four columns "Sow / Plant / Tend / Harvest" through i18n keys; each
ticket carries a bed/plot key, an assignee grower bot and a comment count, with the detail dialog
showing the description, comments and attached background runs. Assign a task through the existing
assignee `<select>` populated by `botsList`. No new component or column state — the board's
backlog/todo/in_progress/done still back the same moves. When the flag is off, the route is absent,
not locked (§8.6). BoardUI `Button`, `Chip` for column state; tokens `bg-background-primary-default`,
`border-border-button-default`, `text-text-secondary`, `cx()`. RPCs (CONFIRMED): `boardsGet`,
`botsList`, `ticketsCreate`, `ticketsMove`, `ticketsComment`.

**(d) Growth photo journal — `/app/artifacts`, `/app/artifacts/:artifactId` (REQ-036).** Reuse the
artifact grid and viewer as the plant photo log: cards for name, type, owner bot and updated time,
filter by type/bot, search by name; the viewer keeps the version chain (growth over time),
download and "open in thread". Images stay in a bounded frame; never render artifact HTML. BoardUI
`Table`/`Card`, `Badge`, `IconButton`, `Skeleton`; tokens `bg-background-tertiary-default`,
`border-border-table`, `text-text-secondary`, `cx()`. RPCs (CONFIRMED): `artifactsListSpace`,
`artifactsGet`, `artifactsListVersions`. No invented growth metric — show only stored versions.

**(e) Garden status rail — `/app` shell left rail (REQ-009, REQ-011, REQ-012).** Replace the bare
unread number with a named `Chip` plus "Care due", "Frost watch" and "Snoozed till …" states, due
detail behind a keyboard-focusable `Tooltip`; group the snoozed shelf by due time with the installed
`date-picker`/`date-calendar` if a picker is added. Run/status rows use the exact §8.9 vocabulary
(queued, leased, running, waiting_input, waiting_takeover, completed, failed, cancelled) as chips
with text + icon. BoardUI `Counter`, `Chip`, `IconButton`, `divider`; tokens `text-text-primary`,
`text-text-secondary`, `bg-background-secondary-hover`, `border-separator-border`, `cx()`. RPCs
(CONFIRMED): `threadsMarkRead`, `threadsMarkUnread`, `threadsSnooze`, `threadsUnsnooze`, `runsList`.

## 3. Accessibility, i18n and RTL

- Every label is a catalogue key (English source; French/Hebrew translations; a missing key falls
  back to English, never a blank or the raw key); nothing hardcoded in JSX. `DirectionProvider`
  gets an explicit BCP 47 locale and the boot script writes `lang`/`dir` together.
- Use logical utilities (`ms`/`me`, `ps`/`pe`, `text-start`/`text-end`, `border-s`/`border-e`).
  Mirror the care-row order and the disclosure arrow with `useDirection()`; keep bed codes, dates,
  doses, URLs and supplier numbers LTR in `<bdi>` / `dir="auto"`.
- WCAG 2.2 AA gate: zero critical/serious axe findings in light, dark and RTL; a keyboard path for
  every primary flow (care row, card decision, board dialog, journal, snooze); visible
  `focus-visible:ring-2 focus-visible:ring-border-focus-ring`; 44 px `.boma-target`; 360 px width;
  `prefers-reduced-motion`; no animation on streaming content. Status and "Sensitive" are text +
  icon, never colour. A card takes focus but does not block reading or other threads (§8.3).
- Error copy is one sentence of what happened plus one of what to do — no plot detail, supplier ID,
  stack trace or provider text on screen; the correlation id sits behind "details" (§8.9). No photo
  or supplier detail in a hover-only tooltip or a log.

## 4. Playwright tests (REQ id in each name)

- `test("REQ-001 a care log tap sends one optimistic message that is replaced, not duplicated", …)`.
- `test("REQ-007 a treatment card names the exact action, takes focus, and does not block reading", …)`.
- `test("REQ-038 the garden board shows care columns and the route is absent when the flag is off", …)`.
- `test("REQ-036 the photo journal shows the version chain and open-in-thread without rendering HTML", …)`.
- `test("REQ-009 an unread care divider moves and the snooze shelf returns the bed on time", …)`.
- `test("REQ-012 every run status renders a distinct named chip with the right terminal action", …)`.
- `test("REQ-057 Hebrew RTL mirrors the care row and axe reports zero critical or serious findings", …)`.
- GUESS gap: the care-log strip, the "Sensitive" treatment marker and the frost-watch chip have no
  REQ yet. The owner must assign REQ ids before implementation; never invent them in test names
  (traceability gate). Goldens stay byte-exact — a new state means a new `REQ-<id>-<slug>.json`.

## 5. Best first slice

**Change (a), the care-log quick strip — GUESS, recommended.** It is one screen with no new
transport (`threadsSend` is exported), no new component (`IconButton`/`Tooltip` are installed), no
new permission model, and it extends REQ-001, which already has a golden
(`REQ-001-send-message.json`) and `thread-ui.spec.ts`. Test-first: write the REQ-001 name above,
then add the labelled care row inside `ThreadComposer` while preserving its Enter/Shift+Enter and
`awaitingApproval` behaviour. Ship only the care row, the photo attach and their catalogue keys;
defer (b)–(e) until the owner assigns REQ ids and approves.
