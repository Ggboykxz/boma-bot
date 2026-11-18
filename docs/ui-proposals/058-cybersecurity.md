# UI proposal 058 — cybersecurity (least privilege and non-disclosure)

Status: PROPOSAL (sector framing and persona are GUESS — a sector lens, not in `spec/`). Sector
specialist #058 — cybersecurity. Read-only on product code; this file is the only write.
Grounding (CONFIRMED, read this session): `docs/UI.md`, `spec/08-ui-ux.md`,
`spec/01-requirements.md` (REQ ids), `apps/web/lib/api/rpc.ts` (procedure names), `apps/web/lib/i18n.ts`
(existing `admin.*` keys), `apps/web/tests/admin.spec.ts` (three named REQ-058 tests), and two
components: `apps/web/components/admin/access-screen.tsx` and
`apps/web/components/admin/plugins-screen.tsx`. No upstream brand, code, assets, wording or
reference capture consulted or copied.

Charter (CONFIRMED): Boma Bot only, fresh design (ADR-015), owner-supplied robot-cat
(`assets/brand/boma-bot-head.svg`) as the sole identity; no upstream mark, copy or asset; BoardUI
semantic tokens only — no raw palette class or hex literal — every class list merged with `cx()`;
one design system, no second library or hand-built lookalike; self-hosted on the operator's own
machine — no SaaS/cloud, no secret, grant, plugin source or audit datum leaving the building.

## 1. Persona and top jobs-to-be-done

**Persona — "the security operator"**: the admin of a self-hosted Boma Bot deployment who owns the
machine and everything on it. She works desktop-first at an ops desk, answers in English/French/Arabic,
and is measured on *what cannot happen*: no over-broad grant, no unreviewed code path, no leaked
revoked link, no secret echoed back. She is accountable for the bots' privileged acts and treats an
ambiguous state as an incident. The robot-cat mark is the only identity art; no vendor, regulator or
security-brand logo is borrowed.

Jobs:
1. **Bound access** — see who belongs, what is pending and why every deployment-level grant exists.
2. **Vet code** — review a third-party plugin submission as inert text; record a decision without
   running anything.
3. **Authorise** — approve a consequential bot action only after reading the exact effect and actor,
   never on a colour cue.
4. **Disclose nothing** — make revoked/unknown shared links indistinguishable in words, shape and
   timing; keep keys and secrets out of the screen.
5. **Attribute** — tie a privileged act to a named actor and keep the refusal path uniform for
   ordinary members.

## 2. Five concrete UI changes

All new copy is a key in `lib/i18n.ts` via `useTranslation()` / `t(locale, key)`; nothing hardcoded
in JSX (the two `Name`/`Role` column headers in `access-screen.tsx` are a CONFIRMED gap). Every
colour rides a `docs/UI.md` token; every class list goes through `cx()`.

**(a) Privileged-access ledger — `/admin/access` (REQ-058).**
Read `access-screen.tsx` as one ledger: members, pending invitations and server-level grants. Add
`reason`, `grantedBy` and `createdAt` columns; revoke asks for confirmation naming what the person
loses. BoardUI `table`, `input`, `button`, `badge`, `tooltip`; replace the hand-built `<input>`/`table`
with the installed items. Tokens `bg-background-full`, `border-border-table`, `text-text-primary`,
`text-text-tertiary` metadata, `text-text-error-primary` + icon + word for destructive rows,
`focus-visible:ring-2 focus-visible:ring-border-focus-ring`. RPCs (CONFIRMED): `adminMembersList`,
`adminInvitationsList`, `adminGrantsList`, `adminGrantsCreate`, `adminGrantsRevoke`,
`adminInvitationsRevoke`; a `FORBIDDEN` envelope renders the refusal sentence, never a table.

**(b) Plugin supply-chain vetting — `/admin/plugins` (REQ-058).**
From `plugins-screen.tsx`: newest-first queue with source, submitter name, status and a caption that
"approving only flips review status, it runs nothing". Source stays `<code dir="auto">` — inert text,
never markup or executed code. Record who decided. BoardUI `table`, `button`, `badge`, `tooltip`;
tokens `border-border-table`, `text-text-secondary`, `border-border-error-default` + icon + word for
rejected/unsafe parts, `cx()`. RPCs (CONFIRMED): `adminPluginsList`, `adminPluginsDecide`.

**(c) High-impact approval card — thread `/app/:botId` (REQ-007, REQ-017).**
Make the "what will happen" sentence dominant, state the authorising member, and give a distinct
confirm label ("Deploy to Live", not just "Approve"). High-impact actions pair
`border-border-error-default` + `text-text-error-primary` with a warning icon and the words
"High impact" — never colour alone. BoardUI `card`, `button`, `badge`, `textarea`; surface
`bg-background-primary-default`, `rounded-3xl`, `border-border-button-default`. Existing RPCs
(CONFIRMED): `threadsAnswer`, `threadsMessages`; announce once via the polite live region — the
transcript itself is not a live region (§8.9).

