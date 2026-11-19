# 0024 — app/app/g/[groupId]/page.tsx — reliability (v0.4.0)

Worker #024 · module `apps/web/app/app/g/[groupId]/page.tsx` · lens: reliability · READ-ONLY
review of `rebuild-workspace/.worktrees/before-m5` (v0.3.0-rc5). Proposals only; no code here.
Spec anchors: `spec/08-ui-ux.md §8.3`, `spec/01-requirements.md` REQ-010 (mention routing),
REQ-068 (reassign), REQ-020/REQ-021 · golden `spec/golden/group-create.json` · ADR-013.
Scope note: the page is a thin server component; runtime risks live in `GroupThreadScreen` and the
static-export dispatch. Findings tagged [page]/[screen].

## What the route is today (CONFIRMED, read this session)

[page] `generateStaticParams()` pins exactly `group_golden`; no `dynamic`/`dynamicParams`
declaration. `GroupPage` awaits `params`, renders `<AppShell><GroupThreadScreen/></AppShell>`.
Non-pinned ids in production fall through Rust static → `app/index.html` → `AppEntry`, whose
`botIdFromPath()` matches `^/app/([^/]+)/?$` (single segment) → `null` for `/app/g/<id>/` →
renders `ThreadsHome`. **Deep links to any non-golden group render the home, never the room**
(same dispatch gap wave-1 #0012 flagged one route family over). CONFIRMED via `app-entry.tsx`,
`lib/bot-directory.ts`.
[screen] One `groupsGet`→`groupsMessages` chain on mount (AbortController wired); send →
`threadsSend` then full message reload; reassign → `groupsReassign` then full get+messages
reload. No polling, no cursor use, no error surface on send, no boundary anywhere.

## Failure modes (reliability lens)

1. **Missing group — one bucket for every failure.** `.catch()` maps all `groupsGet`/
   `groupsMessages` failures (404 gone, 403 not a member, 5xx, network) to phase
   `unavailable`, copy `groups.unavailable`, link home, **no retry**. Transient blip and
   missing group are indistinguishable; `RpcError.status`/`code` discarded.
2. **Member list drift.** `memberBots`/`attribution` fetched once at mount; `memberBots[0]`
   is the hard-coded send target. Mid-session changes (bot removed/renamed, another client's
   reassign) are never reconciled: stale attribution falls back to rendering the **raw
   internal botId**; stale `memberBots[0]` misroutes/fails the send; empty `memberBots` makes
   `send()` `return` silently inside `try` (text kept, zero feedback). `groupsList` imported,
   unused.
3. **Message send failure — silent by design.** `catch {}` ("refusals surface via the wire");
   user sees nothing. Worse, a retry after an ambiguous failure mints a **new** `clientNonce`
   (`web-${Date.now()}-${Math.random()}`, not `crypto.randomUUID`), so server dedup cannot
   collapse it — retry can double-post and double-fire bot runs. Draft is kept (good) but no
   error state exists to say so.
4. **Stale transcript, no freshness path.** First page only (`nextCursor` ignored); no poll
   after mount, so bot replies to others' messages never appear until the user sends or
   reassigns themselves. `logRef` attached to `<ol>` but never scrolled —
   `role="log"`/`aria-live` announces rows the user may never see.
5. **Lost update on reload races.** Send and reassign both replace `messages` wholesale from
   fresh `groupsMessages`; concurrent reloads are last-writer-wins; no seq/cursor merge; the
   user's just-sent row can vanish until the next reload.
6. **No error boundary; [page] param unvalidated.** Render throws blank the export. No
   `error.tsx` under `app/app/g/[groupId]/`, no `global-error.tsx` at root (verified pattern
   in 0012). `groupId` flows raw into the tree; there is no length/charset check and no
   `notFound()` path — contrast with H1 in 0012.
7. **[page] Export pins unspecified for new groups.** Only `group_golden` emitted; dynamic
   groups have no HTML and no client dispatch. Failure is silent — user lands on ThreadsHome
   and believes the room is gone.

## Hardening for v0.4.0 (in order)

- **H1. Fix the export/dispatch gap first.** Either (a) add `/app/g/<id>` handling to
  `AppEntry` (two-segment match + group screen import, mirroring ThreadScreen), or (b) pin
  `dynamicParams = false` + placeholders and document non-golden groups as export-unavailable.
  Decide with ADR-013 owner; (a) is the only option that makes the room reachable. Biggest
  reliability win in this module.
- **H2. Classify load failure.** Catch `RpcError`; 404/403 → `unavailable` copy; 5xx/network →
  new `error` phase, `role="alert"` + explicit **Retry** (re-run effect via `retryToken`).
  Distinct i18n keys; `groups.unavailable` keeps meaning "gone / not yours".
- **H3. Reconcile members, don't trust the mount snapshot.** Rebuild `attribution` from every
  `groupsGet` result, refresh `group` after send/reassign, raw-id fallback →
  `t("groups.member.unknown")`. Send must use a **live** member (re-read at submit), not
  `memberBots[0]`; empty member list → disabled composer + explanatory copy, never silent return.
- **H4. Make send failure legible + retryable.** On catch, set `sendError` (role="alert",
  dismissible), keep draft. Reuse ONE `clientNonce` per draft (mint on first attempt; regen on
  clear/edit) so retry dedups server-side. `crypto.randomUUID()` with `web-…` fallback.
- **H5. Freshness + ordering.** Honor `nextCursor` (older on scroll-up) OR poll
  `groupsMessages` 5–10s while visible (backoff on error, pause on hidden); merge by id, sort
  by server seq. Scroll to bottom on own-send only; otherwise preserve position.
- **H6. Boundaries + validation.** Add `app/app/g/[groupId]/error.tsx` (client, reset() retry,
  §8.3 copy) + `app/global-error.tsx` if absent; validate `groupId` at the page (trim, cap 128,
  reject `/\%` + control chars) → `notFound()`. Declare `dynamic`/`dynamicParams` per this
  Next version's docs (`node_modules/next/dist/docs/`) — not training data.
- **H7. Hygiene.** Remove unused `groupsList` import; render send-pending state on the form
  (`aria-busy`); dismiss `reassignFailed` on next attempt.

## Error states to add (i18n, five locales, mirror `lib/i18n.ts`)

`groups.error.load` (transient + Retry), `groups.send.error`, `groups.members.empty` (composer
disabled), `groups.member.unknown` (attribution fallback), `groups.transcript.error` (poll
banner, non-blocking).

## Test plan (REQ-named)

- `REQ-010 missing group renders not-available, transient failure renders retry and recovers`
  (mock 404 vs 500; retry path).
- `REQ-010 send failure shows role=alert and preserves draft; retry reuses clientNonce`.
- `REQ-010 empty member list disables composer with copy, never silently swallows send`.
- `REQ-068 reassigned run attributes correctly after member removed mid-session` (attribution
  refresh, no raw botId leak).
- `REQ-020 deep link /app/g/<non-golden>/ renders the room, not ThreadsHome` (export e2e,
  `BOMA_EXPORT_E2E=1` lane or dev-server equivalent after H1a).
- `REQ-021 render crash in the group route is contained by error.tsx` (RTL unit test, per
  0012 pattern).
- Export assertion: `out/app/g/group_golden/index.html` exists and carries shell marker.

## Risks

- **R1** H1(a) touches `AppEntry`, which sits over the whole `/app` family; a regex or bundle
  slip blanks every route. Mitigate: additive two-segment match, goldens + axe green, ADR-013
  owner sign-off.
- **R2** H5 polling adds wire traffic and race surface; a bad merge can resurrect deleted
  messages. Mitigate: merge by id with server-authoritative replace; ship behind a flag,
  measure before default-on.
- **R3** Retry-nonce reuse depends on server dedup semantics for `threads.send`; if dedup is
  keyed per (botId, nonce) and the user edits the draft between attempts, reuse would wrongly
  collapse distinct messages — hence regen-on-edit rule in H4. Verify contract before merge.
- **R4** H2/H4 split "unavailable" copy: screenshot/axe goldens pinning current strings must
  be updated; docs/PARITY.md entry required (externally visible).
- **R5** Unvalidated-param `notFound()` changes behavior for absurd-but-harmless ids in dev;
  low risk, record in PARITY.md.
- **R6** `crypto.randomUUID` needs secure context (loopback fine). Mitigate: feature-detect.
- **R7** Member refresh on every send adds one RPC per send; acceptable, but watch p95 on
  `groups.get` and piggyback on the existing post-send `groupsMessages` reload if budgets bite.

## Non-goals / notes

No changes to `generateStaticParams` pins short of H1/H6 decisions, the wire contract, the
mention-routing engine, or live streaming (owned by thread-screen proposals); no new
dependencies; no spec edits. CONFIRMED items were read this session; H1 approach selection,
R3/R4 outcomes and all test results are GUESS until run. Next-version export APIs must be
read from `node_modules/next/dist/docs/` before implementing — this document does not pin
their names.
