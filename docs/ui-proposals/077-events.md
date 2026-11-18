# UI proposal 077 — events (run and notification lifecycle)

Status: PROPOSAL. Sector framing and persona are GUESS (an "events" lens; no sector REQ exists in
`spec/`). File, component, token, RPC and REQ references are CONFIRMED (read this session). Read-only
on product code; this is the only file written.

Sources (CONFIRMED): `docs/UI.md`, `spec/08-ui-ux.md`, `spec/01-requirements.md`,
`apps/web/components/activity/activity-screen.tsx`,
`apps/web/components/thread/notifications-panel.tsx`, `apps/web/lib/api/rpc.ts`,
`apps/web/lib/run-status.ts`. Charter (CONFIRMED, `docs/UI.md` + AGENTS.md): fresh design (ADR-015);
the owner-supplied robot-cat head (`assets/brand/boma-bot-head.svg`) is the only identity — no
upstream brand, code, asset or wording; BoardUI semantic tokens only, merged with `cx()` (no raw
palette class, no hex literal), one design system, no hand-built lookalike; self-hosted only — no
SaaS/cloud forwarding and no event payload leaving the operator's machine; status always in words,
never colour alone.

## 1. Persona and jobs

**Persona — "the watch operator"**: the person who keeps an eye on a fleet of self-hosted bots
during a working day. Desktop-first beside the activity view, phone for pushes between meetings. She
does not read every message; she watches *events* — a run parked on an approval, a run that failed,
a run that finished, a changed access grant. A missed waiting card stalls work; a misread status or a
leaked webhook URL is a trust failure, so a nameable state beats a clever animation. The robot-cat
mark sits in the empty activity state and the push settings, never a vendor or messaging logo.

Top jobs:
1. **Triage** — see working, waiting and failed runs grouped by day, newest first, and jump to the
   message that started each one (REQ-011, REQ-012).
2. **Unblock** — answer a waiting card or retry a failed run knowing the exact effect, and stop a run
   without touching its helpers (REQ-006, REQ-007, REQ-021).
3. **Stay reachable** — opt a bot into content-free pushes with a named per-bot policy and a daily
   cap, and manage webhook destinations without exposing their URLs (REQ-064, REQ-065).
4. **Reconcile** — trust that a reconnecting live stream resumes ordered events without duplicating
   rows and that "not live" is shown plainly (REQ-005).
5. **Audit** — read an operator-only trail of security-relevant events with actor, action, target,
   reason and time, exportable locally (REQ-045, REQ-058).

## 2. Five concrete changes

All strings are new keys in `lib/i18n.ts` via `useTranslation()` / `t(locale, key)`; no hardcoded
copy. All use installed BoardUI components and the tokens in `docs/UI.md`, classes merged with `cx()`.
Run content, prompts and webhook labels are untrusted: render as text, never HTML.

**(a) Activity event feed — `/app` activity view (REQ-011, REQ-012).**
Extend `ActivityScreen`: group runs by day, one row per run with a named status chip (text + icon),
trigger label, duration and a link back to the originating message; working runs sort to the top and
update in place from the live stream. Use `Chip`/`Badge` (`components/base/badges/chip.tsx`),
`Counter`, `Button` (`RiRefreshLine`, pause/resume icons as component references) and `ScrollFade`
for the scrolled edge; surfaces `bg-background-primary-default`/`bg-background-secondary-default`,
hairline `border-separator-border`, text `text-text-primary`/`text-text-secondary`, type
`text-body-medium`/`text-caption-1-semibold`. RPCs already present: `runsList`, `runsPause`,
`runsResume`, `runsRetry`, `threadsSubscribe` (SSE with heartbeat). Keep the eight statuses from
`lib/run-status.ts` and only offer retry on `failed`.

**(b) Live-state strip — `/app/:botId` thread header and right panel (REQ-005, REQ-012).**
A compact band naming the connection state ("live", "reconnecting", "not live") and the current run
phase, with a `Tooltip` that reveals detail on focus, not hover alone. A `skeleton` stands in while
the snapshot loads; no decorative spinner outlives its work. Components: `Badge`, `Tooltip`,
`skeleton`, `Button` (stop). Tokens: `bg-background-secondary-default`, `border-separator-border`,
`text-text-secondary`, `text-foreground-icon-secondary`, `text-caption-1-semibold`. RPCs:
`threadsSubscribe`, `threadsHead`, `threadsMessages`. Run status changes go through the polite live
region once; the transcript is never a live region (spec §8.9).

**(c) Waiting-event approval card — `/app/:botId` pending card (REQ-007, REQ-017, REQ-064).**
Make the "what will happen" sentence dominant ("Pause run R-42 and ask A. B to approve") with an
explicit confirm label ("Approve and resume"), not a bare "Approve". High-impact effects use
`border-border-error-default` + `text-text-error-primary` **plus** a warning icon and the words
"High impact" — never colour alone. Extend `MessageBlocks`' `CardBlock`; use `Button`, `Chip`,
`Tooltip`; card surface `bg-background-primary-default`, `rounded-3xl`, `border-border-button-default`.
RPCs: `threadsAnswer`, `notificationsPolicyGet`/`notificationsPolicySet` (the `notifyWaiting` gate).
Pushes that announce this card stay content-free — no thread title, prompt or tool argument.

