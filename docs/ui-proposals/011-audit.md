# UI proposal 011 — audit sector (state legibility and verification)

Status: PROPOSAL (GUESS where the spec is silent). Sector specialist #011 — audit. Read-only on
product code; this file is the only write. Grounding: `docs/UI.md`, `spec/08-ui-ux.md`,
`docs/audits/2026-11-16-ui-motion-ux-audit.md`, and two components —
`apps/web/components/shell/app-shell.tsx`, `apps/web/components/thread/thread-composer.tsx`.
RPC names checked against `apps/web/lib/api/rpc.ts`; goldens against `spec/golden/ui/`.

Charter: Boma Bot only — no upstream brand, code, assets or wording; BoardUI semantic tokens only,
never a raw palette class or hex literal, classes merged with `cx()`; one design system, no second;
no SaaS (self-hosted on the operator's own machine); keep the owner-supplied robot-cat identity.

Audit baseline (CONFIRMED from the report and the M34 follow-through in `docs/STATE.md`): the
2026-11-16 contrast failures, duplicate `main`/missing `h1`, dead "Settings" link and dormant motion
vocabulary were fixed in M34 — do not re-propose them. Open gaps this proposal extends: 360 px width
and screen-reader announcements are not verified, the rail stacks above the thread at 390 px, the
RTL proof covers the shell only, and the client ships 3 of the 14 locales REQ-057 names (issue #16).

## 1. Persona and top jobs-to-be-done

**Persona — Nadia, operator of a self-hosted Boma Bot deployment.** She runs a handful of bots on a
machine she controls (no hosted account, no cloud); several act on real files and messages. She is
often halfway through something else when she glances at the app, sometimes on a phone in RTL, and
she is accountable for what the bots did. An ambiguous state is worse than a slow screen.

Jobs:
1. **Trust** — name the state of every run out loud (working / paused / terminal vocabulary, §8.9).
2. **Decide** — read exactly what a proposed action will do before answering a card; never confirm on
   colour alone.
3. **Recover** — spot a failure, read a plain-language cause, and retry or pause without hunting.
4. **Reach** — operate every control by keyboard, screen reader and at 360 px, with visible focus.
5. **Orient** — know where she is in light, dark and RTL, with one `main`, one `h1`, a labelled nav.

## 2. Five concrete changes

All copy is a new `lib/i18n.ts` key through `t(locale, key)`; no string in JSX. All colour rides
`docs/UI.md` tokens; every class list goes through `cx()`.

**(a) Named run-status ledger — screen `/app/:botId` (transcript header) and activity (REQ-012,
REQ-067).** One status row per run with text + icon chips for `queued`, `leased`, `running`,
`waiting_input`, `waiting_takeover`, `completed`, `failed`, `cancelled`, and Pause/Resume on the
working/paused states. BoardUI `Chip`, `Tooltip`, `IconButton`; surface `bg-background-secondary-default`,
hairline `border-separator-border`, text `text-text-primary`/`text-text-secondary`, icons
`text-foreground-icon-secondary`, type `text-caption-1-semibold`, focus `focus-visible:ring-2
focus-visible:ring-border-focus-ring`. Existing RPCs: `runsList`, `runsPause`, `runsResume`,
`runsRetry`, `threadsHead`. Transitions go to the polite live region, not the transcript.

**(b) Consequence-first approval card — screen `/app/:botId` (pending card, REQ-007/REQ-017).**
Make "what will happen" the dominant line and the confirm label name the action ("Send message",
not just "Approve"). High-impact cards pair `border-border-error-default` + `text-text-error-primary`
with an icon and the words "High impact" — never colour alone. BoardUI `Button`, `Badge`, `Tooltip`;
card surface `bg-background-primary-default`, `rounded-3xl`, `border-border-button-default`.
Existing RPCs: `threadsAnswer`, `threadsReact`. Reuse `ThreadComposer`'s `awaitingApproval` prop:
the transcript stays readable and other threads reachable; only modal approvals block the composer.

**(c) Failure-and-retry row — screens `/app/:botId` and activity (REQ-021, REQ-012).** A failed run
shows one sentence of cause plus one of what to do, a working Retry `Button`, and the correlation id
behind a `Tooltip` labelled "details" — no stack trace, id or provider text on the surface (§8.9).
BoardUI `Button`, `Badge`, `Tooltip`; `text-text-error-primary`, `border-border-error-default`,
`bg-background-primary-default`. Existing RPCs: `runsList`, `runsRetry`, `threadsMessages`. Keep the
row 44 px tall so it survives 360 px and touch.

**(d) Announcements and discoverable keyboard map — every route (REQ-057, §8.9).** A single polite
live region announces status changes, card arrivals and errors (the transcript itself stays *not*
a live region); a `?` overlay lists send, new line, stop, focus composer, next/previous bot, close
panel, search. BoardUI `Tooltip` and the installed `settings-modal`/dropdown pattern for the overlay,
`IconButton` for its trigger; tokens `bg-background-secondary-default`, `text-text-primary`,
`focus-visible:ring-border-focus-ring`. No new transport — reuse `threadsSubscribe` frames and the
existing `threadsHead`/`runsList` reads.

**(e) 360 px rail and sticky composer — screens `/app`, `/app/:botId` (REQ-055, REQ-057; mobile
REQ-056 stays owner-deferred).** Collapse the stacked rail into the installed BoardUI `Sidebar`
drawer and the context panel into a tabbed bottom sheet; keep the composer pinned above the keyboard
so it never covers the last message. Use installed `ScrollFade` for the transcript's top edge.
Tokens: `bg-background-secondary-default`, `border-separator-border`, `text-text-primary`,
`.boma-target`; type `text-body-medium`. Existing RPCs: `botsList`, `threadsSend`, `threadsMessages`.
Mirror drawer motion and arrows with `useDirection()`; leave computer-screen coordinates physical.

## 3. Accessibility, i18n and RTL

- WCAG 2.2 AA gate: zero critical/serious axe findings in light, dark and RTL, keyboard path for
  every primary flow, visible token-driven focus, 44 px `.boma-target`, `prefers-reduced-motion`
  honoured. Status and impact always text + icon, never colour alone.
- `DirectionProvider` takes the explicit BCP 47 locale; the boot script writes `lang` and `dir`
  together. Logical utilities everywhere (`ms`/`me`, `ps`/`pe`, `text-start`/`text-end`,
  `border-s`/`border-e`); flip drawer motion and chevrons in RTL.
- Keep IDs, URLs, code, OTP digits and timestamps LTR in `<bdi>`/`dir="auto"`. English is the source
  catalogue (en/fr/he); a missing key falls back to English, never to a blank or the raw key.
- Close the audit's open gap on screen readers: assert the announcement text, not just the markup.
  Do not claim the 14-locale REQ-057 requirement met while only three ship; leave it for issue #16.

## 4. Playwright tests (REQ ids in the names)

- `REQ-012 every run status renders a distinct named chip with the right terminal action`
- `REQ-067 pause and resume are keyboard-operable and announced`
- `REQ-007 pending approval card names the exact action, takes focus, and does not block reading`
- `REQ-021 a failed run shows a plain-language cause and a working retry`
- `REQ-057 Hebrew RTL loads dir=rtl with a clean axe run`
- `REQ-055 the shell exposes one main, one h1, and a labelled nav at 360 px`
- `REQ-001 send creates one optimistic message that is not duplicated`
- New behaviour without a REQ (the `?` overlay, announcement region) must get an owner-assigned id
  before code — do not invent one in a test name.

## 5. Highest-impact first slice

**Change (c), the failure-and-retry row.** One screen, no new transport (`runsList`/`runsRetry`/
`threadsMessages` exist), no new component (BoardUI `Button`/`Badge`/`Tooltip` are installed), and
no token re-tune — the 2026-11-16 contrast fixes already landed. It directly answers the audit's
"state is never ambiguous" rule at the moment trust is most at risk. Test-first: write
`REQ-021 a failed run shows a plain-language cause and a working retry`, then ship only the two
sentences, the visible Retry and the behind-details correlation id. Defer (a), (b), (d), (e) until
the owner assigns REQ ids and approves the mobile/RTL scope.
