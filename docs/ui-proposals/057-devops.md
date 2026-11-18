# UI proposal 057 — devops (operator of a self-hosted Boma Bot instance)

Status: PROPOSAL (GUESS where the spec is silent). Sector specialist #057 — devops. Read-only on
product code; this file is the only write. Grounding (CONFIRMED, read this session): `docs/UI.md`,
`spec/08-ui-ux.md`, `spec/01-requirements.md` (REQ ids), `apps/web/lib/api/rpc.ts` (RPC names), and
two components: `apps/web/components/activity/activity-screen.tsx` (run rows, status vocabulary,
retry/pause/resume) and `apps/web/components/admin/plugins-screen.tsx` (submission queue, source as
text, no-code-on-approve). No upstream brand, code, assets, wording or reference capture consulted.

Charter (CONFIRMED): Boma Bot only, fresh design (ADR-015), owner-supplied robot-cat as the sole
identity; no upstream brand/code/assets/copy; BoardUI semantic tokens only — no raw palette class or
hex literal, every class list merged with `cx()`; one design system, no second library or hand-built
lookalike; self-hosted on the operator's own machine — no SaaS/cloud, no run, log, secret or telemetry
leaving it; product name and mark unchanged.

## 1. Persona and top jobs-to-be-done

**Persona — "the operator"**: an SRE/platform lead running Boma Bot on infrastructure they own and
answer for, working in English, French and Arabic across a dashboard and an on-call handheld. They
are measured on the health and safety of the deployment, not on features: an ambiguous run state, an
unreviewed plugin or an access grant with no reason is worse than a slow screen. They read status out
loud before they act, and never decide on a colour cue alone. The robot-cat mark is the only identity
art; no vendor, cloud or platform logo is borrowed.

Top jobs:
1. **Watch fleet health** — see every run by name and status, retry only failures, pause and resume
   queued work, with a link back to the message that started it.
2. **Grant access safely** — review members, invitations and grants with their reasons; role changes
   roll back on failure and removals state exactly what the person loses.
3. **Review plugins without executing them** — read the submitted source as text, see the parts the
   installer refused and why, and approve knowing no plugin code runs on approval.
4. **Take control of a computer** — watch a bot's screen, take over and release it with named states,
   and confirm high-impact infra actions against a one-time capability that is never echoed.
5. **Tune reach and bring-up** — set per-bot push policy and daily caps, manage sealed webhook
   destinations, read partial-install reports, and confirm the local-first owner mode on loopback.

## 2. Five concrete UI changes

All new copy is a new `lib/i18n.ts` key via `useTranslation` / `t(locale, key)`; nothing hardcoded in
JSX. Every class list goes through `cx()`; every colour rides a `docs/UI.md` token. Operator-, bot- and
plugin-supplied values are untrusted text/bytes, never HTML.

**(a) Run-health console — `/app` Activity view (REQ-011/REQ-012/REQ-067).** Grounded in
`activity-screen.tsx`: group runs by day with prompt snippet, bot name, a named status chip, duration
and a link to the source; working runs sort to the top and update in place; only `failed` offers
retry, `queued`/`paused` offer pause/resume (REQ-067). Chips carry the exact §8.9 vocabulary as text +
icon, never colour, and a `role="status"` line; skeletons replace the list while loading — no blank
panel, no decorative spinner. Components: installed `table`/`chip`, `Button`, `IconButton`, `Tooltip`;
tokens `bg-background-primary-default`, `border-border-button-default`,
`text-text-primary`/`text-text-secondary`, focus `focus-visible:ring-2 focus-visible:ring-border-focus-ring`.
RPCs (CONFIRMED): `runsList`, `runsRetry`, `runsPause`, `runsResume`.

**(b) Safe plugin-review queue — `/admin/plugins` (REQ-058/REQ-034).** Grounded in `plugins-screen.tsx`:
keep `source` as text in `<code dir="auto">`, list the parts the installer refused with a reason, and
give approve/reject explicit labels ("Approve for install — no code runs here", not a bare "Approve").
Destructive or high-impact rows pair `border-border-error-default` + `text-text-error-primary` with an
icon and the word "Review first" — never colour alone. Keep the single-decision guard honest.
Components: BoardUI `Button` (primary/danger), `Badge`/`Chip`, `Tooltip`, `table`; tokens
`bg-background-primary-default`, `border-separator-border`, `text-text-tertiary`. RPCs (CONFIRMED):
`adminPluginsList`, `adminPluginsDecide`; `marketplaceList` for skipped-part detail. Approving changes
review state only — it never executes the submission.

**(c) Access and grants console — `/admin/access` (REQ-058).** Members table (name, email, role, last
seen), pending invitations and access grants with reasons. Role changes are optimistic with rollback;
removing a member states what they lose. Prefer the installed BoardUI `Input`/`Select` over raw
fields; expose revoke and grant-create with explicit, named confirm labels. Components: `table`,
`Chip`, `IconButton`, `Tooltip`, `Select`; tokens `border-border-table`, `bg-background-primary-default`,
`text-text-primary`/`text-text-secondary`. RPCs (CONFIRMED): `adminMembersList`, `adminInvitationsList`,
`adminInvitationsRevoke`, `adminGrantsList`, `adminGrantsCreate`, `adminGrantsRevoke`, `spacesInvite`.
Emails, grants and reasons never reach logs; a non-admin sees the plain admin-only screen.

