# UI proposal 078 — sports (self-hosted club / league operations)

Status: PROPOSAL (GUESS where the spec is silent). Sector specialist #078 — sports. Read-only on product
code; this file is the only write. Grounding (CONFIRMED, read this session): `docs/UI.md`, `spec/08-ui-ux.md`,
`spec/01-requirements.md` (REQ ids), `apps/web/lib/api/rpc.ts` (procedure names), `spec/golden/ui/` (golden
names) and two components: `apps/web/components/thread/computer-panel.tsx` (bounded live frame, named
status, Take over / Release that name the holder, `role="status"`/`role="alert"`, untrusted frame bytes)
and `apps/web/components/artifacts/artifact-viewer.tsx` (version selector, bounded image/PDF frame, `<pre>`
for text, confirm-delete, every label through `t()`). No upstream brand, code, asset, wording or capture consulted.

Charter (CONFIRMED): Boma Bot only; fresh design (ADR-015); the owner-supplied robot-cat
(`assets/brand/boma-bot-head.svg`) is the sole identity art — no club, league, sponsor or vendor mark.
BoardUI semantic tokens only — no raw palette class or hex literal; every class list merged with `cx()`;
dark mode rides the tokens; one design system, no second library or lookalike. Self-hosted: the club's own
hardware, no hosted Boma service, cloud signup or SaaS framing; no fixture, athlete, medical, contract or
telemetry datum leaving the network. Product name and mark unchanged.

## 1. Persona and top jobs-to-be-done

**Persona — "the match-day operations lead"**: a performance/ops manager at a self-hosted club or league, working in
English, French and Arabic across a desktop ops desk and a phone on match day. Bots ingest match-data feeds, clip video,
write scouting reports, draft fixtures, queue social posts and keep roster/injury notes. She is measured on correct,
reversible public actions: publishing the wrong lineup, result or transfer figure is a real incident, so ambiguity is
worse than latency. The robot-cat mark is the only identity art; no crest or sponsor logo is borrowed.

Top jobs:
1. **Brief and attach** — hand a bot a clip, stats export or fixture reference without losing the draft to a stray Enter,
   and see what is new, unread or awaiting sign-off without scrolling a transcript.
2. **Publish safely** — know the exact competition, fixture and effect of a bot-proposed lineup, result post or transfer
   note before one click commits it; never decide by colour alone.
3. **Triage runs** — watch feed-ingest, video-processing, report and social-draft runs by name and status, retry only
   failures, pause/resume queued work, with a link to the source message.
4. **Spectate and take over** — watch a bot on a live stats portal or ticketing login, take control for a CAPTCHA or
   credential step, and release cleanly with the holder always named.
5. **Track and share** — keep fixture/scouting work on the feature-flagged board, keep versioned reports searchable, and
   publish a public match report by link with no login and no leakage.

## 2. Five concrete UI changes

All new copy is a key via `useTranslation()` / `t(locale, key)`; English source, fr/he translations, a
missing key falls back to English — never a blank or a bare key. Nothing hardcoded in JSX. Every class
list goes through `cx()`; every colour rides a `docs/UI.md` token. Bot- and feed-produced values
(clips, stats, names, scores) are untrusted text/bytes, never HTML; no credential reaches a log.

**(a) Live feed spectate & takeover — `/app/:botId` right panel (REQ-026/028/029/030), grounded in
`computer-panel.tsx`.** Reuse the bounded frame + polling panel as a "watch the bot on the stats portal"
surface: keep the named `role="status"` badge, the bounded frame, no autoplay, and explicit Take over /
Release buttons that always name the holder, with a `role="alert"` fault line of one cause + one action.
Components: installed `Button`, `Badge`; tokens `bg-background-primary-default`,
`bg-background-secondary-default`, `border-border-button-default`, `text-text-primary`/
`text-text-secondary`/`text-text-error-primary`, focus `focus-visible:ring-2
focus-visible:ring-border-focus-ring`, 44 px `.boma-target`. RPCs (CONFIRMED): `computerStatus`,
`computerOpenScreen`, `computerOpenScreenFrame`, `computerTakeover`, `computerRelease`. Golden
`REQ-026-computer-panel.json` exists to extend. Keep physical frame coordinates unmirrored.

