# UI proposal 070 — security services (guarding, patrols, alarm monitoring)

Status: PROPOSAL (sector framing and persona are GUESS — a sector lens, not in `spec/`). Sector
specialist #070 — security services. Read-only on product code; this file is the only write.

Grounding (CONFIRMED, read this session): `docs/UI.md`, `spec/08-ui-ux.md`, `spec/01-requirements.md`
(REQ ids), `apps/web/lib/api/rpc.ts` (procedures), `apps/web/lib/use-translation.ts` (`t()` via
`useLocale()`), and two components: `apps/web/components/thread/computer-panel.tsx` and
`apps/web/components/artifacts/artifacts-grid.tsx`. No upstream brand, code, asset, wording or capture
was consulted or copied; this is distinct from #058 (cybersecurity = least privilege / non-disclosure).

Charter (CONFIRMED): Boma Bot only — no upstream mark, copy or asset; the owner-supplied robot-cat is
the sole identity; BoardUI semantic tokens only — no raw palette class or hex literal — composite type
and class lists merged with `cx()`; one design system, no second library or lookalike; self-hosted only
— no SaaS/cloud, no patrol log, footage, incident report or guard PII leaving the premises.

## 1. Persona and persona jobs

**Persona — "the duty supervisor"**: the officer running a security-services operations desk who
dispatches patrol bots, watches live alarm/CCTV feeds and signs off incident reports. Desktop-first in
a control room, working in English/French/Arabic, she is measured on *a legible, attributed trail*:
who took the feed, who authorised a response, what the report says. A guard dispatched on a colour
cue, a feed held anonymously or a report edited without a version is an incident. The robot-cat mark
is the only identity art; no vendor, regulator or security-brand logo is borrowed.

Jobs:
1. **Triage alarms** — see which incident runs are queued, working, awaiting a human or failed, and
   pick the next; tell stalled from progressing without reading every thread.
2. **Authorise a response** — confirm a bot-proposed dispatch or escalation knowing the exact site,
   asset and effect; never approve on colour alone.
3. **Watch and take over** — observe a live monitoring feed, take exclusive input for a manual step
   and hand it back with an attributable record.
4. **Keep report evidence** — file, version and retrieve incident reports and attachments, opening
   each in the thread where it was produced; never render report HTML in the app.
5. **Route alerts** — send content-free waiting/failed alerts to guards via push and space-level
   destinations, and publish a redacted incident summary by link without giving every reader a login.

## 2. Five concrete UI changes

All new copy is a key in `lib/i18n.ts` read through `useTranslation()` (`apps/web/lib/use-translation.ts`);
nothing is hardcoded in JSX. Every colour rides the `docs/UI.md` token map, type uses composite
utilities, class lists go through `cx()`, icons come from `@remixicon/react` as component references.

**(a) Alarm/dispatch triage — `/app` Activity view (`REQ-011`, `REQ-012`, `REQ-021`, `REQ-067`).**
Extend the run list so each row names its state out loud plus a short site caption (`queued` →
"alarm queued", `running` → "patrol active", `waiting_input` → "awaiting officer", `failed` →
"dispatch failed"). Keep the exact §8.9 vocabulary; the caption is secondary text. Working/paused
rows keep Pause/Resume, failed rows keep Retry; working runs sort to the top and update in place —
no decorative spinner. Components: BoardUI `Button`, `role="status"`; surfaces
`bg-background-primary-default`/`bg-background-secondary-default`, `border-separator-border`, error
row `border-border-error-default` + `text-text-error-primary` + warning icon. RPCs (CONFIRMED):
`runsList`, `runsRetry`, `runsPause`, `runsResume`, `threadsHead`.

**(b) Response-authorisation card — thread `/app/:botId` (`REQ-007`, `REQ-017`, `REQ-018`).**
Make the "what will happen" sentence dominant: exact site, asset and effect, with an explicit
confirm label ("Dispatch patrol to Gate 3", not just "Approve"). High-impact or over-limit responses
pair `border-border-error-default` + `text-text-error-primary` with a warning icon and the words
"High impact" — never colour alone. BoardUI `card`/`button`/`badge`, surface
`bg-background-primary-default`, `rounded-3xl`, `border-border-button-default`, focus
`focus-visible:ring-2 focus-visible:ring-border-focus-ring`. RPC (CONFIRMED): `threadsAnswer`
(exactly-once effect, keeps the `CONFLICT` guard); announce once via the polite live region — the
transcript stays readable and other threads reachable (§8.9).

**(c) Monitoring feed and attributable takeover — `/app/:botId` right panel (`REQ-026`, `REQ-028`,
`REQ-029`, `REQ-030`).** Extend the read component `computer-panel.tsx`: give each state a name
(starting, running, awaiting takeover, released) plus a caption "Leaving this screen pauses nothing
— the handoff record stays open." Takeover and release stay explicit and attributable. The file
already uses BoardUI `Button`, `role="status"`, `role="alert"`; tokens `bg-background-primary-default`,
`border-border-button-default`, `text-text-error-primary` + icon on errors, `ms-auto`,
`text-body-2-semibold`. RPCs (CONFIRMED): `computerStatus`, `computerOpenScreen`,
`computerOpenScreenFrame`, `computerTakeover`, `computerRelease`. Mirror drawer motion in RTL;
leave physical feed coordinates untouched; no guard or site data in logs.

