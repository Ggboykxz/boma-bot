# Worker #086 — `bot-rail.tsx` reliability hardening (v0.4.0)

- Module: `apps/web/components/shell/bot-rail.tsx`; lens: reliability; mode: READ-ONLY proposal.
- Evidence marked CONFIRMED were read this session from the target plus `lib/roster.ts`,
  `lib/bot-directory.ts`, `lib/api/rpc.ts`, `components/shell/app-shell.tsx`,
  `components/shell/acp-detected-row.tsx`, `spec/01-requirements.md` (REQ-009/010/011).
- No wire/API change is proposed; anything externally visible is flagged as a DEVIATION below.

## Failure modes

### FM1 — Detection RPC failure (CONFIRMED)
- `groupsList` (L91-94) and the group-create `botsList` (L102-104) both end in
  `.catch(() => setX([]))`. A timeout, 5xx, 401, or a navigation abort is treated as "empty", so
  previously loaded groups/bots are wiped, a transient failure reads as "no groups"/"no bots", and
  there is no error state and no retry. Aborting on `pathname` change (L94) actively fires the catch.
- There is no timeout: `rpc.call` (rpc.ts L245-272) only forwards the component's `AbortSignal`;
  a hung request leaves `bots === null` → "loading" forever (L267-268).
- `refreshRoster` (roster.ts L36-45) swallows every failure. If the first fetch fails, `roster`
  stays `null`; with an empty client directory the rail shows only `bots.empty` (L168-170) with no
  recovery. The child executor probe (`AcpDetectedRow`, mounted at L195) likewise fails silently.
- No in-flight de-dup: rapid pathname changes can race overlapping group fetches.

### FM2 — Active-bot drift (CONFIRMED)
- "current" is exact string equality `pathname === /app/${bot.id}/` (L177, L188, L217, L308).
  `bot-directory.ts` already provides `botIdFromPath` (L60-67) to normalize trailing slashes; the
  rail does not use it, so a highlight is lost when `usePathname()` omits the slash.
- Two stores diverge: roster (L85) vs directory (L86). Before the roster arrives, fallback rows
  render from the directory; when it arrives the rows swap and the active bot can vanish if it is
  archived/snoozed (L161-162). A snoozed active bot lands on the shelf, which defaults to collapsed
  (`expanded = false`, L76), so the user loses "where am I".
- REQ-009 requires a snoozed chat to return "when the snooze ends", and the header comment claims
  it "returns on its own" (L68-71), but no timer exists — it only returns after a navigation or
  an action calls `refreshRoster`. Snooze state is stale until then.
- Space drift: RPC scope is a module-level `knownSpaceId`/localStorage (rpc.ts L191-211); the rail
  refetches on `[pathname]` only (L88-95). A space switch without a path change leaves the prior
  space's roster (and a possible id collision highlighted as active). `subscribeRoster` listeners
  fire only from `refreshRoster` (roster.ts L40), so unread badges drift after push/other tabs.
- `createGroup` (L128-149) injects synthetic `ownerBotId: ""` / `createdAt: ""` and lets `selected`
  (L81) hold ids archived since load; a stale id becomes a generic error (L287-291). `unsnooze`
  (L152-159) ignores failure and refreshes unconditionally while `refreshRoster` swallows errors.

### FM3 — Mobile collapse (CONFIRMED)
- `app-shell.tsx` L56-61 stacks the rail (`w-full`) above `main` below `lg`; the rail has no
  collapse control and no height cap. On 360px the whole rail (create forms + bots + groups +
  activity) pushes the thread below the fold; AGENTS.md makes 360px / WCAG 2.2 AA a gate.
- The `<aside>` (L165) has no `max-h`/scroll; `lg:w-56` is fixed. Many bots or an expanded snoozed
  shelf (L211-224) grow the nav unbounded, making Activity/snooze controls unreachable on short
  viewports; nothing is sticky.
- `RosterRow` link (L39) is `inline-flex flex-1` with a `truncate` name (L41) but no `min-w-0`;
  a large unread badge (L43-48, e.g. "9999") does not shrink and can squeeze the name to zero.
- `expanded`/`creating` (L76-83) are not responsive; `autoFocus` on the group name (L257) raises
  the mobile keyboard over content with no way to collapse the rail.

## Hardening proposals (v0.4.0)

- **H1 (FM1):** Replace the two bare catches with a tri-state `{status, data}` that keeps
  last-good data on error, skips `setState` on `AbortError`, and sets an error flag; add a
  `role="alert"` message + retry button using existing tokens and a new catalogue key
  (e.g. `shell.rail.load.retry`). Add a local `withTimeout(signal, ms)` wrapper (or a timeout in
  `rpc.call`, larger blast radius) so a hung request resolves. *Risks:* new i18n keys need all
  locales; stale data may briefly show old counts; retry control changes golden DOM. Internal only.
- **H2 (FM2):** Use `botIdFromPath(pathname)` for bot rows and an analogous group-id parse for
  group rows; derive initial `expanded` from whether the active bot is snoozed, and auto-expand
  when it is. Add a bounded timer to the nearest `snoozedUntil` that calls `refreshRoster()` and
  clears on unmount/visibility, making REQ-009's return real without navigation. Add space id to
  the effect dependency (read in an effect to avoid hydration mismatch) so space switches refetch.
  *Risks:* timers need cancellation/pause when hidden; auto-expand alters snoozed-state goldens.
- **H3 (FM3):** Add a `<lg` "Navigation" disclosure (`aria-expanded`/`aria-controls`) wrapping the
  rail content in a scrollable, height-capped region (reuse BoardUI `ScrollFade` per AGENTS.md);
  keep a visible, keyboard-reachable toggle with `focus-visible:ring-2`. Add `min-w-0` to the
  RosterRow link and `shrink-0` to the badge. *Risks:* visible layout change → record a DEVIATION
  in `docs/PARITY.md` and get approval; update shell landmarks goldens; never hide the only
  mobile navigation path behind an unreachable toggle.
- **H4 (FM2/FM1):** Drop synthetic group fields — refetch `groupsList` after `router.push` so the
  canonical row replaces the optimistic one; filter `selected` against current `bots` before
  submit; branch on `RpcError.code` for actionable messages; make `unsnooze` report failure and
  retry. *Risks:* extra round-trip; possible new i18n keys.

## Verification (proposed; not run — read-only)
- Playwright, REQ-tagged: REQ-009 "snoozed bot returns without navigation" (fake timers), "rail
  keeps last-good data on `bots.list` failure + retry", "active highlight with/without trailing
  slash", "360px rail collapses, Activity reachable by keyboard"; axe zero critical/serious.
