# UI proposal 010 — accounting and bookkeeping

Status: PROPOSAL (GUESS where the spec is silent). Sector specialist #010. Read-only on product code.
Grounding: `docs/UI.md`, `spec/08-ui-ux.md`, and
`apps/web/components/{thread/message-blocks,activity/activity-screen}.tsx`. RPC names checked against
`apps/web/lib/api/rpc.ts`. Charter held: Boma Bot only — no upstream/OpenBot brand, code, wording or
assets; BoardUI semantic tokens only (no raw palette classes or hex literals), `cx()` to merge, dark
mode via tokens; one design system, no second; **no SaaS** — self-hosted, REQ-051 hosted checkout and
subscription UI stay off; keep the owner-supplied robot-cat identity.

## 1. Persona and top jobs

**Persona — Nadia, bookkeeping lead in a 6-person accounting firm.** She self-hosts Boma Bot on the
firm's own machine: client ledgers, invoices and payment approvals never leave the office and no
third-party billing service is involved. She is not a developer; a wrong click on a payment card is a
real disbursement, so an ambiguous "Approve" is worse than a slow screen. She works at 360–1280 px,
often mid-reconciliation with a client on the phone.

Top jobs:
1. **Authorise** — review a bot's purchase request (amount, payee, purpose) and be certain what will
   be paid before committing; never confirm by colour alone.
2. **Contain** — see instantly when a request was refused for reaching the spending cap, and know the
   one next action (raise the cap, or cancel) without reading a stack trace.
3. **Reconcile** — trace what each bot did, when and with what outcome, for a period-end review.
4. **File** — find an exported ledger or statement in the artifact library, check its version and
   download the right one.
5. **Configure** — set the per-account credit cap and see which model credentials are connected,
   without any key ever appearing on screen.

## 2. Five concrete changes

All strings are new keys through `useTranslation()` / `t(locale, key)` (`lib/i18n.ts`, English source,
French/Hebrew translations, English fallback); no hardcoded copy. Components and tokens come from
`apps/web/components/` and `docs/UI.md`; classes joined with `cx()`.

**(a) Payment-authorisation card — `/app/:botId`, `CardBlock` (`thread/message-blocks.tsx`).**
The `purchase` kind is already branched today (CONFIRMED). Make the named consequence the dominant
line — "Pay <amount> to <payee> for <purpose>" — and replace the generic `thread.card.purchase.explain`
with an explicit confirm label ("Authorise payment", not "Approve"). Over-cap/high-value cards carry
`border-border-error-default` + `text-text-error-primary` **plus** a `RiAlertLine` icon and the word
"High impact" — never colour alone. Migrate the raw `<button>`/`<input>` in this block to the
installed BoardUI `Button`/`Input` (design rule: extend installed form components, no raw controls).
Surface `bg-background-secondary-default`, `rounded-3xl`, `border-separator-border`; text
`text-text-primary`/`text-text-secondary`; type `text-body-medium`/`text-body-2-regular`. RPC:
`threadsAnswer` (already wired). REQ-018 + REQ-017. Do **not** invent a remaining-credit number: no
spend figure is on the web wire today (CONFIRMED from `rpc.ts`); that needs a REQ id and a read RPC.

**(b) Cap-refusal row — `/app/:botId`, transcript (`thread/message-blocks.tsx` / `thread-screen.tsx`).**
When a paid request is refused at the cap, render a named error row: one sentence of what happened,
one of what to do ("Spending cap reached. Raise the cap in Settings or cancel this request."), with a
single `Button` action. Branch on the typed `RpcError.code` already returned by `call()` (e.g.
`CONFLICT`/`TOO_MANY_REQUESTS`) — never parse message strings. Use
`border-border-error-default`, `text-text-error-primary`, `text-foreground-icon-error`, plus icon and
text; keep the correlation id behind a "Details" disclosure (spec §8.9). RPCs: the existing
`threadsSend` / `threadsAnswer` error envelopes; no new transport. REQ-018.

**(c) Period-reconciliation ledger — Activity (`activity/activity-screen.tsx`).**
Adapt the single run list into a reviewable ledger: group by day, keep run status as text + icon
(already true, CONFIRMED), add bot name via the existing `botsGet`, and reuse installed `Tabs` as a
status filter (working / waiting / failed / terminal). Keep pause/resume/retry unchanged. Components:
`Tabs`, `Button`, `Badge`/`chip`; tokens `bg-background-primary-default`, `border-border-button-default`,
`text-text-secondary`, `animate-number-fade` on count changes. RPCs: `runsList`, `runsPause`,
`runsResume`, `runsRetry`, `botsGet`. REQ-011, REQ-012, REQ-045.

