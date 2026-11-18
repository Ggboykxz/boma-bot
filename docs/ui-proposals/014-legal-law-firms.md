# UI proposal 014 — legal (law firms)

Status: PROPOSAL (GUESS where the spec is silent). Sector specialist #014. Read-only on product code;
only this file was written. Grounding: `docs/UI.md`, `spec/08-ui-ux.md`, and
`apps/web/components/{shell/app-shell,thread/computer-panel}.tsx`. Existing RPC names checked against
`apps/web/lib/api/rpc.ts`. No upstream brand, code, wording or assets consulted or copied.

Charter: Boma Bot only; no upstream brand/code/wording/assets; BoardUI semantic tokens only — no raw
palette classes or hex literals — `cx()` to merge classes; one design system, no second; no SaaS
(self-hosted inside the firm; no client data, matter file or privileged text leaves the building);
keep the robot-cat identity unchanged.

## 1. Persona and top jobs-to-be-done

**Persona — Amara, a litigation paralegal at a 20-lawyer firm.** She coordinates 25–40 live matters,
works at a shared case-desk, and serves English- and French-speaking clients (plus Hebrew matters).
Boma Bot runs self-hosted on the firm's own machine: no cloud and no matter data leaving the building.
Legal work is privileged and deadline-driven, so "what will happen" must be explicit and attributable
before a bot sends, files or amends anything — an ambiguous confirmation is worse than a slow screen.

Top jobs:
1. **Triage** — see which matters are working, which await her, and which deadlines fall this week,
   without hunting through the rail.
2. **Capture** — enter an intake note or draft instruction quickly, without losing the draft to a
   stray Enter.
3. **Review** — check a bot-proposed legal action (engagement letter, filing, client message) and be
   certain of the consequence before approving; never confirm by colour alone.
4. **Record** — let the bot work the case-management system on-screen, and take over cleanly when it
   needs her hands, with an attributable matter-file audit trail.
5. **Hand over** — pass a matter or draft to a supervising attorney with a clear, attributable record.

## 2. Five concrete changes

All strings are new keys through `t(locale, key)` (`lib/i18n.ts`, hook `lib/use-translation.ts`); no
hardcoded copy. All reuse BoardUI components under `apps/web/components/` and the semantic tokens in
`docs/UI.md`; classes merged with `cx()`.

**(a) Matter-status context strip — screen `/app/:botId` (thread header).** A compact, read-only band
above the transcript: chips for "open matters", "conflict check pending", "deadlines this week", each
with a `Tooltip` for detail. BoardUI `Chip`/`Badge` (`base/badges/chip.tsx`), `Tooltip`, `IconButton`;
surface `bg-background-secondary-default`, hairline `border-separator-border`, text
`text-text-secondary`/`text-text-primary`, icons `text-foreground-icon-secondary`, type
`text-caption-1-semibold`. Existing RPCs: `botsGet`, `threadsHead`, `threadsMessages`. Bot-produced
text is untrusted — render as text, never HTML; keep client/matter identifiers out of the DOM and out
of hover-only tooltips where possible.

**(b) Legal-action approval card — screen `/app/:botId` (pending card, REQ-007).** Make the "what will
happen" sentence the dominant line; add an explicit consequence summary and a distinct confirm label
("Approve filing", not just "Approve"), plus who is authorising it. Privileged/high-impact actions use
`border-border-error-default` + `text-text-error-primary` **plus** a shield icon and the words
"Privileged & high impact" — never colour alone. BoardUI `Button`, `Badge`, `Tooltip`; card surface
`bg-background-primary-default`, `rounded-3xl`, `border-border-button-default`. Existing RPCs:
`threadsAnswer`, `threadsReact`. Announce once via the polite live region (spec §8.9); the transcript
is not a live region.

**(c) Confidential draft composer — screen `/app/:botId` (composer, REQ-001).** Keep Enter-to-send /
Shift+Enter-newline exactly as today and keep send disabled while a modal approval is pending; add a
small "Privileged & confidential — stays on this machine" caption and an exhibit-attachment
`IconButton`. BoardUI `IconButton` + `Tooltip` (`base/buttons/icon-button.tsx`); focus
`focus-visible:ring-2 focus-visible:ring-border-focus-ring`, 44 px `.boma-target`, icon
`text-foreground-icon-primary`. Existing RPC `threadsSend` (`clientNonce` idempotent), so no new
transport. Citations, dates and matter numbers stay LTR in `<bdi>`.

