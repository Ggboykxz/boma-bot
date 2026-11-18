# UI proposal 079 — fitness (strength and conditioning)

Status: PROPOSAL (GUESS where the spec is silent). Sector specialist #079. Read-only on product code.
Grounding: `docs/UI.md`, `spec/08-ui-ux.md`, and
`apps/web/components/{thread/thread-composer,activity/activity-screen}.tsx`.
RPC names CONFIRMED against the exports in `apps/web/lib/api/rpc.ts`; the sector mapping is GUESS
(no "fitness" REQ exists in the spec — REQ-007, REQ-011/012 and REQ-036 are the closest anchors).
Charter: Boma Bot only; no upstream brand, code, wording or assets; BoardUI tokens only, merged with
`cx()`, dark mode via tokens; one design system; self-hosted (no SaaS); keep the robot-cat identity
in empty and onboarding states.

## 1. Persona and top jobs-to-be-done

**Persona — Leo, a strength coach running a small independent training studio.** He writes weekly
programs, logs sessions from his phone minutes after they happen, tracks client progress across
weeks, and coordinates with a physio on injury rehab. Client records hold body metrics, injury
notes and load history: Boma Bot is self-hosted on the studio's own machine, so that data never
leaves the premises. He is not a technologist; logging the right load to the wrong client, or an
ambiguous program change, is a trust problem worse than latency.

Top jobs:
1. **Program** — build and assign a training block to a client, with a named progression.
2. **Log** — capture a completed session (sets, reps, load, RPE) immediately, from the phone.
3. **Track** — see progress over weeks (1RM, adherence) and spot a stall early.
4. **Coordinate** — hand a case to a physio or nutrition bot, including supervised entry into a
   legacy gym-management system.
5. **Share** — send a progress report to a client, confirming the recipient and what is shared.

## 2. Five concrete changes

All strings are new keys in `lib/i18n.ts` through `t(locale, key)` (English source; French/Hebrew
translations; a missing key falls back to English, never a blank or the key). All use BoardUI
components under `apps/web/components/` and the semantic tokens in `docs/UI.md`; classes merged with
`cx()` — no raw palette, hex or second design system.

**(a) Client-and-program context strip — screen `/app/:botId` (thread header).**
A compact, read-only band above the transcript: chips for "client: <name>", "block week 3/6",
"last session <date>" and "adherence 4/5", each with a keyboard-focusable `Tooltip` carrying detail.
BoardUI `Chip`, `Counter`, `Tooltip`, `IconButton` (`components/base/badges/chip.tsx`, `counter.tsx`,
`components/base/tooltip/tooltip.tsx`, `components/base/buttons/icon-button.tsx`); surfaces
`bg-background-secondary-default`, hairline `border-separator-border`, text `text-text-secondary` /
`text-text-primary`, icons `text-foreground-icon-secondary`, type `text-caption-1-semibold`.
RPCs already present: `botsGet`, `threadsHead`, `runsList`. Client content is untrusted — render as
text, never HTML; keep metrics out of hover-only tooltips and never signal adherence by colour alone.

**(b) Session check-in / program-change approval card — screen `/app/:botId` (pending card, REQ-007).**
Make the "what will happen" line dominant: name the client, the load and the exact effect —
"Log 3x5 @ 100 kg for A. Client and update 1RM", not just "Approve". Large load jumps use
`border-border-error-default` + `text-text-error-primary` **plus** an icon and the words "High
impact" — never colour alone. A failed run offers a named retry. BoardUI `Button`, `Chip`,
`Tooltip`; card surface `bg-background-primary-default`, `rounded-3xl`,
`border-border-button-default`. RPCs: `threadsAnswer`, `runsRetry`. The card is announced once
through the polite live region (spec §8.9); the transcript is not a live region.

**(c) Client-data composer and reply banner — screen `/app/:botId` (composer, REQ-001).**
Extend `ThreadComposer` with a "contains health data" `Switch` and a redaction hint; keep
Enter-to-send / Shift+Enter-newline and keep send disabled while a modal approval is pending. Client
names, body metrics and dates stay LTR in `<bdi>`/`dir="auto"` inside the reply chip. BoardUI
`Switch`, `IconButton`, `Tooltip` from the installed base set; focus
`focus-visible:ring-2 focus-visible:ring-border-focus-ring`, 44 px `.boma-target`, icons
`text-foreground-icon-primary`. RPCs: `threadsSend`, `threadsReact` — text enters the existing
`value` state, so no new transport.