**(d) Computer takeover and secrets approval — thread right panel `/app/:botId` (REQ-026/028/029/REQ-060).**
Reuse the installed computer panel: name each state starting / running / taken over / released with a
chip, a banner over the transcript on takeover, and explicit "Take control" / "Release control"
buttons with focus on the panel; announce once through the polite live region (spec §8.9), never make
the transcript a live region. High-impact approval cards name the exact effect and use error token +
icon + words, never colour. Components: `drawer`/`spinner`/`skeleton`, `Button`, `Badge`, `Tooltip`;
tokens `bg-background-secondary-default`, `border-separator-border`,
`text-foreground-icon-primary`. RPCs (CONFIRMED): `computerStatus`, `computerTakeover`,
`computerRelease`, `computerOpenScreen` (one-time capability, `credentials: "omit"`, never displayed
or logged), `threadsAnswer` for the approval.

**(e) Reach and bring-up panel — `/app` settings and `/integrations/setup` (REQ-064/REQ-066/REQ-034).**
Reuse `notifications-panel`/`integrations-setup`: per-bot policy switches (waiting/completed, daily
cap), destination list where the webhook is sealed and never echoed, a partial-install report rather
than a green check, and a local-first owner strip ("this machine is the trust anchor; no sign-in on
loopback") driven by `authCapabilities.localFirst`. Components: BoardUI `Switch`, `Input`, `Select`,
`Badge`/`Chip`, `Tooltip`; tokens `bg-background-primary-default`, `border-border-button-default`,
`text-text-primary`/`text-text-secondary`, `text-text-error-primary` for skipped parts with text+icon.
RPCs (CONFIRMED): `notificationsStatus`, `notificationsSubscribe`, `notificationsPolicyGet`,
`notificationsPolicySet`, `notificationsDestinationsList`, `notificationsDestinationsCreate`,
`notificationsDestinationsDelete`, `authCapabilities`, `marketplaceList`.

## 3. Accessibility

WCAG 2.2 AA gate: zero critical/serious axe findings in light, dark and RTL; a keyboard-only path for
every primary flow (pause/resume/retry, approve/reject, grant/revoke, takeover/release, policy toggles);
visible `focus-visible:ring-2 focus-visible:ring-border-focus-ring`; ≥44 px `.boma-target`; 360 px width;
`prefers-reduced-motion` honoured; no animation on streaming content. Run state, review state and impact
are text + icon, never colour alone (spec §8.9). Tables carry `<caption>`/`<th scope>`. Error copy is
one sentence of what happened plus one of what to do; no run id, webhook, host, stack trace or provider
text on screen (correlation id behind "details" only). The uniform unavailable screen is announced in
identical text.

## 4. i18n and RTL

English is the source catalogue, fr/he the translations; a missing key falls back to English — never a
blank or the raw key. `DirectionProvider` (`@/components/foundations/direction/direction`) gets an
explicit BCP 47 locale; the boot script writes `lang`/`dir` together. RTL mirrors the rail, admin
tables and drawer motion via `useDirection()`; navigation glyphs flip. Logical utilities throughout
(`ms`/`me`, `ps`/`pe`, `text-start`/`text-end`, `border-s`/`border-e`). Run ids, webhooks, emails,
hostnames, URLs and timestamps stay LTR in `<bdi>`/`dir="auto"`. Preserve physical coordinates for the
screen stream — a remote desktop is never mirrored. Formatting application data is separate from layout
direction.

## 5. Playwright tests (REQ id in each name)

- `REQ-011 the activity view groups runs by day and links to the source message`
- `REQ-012 every run status renders a distinct named text-and-icon chip`
- `REQ-067 a queued run can be paused and resumed from the activity list`
- `REQ-058 the plugin queue shows source as text and refused parts with a reason`
- `REQ-058 the plugin queue approves one submission and refuses the second decision`
- `REQ-058 the access screen renders members, invitations and grants with reasons`
- `REQ-058 a plain member is told the page is admin-only`
- `REQ-034 a partial install lists the refused parts and approving never runs code`
- `REQ-026 the computer panel states starting, running, taken over and released by name`
- `REQ-029 takeover and release announce through the polite live region`
- `REQ-064 a per-bot policy caps pushes and destination webhooks are never echoed`
- `REQ-066 local-first owner mode shows no sign-in on a loopback bind`
- `REQ-057 Hebrew RTL mirrors the admin tables and axe reports zero critical or serious findings`

Behaviour without a REQ yet (run-health strip, local-first owner strip) must get an owner-assigned id
before code — do not invent one in a test name. Goldens under `spec/golden/ui/` stay byte-exact.

## 6. Best first slice

**Change (b), the safe plugin-review queue** — GUESS, recommended. Smallest, highest-value step: one
screen, no new transport (`adminPluginsList`/`adminPluginsDecide` exist), no new component (BoardUI
`Button`/`Badge`/`Tooltip`/`table` installed), no token re-tune, and the queue already lives in
`plugins-screen.tsx`. It directly prevents approving untrusted plugin code on a colour cue or without
seeing what was refused. Test-first: extend `REQ-058 the plugin queue approves one submission and
refuses the second decision` with a check that source is text and refused parts carry a reason, then
ship only the explicit "no code runs here" confirm label and the text+icon review marker. Defer (a),
(c), (d), (e) until the owner scopes them.
**Decision needed from the owner**: confirm the operator/devops framing and assign REQ ids for the
run-health and local-first strips. Provenance: REQ ids verified against `spec/01-requirements.md`; RPC names read from
`apps/web/lib/api/rpc.ts`; component structure read from `activity-screen.tsx` and `plugins-screen.tsx`;
design rules from `docs/UI.md` and `spec/08-ui-ux.md` — CONFIRMED. Persona/jobs are a GUESS (no devops
sector exists in the spec). No product code was modified.
