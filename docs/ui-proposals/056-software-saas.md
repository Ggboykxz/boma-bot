# UI proposal 056 — software / SaaS (self-hosted engineering tooling)

Status: PROPOSAL (GUESS where the spec is silent). Sector #056. Read-only on product code; this file is
the only write. Grounding (CONFIRMED, read this session): `docs/UI.md`, `spec/08-ui-ux.md`,
`spec/01-requirements.md`, `apps/web/lib/api/rpc.ts`, and two components:
`apps/web/components/admin/plugins-screen.tsx` (submission queue, source as text, `role="alert"` error)
and `apps/web/components/integrations/integrations-setup.tsx` (partial-install report, no false green
check, per-bot vs space-wide honesty). No upstream brand, code, asset, wording or capture consulted.

Charter (CONFIRMED): Boma Bot only; fresh design (ADR-015); owner-supplied robot-cat is the sole identity
art; no upstream brand/code/assets/copy; BoardUI semantic tokens only — no raw palette class or hex,
every class list merged with `cx()`; one design system. Self-hosted: tooling runs on the company's own
machine — no hosted Boma service, cloud signup, or code, incident, customer datum, token or telemetry
leaving the network. Product name and mark unchanged.

## 1. Persona and top jobs-to-be-done

**Persona — "the release captain"**: an engineering/platform lead at a self-hosted software company,
working in English, French and Arabic across a desktop release console and a phone on call. Bots triage
builds, incidents, dependency bumps and customer bugs on the company's own infrastructure; Boma is never
a hosted control plane. She is measured on safe, reversible releases. Approving a deploy or a migration
against the wrong version or environment, on a green colour alone, is an outage. Ambiguity is worse than
latency. The robot-cat mark is the only identity art; no vendor, cloud or company logo is borrowed.

Top jobs:
1. **Brief and attach** — hand a bot a repo, issue or log excerpt without losing the draft to a stray
   Enter, and see what is new, unread or awaiting sign-off without scrolling a transcript.
2. **Ship safely** — know the exact version, environment and effect of a bot-proposed deploy, migration
   or merge before one click commits it; never decide by colour alone.
3. **Triage builds and incidents** — watch CI, build, test and incident runs by name and status, retry
   only failures, pause/resume queued work, with a link to the message that started the run.
4. **Review extensions** — decide plugin submissions and connector installs with the source readable as
   text, the no-local-code rule stated, and no code executed by the review itself.
5. **Track and share** — move a release ticket from idea to production, see its attached runs, and share
   a runbook by link with no login and no leakage.

## 2. Five concrete UI changes

New copy is a key through `useTranslation()` (`@/lib/use-translation`) → `t(locale, key)`; English source,
fr/he translations, a missing key falls back to English. Nothing hardcoded in JSX. Every class list goes
through `cx()`; every colour rides a `docs/UI.md` token. Bot/user values (repo names, logs, source refs,
plugin output) are untrusted text/bytes, never HTML; no credential reaches the DOM or a log.

