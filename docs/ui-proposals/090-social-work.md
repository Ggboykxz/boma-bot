# UI proposal 090 — social work (child & family case management)

Status: PROPOSAL (GUESS where the spec is silent). Sector specialist #090 — social-work lens.
Read-only on product code; this file is the only write.

Grounding (CONFIRMED, read this session): `docs/UI.md`, `spec/08-ui-ux.md` (§8.3, §8.4, §8.5, §8.9),
`spec/01-requirements.md` (REQ ids), `apps/web/lib/api/rpc.ts` (procedure names), `use-translation.ts`
(`t()` via `useLocale()`), and two components: `apps/web/components/activity/activity-screen.tsx`
(run-centric list, named status by text + icon, `runsList` in an effect, retry only on retryable rows,
`border-border-error-default` error row) and `apps/web/components/thread/thread-composer.tsx` (React
Aria `TextField`/`TextArea`, real `textbox` role + `sr-only` label, Enter-sends/Shift+Enter-newline,
`awaitingApproval` gate, `cx()` merge, `focus-visible:ring-border-focus-ring`). No upstream brand,
code, wording, asset or capture was consulted or copied; distinct from #074 (NGO field aid) and #088
(childcare).

Charter (CONFIRMED): Boma Bot only; the owner-supplied robot-cat is the sole identity art in empty and
onboarding states — no agency, court or service logo is borrowed. BoardUI semantic tokens only — no
raw palette class or hex/oklch literal — composite type utilities, class lists merged with `cx()`,
dark mode rides the tokens. One design system, no second library or hand-built lookalike. Self-hosted
on the service's hardware — **no SaaS and no client, family, case or court datum leaving the network**.
Gaps close with already-installed BoardUI components.

## 1. Persona and jobs

**Persona — "the case manager"**: a social worker at a local-authority family-services team running
Boma Bot self-hosted on service hardware, carrying child-protection enquiries, family support plans,
foster placements and court reports. Desktop-first in the office, on a phone between visits; works in
English/French/Arabic. Measured on *statutory timing and attributable judgement*: a missed deadline, an
unattributed note, or a referral sent to the wrong partner is a safeguarding or disclosure incident —
ambiguity is worse than latency. The bot drafts and assembles; it never replaces the worker's decision
or the legal threshold. The robot-cat is the only identity art.

Jobs:
1. **Triage the caseload** — see queued, working, paused and stuck tasks with named statuses and due times.
2. **Write case notes safely** — draft a note, keep it through rerenders, know what will be recorded.
3. **Gate a referral** — know the exact reference, subject, recipient, effect and requester before commit.
4. **Hand over between agencies** — pass a case with an attributable record, no cross-case client detail.
5. **File court-report evidence** — keep versioned reports, supervise legacy takeover, never render HTML.

## 2. Five concrete UI changes

All new copy is a key in `lib/i18n.ts` via `useTranslation()`/`t()`; nothing is hardcoded in JSX.
Every colour rides the `docs/UI.md` token map, type uses composite utilities, class lists go through
`cx()`, icons from `@remixicon/react` as component references. Client- and bot-produced content is
untrusted: render as text, never HTML (spec §8.5).

**(a) Caseload triage — `/app` activity view (`REQ-011`, `REQ-012`), grounded in `activity-screen.tsx`.**
Read each run as a case task, one row per task, newest first, with the named status from
`runStatusKey` (text + icon, never colour alone) and a due-time chip; retry stays only on retryable
rows (REQ-012). CONFIRMED shape: `runsList` in an effect with `AbortController`, skeleton rows,
`border-border-error-default` error row + `Button` retry. Add a keyboard-focusable `Tooltip` for due
detail; no client name in the row. BoardUI `Button`, `Counter`, `Tooltip`; surfaces
`bg-background-primary-default`/`bg-background-secondary-default`, hairline `border-separator-border`,
text `text-text-primary`/`text-text-secondary`, `text-text-error-primary` + icon. RPCs (CONFIRMED):
`runsList`, `runsPause`, `runsResume`, `runsRetry`.

**(b) Case-note composer — `/app/:botId` composer (`REQ-001`, `REQ-008`), grounded in `thread-composer.tsx`.**
Keep the real React Aria `textbox` + `sr-only` label, Enter-to-send/Shift+Enter-newline, send disabled
only when empty/sending/`awaitingApproval`, and the reply quote as a case reference. Add a
"case-sensitive" `Switch`/`Tooltip` hint; keep the draft across rerenders and never send while a card
awaits a decision. Tokens: `bg-background-primary-default`, `border-border-button-default`,
`text-text-primary`/`text-text-placeholder`, icons `text-foreground-icon-secondary`, focus
`focus-visible:ring-2 focus-visible:ring-border-focus-ring`. RPCs (CONFIRMED): `threadsSend`,
`threadsReact` — text enters existing `value` state, no new transport.

