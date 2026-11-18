# UI proposal 026 — quality-control sector (verify before you trust)

Status: PROPOSAL. Sector framing and priorities are GUESS (a quality lens, not in `spec/`);
file, component, token and RPC references are CONFIRMED (read this session). Read-only on product
code; this file is the only write. Grounding: `docs/UI.md`, `spec/08-ui-ux.md`,
`apps/web/components/board/board-screen.tsx`, `apps/web/components/activity/activity-screen.tsx`,
`apps/web/components/artifacts/artifact-viewer.tsx`, `apps/web/lib/api/rpc.ts`,
`apps/web/lib/run-status.ts`, `spec/golden/ui/`. No upstream brand, code, wording or asset consulted.

Charter (CONFIRMED): Boma Bot only; fresh design (ADR-015) with the owner-supplied robot-cat as the
sole identity; no upstream mark or wording; self-hosted only (no SaaS, no output leaving the
machine); BoardUI semantic tokens only — no raw palette classes or hex literals — classes merged
with `cx()`; one design system, no second library, no hand-built lookalike of an installed component.

## 1. Persona and top jobs-to-be-done

**Persona — "the quality controller":** the person accountable for what the bots *produced*, not
just what they were asked to do. She works at a desk over long runs, sometimes on a tablet, serves
English/French/Arabic colleagues, and is measured on evidence: every output must be traceable to a
run and a version, every failure classified, every consequential effect gated. Boma Bot is
self-hosted; nothing she inspects leaves the machine. The robot-cat mark is the only identity art.

Jobs:
1. **Verify** — read a run's exact state out loud and see the evidence behind "done" (REQ-012).
2. **Triaging** — separate a real failure from a stopped/cancelled run, read a plain cause, retry
   only what is retryable (REQ-021).
3. **Trace** — tie an artifact to its hash, version chain, owner bot and originating run (REQ-036).
4. **Gate** — approve or refuse a consequential action after reading precisely what will happen
   (REQ-007/REQ-017); never confirm on colour alone.
5. **Quarantine** — move a suspect ticket through verification states and attach the run evidence
   without hiding the board when the deployment has it off (REQ-038).

## 2. Five concrete UI changes

All new strings are `lib/i18n.ts` keys through `useTranslation`/`t(locale, key)`; nothing hardcoded
in JSX. Every colour rides the `docs/UI.md` token map; every class list goes through `cx()`.

**(a) Run verification ledger — activity `/app` (REQ-011, REQ-012, REQ-067).**
One row per run, newest first, with text + icon chips for the full `lib/run-status.ts` vocabulary
(`queued`, `paused`, `leased`, `running`, `waiting_input`, `waiting_takeover`, `completed`, `failed`,
`cancelled`) and Pause/Resume on the working states. Add a filter by status and bot. BoardUI `Chip`,
`Tooltip`, `IconButton`, `Button`, `Select`; surface `bg-background-primary-default`, hairline
`border-separator-border`, text `text-text-primary`/`text-text-secondary`, icons
`text-foreground-icon-secondary`, type `text-caption-1-semibold`, focus
`focus-visible:ring-2 focus-visible:ring-border-focus-ring`. RPCs: `runsList`, `runsPause`,
`runsResume`, `runsRetry`. Status changes go to the polite live region, never the transcript.

**(b) Failure triage row — thread `/app/:botId` and activity (REQ-021, REQ-012).**
Distinguish failed from stopped/cancelled: one sentence of plain-language cause, one of what to do,
a working Retry only for `isRetryable` (`failed`), and the correlation id behind a "details"
`Tooltip` — no stack trace, id or provider text on the surface (§8.9). BoardUI `Button`, `Chip`,
`Tooltip`; `text-text-error-primary` **plus** icon and the word "Failed", `border-border-error-default`,
`bg-background-primary-default`, `.boma-target` so the row survives 360 px and touch. RPCs:
`runsList`, `runsRetry`, `threadsMessages`.

**(c) Artifact integrity / provenance panel — `/app/artifacts/:artifactId` (REQ-036).**
Surface the full sha256 (copyable, in `<bdi>`), `rootArtifactId`, owner bot and originating run, and
the version chain newest-first with a per-version hash — the evidence needed to answer "is this the
version I approved?". Replace the raw `<select>` with BoardUI `Select`; add `Chip`/`Tooltip` for
type and size. Keep content untrusted: images/PDFs in a bounded frame, text in `<pre>`, never HTML.
Tokens `text-text-secondary` metadata, `border-border-button-default`, `bg-background-secondary-default`,
`cx()`. RPCs: `artifactsGet`, `artifactsListVersions`, `artifactsListSpace`. GUESS: a
"re-verify checksum" action has no RPC — do not add the button or its test until the owner assigns
one; keep the hash read-only.