**(d) Compliant notification policy and destinations — `/app/:botId` right panel (REQ-064, REQ-065).**
`NotificationsPanel` currently uses raw `<input>`/`<select>`, hardcoded `×`, `https://`,
`Remove destination <label>`, `text-xs`, `text-[10px]`, `font-semibold` and `accent-accent-600` —
all charter deviations. Replace with BoardUI `Switch`, `Checkbox`, `Input`, `Select`, `Button`,
`Tooltip`; composite type utilities and semantic tokens only; target 44 px via `.boma-target`. Keep
the daily cap an integer, confirm removal with a dialog that states what is lost, and never render a
webhook URL or secret in the DOM, a tooltip or a log. RPCs: `notificationsStatus`,
`notificationsSubscribe`, `notificationsUnsubscribe`, `notificationsPolicyGet`,
`notificationsPolicySet`, `notificationsDestinationsList/Create/Delete`.

**(e) Operator audit-event trail — `/admin/access` (REQ-045, REQ-058).**
Add a named "recent events" table: time, actor, action, target, reason and a local export action;
role changes and grant revocations are attributable and state what the member loses. BoardUI
`table`, `Badge`, `Chip`, `Button`, `Tooltip`; surfaces `bg-background-primary-default`/
`bg-background-secondary-default`, hairline `border-separator-border`, status `text-text-error-primary`
plus text + icon. RPCs: `adminMembersList`, `adminGrantsList`, `adminGrantsCreate`,
`adminGrantsRevoke`, `adminPluginsList`, `adminPluginsDecide`. Optional forwarding stays an explicit
self-hosted destination configured by the operator — never a SaaS relay; no id, stack trace or
provider text on screen (correlation id behind "details", spec §8.9).

## 3. Accessibility, i18n and RTL

- Every new label is a `lib/i18n.ts` key (English source; French/Hebrew translations; a missing key
  falls back to English, never a blank or the key). No string in JSX — this also retires the hardcoded
  strings in (d).
- `DirectionProvider` (explicit BCP 47 locale) sets direction; the boot script writes `lang`/`dir`.
  Use logical utilities (`ms`/`me`, `ps`/`pe`, `text-start`/`text-end`, `border-s`/`border-e`,
  `rounded-s`/`rounded-e`) and `useDirection()` to mirror arrows and drawer/dialog motion in RTL.
  Keep run ids, webhook URLs, timestamps, notification endpoints and correlation ids LTR in `<bdi>` /
  `dir="auto"`.
- WCAG 2.2 AA gate: zero critical/serious axe findings in light, dark and RTL; 44 px targets via
  `.boma-target`; visible token-driven focus (`focus-visible:ring-2 focus-visible:ring-border-focus-ring`);
  full keyboard path (triage, approve/retry, stop, push toggle, revoke, destination removal); status
  by text + icon, not colour; 360 px width; `prefers-reduced-motion` honoured (no animation on
  streaming content, spec §8.9).
- Announcements: status changes, card arrivals and errors use one polite live region; the transcript
  is not a live region. Error copy is one sentence of what happened plus one of what to do, with no
  prompt text, webhook URL, id or provider message on screen.

## 4. Playwright coverage (REQ id in the test name)

- `test("REQ-005 live stream heartbeats and resumes ordered events without duplicating rows", ...)`.
- `test("REQ-006 stop cancels the active run and leaves helper runs untouched", ...)`.
- `test("REQ-007 waiting card names the exact action, takes focus, and does not block chat", ...)`.
- `test("REQ-009 unread divider moves and snooze hides new activity until the snooze ends", ...)`.
- `test("REQ-011 activity view replaces the shelves and restores them on exit", ...)`.
- `test("REQ-012 every run status renders a distinct named chip with the right terminal action", ...)`.
- `test("REQ-021 failed-run retry updates the row in place without a duplicate", ...)`.
- `test("REQ-045 audit trail names actor, action, target and reason for an operator only", ...)`.
- `test("REQ-057 rtl mirrors the event feed and axe reports zero critical or serious findings", ...)`.
- `test("REQ-064 push opt-in is per bot, content-free, and the daily cap suppresses without changing the run", ...)`.
- `test("REQ-065 destination add/remove is confirmed and the webhook value stays LTR", ...)`.
- New behaviour without a REQ (live-state strip, event export) must get an owner-assigned REQ id
  before implementation — do not invent one in a test name (traceability gate). Every golden under
  `spec/golden/ui/` stays byte-exact; a new seeded state means a new `REQ-<id>-<slug>.json` golden,
  never a click path.

## 5. Best first slice

**Change (a), the activity event feed** — GUESS, recommended. It is the smallest, highest-value step:
one screen, no new transport (`runsList`/`runsPause`/`runsResume`/`runsRetry`/`threadsSubscribe`
already exist), no new component (`Button`/`Chip`/`Badge`/`Tooltip` are installed), an existing golden
(`spec/golden/ui/REQ-012-run-statuses.json`) to pin it, and it makes every event state legible before
any push or audit work. Test-first: write `REQ-011 ... replaces the shelves` and `REQ-012 ... distinct
named chip`, extend the golden, then ship only the day grouping, text + icon status and the link back
to the originating run. Change (d) is a close second because it removes live charter violations.
Defer (b), (c), (e) and the new REQs until the owner assigns ids and approves.

**Decision needed from the owner**: confirm the "events" sector framing and assign REQ ids for
behaviour the spec does not already cover; record any externally visible deviation in `docs/STATE.md`
/ `docs/PARITY.md` before shipping.