**(c) Safeguarding referral approval card — `/app/:botId` pending card (`REQ-007`, `REQ-017`).**
Make "what will happen" dominant: exact case reference, subject, multi-agency recipient, effect and a
named requester; the confirm label says the action ("Send referral about CS-114 to the safeguarding
team"), not just "Approve". A high-impact action pairs `border-border-error-default` +
`text-text-error-primary` with an icon and "High impact" — never colour alone. Keep the `threadsAnswer`
guard and one-click decision; the transcript stays readable, other cases reachable, focus lands on the
card, announced once in the polite live region (spec §8.9). BoardUI `card`/`button`/`badge`, surface
`bg-background-primary-default rounded-3xl border-border-button-default`, type `text-body-medium`/
`text-caption-1-semibold`. RPCs (CONFIRMED): `threadsAnswer`, `threadsReact`. Extend golden
`spec/golden/ui/REQ-007-pending-card.json`.

**(d) Multi-agency handover — `/app/g/:groupId` case group (`REQ-010`, `REQ-067`, `REQ-068`).**
Add a named handover banner with outgoing/incoming worker or bot and a status chip, reusing the
reassign row; a failed handover shows a named retry, never a silent failure. Every message keeps its
author attribution so the record stays accountable. BoardUI `Button`, `Badge`, `Avatar`
(`components/base/avatar/avatar.tsx`); tokens `bg-background-secondary-default`,
`border-separator-border`, `text-text-secondary`, `text-text-error-primary` plus text + icon. RPCs
(CONFIRMED): `groupsGet`, `groupsMessages`, `groupsReassign` (`runsPause`/`runsResume` if a task pauses).

**(e) Court-report files and supervised legacy takeover — `/app/artifacts` & panel
(`REQ-036`, `REQ-026`, `REQ-028`, `REQ-029`).** Grid of versioned reports newest first, filter by
type/owner bot, search by name; viewer keeps version selector, download, "open in thread" and copy —
the version chain is a chain (spec §8.5). Add a "confirm recipients" gate before sharing. For the
legacy system, raise the computer panel with explicit takeover/release; never render report HTML.
BoardUI `table`, `drawer`, `spinner`, `skeleton`, `Button` (per `docs/UI.md`); surfaces
`bg-background-primary-default`/`bg-background-secondary-default`, hairline
`border-separator-border`, status by text + icon. RPCs (CONFIRMED): `artifactsListSpace`,
`artifactsGet`, `artifactsListVersions`, `computerStatus`, `computerTakeover`, `computerRelease`,
`computerOpenScreen`.

## 3. Accessibility, i18n and RTL

WCAG 2.2 AA: zero critical/serious axe findings in light, dark and RTL; ≥44 px `.boma-target` on touch
controls; token-driven focus `focus-visible:ring-2 focus-visible:ring-border-focus-ring`; a keyboard
path for every primary flow (triage, note send, referral decision, handover, version select, takeover);
360 px width; `prefers-reduced-motion` honoured; no animation on streaming text. Task status,
high-impact marker and handover state are text + icon, never colour alone; tables carry `<caption>`/
`<th scope="col">`. Error copy is one sentence of what happened plus one of what to do — no case
reference, client name, court detail, stack trace or provider text on screen (correlation id behind
"details" only, spec §8.9). Nothing a worker must act on lives in a hover-only tooltip; no client data
in logs or another case's view.

English is the source catalogue, fr/he translations; a missing key falls back to English — never a
blank or a bare key. `DirectionProvider` takes an explicit BCP 47 locale; the boot script writes
`lang` and `dir` together. Use logical utilities (`ms`/`me`, `ps`/`pe`, `start`/`end`,
`text-start`/`text-end`, `border-s`/`border-e`) and `useDirection()` to mirror activity rows, the
handover arrow and drawer motion for `he` (`REQ-057`); preserve physical chart coordinates. Case
references, dates, emails, URLs and phone numbers stay LTR in `<bdi>`/`dir="auto"`; formatting data
is separate from direction.

## 4. Playwright tests (REQ id in each name)

- `REQ-011 the caseload lists named case-task statuses and due times without client names`
- `REQ-012 every run status renders a distinct named chip and only failed rows offer retry`
- `REQ-001 a case note creates one optimistic message that is not duplicated`
- `REQ-007 the referral card names case reference, recipient, effect and requester without blocking reading`
- `REQ-017 a high-impact referral is marked by text and icon and answers the card exactly once`
- `REQ-010 a case group attributes each message and hands over by keyboard alone`
- `REQ-036 the report library selects a version, opens its thread and never injects HTML`
- `REQ-026 the legacy-system panel moves from provisioning to takeover with named states`
- `REQ-057 Hebrew RTL loads dir=rtl with a clean axe run`

Behaviour without a REQ yet (note switch, client-data hint, recipient gate, due-time chip) needs an
owner-assigned id before code — do not invent one in a test name. Goldens stay byte-exact.

## 5. Best first slice

**Change (c), the safeguarding referral approval card** — GUESS, recommended: one screen, no new
transport (`threadsAnswer`/`threadsReact` exist), no new component (BoardUI `button`/`badge`/`card`
installed), and it directly reduces the worst sector risk — an irreversible referral committed on an
ambiguous cue. Test-first: write `REQ-007 ... names case reference, recipient, effect and requester`
and extend `spec/golden/ui/REQ-007-pending-card.json` first; ship only the dominant consequence
sentence, the explicit send label and the text + icon high-impact marker. Change (a) is a close second
(component read this session, `REQ-011`/`REQ-012` tests exist; only due-time copy via `t()` needed).
Defer (b), (d), (e) and the new behaviours until the owner assigns ids and approves.

**Decision needed**: confirm the statutory framing (distinct from #074/#088) and that the "recipient +
effect" copy is covered by REQ-007 before it is asserted.

Provenance: REQ ids CONFIRMED from `spec/01-requirements.md`; RPC names from `apps/web/lib/api/rpc.ts`;
component structure CONFIRMED by reading the two files; design rules CONFIRMED from `docs/UI.md` and
`spec/08-ui-ux.md`. Persona, jobs, mapping, five changes and slice ranking are GUESS (proposal only — no
"social-work" REQ exists). No upstream asset used; no product code modified.