**(b) Publication / lineup approval card — thread `/app/:botId` pending card (REQ-007/017/018).**
Make "what will happen" dominant: exact competition, fixture and effect ("Publish starting XI for
<fixture>", "Post final score <score> to <channel>", "Approve transfer note <amount>"). Confirm gets an
explicit label, not "Approve". A public publish or transfer note pairs `border-border-error-default` +
`text-text-error-primary` with a warning icon and the words "High impact" — never colour alone. Keep the
`threadsAnswer` double-answer guard. Components: BoardUI `Button`, `Badge`/`Chip`, `Tooltip`; card
`bg-background-primary-default rounded-3xl border-border-button-default`, type `text-body-medium` /
`text-caption-1-semibold`. Transcript stays readable, other threads reachable, focus lands on the card;
announce once through the polite live region (spec §8.9) — the transcript is not a live region. RPC
(CONFIRMED): `threadsAnswer`. Golden `REQ-007-pending-card.json` exists to extend.

**(c) Scouting-report & clip composer — `/app/:botId` (REQ-001/REQ-008).** Extend the composer with an
attach control for a match clip or stats export (prefer the installed `file-upload` over a hand-built
input) and reuse the reply/mention-chip pattern; keep Enter-to-send / Shift+Enter-newline and send
disabled while `awaitingApproval`. Components: BoardUI `IconButton`, `Tooltip`; `RiAttachment2Line` as a
component reference via `@remixicon/react`; token-driven focus, `.boma-target`, icon
`text-foreground-icon-primary`. RPC (CONFIRMED): `threadsSend`; the reference enters the same `value`
state, so no new transport. Fixture ids, jersey numbers and scores stay LTR in `<bdi>`.

**(d) Match-run triage — `/app` Activity view (REQ-011/012/064/067).** Group feed-ingest, video-processing, report and
social-draft runs by day with prompt snippet, bot name, a named status chip, duration and a link to the starting message;
working runs sort to the top and update in place; failures get one plain cause and a retry, never a decorative spinner;
skeletons while loading — a blank panel is a bug. Components: installed `table`/`chip`, `Button`, `IconButton`, `Tooltip`;
surfaces `bg-background-primary-default`/`bg-background-secondary-default`, hairline `border-separator-border`; use the
exact §8.9 vocabulary (`queued`, `leased`, `running`, `waiting_input`, `waiting_takeover`, `completed`, `failed`,
`cancelled`) as text + icon. RPCs (CONFIRMED): `runsList`, `runsRetry`, `runsPause`, `runsResume`, `threadsHead`,
`notificationsStatus`.

**(e) Versioned report viewer & public match page — `/app/artifacts/:artifactId` and `/p/:slugToken`
(REQ-036/037/039), grounded in `artifact-viewer.tsx`.** Keep the version selector, bounded image/PDF frame, `<pre>` for
text, download/copy, "open in thread" and confirm-delete with a `role="alert"` error; reports and clips stay untrusted
bytes, never markup. For the signed-out match report, keep no session (`credentials: "omit"`), no chrome beyond a small
robot-cat footer, `noindex`/`no-referrer`, and **one** "not available" document for unknown, revoked and expired tokens —
same words, shape and timing. Tokens: `bg-background-full`, `border-border-button-default`/`border-border-table`,
`text-text-primary`/`text-text-secondary`. RPCs (CONFIRMED): `artifactsGet`, `artifactsListVersions`, `artifactsRemove`,
`artifactsListSpace`; transport (CONFIRMED): `GET /api/v1/pages/shared/{token}`. No upstream wording is reused.

## 3. Accessibility

WCAG 2.2 AA: zero critical/serious axe findings in light, dark and RTL; a keyboard-only path for every primary flow
(takeover/release, publish decision, attach clip, retry/pause, board-free triage, version select); visible
`focus-visible:ring-2 focus-visible:ring-border-focus-ring`; ≥44 px `.boma-target`; 360 px width; `prefers-reduced-motion`
honoured; no animation on streaming content or the live frame. High-impact markers, holder state and run status are text
+ icon, never colour alone. The live frame has meaningful alt; report bytes are never rendered as HTML. The uniform
unavailable page is announced in identical text — no accessibility-tree leak. Error copy is one sentence of what happened
plus one of what to do; no fixture id, athlete name in a bare chip, stack trace, provider text or secret on screen
(correlation id behind "details" only). Tables carry `<caption>`/`<th scope>`.

## 4. i18n and RTL

English source catalogue, fr/he translations; a missing key falls back to English — never a blank or raw key.
`DirectionProvider` (`@/components/foundations/direction/direction`) gets an explicit BCP 47 locale; the boot script writes
`lang`/`dir` together. RTL mirrors the rail, panel, board columns, takeover glyphs and drawer motion via `useDirection()`;
navigation and disclosure glyphs flip. Logical utilities throughout (`ms`/`me`, `ps`/`pe`, `text-start`/`text-end`,
`border-s`/`border-e`). Jersey numbers, scores, fixture refs, club codes, URLs and match times stay LTR in `<bdi>`/
`dir="auto"`. Preserve physical coordinates for the spectated frame and any chart — never mirror them. Formatting data is
separate from layout direction.

## 5. Playwright tests (REQ id in each name)

- `test("REQ-026 the spectate panel renders starting, running and stopped states with a bounded frame", ...)`
- `test("REQ-029 takeover and release name the holder and are keyboard-operable", ...)`
- `test("REQ-007 the publish card names competition, fixture and effect and takes focus without blocking reading", ...)`
- `test("REQ-017 a public lineup post and its denial answer the card exactly once with an attributable confirmation", ...)`
- `test("REQ-001 the clip-aware composer sends one optimistic message that is not duplicated", ...)`
- `test("REQ-011 the activity view groups feed-ingest and report runs by day and links to the source", ...)`
- `test("REQ-012 every match-run status renders a distinct named text-and-icon chip", ...)`
- `test("REQ-067 pause and resume a queued social-draft run in place with the right terminal action", ...)`
- `test("REQ-036 a scouting report shows its version chain and renders untrusted bytes as text or a bounded frame", ...)`
- `test("REQ-037 unknown, revoked and expired tokens render the identical unavailable document", ...)`
- `test("REQ-057 Hebrew RTL mirrors the rail and panel, keeps scores LTR, and axe reports zero critical or serious findings", ...)`

Behaviour without a REQ yet (high-impact marker, clip chip, day grouping) needs an owner-assigned id before
code — never invent one. Goldens under `spec/golden/ui/` stay byte-exact; a new state gets a new
`REQ-<id>-<slug>.json`.

## 6. Best first slice

**Change (a), the live feed spectate & takeover panel** — GUESS, recommended. Smallest, highest-value step: one panel,
no new transport (the five `computer*` RPCs exist), no new component (`Button`/`Badge` installed), no token re-tune, and
an existing golden (`REQ-026-computer-panel.json`) to extend. It is the sports differentiator — a human clearing a
stats-portal CAPTCHA or ticketing login at match speed — and it exercises the "holder always named" and "no colour-only
status" gates. Test-first: write `REQ-029 takeover and release name the holder and are keyboard-operable`, extend the
REQ-026 golden, then ship only the named holder line, explicit Take over / Release labels and the `role="status"` state.
Defer (b)–(e) until REQ ids are assigned and scope is approved; (b) is the next safety priority.

**Decision needed:** confirm the self-hosted club/league framing, and whether a "publish result" RPC exists in the backend
before any publish UI is scheduled — no such procedure is in `rpc.ts` today.

Provenance: REQ ids from `spec/01-requirements.md`; RPC names from `apps/web/lib/api/rpc.ts`; golden names from
`spec/golden/ui/`; component structure from `computer-panel.tsx` and `artifact-viewer.tsx`; design rules from `docs/UI.md`
and `spec/08-ui-ux.md` — all CONFIRMED. Persona, jobs, five changes and first slice are a GUESS (proposal only — no sports
sector exists in the spec). No product code was modified.