**(d) Program and progress artifacts — screens `/app/artifacts`, `/app/artifacts/:artifactId`
(REQ-036).**
In `ArtifactsGrid`, show training plans, progress charts and PDF programs as cards (name, type icon,
size, owner bot, updated time); filter by type and bot, search by name. The viewer keeps the version
chain selector, download, "open in thread" and copy; images render in a bounded frame. Content is
untrusted: never render an artifact's HTML in the app document. Keep the robot-cat empty state.
BoardUI `Input`, `Button`, `tabs`, `table`; surfaces `bg-background-primary-default` /
`bg-background-secondary-default`, hairline `border-separator-border`. RPCs: `artifactsListSpace`,
`artifactsGet`, `artifactsListVersions`.

**(e) Training-log triage and supervised legacy entry — screen `/app` activity view (REQ-011/012)
and `/app/:botId` right panel (REQ-026/028/029).**
In `ActivityScreen`, give each training-log run a named status chip, duration and a link back to the
message that started it; working runs sort to the top and update in place; filter by bot and status.
For the legacy gym-management system, raise the computer panel with explicit takeover/release
controls. BoardUI `table`, `Chip`, `Badge`, `drawer`, `spinner`, `skeleton`; status by text + icon,
never colour alone. RPCs: `runsList`, `runsPause`, `runsResume`, `runsRetry`, `computerStatus`,
`computerTakeover`, `computerRelease`, `computerOpenScreen`. Takeover is explicit and attributable.

## 3. Accessibility, i18n and RTL

- New labels go in `lib/i18n.ts` (English source; French/Hebrew translations; a missing key falls back
  to English, never blank or key). No string in JSX.
- `DirectionProvider` (explicit BCP 47 locale) sets direction; the boot script writes `lang`/`dir`.
  Use logical utilities (`ms`/`me`, `ps`/`pe`, `text-start`/`text-end`, `border-s`/`border-e`) and
  `useDirection()` to mirror arrows and drawer motion in RTL. Metrics, dates, client IDs, emails and
  URLs stay LTR in `<bdi>`/`dir="auto"`.
- WCAG 2.2 AA gate: zero critical/serious axe findings in light, dark and RTL; 44 px targets via
  `.boma-target`; visible token-driven focus; a full keyboard path (approval, health-data switch,
  artifact filter, takeover); status by text + icon, not colour; 360 px width;
  `prefers-reduced-motion` honoured and no animation on a value the coach is reading.
- Health-data privacy: error copy is one sentence of what happened plus one of what to do — no body
  metrics, injury notes, client names, stack traces or provider text on screen (correlation id behind
  "details" only, spec §8.9). Nothing confidential lives in a hover-only tooltip; no client data is
  written to logs; the share action confirms the recipient first.

## 4. Playwright tests (REQ ids where they exist)

- `REQ-001 send creates one optimistic message that is not duplicated`
- `REQ-007 pending session card names the client, load and effect, takes focus, and does not block reading`
- `REQ-011 activity groups training-log runs by day with a named status and a link back`
- `REQ-012 every run status renders a distinct named chip with the right terminal action`
- `REQ-026 computer panel moves from provisioning to takeover with named states`
- `REQ-028 takeover and release announce their status through the polite live region`
- `REQ-036 artifacts list, filter and version selector show programs and progress reports`
- `REQ-057 Hebrew RTL loads dir=rtl with a clean axe run`
- New behaviour without a REQ yet (context strip, health-data switch, recipient gate) must be given a
  REQ id by the owner before implementation — do not invent one in test names per the traceability gate.

## 5. Highest-impact first slice

**Change (b), the session check-in / program-change approval card.** It is the smallest,
highest-value step: one screen, no new transport (`threadsAnswer`/`runsRetry` already exist), no new
component (BoardUI `Button`/`Chip`/`Tooltip` are installed), and it directly reduces the risk of
logging the wrong load to the wrong client. Test-first: write
`REQ-007 ... names the client, load and effect` and extend
`spec/golden/ui/REQ-007-pending-card.json` first; ship only the dominant consequence sentence, the
explicit confirm label and the text+icon high-impact marker. Change (e) activity triage is a close
second because `REQ-011/012` and their Playwright tests already exist. Defer (a), (c), (d) and the new
REQs until the owner assigns ids and approves. No product code was modified.
