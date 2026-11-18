# UI proposal 065 — call centres (contact centre)

Status: PROPOSAL (GUESS where the spec is silent). Sector specialist #065 — call centres. Read-only on
product code; only this file was written. Grounding read this session: `docs/UI.md`,
`spec/08-ui-ux.md`, `spec/01-requirements.md` (REQ ids), `apps/web/lib/api/rpc.ts` (RPC names), and
two components under `apps/web/components/`: `activity/activity-screen.tsx` and
`thread/notifications-panel.tsx`. Charter held: Boma Bot only — no upstream brand, code, assets or
wording; BoardUI semantic tokens only — no raw palette class or hex literal, classes merged with
`cx()`; one design system, no second; no SaaS (self-hosted — customer records and transcripts never
leave the premises); robot-cat identity unchanged.

## 1. Persona and top jobs-to-be-done

**Persona — Samira, a supervisor in a mid-size contact centre.** She monitors a live queue of
customer contacts arriving by chat, e-mail and messaging (voice when the flag is on, REQ-049),
coaches agents and verifies bot-proposed refunds and account changes before they execute. Teams work
in English, French and Arabic. Boma Bot runs on the centre's own server: customer records, call
recordings and transcripts are never sent to a third party. A refund approved on a colour cue alone,
or a customer moved to the wrong queue, is a complaint or a data breach — legible state, explicit
"what will happen" and an attributable handoff matter more than a fast screen.

Top jobs:
1. **Triage** — see which contact runs are queued, working, waiting on a human or failed, and pick
   the next one; tell working from stalled without reading every transcript.
2. **Verify before committing** — confirm a bot-proposed refund, credit or account change knowing
   the exact customer, amount and effect; never approve on colour alone.
3. **Intervene** — take exclusive control of the CRM browser for a manual step (identity check,
   payment retry) and hand it back with an attributable record.
4. **Route and escalate** — opt a bot into content-free push and space-level Slack/Teams
   destinations so an unanswered contact reaches a human before the SLA lapses (REQ-064, REQ-065).
5. **Prove and share** — keep versioned call summaries and knowledge notes, open each in the thread
   where it was produced, and publish a customer summary by link without giving every reader a login.

## 2. Five concrete changes

All new copy is catalogued through `t(locale, key)` (`lib/i18n.ts`; English source, French/Arabic
translations, a missing key falls back to English — never blank or the raw key). Every colour rides
the `docs/UI.md` token map, type uses composite utilities, class lists go through `cx()`. Icons come
from `@remixicon/react` as component references. Customer-supplied text is untrusted, never HTML.

**A. Contact-context strip — `/app/:botId` thread header.** A compact read-only band above the
transcript with chips for channel, customer reference, queue and wait/SLA state, each with a
`Tooltip` carrying detail. Components: `Badge`/`Chip` (`components/base/badges/chip.tsx`),
`Tooltip`, `IconButton`. Tokens: `bg-background-secondary-default`, `border-separator-border`,
`text-text-primary`/`text-text-secondary`, `text-foreground-icon-secondary`,
`text-caption-1-semibold`, joined with `cx()`. Existing RPCs: `botsGet`, `threadsHead`,
`threadsMessages`. Customer ids and timestamps stay LTR in `<bdi>`; never show full PII here.

**B. Refund / credit approval card — `/app/:botId` pending card (`REQ-007`/`REQ-017`/`REQ-018`).**
Make "what will happen" dominant: exact customer reference, amount and effect, with an explicit
confirm label ("Refund 40.00 to customer C-1182", not just "Approve"). High-impact or over-limit
requests pair `border-border-error-default` + `text-text-error-primary` with a leading icon and the
words "High impact" — never colour alone. Components: BoardUI `Button`, `Badge`, `Tooltip`; card
surface `bg-background-secondary-default`, `rounded-3xl`, `border-border-button-default`, focus
`focus-visible:ring-2 focus-visible:ring-border-focus-ring`. Existing RPC `threadsAnswer` (exactly-once
effect, keeps the `CONFLICT` guard). Announced once via the polite live region; the transcript stays
readable and other threads reachable (spec §8.9).

**C. CRM takeover panel — `/app/:botId` right panel (`REQ-026`/`REQ-028`/`REQ-029`).** Extend
`computer-panel.tsx`: give each state a name (starting, running, awaiting takeover, released) plus a
plain caption — "Leaving this screen pauses nothing — the handoff record stays open." Takeover and
release stay explicit, attributable actions. Components already used there: `Button`, status
`role="status"`, `text-text-error-primary` error `role="alert"`; surface
`bg-background-primary-default`, hairline `border-border-button-default`, `text-caption-1-semibold`.
Existing RPCs: `computerStatus`, `computerOpenScreen`, `computerOpenScreenFrame`, `computerTakeover`,
`computerRelease`. Mirror drawer motion in RTL; leave physical coordinates untouched; no customer data in logs.

**D. Contact-queue triage — `/app` Activity view (`REQ-011`/`REQ-012`/`REQ-021`/`REQ-067`).** Extend
`ActivityScreen` so each run row names its state out loud and adds a contact caption (`queued` →
"contact queued", `running` → "agent handling", `waiting_input` → "awaiting customer", `failed` →
"contact failed"). The canonical §8.9 vocabulary stays; the caption is secondary text. Working/paused
rows keep Pause/Resume (REQ-067), failed rows keep Retry (REQ-021); working runs sort to the top and
update in place, no decorative spinner. Components already in the file: `Button`, `role="status"`,
`runStatusKey`/`isRetryable`; surfaces `bg-background-primary-default`/`bg-background-secondary-default`,
`border-separator-border`, error row `border-border-error-default` + `text-foreground-icon-error`.
Existing RPCs: `runsList`, `runsPause`, `runsResume`, `runsRetry`, `threadsHead`.

