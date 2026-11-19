# 0091 — apps/web/components/shell/threads-home.tsx — architecture (v0.4.0)

Worker #091 · module `apps/web/components/shell/threads-home.tsx` · lens: architecture · READ-ONLY
review of `before-m5` (v0.3.0-rc5-era source). Proposals only; no product code edited here.

## What the file is today

20 lines, `"use client"`, no props, no state, no data. It renders one `<h1>` from
`shell.nav.threads` and one `<p>` from `shell.placeholder` inside a `flex flex-col gap-3` div;
both already use BoardUI tokens (`text-headline-medium`, `text-text-primary`, `text-text-secondary`,
`max-w-prose`). The docstring is accurate: the rail owns the bots, the centre only says what to do
next, and every word comes from the catalogue.

Placement: `app/app/page.tsx` mounts `<AppShell><AppEntry/></AppShell>`; `AppEntry` calls
`botIdFromPath()` via `useSyncExternalStore` (server snapshot `null`) and returns `<ThreadsHome/>`
when there is no id — so under ADR-013 this is also the first paint of every `/app/*` deep link,
not just `/app/`. The public `/` landing (`landing-screen.tsx`) is a different screen; do not merge
the two.

## Architectural gaps

1. **No contract, no REQ.** §8.1 calls `/app` "shell with the selected bot's thread"; §8.3 lists
   thread states but no "no bot selected" state. The home exists in code, not in the spec, and no
   Playwright test names a REQ for it (the shell suite covers REQ-057 only). Promoting it needs an
   explicit requirement (a new extension REQ, e.g. REQ-076) plus a REQ-named test — UI/UX policy
   demands the REQ id in the test title, and any user-visible change beyond the current copy is a
   DEVIATION to record in docs/PARITY.md.
2. **One generic sentence is the whole design.** `shell.placeholder` is translated in only en/fr/he
   (i18n.ts:29/382/735); the other eleven locales fall back to English. That fallback is legal
   under REQ-057, but a real landing multiplies the copy (title, guidance, action, empty/error) —
   without a key policy the fallback becomes accidental, not designed.
3. **It is a fallthrough, not a route.** `AppEntry` renders it for any path `botIdFromPath()` cannot
   parse — including `/app/g/<id>/` deep links and malformed ids (brief #0014, finding 7). A landing
   that starts offering actions must not be the error sink.
4. **No data boundary, no states.** The spec requires skeletons, legible errors and an empty state;
   a static paragraph has none. When this becomes a real landing, data must come from the existing
   read procedures (`botsList`, `runsList`) or the shared `lib/roster`/`lib/bot-directory` stores —
   never a second module-level cache.

## Concrete upgrades for v0.4.0 (dependency order)

### U1. Give the home a spec/contract and a REQ-named test before changing markup
Write the "no bot selected" state into §8.1/§8.3 (or a small ADR) as a named screen: heading,
one-sentence guidance, one primary next action. Add extension REQ-076 with
`test("REQ-076 ...")` in `tests/shell.spec.ts`: h1 from the catalogue, a focusable action, single
`main`, and the deep-link case. Record the copy/behaviour delta in docs/PARITY.md and get owner
sign-off before shipping — the spec has no home today, so this is a DEVIATION if done silently.
- Risk: owner veto or REQ numbering collision. Mitigate: propose the number in docs/STATE.md and
  keep the current h1 key (`shell.nav.threads`) so REQ-057 shell tests stay green.

### U2. Compose the landing from installed BoardUI components, tokens intact
Replace the bare `<div>` with a small composition: a Card-like surface (install via BoardUI MCP if
not present under `components/base/` — do not hand-build a lookalike), one `ButtonLink`/`Button`
next action, `text-headline-medium`/`text-body-medium`, `bg-background-primary-default`,
`border-separator-border`, `cx()` for merges, Remixicon refs, `boma-focus` on the action, logical
`ps`/`ms` so RTL mirrors. Keep dark mode on tokens only — no `dark:` literals, no raw palette.
- Risk: new landmarks/roles shift axe and golden snapshots; a Card may not be installed. Mitigate:
  check `components/base/` and `components/application/` first, replay goldens, axe at 360 px in
  light/dark/RTL, and keep the `<h1>` count at one.

### U3. Make it data-aware through the existing read boundary, with real states
Source "recent threads / suggested bots" from `botsList` and the shared roster store and, if useful,
`runsList` for recent activity; render loading skeletons, an error with `role="alert"` and a
catalogue retry, and a genuinely empty state (no bots yet → point at the rail's new-bot form).
One `AbortController` per mount, no new procedure (a wire change = spec edit + trace + ADR).
- Risk: duplicating the rail's roster becomes a second invisible state layer; RPC latency puts a
  blank panel on the ADR-013 first paint. Mitigate: reuse `useSyncExternalStore(subscribeRoster,
  listRoster, serverRoster)`, render the static heading/guidance synchronously and only the data
  region behind the skeleton, and never block the primary action on a fetch.

### U4. Stop using the landing as the route fallthrough
Coordinate with #0081 U3 and #0014 H1/H3: one pure `isAppHome(pathname)` in `lib/nav.ts`; `AppEntry`
renders `ThreadsHome` only for `/app` and `/app/`; bot and group ids go to their screens and
malformed ids to not-found, not to the landing. `botIdFromPath()` stays the single id parser.
- Risk: ADR-013 first paint is the home for all deep links, so tightening dispatch changes
  hydration order and golden `aria-current` state. Mitigate: table-driven tests for `/app/`,
  `/app/<id>/`, `/app/g/<id>/`, bad encodings; replay shell goldens and the export path in CI.

### U5. Catalogue and test parity, not just English polish
Add every new key to en/fr/he (the three full catalogues) and an explicit note in i18n.ts that the
other eleven locales fall back to English by rule (REQ-057); keep emails/URLs/bot names in `<bdi>`
or `dir="auto"`. Extend the REQ-057 label test to the new action and the RTL test to its mirroring.
- Risk: key churn across 14 locales and a fallback that looks like a bug in screenshots. Mitigate:
  a key-audit test (every `t("shell.home.*")` key exists in en) and asserting the known fallback in
  tests rather than treating it as a defect.

## Execution order & gate
U1 (spec + REQ + owner sign-off) → U2 (structure) → U4 (dispatch, shared with #0081/#0014) →
U3 (data) → U5 (copy/tests throughout). Each step: shell + new REQ-076 Playwright green, axe 2.2 AA
clean (single `main`, 360 px, light/dark, RTL), UI goldens green, docs/PLAN.md bundle budget held.

## What not to touch
The public `/` `LandingScreen` probe and its CTA; `shell.nav.threads` (pinned by REQ-057 tests);
BoardUI semantic tokens/composite type scale; `AppShell`'s single `<main>`; ADR-013 static fallback;
`botIdFromPath()`'s contract beyond the shared parse fix.

CONFIRMED by reading `threads-home.tsx`, `app-entry.tsx`, `app/app/page.tsx`, `landing-screen.tsx`,
`i18n.ts`, `shell.spec.ts`, spec §8.1/§8.3 and briefs #0014/#0081. GUESS: REQ number, Card
availability, and golden/axe deltas — verify before implementation.
