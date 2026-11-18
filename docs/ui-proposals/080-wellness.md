# UI proposal 080 — wellness (coaching studio)

Status: PROPOSAL. The sector framing and the new interactions are GUESS (a sector lens, not in
`spec/`); file, component, token and RPC references are CONFIRMED (read this session). Read-only
on product code; this is the only file written. Sources: `docs/UI.md`, `spec/08-ui-ux.md`
(§8.3, §8.5, §8.9), two components — `apps/web/components/thread/thread-composer.tsx`,
`apps/web/components/thread/message-blocks.tsx` — `apps/web/lib/api/rpc.ts` (exports confirmed),
`apps/web/tests/{thread-ui,a11y,snooze,artifacts}.spec.ts`, `spec/golden/ui/REQ-001-send-message.json`.

Charter (CONFIRMED): Boma Bot only; no upstream brand, code, wording or assets; fresh design
(ADR-015) with the owner-supplied robot-cat as the sole identity mark — no borrowed health seal,
clinic logo or stock wellness art. BoardUI semantic tokens only, never raw palette classes or hex;
classes merged with `cx()`; one design system, no second library, no hand-built lookalike.
Self-hosted on the studio's own machine: no SaaS, no client record leaving the building. A wellness
bot is a coaching aid — no diagnosis, dosage or medical claim in any string.

## 1. Persona and top jobs-to-be-done

**Persona — Maya, a wellness coach in a small studio.** She runs Boma Bot on the studio's own
machine: a private bot for session prep and a client-facing bot for daily habit check-ins and
between-session support. Clients write in French, English, Arabic and Hebrew, mostly from a phone.
Check-in logs are personal data, not marketing: nothing may reach a third party or a log. Maya is
not a technologist and is not a clinician; an ambiguous safety cue or an exposed client note is a
harm, not a nuisance. The robot-cat mark stays unchanged.

Top jobs:
1. **Check in** — collect a daily habit/movement/mood check-in in one tap, no long form, no lost draft.
2. **Keep the boundary** — make plain the bot is coaching support, not medical advice or a diagnosis.
3. **Protect client data** — control what is stored, exported or shown on a shared device.
4. **Follow up** — bring a missed check-in back at the right time, not bury it in a transcript.
5. **See progress** — reopen a private log with its versions and owner, without inventing metrics.

## 2. Five concrete UI changes

All new strings are keys in `lib/i18n.ts` via `t(locale, key)` / `useTranslation()`; none in JSX.
All colours ride semantic tokens; classes merge with `cx()` from `@/utils/cx`.

**(a) Habit check-in strip — `/app/:botId`, composer (REQ-001).** Extend `ThreadComposer` with a
compact row of mood/habit `IconButton`s above the real React Aria `TextField`, reusing its
`value`/`onSend` path: a choice sends one short, catalogue-authored message. Each button has a
visible text label and an `aria-label` from the catalogue. Keep Enter-to-send / Shift+Enter-newline
and the `awaitingApproval` gate exactly as today. BoardUI `IconButton`, `Tooltip`, `Badge`; tokens
`text-foreground-icon-primary`, `bg-background-primary-default`, `border-border-button-default`,
`rounded-full`, `focus-visible:ring-2 focus-visible:ring-border-focus-ring`, `cx()`. RPC (CONFIRMED):
`threadsSend`. GUESS gap: there is no dedicated check-in RPC — do not invent one.

**(b) Scope and safety boundary — `/app/:botId`, header band and card.** A read-only band above the
transcript: a named chip "Wellness coaching — not medical advice", a "Today's focus" chip and a
"How data is used" chip, each with a `Tooltip` and a plain sentence. If a client signals distress,
reuse `CardBlock` from `message-blocks.tsx`: a "Talk to a person" hand-off, urgency shown by
`border-border-error-default` + `text-text-error-primary` **plus** `RiAlertLine` and the word
"Urgent" — never colour alone. It scrolls into view and takes focus without blocking reading or
other threads (spec §8.3); resources are locally configured copy, no third-party call. BoardUI
`Badge`/`Chip`, `Tooltip`, `IconButton`; tokens `bg-background-secondary-default`,
`border-separator-border`, `text-text-secondary`, `text-caption-1-semibold`, `cx()`. RPCs
(CONFIRMED): `botsGet`, `threadsHead`, `threadsMessages`, `threadsAnswer`. Bot text is untrusted —
render as text, never HTML; the robot-cat is the only identity art.

**(c) Private log library — `/app/artifacts`, `/app/artifacts/:artifactId` (REQ-036).** Reuse the
artifact grid and viewer as the progress log: cards for name, type, owner bot and updated time,
filter by type/bot, search by name; the viewer keeps the version chain, download and "open in
thread". No invented chart — show only what an RPC actually stores. BoardUI `Table`/`Card`,
`Badge`, `IconButton`, `Skeleton`; tokens `bg-background-tertiary-default`, `border-border-table`,
`text-text-secondary`, `cx()`. RPCs (CONFIRMED): `artifactsListSpace`, `artifactsGet`,
`artifactsListVersions`. Never render artifact HTML; images stay in a bounded frame.