**(d) Case-system takeover panel — screen `/app/:botId` (computer panel, REQ-026/REQ-028/REQ-029).**
Add a named status for each computer state (starting, running, awaiting handover, released) and a clear
"Leaving this screen pauses nothing — the matter file stays open" caption above takeover/release
controls. BoardUI `Button` + `Badge` and the installed spinner/skeleton states; surface
`bg-background-secondary-default`, text `text-text-secondary`, status as text + icon, never colour
only. Existing RPCs: `computerStatus`, `computerTakeover`, `computerRelease`, `computerOpenScreen`.
Mirror drawer motion in RTL; keep physical screen coordinates untouched; no matter or client data in
logs.

**(e) Matter deadline board — screen `/app/board` (REQ-038).** Retitle columns to matter stages
(intake, discovery, drafting, filing, closed) on the existing flag-gated board; each card shows matter
key, title, responsible bot/attorney and last comment. Reuse installed `table`/`chip`, `Button`,
`input`, `select`, `Badge`; surfaces `bg-background-primary-default` and
`bg-background-secondary-default`, hairline `border-separator-border`, status text + icon. Existing
RPCs: `boardsGet`, `ticketsCreate`, `ticketsMove`, `ticketsComment`. Working (ticket) runs appear in
Activity, not the transcript; when the flag is off the route is absent, not locked.

## 3. Accessibility, i18n and RTL

- New labels go in `lib/i18n.ts` (English source; French/Hebrew translations; a missing key falls back
  to English, never a blank or the raw key). No string in JSX.
- `DirectionProvider` sets locale/direction; the boot script writes `lang`/`dir`. Use logical utilities
  (`ms`/`me`, `ps`/`pe`, `text-start`/`text-end`, `border-s`/`border-e`). Mirror takeover and pipeline
  arrows via `useDirection()`; keep citations, dates, matter numbers, currency and URLs LTR in `<bdi>`
  or `dir="auto"`.
- WCAG 2.2 AA gate: zero critical/serious axe findings in light, dark and RTL; 44 px targets via
  `.boma-target`; visible token-driven focus; a full keyboard path (card decisions, takeover/release,
  ticket move, send); status conveyed by text + icon, not colour (spec §8.9 run-status vocabulary);
  360 px width; `prefers-reduced-motion` honoured; no animation on streaming content.
- Privilege and privacy: error copy is one sentence of what happened plus one of what to do — no client
  names, matter IDs, stack traces or provider text on screen (correlation id behind "details" only).
  Client data is never passed to a hover-only tooltip nor written to logs. The robot-cat mark stays
  unchanged.

## 4. Playwright tests (REQ ids where they exist)

- `REQ-001 send creates one optimistic message that is not duplicated`
- `REQ-007 pending approval card names the exact action and consequence, takes focus, and does not block reading`
- `REQ-009 unread divider moves and the snooze shelf returns the matter on time`
- `REQ-012 every run status renders a distinct named chip with the right terminal action`
- `REQ-026 computer panel names starting, running, handover and released states`
- `REQ-028 takeover and REQ-029 release are keyboard-operable and announced`
- `REQ-038 board shows matter-stage columns and a keyboard-only ticket move`
- `REQ-057 Hebrew RTL loads dir=rtl with a clean axe run`
- New behaviour without a REQ yet (matter-status strip, privileged caption) must be assigned a REQ id
  by the owner before implementation, per the traceability gate — do not invent one in test names.

## 5. Highest-impact first slice

**Change (b), the legal-action approval card.** Smallest, highest-value step: one screen, no new
transport (`threadsAnswer`/`threadsReact` already exist), no new component (BoardUI `Button`/`Badge`/
`Tooltip` are installed), and it directly reduces the risk of an implicit, unattributable legal action
with privilege consequences. Test-first: write `REQ-007 ... names the exact action and consequence`
and extend `spec/golden/ui/REQ-007-pending-card.json` first; ship only the dominant consequence
sentence, the explicit confirm label, the authorisation line, and the text+icon privileged marker.
Defer (a), (c), (d), (e) until the owner assigns REQ ids and approves.