**E. Escalation routing, knowledge library and shared summary — panel, library and `/p/:slugToken`
(`REQ-064`/`REQ-065`/`REQ-036`/`REQ-038`/`REQ-037`).** In `notifications-panel.tsx`, keep the
per-bot waiting/completed policy and space-level Slack/Teams destinations, but replace raw
`<input>`/`<select>` with installed BoardUI `Checkbox`, `Select` and `Input` and route all strings
through `t()`; pushes stay content-free, keyed by the existing per-(space, member, bot) cap. RPCs:
`notificationsStatus`, `notificationsSubscribe`, `notificationsUnsubscribe`,
`notificationsPolicyGet`, `notificationsPolicySet`, `notificationsDestinationsList/Create/Delete`
(Owners/Admins only). In the library, show versioned call summaries and knowledge notes via
`artifactsListSpace`, `artifactsGet`, `artifactsListVersions`; never inject artifact HTML. A
flag-gated board (`boardsGet`, `ticketsCreate`, `ticketsMove`, `ticketsComment`) shows escalation
tickets; with the flag off the route is absent, not locked. A compiled customer summary can be
published through `/p/:slugToken` with no chrome beyond a robot-cat footer; unknown, revoked and
expired tokens show one identical "not available" screen — same wording and timing — with
`noindex`, `no-referrer` and its own CSP.

## 3. Accessibility, i18n and RTL

- All visible strings go through `t(locale, key)`, no string hardcoded in JSX. `DirectionProvider`
  takes the explicit BCP 47 locale; the boot script writes `lang`/`dir` together. Use logical
  utilities (`ms`/`me`, `ps`/`pe`, `text-start`/`text-end`, `border-s`/`border-e`) and `useDirection()`
  to mirror the context strip, queue rows and drawer motion in RTL (REQ-057). Customer ids, phone
  numbers, URLs and dates stay LTR in `<bdi>`/`dir="auto"`, money in integer minor units via `Intl`.
- WCAG 2.2 AA gate: zero critical/serious axe findings in light, dark and RTL; 44 px `.boma-target`
  on touch controls; token-driven visible focus; a keyboard path for every decision (approval,
  takeover/release, pause/resume/retry, ticket move, version select, destination add/remove); run
  status and high-impact markers are text + icon, never colour alone (spec §8.9); 360 px width;
  `prefers-reduced-motion` honoured; no animation on streaming content.
- Error copy is one sentence of what happened plus one of what to do — no customer PII, stack traces
  or provider text (correlation id behind "details" only, spec §8.9). The approval decision is
  announced once via the polite live region; the transcript is not a live region, so tokens are not read aloud. The robot-cat mark stays unchanged.

## 4. Playwright tests (REQ ids in the names)

```
test("REQ-001 the contact composer sends one optimistic message that is not duplicated", ...)
test("REQ-007 a pending refund card names customer, amount and effect and does not block reading", ...)
test("REQ-017 a high-impact refund answers once and executes its effect once", ...)
test("REQ-018 an over-limit credit is refused with a visible reason and no payment", ...)
test("REQ-011 the activity view groups contact runs by day and links back to the thread", ...)
test("REQ-012 every run status renders as text plus icon in light, dark and RTL", ...)
test("REQ-021 a failed contact run shows a plain cause and a working retry action", ...)
test("REQ-026 the CRM panel names starting, running, takeover and released states", ...)
test("REQ-028 observe and act on the CRM browser through the existing RPCs", ...)
test("REQ-029 takeover and release announce their status and pause the bot", ...)
test("REQ-064 a waiting approval produces a content-free push that opens the thread", ...)
test("REQ-065 a space Slack destination receives the same content-free text, PII-free", ...)
test("REQ-036 the summary library selects a version and opens it in its thread", ...)
test("REQ-038 the escalation board moves a ticket by keyboard and hides when the flag is off", ...)
test("REQ-037 the shared summary renders without chrome; a bad token shows the uniform screen", ...)
test("REQ-057 the contact screens mirror for Hebrew/Arabic with dir=rtl and a clean axe run", ...)
```

Behaviour without a REQ yet (context strip, contact captions, high-impact marker) must be assigned a
REQ id by the owner before implementation — do not invent one in test names.

## 5. Highest-impact first slice

**Change B — the refund/credit approval card.** It is the cheapest and the only change that directly
reduces the risk of an irreversible refund approved on an ambiguous cue: `threadsAnswer`, `Button`
and `Badge` are installed, the semantic tokens exist, and no new RPC, component install, data model
or token re-tune is needed. Test-first: write `REQ-017 a high-impact refund answers once and executes
its effect once` and extend `spec/golden/ui/REQ-007-pending-card.json` first; ship only the dominant
effect sentence, the explicit confirm label, customer/amount attribution, the text+icon high-impact
marker, keyboard operability and the polite announcement. Defer (A), (C), (D), (E) until the owner
assigns REQ ids and approves.

Provenance: REQ ids CONFIRMED from `spec/01-requirements.md`; RPC names CONFIRMED from
`apps/web/lib/api/rpc.ts`; component structure (`ActivityScreen` `runStatusKey`/`isRetryable`/pause
and `NotificationsPanel` policy/destination flows with raw `<input>`/`<select>`) CONFIRMED by reading
the two files this session; design rules CONFIRMED from `docs/UI.md` and `spec/08-ui-ux.md`. The
persona, its jobs and the contact-centre mapping are GUESS (proposal only — no call-centre sector or
REQ exists in the spec). No product code was modified.