**(d) Consent and data controls — `/app/:botId`, header overflow menu.** Use the installed
`Dropdown` + `Switch` + `Button` for "Private check-ins" (hide previews on this device), "Export
this log" and a sentence on what is stored locally; role-gated like other header actions so a
client-facing bot cannot expose coach views. Destructive rows use `text-text-error-primary` **plus**
an icon and the word, never red alone. Tokens `bg-background-secondary-default`,
`border-separator-border`, `text-text-secondary`, `cx()`. RPCs available today (CONFIRMED):
`botsGet`, `threadsMarkRead`, `threadsMarkUnread`, `threadsSnooze`, `threadsUnsnooze`. GUESS gap:
no real export/delete procedure exists — do not fake one with snooze; a destructive control needs
a REQ id first.

**(e) Follow-up shelf — `/app/:botId` rail and thread (REQ-009, REQ-012).** A missed check-in stays
visible and on time: unread divider, "jump to latest" and the snooze shelf; each run/status uses a
named chip with text + icon from the exact §8.9 vocabulary (queued, leased, running,
waiting_input, waiting_takeover, completed, failed, cancelled). BoardUI `divider`, `Chip`, `Badge`,
`Button`; tokens `border-separator-border`, `text-text-tertiary`, `bg-background-secondary-hover`,
`cx()`. RPCs (CONFIRMED): `threadsSnooze`, `threadsUnsnooze`, `threadsMarkRead`, `threadsMarkUnread`,
`runsList`. Announce a card arrival or a failed hand-off once via the polite live region.

## 3. Accessibility

WCAG 2.2 AA gate: zero critical or serious axe findings in light, dark and RTL; a keyboard path for
every primary flow (check-in row, card decision, menu controls, viewer); visible
`focus-visible:ring-2 focus-visible:ring-border-focus-ring`; 44 px `.boma-target`; 360 px width;
`prefers-reduced-motion`; no animation on streaming content. Status is text + icon, never colour
alone. A safety card takes focus but does not block reading or other threads. The transcript is
not a live region; status changes and errors are announced once. Error copy is one sentence of what
happened plus one of what to do — no client name, id, stack trace or provider text on screen; the
correlation id sits behind "details". No client-related value goes into a hover-only tooltip or a log.

## 4. i18n and RTL

English is the source catalogue; French, Arabic and Hebrew are translations; a missing key falls
back to English, never a blank or the raw key. `DirectionProvider` gets an explicit BCP 47 locale
and the boot script writes `lang`/`dir` together. Use logical utilities (`ms`/`me`, `ps`/`pe`,
`text-start`/`text-end`, `border-s`/`border-e`) throughout. Mirror the check-in row and the
disclosure arrow with `useDirection()`; keep hotline numbers, URLs, dates and OTP digits LTR in
`<bdi>`/`dir="auto"`. All boundaries and disclaimers are translated keys, not inline wording.

## 5. Playwright tests (REQ id in each name)

- `test("REQ-001 a habit check-in sends one optimistic message that is replaced, not duplicated", ...)`.
- `test("REQ-007 a safety card names the hand-off, takes focus, and does not block reading", ...)`.
- `test("REQ-012 every run status renders a distinct named chip with the right terminal action", ...)`.
- `test("REQ-009 a snoozed follow-up stays on the shelf and a missed check-in returns on time", ...)`.
- `test("REQ-036 the log viewer shows the version chain and open-in-thread without rendering HTML", ...)`.
- `test("REQ-057 Hebrew RTL mirrors the check-in row and axe reports zero critical or serious findings", ...)`.
- GUESS gap: the boundary band, the check-in row as a new interaction and the consent controls have
  no REQ yet. The owner must assign REQ ids before implementation; never invent them in test names
  (traceability gate). Goldens stay byte-exact — a new state means a new `REQ-<id>-<slug>.json`.

## 6. Best first slice

Change (a), the habit check-in strip — GUESS, recommended. It is one screen with no new transport
(`threadsSend` is exported), no new component (`IconButton`/`Tooltip` are installed) and no new
permission model, and it extends REQ-001, which already has a golden
(`REQ-001-send-message.json`) and `thread-ui.spec.ts`. Test-first: write the REQ-001 name above,
then add the labelled icon row inside `ThreadComposer` while preserving its Enter/Shift+Enter and
`awaitingApproval` behaviour. Ship only the check-in row and its catalogue keys; defer (b)–(e)
until the owner assigns REQ ids, confirms locally configured support resources and approves.
