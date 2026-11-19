# Worker #114 — `roster.ts` reliability hardening (v0.4.0)

- Module: `apps/web/lib/roster.ts`; lens: reliability; mode: READ-ONLY proposal.
- Evidence marked CONFIRMED were read this session from the target plus `lib/api/rpc.ts`,
  `lib/bot-directory.ts`, `components/shell/bot-rail.tsx`, `components/thread/thread-screen.tsx`,
  `components/shell/bot-create-form.tsx`, `components/shell/acp-detected-row.tsx` and the
  reliability proposal for worker #086 (`bot-rail.tsx`).
- No wire/API change is proposed; anything externally visible is flagged as a DEVIATION below.

## Failure modes

### FM1 — Stale roster (CONFIRMED)
- `refreshRoster()` (L36-45) stores whatever the last completed call returned, with no timestamp,
  version, or `AbortSignal`. `rpc.call` (rpc.ts L245-272) has no timeout of its own, so a hung
  `bots.list` leaves the roster frozen at its previous value (or `null` on first load) forever.
- Three independent callers fire refreshes: rail on every `pathname` change (bot-rail L88-95),
  thread actions (thread-screen L258), bot create (bot-create-form L43) and the ACP row
  (acp-detected-row L44). There is no single-flight and no sequencing: an *older* request that
  finishes last overwrites a newer answer (last-completed-wins). Rapid navigation can therefore
  restore a roster already replaced by a newer one.
- Store scope is the module-global `knownSpaceId`/localStorage (rpc.ts L191-211). A space switch
  with no path change keeps the prior space's roster; listeners fire only from `refreshRoster`
  (L40), so unread counts and snooze state never update from push, other tabs, or a timer. REQ-009's
  "snoozed chat returns when the snooze ends" is not enforced here.
- SSR/client snapshot mismatch is latent: `serverRoster()` returns `[]` (L15, L23-25) while
  `listRoster()` returns `null` before the first answer (L17-20); the rail branches on `roster ===
  null` (bot-rail L161-162, L168-172), so hydration can paint one state then swap to another.

### FM2 — Missing members (CONFIRMED)
- `null` conflates "not fetched", "in flight", and "failed": on the first `botsList()` rejection the
  catch (L43-45) is silent, `roster` stays `null`, and the rail shows only `bots.empty`
  (bot-rail L168-170). The store exposes no error/status signal, so a transient outage reads as
  "no bots" and there is no retry path inside the store.
- `botsList()` (rpc.ts L422-425) is a bare `call<BotsListReply>` cast: unlike `threadsMessages`
  (L468-482) and `groupsMessages` (L1460-1481) it performs **no normalization**. A shape drift,
  partial reply, or server bug that returns `bots: undefined`/non-array makes `roster` that value;
  consumers then call `.filter()` (bot-rail L161-162) and throw. Rows missing `id`/`name`/
  `threadId`/`unreadCount` are published verbatim and break keys and labels.
- After an action (create, snooze, unsnooze) a failed refresh silently drops the affected member
  from the rail with no error; `bot-create-form` L43 and `unsnooze` (bot-rail L152-159) rely wholly
  on this swallow-and-hope path.

### FM3 — Dedup (CONFIRMED)
- No in-flight coalescing: the callers above can issue several concurrent `botsList()` RPCs for the
  same snapshot (wasted work plus the FM1 overwrite race).
- No entry dedup: `roster = reply.bots` (L39) publishes duplicates unchanged. A repeated `id`
  (e.g. from a join/replicated membership) renders duplicate rail rows with duplicate React keys —
  reconciliation warnings and a confusing UI. `bot-directory.registerBot` (bot-directory L22-31)
  already does id-based replace-or-append; the roster does not mirror it.
- No change detection: every successful refresh assigns a fresh array and notifies all listeners
  (L39-42), so `useSyncExternalStore` subscribers re-render even when the payload is unchanged.

## Hardening proposals (v0.4.0)

- **H1 (FM1):** Give the store a typed snapshot `{ status: "idle"|"loading"|"ready"|"error",
  data, fetchedAt, error }`; stamp `fetchedAt`, expose a "stale" flag when it exceeds a bound, and
  let consumers show a retry instead of `bots.empty`. Thread an `AbortSignal` through
  `botsList(signal)` and add a bounded timeout so a hung call settles. Apply a generation counter
  so only the newest *started* refresh may publish. Wire a visibility/focus revalidation and a
  timer to the nearest `snoozedUntil`. *Risks:* new public shape breaks the three consumers and
  worker #086's rail plan; timeout value is a guess; a visible "stale"/retry control changes golden
  DOM and needs i18n keys in all locales. Keep behavior stable when status stays `ready`.
- **H2 (FM2):** Add a `normalizeRosterBot(raw): RosterBotView | null` and a `normalizeRoster(reply)`
  in `rpc.ts`/`roster.ts` that drops malformed rows, coerces `unreadCount` to a finite number and
  defaults nullable timestamps — matching the existing message normalizers. On error keep the
  last good snapshot *and* set `status: "error"` so "failed" is distinguishable from "empty".
  *Risks:* dropping a valid-but-unusual row is a behavior change → record a DEVIATION if a golden
  depends on it; normalization adds a small hot-path cost; coercion must not mask server defects
  silently (log/count drops in dev).
- **H3 (FM3):** Coalesce with a module-level `inFlight: Promise<void> | null` shared by all callers
  (callers await the same promise; clear in `finally`), and dedup entries by `id` with a
  first-or-last-wins policy and stable order before publish. Skip notify + reuse the prior array
  reference when the normalized payload is structurally equal (shallow per-field compare).
  *Risks:* coalescing means a caller may receive another's failure; use `try/finally` so a
  rejected promise is never cached. Dedup policy and ordering changes are externally visible →
  DEVIATION if any golden asserts duplicate rows; equality check must not suppress genuine
  unread-count changes.
- **H4 (FM1/FM2):** Key refreshes by space id (read in an effect, not at module init) so a space
  switch invalidates the snapshot, and clear roster on sign-out/401 via the existing unauthorized
  handler. Make `serverRoster()` and the first client snapshot agree to avoid hydration churn.
  *Risks:* clearing on 401 can briefly blank the rail before redirect; snapshot agreement may
  require a stable empty constant shared by server and client.

## Verification (proposed; not run — read-only)
- Unit: `bots.list` failure keeps last-good data and sets `status: "error"`; malformed/duplicate
  rows are dropped/deduped; out-of-order responses cannot overwrite a newer snapshot; concurrent
  calls issue one RPC; timeout settles a hung fetch.
- Playwright, REQ-tagged: REQ-009 "snoozed bot returns without navigation" (fake timers), "rail
  shows retry on `bots.list` failure", "no duplicate rows after duplicate ids"; axe zero
  critical/serious.