**(d) Consequential-action verification gate — thread pending card (REQ-007, REQ-017).**
Make the "what will happen" sentence dominant and the confirm label name the verdict ("Approve
payout", not "Approve"). High-impact cards pair `border-border-error-default` + `text-text-error-primary`
with an icon and the words "High impact" — never colour alone. Focus lands on the card; the
transcript stays readable and other threads reachable; the answered state replaces controls with an
attributable confirmation line. BoardUI `Button`, `Chip`, `Tooltip`, `textarea`; `rounded-3xl`, `cx()`.
RPCs: `threadsAnswer`, `threadsSubscribe`, `threadsMessages`; announced once via the polite live
region, the transcript itself is not a live region.

**(e) Verification queue on the board — `/app/board` (REQ-038).**
Treat columns as verification states; each card shows key, title, assignee bot and comment count, and
the detail panel shows the attached run's status chip. Replace the hand-built `<input>`, `<select>`
and `<button>` in the create form with BoardUI `Input`, `Select`, `Button`; keep the 404 "absent"
shape — with the flag off the route is absent, not locked. Tokens `bg-background-full` page,
`border-border-button-default`, `text-text-primary`, `text-text-tertiary` metadata, `cx()`. RPCs:
`boardsGet`, `botsList`, `ticketsCreate`, `ticketsMove`, `ticketsComment`, `runsList`.

## 3. Accessibility

WCAG 2.2 AA gate: zero critical/serious axe findings in light, dark and RTL; keyboard-only path for
every primary flow (retry, pause/resume, version switch, verdict, ticket move); visible
`focus-visible:ring-2 focus-visible:ring-border-focus-ring`; ≥44 px `.boma-target`; 360 px width;
`prefers-reduced-motion`; no animation on streaming content. Status, impact and failure are always
text + icon, never colour (exact vocabulary from `lib/run-status.ts`). The version `Select` keeps a
programmatic label; the provenance table uses `<caption>`/`<th scope>`. Reset/close in the dialog
returns focus to the trigger; Escape closes. Nothing is announced twice through the live region.

## 4. i18n and RTL

English is the source catalogue, fr/he the translations, missing keys fall back to English
(`docs/UI.md`). `DirectionProvider` gets an explicit BCP 47 locale and the boot script writes
`lang`/`dir` together. RTL mirrors the ledger rail and board columns and flips arrows/drawer motion
with `useDirection()`; keep screen coordinates physical. Logical utilities throughout (`ms`/`me`,
`ps`/`pe`, `text-start`/`text-end`, `border-s`/`border-e`). Hashes, run/ticket ids, URLs, dates and
version numbers stay LTR in `<bdi>`/`dir="auto"`. Error copy is one sentence of what happened plus
one of what to do — no id, stack trace or provider text on screen; the correlation id sits behind
"details".

## 5. Playwright tests (REQ id in each name)

- `test("REQ-011 the activity ledger lists runs newest first with a status and bot filter", ...)`.
- `test("REQ-012 every run status renders a distinct named chip with the correct terminal action", ...)`.
- `test("REQ-067 pause and resume are keyboard-operable and announced once", ...)`.
- `test("REQ-021 a failed run shows a plain cause, one retry, and the correlation id behind details", ...)`.
- `test("REQ-036 the artifact viewer shows the sha256, version chain, owner bot and run without rendering HTML", ...)`.
- `test("REQ-007 a pending card names the action and consequence, takes focus and does not block reading", ...)`.
- `test("REQ-017 approve and deny answer the card exactly once and leave an attributable confirmation", ...)`.
- `test("REQ-038 a ticket moves between verification states with its run status visible", ...)`.
- `test("REQ-057 Hebrew RTL mirrors the ledger and board and axe reports zero critical or serious findings", ...)`.
- Any new state gets a `REQ-<id>-<slug>.json` golden asserting seeded state, never a click path;
  the re-verify action (change c) gets no test until an owner assigns its REQ/RPC.

## 6. Best first slice

**Change (b), the failure triage row — GUESS, recommended.** It reuses landed RPCs (`runsList`/
`runsRetry`/`threadsMessages`), installed components (`Button`/`Chip`/`Tooltip`) and already-tuned
contrast tokens; no new transport, component, permission or wire format. One screen: give a failed
row one cause sentence + one action sentence, a visible Retry gated on `isRetryable`, and the
correlation id behind "details". Test-first: write
`REQ-021 a failed run shows a plain cause, one retry, and the correlation id behind details`, then
ship only that row. Defer (a), (c), (d), (e) until the owner confirms the sector framing and assigns
an id/RPC for the re-verify action; no externally visible deviation outside `docs/STATE.md` /
`docs/PARITY.md`.