**(d) Uniform unavailable document — `/p/:slugToken` (REQ-037).**
No navigation or product chrome. Unknown, revoked and expired tokens render one identical "Page not
available" document — same words, same shape, no wording or timing difference. Keep block rendering
data-only with the existing `safeLink`/`safeImage` allow-lists and caps. BoardUI `table`, `badge`,
`tooltip`; tokens `bg-background-full`, `border-border-table`, `fill-accent-400`, `cx()`. Transport
(CONFIRMED): `GET /api/v1/pages/shared/{token}` with `credentials: "omit"`; `noindex`/`no-referrer`
and the page CSP stay backend concerns.

**(e) Credential and secret hygiene — onboarding step 3 / model settings (REQ-052, REQ-046).**
In the model-connect step, validate then seal a key: show provider and status only, never the value;
a forgotten secret is re-requested, never re-displayed. Use a masked BoardUI `input`, `switch`,
`badge`, `tooltip`. Tokens `bg-background-primary-default`, `border-border-button-default`,
`text-text-placeholder`, `text-text-error-primary` + icon + word on a rejected key, `cx()`. RPCs
(CONFIRMED): `modelsConnect`, `modelsCredentials`, `modelsCatalog`, `authCapabilities` — `modelsCredentials`
returns status, never the key. GUESS gap: REQ-045 audit export and the REQ-046 secret panel have no
web client yet; if they enter scope the owner must assign their RPCs and REQ ids first — do not
invent them.

## 3. Accessibility

WCAG 2.2 AA gate: zero critical/serious axe findings in light, dark and RTL; a keyboard-only path
for every primary flow (grant/revoke, decide, approve, connect key); visible
`focus-visible:ring-2 focus-visible:ring-border-focus-ring`; ≥44 px `.boma-target`; 360 px width;
`prefers-reduced-motion` honoured. Tables carry `<caption>` and `<th scope>`; chips and destructive
states are text + icon, never colour. Focus lands on a pending card; the refusal and the uniform
unavailable screen are announced in identical text — no leak through the accessibility tree. Error
copy is one sentence of what happened plus one of what to do, no id, stack trace, source or provider
text on screen (correlation id behind "details", §8.9).

## 4. i18n and RTL

English is the source catalogue, fr/he translations; a missing key falls back to English — never a
blank or the raw key. `DirectionProvider` (`@/components/foundations/direction/direction`) takes an
explicit BCP 47 locale; the boot script writes `lang` and `dir` together. RTL mirrors the ledger,
queue and cards and flips arrows/drawer motion via `useDirection()`; logical utilities throughout
(`ms`/`me`, `ps`/`pe`, `text-start`/`text-end`, `border-s`/`border-e`). Emails, plugin sources, keys,
URLs, ids and timestamps stay LTR in `<bdi>`/`dir="auto"`. Preserve any measured geometry unchanged.

## 5. Playwright tests (REQ id in each name)

- `test("REQ-058 the access ledger lists members, invitations and grants with reason, granter and date", ...)`.
- `test("REQ-058 revoking a grant asks for confirmation and states what the person loses", ...)`.
- `test("REQ-058 a plain member sees the refusal sentence and no ledger rows", ...)`.
- `test("REQ-058 plugin vetting shows source as inert text and records one decision without executing it", ...)`.
- `test("REQ-007 a high-impact card names the effect, actor and authorizer and takes focus without blocking reading", ...)`.
- `test("REQ-017 approve and cancel answer the card exactly once with an attributable confirmation", ...)`.
- `test("REQ-037 unknown, revoked and expired tokens render the identical unavailable document", ...)`.
- `test("REQ-052 a rejected key shows one cause sentence and no value is echoed on screen", ...)`.
- `test("REQ-057 Hebrew RTL mirrors the ledger and axe reports zero critical or serious findings", ...)`.

Behaviour without a REQ (high-impact marker, confirmation copy) needs an owner-assigned id before
code — do not invent one in a test name. Goldens under `spec/golden/ui/` stay byte-exact.

## 6. Best first slice

**Change (a), the privileged-access ledger** — GUESS, recommended. All six `admin.*` RPCs are landed,
the `admin.*` i18n keys already exist, and `admin.spec.ts` carries three named REQ-058 tests, so it
needs no new transport, permission model, wire format or locale work. One screen: swap the raw
`<input>`/`<table>` for BoardUI items, route the two hardcoded headers through `t()`, add
reason/grantedBy/createdAt columns, and add the revocation confirmation — all inside REQ-058.
Test-first: write `REQ-058 revoking a grant asks for confirmation and states what the person loses`
before implementation. Defer (b)–(e) until the owner assigns any missing REQ ids and approves scope.

**Decision needed from the owner**: confirm the security-operator framing, and confirm whether the
REQ-045 audit export and REQ-046 secret panel are in scope — both currently have no web client.

Provenance: REQ ids verified against `spec/01-requirements.md`; RPC names read from
`apps/web/lib/api/rpc.ts`; component structure read from `access-screen.tsx` and `plugins-screen.tsx`;
design rules from `docs/UI.md` and `spec/08-ui-ux.md` — all CONFIRMED readings. The persona, its jobs
and the slice ranking are GUESS (proposal only — no cybersecurity sector exists in the spec).
No product code was modified.
