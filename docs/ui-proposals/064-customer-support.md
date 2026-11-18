# UI proposal 064 — customer support (self-hosted support desk)

Status: PROPOSAL (sector framing, persona and jobs are GUESS — a sector lens, not in `spec/`).
Sector specialist #064 — customer support. Read-only on product code; this file is the only write.
Grounding (CONFIRMED, read this session): `docs/UI.md`, `spec/08-ui-ux.md`,
`spec/01-requirements.md` (REQ ids), `spec/03-api-contract.md` (procedure names), `apps/web/lib/api/rpc.ts` (client procedure names), `apps/web/tests` conventions, and two components:
`apps/web/components/thread/thread-composer.tsx` (reply chip, send semantics) and
`apps/web/components/thread/notifications-panel.tsx` (per-bot policy and destinations). No upstream
brand, code, assets, wording or reference capture consulted or copied.

Charter (CONFIRMED): Boma Bot only, fresh design (ADR-015), owner-supplied robot-cat
(`assets/brand/boma-bot-head.svg`) as sole identity; no upstream brand/code/assets/copy; BoardUI
semantic tokens only — no raw palette class or hex literal — every class list merged with `cx()`;
one design system, no second library or lookalike; self-hosted only — no SaaS/cloud, no customer
message, ticket, attachment or PII leaving the building.

## 1. Persona and top jobs-to-be-done

**Persona — "the support lead"**: an agent who runs a self-hosted Boma Bot desk where named bots
handle tickets, drafts and customer replies. She works desktop-first at the queue and mobile on
callout, answers in English/French/Hebrew, and is measured on first response and on never losing a
customer's context. A wrong handoff, a reply sent to the wrong ticket or a customer detail echoed
on screen is an incident — an ambiguous state is worse than a slow one. The robot-cat mark is the
only identity art; no vendor, helpdesk or customer logo is borrowed.

Jobs:
1. **Triage** — see which bot owns which ticket, what is working, waiting or failed, and jump to the
   exact message that started it.
2. **Reply with context** — answer inside the thread with the ticket reference, the prior message and
   an attachment, without retyping what the customer already said.
3. **Escalate safely** — hand a case to a human or a specialist bot only after reading the exact
   effect, target queue and actor; never confirm on a colour cue.
4. **Follow up** — be told when a bot waits on input or finishes, take over a stuck session, and
   release it cleanly with the holder named.
5. **Disclose nothing** — keep customer data on this machine; make an unknown or revoked shared
   summary indistinguishable in words, shape and timing.

## 2. Five concrete UI changes

All new copy is a `lib/i18n.ts` key via `useTranslation()` / `t(locale, key)`; nothing hardcoded in
JSX. Every colour rides a `docs/UI.md` token; every class list goes through `cx()`. Customer text,
ticket keys and attachment names are untrusted data, never markup.

**(a) Support triage queue — `/app` Activity view (REQ-011, REQ-012).**
Follow `ActivityScreen`: group runs by day with prompt snippet, bot name, named status chip,
duration and a link to the source message; working runs sort to the top and update in place, failures
get a plain cause and a retry, never a decorative spinner. Add a bot/status filter (client-side) and
a "needs reply" reading of the `waiting_input` state. Components: installed `table`/`chip`, `Button`,
`Tooltip`, `ScrollFade`; surfaces `bg-background-primary-default`/`bg-background-secondary-default`,
hairline `border-separator-border`, status as text + icon using the exact §8.9 vocabulary. Existing
RPCs (CONFIRMED): `runsList`, `runsPause`, `runsResume`, `runsRetry`, `botsList`, `threadsHead`.

**(b) Ticket-to-thread bridge — `/app/board` (REQ-038).**
Columns by state; a ticket card carries key, title, assignee bot and last comment. A detail panel
shows description, comments and the background runs attached with their status. Assigning a ticket
to a bot queues work; progress shows on the board, not in the chat. When the flag is off the route
is absent, not locked. Components: installed `table`, `chip`, `button`; tokens `border-border-table`,
`text-text-primary`/`text-text-tertiary` metadata, `focus-visible:ring-2 focus-visible:ring-border-focus-ring`,
`cx()`. Existing RPCs (CONFIRMED): `boardsGet`, `ticketsCreate`, `ticketsMove`, `ticketsComment`.

**(c) Context-aware reply composer — `/app/:botId` (`thread-composer.tsx`, REQ-001, REQ-009).**
Extend `ThreadComposer` so a reply keeps the quoted customer message as a chip and a ticket key can
be pasted as a reference beside it. Prefer the installed `file-upload` item over a hand-built control;
keep Enter-to-send / Shift+Enter-newline and send disabled only when empty, sending or
`awaitingApproval`. Components: BoardUI `IconButton`, `Tooltip`, `file-upload`; token-driven focus,
44 px `.boma-target`, icon `text-foreground-icon-primary`. Existing RPCs (CONFIRMED): `threadsSend`,
`threadsMessages`, `threadsHead`, `threadsMarkRead`; `artifactsListSpace`/`artifactsGet` for an
already-uploaded reference. Ticket keys and URLs stay LTR in `<bdi>`. GUESS gap: no `artifactsCreate`
client exists, so new attachment bytes are deferred until an owner assigns the id.