**(d) Exported-ledger shelf — `/app/artifacts` (`artifacts/artifacts-grid.tsx`, `artifact-viewer.tsx`).**
Filter the grid to exported statements and give each card a kind badge and version number so the
right copy is downloaded. Keep the bounded viewer: render text/PDF natively, never inject artifact
content as HTML (spec §8.5). Components: `Badge`, `Button`, installed `Select`/`Input` for filters;
tokens `text-text-primary`/`text-text-secondary`, `border-border-button-default`, `text-start`.
RPCs: `artifactsListSpace`, `artifactsGet`, `artifactsListVersions`. REQ-036, REQ-045.

**(e) Credit-cap and credential panel — settings (`application/settings/settings-modal.tsx`).**
Expose the per-account credit cap with the installed `Input`/`Button`, and list connected providers
with status chips — keys are never listed (CONFIRMED in `rpc.ts` `modelsCredentials`). Hosted
checkout/subscription is deliberately absent: this is self-hosted spend control, not SaaS billing.
Components: `Input`, `Button`, `Switch`, `Badge`, `Tooltip`; tokens `bg-background-secondary-default`,
`border-separator-border`, `text-text-secondary`, focus `focus-visible:ring-border-focus-ring`.
RPCs: `modelsCredentials`, `modelsCatalog`, `modelsConnect`, `authCapabilities` (billing flag).
REQ-052; REQ-051 stays off by owner charter.

## 3. Accessibility, i18n and RTL

- Every label goes through `t(locale, key)`; no string in JSX. English fallback, never a blank or the key.
- `DirectionProvider` sets locale/direction; the boot script writes `lang`/`dir`. Use logical utilities
  (`ms`/`me`, `ps`/`pe`, `text-start`/`text-end`, `border-s`/`border-e`); mirror the ledger's
  sort/filter arrows via `useDirection()`.
- Amounts, payee identifiers, dates and URLs stay LTR in `<bdi>`/`dir="auto"` and are formatted with
  `Intl.NumberFormat`/`Intl.DateTimeFormat`; formatting is separate from layout direction.
- WCAG 2.2 AA gate: zero critical/serious axe in light, dark and RTL; 44 px `.boma-target`; visible
  token focus ring; full keyboard path (authorise, cap-refusal action, filters, export download);
  status and impact conveyed by text + icon, never colour; 360 px width; `prefers-reduced-motion`.
- Error copy is two sentences of what happened and what to do — no ids, stack traces or provider text
  on screen (correlation id behind "Details" only, spec §8.9). No financial value is passed to a
  hover-only tooltip, written to a log, or rendered as markup.

## 4. Playwright test names (REQ ids in the name)

```ts
test("REQ-018 a purchase card names the amount, payee and explicit authorisation before payment", ...)
test("REQ-017 an answered payment card takes focus and does not block reading or other threads", ...)
test("REQ-018 a cap refusal shows a visible reason, one next action and no stack trace", ...)
test("REQ-011 the reconciliation ledger groups runs by day with bot attribution and status filters", ...)
test("REQ-012 every run status renders as text plus icon in light, dark and RTL", ...)
test("REQ-036 an exported-ledger artifact opens in a bounded viewer and downloads the right version", ...)
test("REQ-052 the credit-cap panel shows connected providers and never renders a secret", ...)
test("REQ-057 the accounting screens mirror for Hebrew with a clean axe run", ...)
```

New behaviour without a REQ yet (the cap-refusal row as a distinct block) must be assigned a REQ id by
the owner before implementation, per the traceability gate — do not invent one in a test name.

## 5. Highest-impact first slice

**Change (a), the payment-authorisation card.** Smallest and highest-value: one component, no new
transport (`threadsAnswer` exists), no new install (BoardUI `Button`/`Input` are vendored), and it
directly reduces the risk of an unintended disbursement. Test-first: write
`REQ-018 ... names the amount, payee and explicit authorisation` and extend
`spec/golden/ui/REQ-007-pending-card.json` first. Ship only the dominant consequence sentence, the
explicit "Authorise payment" label, the text+icon high-impact marker and the raw-control migration.
Defer (b)–(e) until the owner assigns REQ ids and approves.

Provenance: CONFIRMED claims were read this session from `docs/UI.md`, `spec/08-ui-ux.md`,
`spec/01-requirements.md` (REQ-018/051/052), `apps/web/lib/api/rpc.ts` and the two named components;
the persona, its jobs and the cap-refusal presentation are a GUESS (proposal only — no spec sector).