**(a) Deploy / migration approval card — thread `/app/:botId` pending card (REQ-007/REQ-017).**
Make "what will happen" dominant: exact version and commit, target environment, effect ("Deploy 1.4.2
to production", "Apply migration 0042", "Merge dependency bump #318"). Confirm gets an explicit label,
not "Approve". High-impact actions pair `border-border-error-default` + `text-text-error-primary` with
an icon and the word "Release-critical" — never colour alone. Keep the `threadsAnswer` guard so a double
answer is honest. BoardUI `Button`, `Badge`/`Chip`, `Tooltip`; card `bg-background-primary-default
rounded-3xl border-border-button-default`, type `text-body-medium`/`text-caption-1-semibold`, focus
`focus-visible:ring-2 focus-visible:ring-border-focus-ring`, text `text-text-primary`/`text-text-secondary`.
Transcript stays readable, other threads reachable, focus lands on the card; announce once via the polite
live region (spec §8.9) — the transcript is not a live region. RPC (CONFIRMED): `threadsAnswer`.

**(b) Connector & plugin trust panel — `/integrations/setup` (REQ-032/REQ-034/REQ-048).**
Extend `IntegrationsSetupScreen`: keep the partial-install report (skipped parts + reasons) instead of a
false green check; state the no-local-code rule plainly. Show source references as text with
`rel="noopener noreferrer nofollow"` and a named status chip; keep the honest distinction that messaging
binds one bot while connectors/plugins are space-wide — do not fake a per-bot gate. Prefer installed
BoardUI `Tabs`/`Select`/`Tooltip`; icons as component references. Tokens: `bg-background-primary-default`,
`bg-background-secondary-default`, `border-border-button-default`, `border-separator-border`,
`text-text-primary`/`text-text-secondary`/`text-text-tertiary`, `text-foreground-icon-secondary`.
RPCs (CONFIRMED): `connectionsList`, `marketplaceList`, `messagingList`, `modelsConnect` (local key only).

**(c) Build and incident triage — `/app` Activity view (REQ-011/REQ-012/REQ-064).**
Follow `ActivityScreen`: group CI, build, test and incident runs by day with prompt snippet, bot name,
named status chip, duration and a link to the starting message. Working runs sort to the top and update
in place; failures get a plain cause and retry, never a decorative spinner. Skeletons while loading — a
blank panel is a bug. Components: installed `table`/`chip`, `Button`, `IconButton`, `Tooltip`; surfaces
`bg-background-primary-default`/`bg-background-secondary-default`, hairline `border-separator-border`;
use the exact §8.9 vocabulary (`queued`, `leased`, `running`, `waiting_input`, `waiting_takeover`,
`completed`, `failed`, `cancelled`) as text + icon. RPCs (CONFIRMED): `runsList`, `runsRetry`,
`runsPause`, `runsResume`, `threadsHead`, `notificationsStatus`.

**(d) Plugin submission review queue — `/admin/plugins` (REQ-058).**
Grounded in `PluginsScreen`: keep the source exactly as submitted — `<code dir="auto">` to read, never to
render or execute. Replace bare coloured approve/reject buttons with explicit labels ("Approve submission
<name>", "Reject submission <name>") plus icon and text, and add a one-line consequence: approval only
flips review status so the ordinary install flow can pick it up; no code runs here. Keep the `role="alert"`
error, distinct forbidden state and retry; list rows carry `scope` headers. Components: installed
`Button`, `Badge`/`Chip`, `Tooltip`, `table`; tokens `border-border-error-default`,
`text-text-error-primary`, `bg-background-full`, `border-separator-border`, `text-text-primary`.
RPCs (CONFIRMED): `adminPluginsList`, `adminPluginsDecide`. A non-admin gets a refusal, not a locked screen.

**(e) Release board and shared runbook — `/app/board` and `/p/:slugToken` (REQ-038/REQ-037).**
Reuse `BoardScreen`: columns idea → in progress → review → deployed, cards with key, title, assignee bot
and last comment; assigning a ticket queues bot work shown here, not in chat, with run status chips in
the detail panel. Flag off means the route is absent, not locked (§8.6). Share a runbook with no chrome
and no session beyond a small robot-cat footer; unknown, revoked and expired tokens render one identical
"not available" screen — same words, shape and timing. Components: installed `table`/`chip`, `Input`,
`Select`, `Button`; tokens `border-border-table`, `bg-background-primary-default`, `text-text-primary`/
`text-text-secondary`. RPCs (CONFIRMED): `boardsGet`, `ticketsCreate`, `ticketsMove`, `ticketsComment`,
`runsList`, `artifactsListSpace`. Transport (CONFIRMED): `GET /api/v1/pages/shared/{token}`.

## 3. Accessibility

WCAG 2.2 AA: zero critical/serious axe findings in light, dark and RTL; keyboard-only path for every
primary flow (deploy decision, plugin review, connector install, run retry/pause, board move, version
select); visible `focus-visible:ring-2 focus-visible:ring-border-focus-ring`; ≥44 px `.boma-target`;
360 px width; `prefers-reduced-motion` honoured; no animation on streaming content. Release-critical
markers, run status and install state are text + icon, never colour alone. The unavailable screen is
announced in identical text — no accessibility-tree leak. Source refs get meaningful accessible names.
Error copy: one sentence of what happened plus one of what to do; no hashes, ids, stack traces, provider
text or secrets on screen (correlation id behind "details" only). Tables carry `<caption>`/`<th scope>`.

## 4. i18n and RTL

English source catalogue, fr/he translations; a missing key falls back to English. `DirectionProvider`
(`@/components/foundations/direction/direction`) gets an explicit BCP 47 locale; the boot script writes
`lang` and `dir` together. RTL mirrors the rail, board columns and drawer motion via `useDirection()`;
navigation and disclosure glyphs flip. Logical utilities (`ms`/`me`, `ps`/`pe`, `text-start`/`text-end`,
`border-s`/`border-e`). Version tags, commit SHAs, ticket keys, repo paths, URLs, ports and timestamps
stay LTR in `<bdi>`/`dir="auto"`. Preserve physical coordinates for measured diagrams — never mirror them.

## 5. Playwright tests (REQ id in each name)

- `test("REQ-001 briefing a bot creates one optimistic message that is not duplicated", ...)`
- `test("REQ-007 the deploy card names version, environment and effect and takes focus without blocking reading", ...)`
- `test("REQ-017 release-critical and deny answer the card exactly once with an attributable confirmation", ...)`
- `test("REQ-032 connector install states are named in text and never a false green check", ...)`
- `test("REQ-034 a plugin's skipped parts and no-local-code rule are text, never markup", ...)`
- `test("REQ-058 plugin review names the source and never executes it, refusing non-admins", ...)`
- `test("REQ-011 the activity view groups build and incident runs by day and links to the source", ...)`
- `test("REQ-012 every CI run status renders a distinct named text-and-icon chip", ...)`
- `test("REQ-038 the release board shows text-and-icon columns and moves a ticket by keyboard", ...)`
- `test("REQ-038 the board route is absent, not locked, when the flag returns 404", ...)`
- `test("REQ-037 unknown, revoked and expired tokens render the identical unavailable document", ...)`
- `test("REQ-057 Hebrew RTL mirrors the board and axe reports zero critical or serious findings", ...)`

Behaviour without a REQ yet (release-critical marker, day grouping, consequence line) needs an
owner-assigned id before code. Goldens stay byte-exact; a new state gets a new `REQ-<id>-<slug>.json`.

## 6. Best first slice

**Change (a), the deploy / migration approval card** — GUESS, recommended. One screen, no new transport
(`threadsAnswer` exists), no new component (BoardUI `Button`/`Badge`/`Tooltip` installed), no token
re-tune, and an existing golden (`spec/golden/ui/REQ-007-pending-card.json`) to extend. It removes the
risk of an irreversible deploy confirmed on a colour cue or against the wrong version. Test-first: write
`REQ-017 release-critical and deny answer the card exactly once with an attributable confirmation`,
extend the REQ-007 golden, then ship only the dominant consequence line, the explicit "Deploy 1.4.2 to
production" label and the text + icon release-critical marker. Defer (b)–(e) until REQ ids are assigned.

**Decision needed**: confirm the self-hosted engineering-tooling framing and whether the feature-flagged
board (REQ-038) is in scope before change (e) is scheduled.

Provenance: REQ ids from `spec/01-requirements.md`; RPC names from `apps/web/lib/api/rpc.ts`; component
structure from `plugins-screen.tsx` and `integrations-setup.tsx`; rules from `docs/UI.md` and
`spec/08-ui-ux.md` — all CONFIRMED. Persona, jobs and changes are a GUESS. No product code was modified.