**(d) Escalation / approval card — thread pending card (REQ-007, REQ-017).**
Make the "what will happen" sentence dominant: exact ticket and customer, target queue and effect,
plus a distinct confirm label ("Escalate ticket CS-42 to Tier 2", not just "Approve"). A handoff to a
specialist bot names the receiving bot. High-impact actions pair `border-border-error-default` +
`text-text-error-primary` with a warning icon and the words "High impact" — never colour alone.
Components: BoardUI `card`, `button`, `badge`, `tooltip`; surface `bg-background-primary-default`,
`rounded-3xl`, `border-border-button-default`. Existing RPCs (CONFIRMED): `threadsAnswer`,
`threadsMessages`, `groupsReassign`. The effect is modal only: transcript readable, other threads
reachable, focus lands on the card; announce once via the polite live region (§8.9).

**(e) Follow-up and human handoff — right panel + notifications (`notifications-panel.tsx`,
REQ-064, REQ-028, REQ-029).**
Keep the named per-bot push state and policy with a clear "waiting on customer" toggle, an explicit
daily cap and a partial-install report rather than a bare green check. Beside it, keep the computer
panel's bounded frame with Take over / Release buttons that always name the holder, no autoplay.
Replace the panel's raw checkboxes/`<select>` with BoardUI `switch`, `select`, `button`, `badge`,
`tooltip`; tokens `border-border-button-default`, `text-text-secondary`, `text-text-error-primary`
+ icon + word on failure, `cx()`. Existing RPCs (CONFIRMED): `notificationsStatus`,
`notificationsSubscribe`, `notificationsUnsubscribe`, `notificationsPolicyGet`,
`notificationsPolicySet`, `computerStatus`, `computerTakeover`, `computerRelease`, `threadsHead`.
A shared public summary stays a transport concern (`GET /api/v1/pages/shared/:token`, `credentials:
"omit"`); unknown, revoked and expired tokens render one identical unavailable document.

## 3. Accessibility

WCAG 2.2 AA gate: zero critical/serious axe findings in light, dark and RTL; a keyboard-only path for
every primary flow (triage, reply, decide, handoff, takeover, push toggle); visible
`focus-visible:ring-2 focus-visible:ring-border-focus-ring`; ≥44 px `.boma-target`; 360 px width;
`prefers-reduced-motion` honoured; no animation on streaming text or the live frame. Tables carry
`<caption>`/`<th scope>`; status, "needs reply" and high-impact markers are text + icon, never
colour. The pending card takes focus; run changes and errors go through the polite live region, the
transcript is not one. Error copy is one sentence of what happened plus one of what to do — no
customer text, ticket payload, stack trace or provider text on screen (correlation id behind
"details", §8.9).

## 4. i18n and RTL

English is the source catalogue, fr/he translations; a missing key falls back to English — never a
blank or raw key. `DirectionProvider` (`@/components/foundations/direction/direction`) takes an
explicit BCP 47 locale; the boot script writes `lang`/`dir` together. RTL mirrors the queue, board
columns, reply chip, card and drawer motion via `useDirection()`; logical utilities throughout
(`ms`/`me`, `ps`/`pe`, `text-start`/`text-end`, `border-s`/`border-e`). Ticket keys, customer ids,
URLs and timestamps stay LTR in `<bdi>`/`dir="auto"`. Preserve physical coordinates for the live
frame; formatting data is separate from layout direction.

## 5. Playwright tests (REQ id in each name)

- `test("REQ-011 the support queue groups runs by day, names each status and links to the source message", ...)`
- `test("REQ-012 waiting_input renders a named needs-reply chip that filters the queue without hiding the run", ...)`
- `test("REQ-001 the reply chip keeps the quoted customer message and sends one optimistic message not duplicated", ...)`
- `test("REQ-038 assigning a ticket to a bot queues work and the board shows its run status while chat does not", ...)`
- `test("REQ-007 the escalation card names ticket, customer, target queue and receiving bot and takes focus", ...)`
- `test("REQ-017 escalate and cancel answer the card exactly once with an attributable confirmation", ...)`
- `test("REQ-064 the waiting-on-customer toggle and daily cap persist and a partial install reports skipped parts", ...)`
- `test("REQ-029 takeover and release name the holder and are keyboard-operable", ...)`
- `test("REQ-037 unknown, revoked and expired summary tokens render the identical unavailable document", ...)`
- `test("REQ-057 Hebrew RTL mirrors the queue and card, keeps ticket keys LTR, and axe reports zero critical or serious findings", ...)`

Behaviour without a REQ yet (needs-reply filter, escalation label) needs an owner-assigned id before
code — never invent one. Goldens under `spec/golden/ui/` stay byte-exact.

## 6. Best first slice

**Change (a), the support triage queue** — GUESS, recommended. All RPCs are landed in `rpc.ts`,
`ActivityScreen` already exists with its loading/error/empty phases, the §8.9 status vocabulary is
shared with `lib/run-status`, and REQ-011/REQ-012 tests are named — so it needs no new transport,
permission model, wire format or locale work. One screen: name every status chip with text + icon,
add the client-side bot/status filter and the `waiting_input` "needs reply" reading, and keep the
source link. Test-first: write `REQ-011 the support queue groups runs by day, names each status and
links to the source message` before implementation. Defer (b)–(e) until scope and ids are approved.

**Decision needed from the owner**: confirm the support-lead framing, and whether the feature-flagged
board (REQ-038) and customer-facing shared summaries are in scope before (b) and (e) are scheduled.

Provenance: REQ ids from `spec/01-requirements.md`; procedure names from `spec/03-api-contract.md`
and `apps/web/lib/api/rpc.ts`; component structure from `thread-composer.tsx` and
`notifications-panel.tsx`; design rules from `docs/UI.md` and `spec/08-ui-ux.md` — all CONFIRMED
readings. Persona, jobs and slice ranking are GUESS (no customer-support sector in the spec).
No product code was modified.
