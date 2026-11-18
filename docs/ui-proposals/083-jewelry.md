# UI proposal 083 — jewelry (commissions, valuations and high-value sign-off)

Status: PROPOSAL (GUESS where the spec is silent). Sector specialist #083 — jewelry. Read-only on
product code; this file is the only write. Grounding (CONFIRMED, this session): `docs/UI.md`,
`spec/08-ui-ux.md`, `spec/01-requirements.md` (REQ ids), `apps/web/lib/api/rpc.ts` (RPCs) and two
components — `thread/thread-composer.tsx` (attachment/composer, Enter-to-send, send waits on approval)
and `artifacts/artifacts-grid.tsx` (filters, search, bounded cards, untrusted bytes). No upstream
brand, code, assets or wording consulted or copied.

Charter (CONFIRMED): Boma Bot only, fresh design (ADR-015), the owner-supplied robot-cat as sole
identity; no upstream brand/code/assets/copy; BoardUI semantic tokens only — no raw palette class or
hex literal, every class list merged with `cx()`; one design system, no second library or hand-built
lookalike; self-hosted on the jeweller's own machine — no SaaS/cloud, and no design, gemstone
certificate, valuation, client name or quote leaving the premises. No gemstone, hallmark, atelier or
assay-office logo is borrowed as identity art.

## 1. Persona and top jobs-to-be-done

**Persona — "the atelier manager"**: owner/bench lead at an independent jewellery atelier, running
Boma Bot on the shop's own machine. She directs bots to draft bespoke pieces, document stones, write
valuations and prepare insurance certificates, reviewing on a bench display and a phone between client
appointments, in English, French, Arabic and Hebrew. Designs, carat figures, valuations and client names
never leave the shop. She is measured on judgment: releasing the wrong design version, quoting the wrong
stone or approving a high-value commission on a colour cue is an irreversible loss, so ambiguity is
worse than latency. The robot-cat mark is the sole identity art; the piece's photographs are user
content, never brand art.

Top jobs:
1. **Approve a commission** — know client, piece, metal, gemstone, price and certificate before sign-off.
2. **Document the stone** — attach a certificate/photo as a bounded image block with real alt text on a
   poor shop network, keeping provenance and valuation attached to the piece.
3. **Direct from the bench** — take over a bot's CAD/design session to adjust a setting, then release it.
4. **Name the work** — state every design, valuation and repair run out loud (§8.9); retry only failures.
5. **Register and share** — keep design → valuation → certificate versions; publish a client
   certificate by link with no login, never leaking another client's piece.

## 2. Five concrete UI changes

All new copy is a `lib/i18n.ts` key via `useTranslation()` → `t(locale, key)` (English source; fr/he
translations; missing key falls back to English); no string in JSX. Every class list goes through
`cx()`; every colour rides the `docs/UI.md` token map. Bot-produced bytes are untrusted text or
images, never markup; keep client names, carat figures and quote ids out of logs.

**(a) High-value commission sign-off card — thread `/app/:botId` (REQ-007, REQ-017).** Extend the
`CardBlock` path: make the "what will happen" sentence dominant — client, piece, metal, gemstone,
quoted price, certificate — with an explicit label ("Release the sapphire ring quote to <client>",
not just "Approve"). An irreversible commission pairs `border-border-error-default` +
`text-text-error-primary` with a leading icon and "High value" — never colour alone. Replace raw
buttons with BoardUI `Button`, plus `Badge`/`Tooltip`; card `bg-background-primary-default`,
`rounded-3xl`, `border-border-button-default`, text `text-text-primary`/`text-text-secondary`, focus
`focus-visible:ring-2 focus-visible:ring-border-focus-ring`, 44 px `.boma-target`. Existing RPCs:
`threadsAnswer` (the `CONFLICT` guard owns the double answer), `threadsReact`. Transcript stays
readable, other threads reachable, focus lands on the card; announce once via the polite live region
(§8.9), the transcript is not a live region; send waits while approval is pending (`awaitingApproval`).

**(b) Certificate-and-stone composer — thread composer and transcript (REQ-001, REQ-002, REQ-008,
REQ-036).** Extend `ThreadComposer` with an "attach certificate" `IconButton` (`RiImageLine`) and a
named file `Chip`; keep `@bot` mention, reply chip, Enter-to-send / Shift+Enter-newline, stop while a
run is active, send-disabled-on-empty. Render image blocks in the viewer's bounded frame
(`max-h-[60vh] max-w-full object-contain`, real `alt` from the certificate/stone name) — never wider
than the panel. BoardUI `TextArea`/`Button`/`IconButton`/`Chip`; surface
`bg-background-primary-default`, border `border-border-button-default`, icons
`text-foreground-icon-primary`, token focus ring. Existing RPCs: `threadsSend`, `threadsMessages`,
`artifactsGet`. Bytes are untrusted: blob/data URL only, never HTML; strip EXIF/GPS and never expose a
client address in DOM, `alt` or logs.

**(c) Bench/job board — `/app/board` (REQ-038, feature-flagged).** Reuse `BoardScreen`: columns
received → design approved → at bench → polished → valued/delivered, with ticket key, title, assignee
bot and last comment; a detail panel shows attached bot runs with named status chips (text + icon,
never colour alone). Prefer installed BoardUI `Input`/`Select`/`Chip` over raw fields, merged with
`cx()`; tokens `bg-background-primary-default`, `border-border-button-default`,
`text-text-primary`/`text-text-secondary`. Existing RPCs: `boardsGet`, `botsList`, `ticketsCreate`,
`ticketsMove`, `ticketsComment`. A 404 keeps the route absent, not locked (§8.6).