**(d) Incident/evidence report library — `/app/artifacts` (`REQ-036`).** From the read
`artifacts-grid.tsx`: versioned incident reports and attachments, newest first, filter by type and
owner bot, search by name; each card shows version, id, size, owner and date. CONFIRMED gap: the
search `<input>` and two `<select>` are raw — replace with installed BoardUI `Input`/`Select` and
keep every string on `t()`. Report content is untrusted: never inject artifact HTML; keep the
version selector and download in the viewer. Tokens already used: `bg-background-primary-default`,
`border-border-button-default`, `text-text-tertiary`, `text-text-error-primary`. RPCs (CONFIRMED):
`artifactsListSpace`, `artifactsGet`, `artifactsListVersions`.

**(e) Guard alert routing and shared incident summary — panel + `/p/:slugToken` (`REQ-064`,
`REQ-065`, `REQ-037`).** Keep per-bot waiting/failed push policy and space-level Slack/Teams
destinations, but replace raw controls with installed BoardUI `Checkbox`/`Select`/`Input`; pushes
stay content-free against the per-(space, member, bot) cap. RPCs (CONFIRMED): `notificationsStatus`,
`notificationsSubscribe`, `notificationsUnsubscribe`, `notificationsPolicyGet`,
`notificationsPolicySet`, `notificationsDestinationsList/Create/Delete` (Owners/Admins only). A
compiled incident summary can be published with no chrome beyond a robot-cat footer; unknown,
revoked and expired tokens render one identical "not available" document — same words, shape and
timing. Transport: `GET /api/v1/pages/shared/{token}` with `credentials: "omit"`; `noindex`/
`no-referrer` and the CSP stay backend concerns.

## 3. Accessibility, i18n and RTL

WCAG 2.2 AA gate: zero critical/serious axe findings in light, dark and RTL; 44 px `.boma-target`;
token-driven focus `focus-visible:ring-2 focus-visible:ring-border-focus-ring`; a keyboard path for
every decision (triage, authorise, takeover/release, version select, destination add/remove); 360 px
width; `prefers-reduced-motion` honoured; no animation on streaming content.
Run status and high-impact markers are text + icon, never colour alone. Takeover attribution is
announced once via the polite live region; the transcript is not a live region. Error copy is one
sentence of what happened plus one of what to do — no site detail, guard PII or stack trace (correlation id behind "details").

English is the source catalogue, fr/he translations, a missing key falls back to English — never a
blank or the raw key. `DirectionProvider` (`@/components/foundations/direction/direction`) takes an
explicit BCP 47 locale; the boot script writes `lang` and `dir` together. Logical utilities
(`ms`/`me`, `ps`/`pe`, `text-start`/`text-end`, `border-s`/`border-e`) and `useDirection()` mirror the
triage rows, cards and drawer motion for `he` (REQ-057) while preserving physical feed geometry.
Emails, URLs, hashes, ids and timestamps stay LTR in `<bdi>`/`dir="auto"`; money uses `Intl`.

## 4. Playwright tests (REQ id in each name)

```
test("REQ-011 the activity view groups alarm runs by day and links back to the thread", ...)
test("REQ-012 every run status renders as text plus icon in light, dark and RTL", ...)
test("REQ-021 a failed dispatch shows a plain cause and a working retry action", ...)
test("REQ-007 a response card names site, asset and effect and does not block reading", ...)
test("REQ-017 a high-impact response answers once and executes its effect once", ...)
test("REQ-026 the monitoring panel names starting, running, takeover and released states", ...)
test("REQ-028 observe the feed through the existing computer RPCs with no guard data in logs", ...)
test("REQ-029 takeover and release announce an attributable status and let the bot resume", ...)
test("REQ-036 the report library selects a version, opens its thread and never injects HTML", ...)
test("REQ-064 a waiting response produces a content-free alert that opens the thread", ...)
test("REQ-037 the shared summary renders without chrome; a bad token shows the uniform screen", ...)
test("REQ-057 Hebrew RTL mirrors the triage rows and axe reports zero critical or serious findings", ...)
```

Behaviour without a REQ yet (site caption, high-impact marker, handoff caption) must be assigned a REQ
id by the owner before code — do not invent one in a test name. `spec/golden/ui/` goldens stay byte-exact.

## 5. Best first slice

**Change (c), the monitoring feed and attributable takeover** — GUESS, recommended. The read
component already carries all five landed `computer.*` RPCs, the tokens and the `t()` keys; no new
transport, permission model, data model, token re-tune or component install is needed. One contained
change: name each state, add the "pauses nothing" caption, and keep the takeover/release attribution
and polite announcement — all inside REQ-026/028/029. Test-first: write `REQ-029 takeover and release
announce an attributable status and let the bot resume` before code, then extend the required goldens.
Defer (a), (b), (d), (e) until the owner assigns the missing REQ ids and approves scope; (d) also needs
the confirmed raw `<input>`/`<select>` swap.

**Decision needed from the owner**: confirm the guarding/monitoring framing (distinct from #058) and
which REQ ids, if any, cover the new site captions and high-impact marker.

Provenance: REQ ids/RPC names CONFIRMED from `spec/01-requirements.md` and `apps/web/lib/api/rpc.ts`;
component structure CONFIRMED by reading `computer-panel.tsx` and `artifacts-grid.tsx` (attributable
takeover via RPCs; raw search/filter controls in the grid); design rules CONFIRMED from `docs/UI.md`
and `spec/08-ui-ux.md`. Persona, jobs, sector mapping and slice ranking are GUESS (no security-services
sector or REQ in the spec). No upstream asset used and no product code modified.