**(d) Valuation and repair run triage — `/app` Activity view (REQ-011, REQ-012, REQ-021).** Follow
`ActivityScreen`: group design, valuation and repair runs by day with prompt snippet, bot name and a
named status chip (queued/leased/running, waiting_input/waiting_takeover, completed/failed/cancelled
— §8.9); retry only `failed`, pause/resume queued work. BoardUI `Button`/`Chip`/`Tooltip`; error row
`border-border-error-default` + `text-text-error-primary` + `text-foreground-icon-primary`; surfaces
`bg-background-primary-default`/`bg-background-secondary-default`, hairline `border-separator-border`.
Existing RPCs: `runsList`, `runsRetry`, `runsPause`, `runsResume`, `threadsHead`. Working runs sort to
the top and update in place; a failure gets one sentence of cause plus one of what to do, correlation
id behind "details" — never a stack trace or provider text.

**(e) Valuation library and shared client certificate — `/app/artifacts` + `/p/:slugToken`
(REQ-036, REQ-037).** Reuse `ArtifactsGrid` as a versioned register: name certificates by piece,
filter by type/bot, search by name, show the design → valuation → certificate chain and "open in
thread"; replace raw `<select>`/`<input>` with installed BoardUI `Select`/`Input` to honour the
one-system rule, keep the bounded frame and untrusted-byte rules. Existing RPCs: `artifactsListSpace`,
`artifactsGet`, `artifactsListVersions`, `artifactsRemove`. Publish a client certificate with no
session and no chrome beyond a small robot-cat footer; unknown, revoked and expired tokens render one
identical "not available" screen — same words, shape and timing. `noindex`/`no-referrer` and its CSP
stay backend concerns; transport (CONFIRMED): `GET /api/v1/pages/shared/{token}`. Keep the robot-cat
empty state when nothing is filed.

## 3. Accessibility

WCAG 2.2 AA gate: zero critical/serious axe findings in light, dark and RTL; keyboard-only path for
every primary flow (attach, sign-off, board moves, takeover/release, retry/pause, filters, version
select); `focus-visible:ring-2 focus-visible:ring-border-focus-ring`; ≥44 px `.boma-target`; 360 px;
`prefers-reduced-motion`; no animation on streaming. Stone images carry meaningful `alt`, run status
uses the §8.9 vocabulary, high-value impact is text + icon not colour. Tables carry `<caption>` and
`<th scope>`. Focus lands on a pending card; the uniform unavailable screen is announced identically —
leak through the a11y tree. Error copy: one sentence of what happened, one of what to do — no carat,
valuation, client name, EXIF or stack trace (correlation id behind "details"); robot-cat unchanged.

## 4. i18n and RTL

English source, fr/he translations; a missing key falls back to English, never a blank or raw key.
`DirectionProvider` (`@/components/foundations/direction/direction`) gets an explicit BCP 47 locale;
the boot script writes `lang`/`dir` together. RTL mirrors rail, board columns and drawer motion via
`useDirection()`; arrows flip. Logical utilities (`ms`/`me`, `ps`/`pe`, `text-start`/`text-end`,
`border-s`/`border-e`). Carats, certificate/hallmark codes, prices, dates, URLs and OTP digits stay
LTR in `<bdi>`/`dir="auto"` via `Intl`; a stone's measured geometry and frame are never mirrored.

## 5. Playwright tests (REQ id in each name)

- `test("REQ-001 attaching a certificate sends one optimistic message that is not duplicated", ...)`
- `test("REQ-036 a stone image renders in a bounded frame with alt text and never as HTML", ...)`
- `test("REQ-007 the sign-off card names client, piece, metal, gemstone, price, certificate and takes focus", ...)`
- `test("REQ-017 approving a high-value commission answers once with an attributable confirmation", ...)`
- `test("REQ-012 every run status renders a distinct text-plus-icon chip with the right terminal action", ...)`
- `test("REQ-021 a failed valuation shows a plain-language cause and a working retry", ...)`
- `test("REQ-038 the bench board moves a job by keyboard and is absent when the flag returns 404", ...)`
- `test("REQ-036 the register filters by type and bot and shows the design-to-certificate chain", ...)`
- `test("REQ-037 unknown, revoked and expired tokens render the identical unavailable document", ...)`
- `test("REQ-057 Hebrew RTL mirrors the board and axe reports zero critical or serious findings", ...)`

Goldens under `spec/golden/ui/` stay byte-exact; a new seeded state means a new
`REQ-<id>-<slug>.json`, never a click path. Behaviour without a REQ yet (high-value marker, bench
columns, version framing) needs an owner-assigned id before code — do not invent one in a test name.

## 6. Best first slice

**Change (a), the high-value commission sign-off card** — GUESS, recommended: smallest, highest-value
step, one screen, no new transport (`threadsAnswer`/`threadsReact` exist), no new component (BoardUI
`Button`/`Badge`/`Tooltip` installed), no token re-tune. It prevents a wrong design version, stone or
quote released on a colour cue. Test-first: write `REQ-007 the sign-off card names client, piece,
metal, gemstone, price, certificate and takes focus`, extend
`spec/golden/ui/REQ-007-pending-card.json`, then ship only the dominant consequence line, explicit
"Release ... to <client>" label, text+icon high-value marker, keyboard path and polite announcement.
Defer (b)–(e) until the owner assigns missing REQ ids and approves scope.

**Decision needed from the owner**: confirm the sector framing and whether REQ-038's feature flag is
in scope before (c).

Provenance: REQ ids from `spec/01-requirements.md`, RPCs from `apps/web/lib/api/rpc.ts`, the two listed
components and rules from `docs/UI.md` / `spec/08-ui-ux.md` — all CONFIRMED. Persona and jobs are a
GUESS (no jewelry sector in the spec). No product code was modified.
