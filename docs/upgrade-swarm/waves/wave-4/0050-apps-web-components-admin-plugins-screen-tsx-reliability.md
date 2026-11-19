# 0050 · apps/web/components/admin/plugins-screen.tsx · LENS=reliability · v0.4.0 proposal

## Scope note

This module is the stateful screen (168 lines), not the thin `app/admin/plugins/page.tsx` wrapper
reviewed in wave 1 #0012. All reliability logic lives here plus `lib/api/rpc.ts` (`call`,
`adminPluginsList`, `adminPluginsDecide`) and the Rust side (`admin_plugins_list/decide`,
usecases.rs:9160-9235). Everything below is CONFIRMED by reading before-m5; server-behaviour claims
are from usecases.rs/wire.rs, not inferred. No code edits here.

## Current behaviour (as read)

- `phase: loading|ready|error|forbidden`; one `adminPluginsList({status:"pending"})` on mount and on
  `reloadToken` bumps, guarded by an `AbortController` that aborts on unmount and on re-run (39-60).
- `deciding: string|null` gates both buttons of one row (31, 62-76, 142/151). On success the row is
  filtered out locally (68); any `catch` shows `t("admin.plugins.error")` (70). `decide` passes no
  signal; `call()` wraps `fetch` with no timeout (rpc.ts:245-272).
- Render is text-only: source in `<code dir="auto">` (133), `name ?? source` (132), raw `status`
  (134), `createdAt` received but unused. `reload` is reachable only from the error state (118-121).

## Failure modes

### Review races

1. **Shared `deciding` slot clobbers concurrent rows (31, 62-76).** Decide A, then decide B before A
   settles: B sets `deciding=B`; A's `finally` resets it to `null`, re-enabling B's buttons while B's
   RPC is still in flight → a second B click fires a duplicate decide → server answers `Conflict`
   → false "error" on an action that already succeeded. The same window exists for a fast
   double-click on one row before React re-renders `disabled`.
2. **Two-admin / two-tab contention is misreported.** `decide_submission` is atomic (non-pending →
   `Conflict`, usecases.rs:9189-9191), but the catch (69-71) ignores `RpcError.code`: a loser sees the
   same generic error as a 500, the row stays visible, and no recovery path is offered. The queue
   never refetches, so the stale row persists until a full page reload.
3. **No cancellation or deadline on `decide`.** No signal and no fetch timeout: a hung decide leaves
   both buttons disabled forever (`finally` never runs); unmounting mid-decide leaves `setSubmissions`
   / `setActionError` firing on a dead component. A client-side timeout may race a committed server
   write, so the retry becomes a `Conflict` (compounded by #2).

### Stale list

4. **Single load, no freshness controls.** No poll, no refetch-on-focus, and no manual refresh in the
   ready state — `reload()` exists only in the error branch. A second admin's decisions are invisible
   for the whole session. Combined with #2 the operator cannot self-heal a stale row.
5. **Capped, filtered view presented as complete.** `status` is hardcoded to `pending` (41), so
   `unconfirmed` submissions (a legal filter server-side, usecases.rs:9166-9168) never appear; the
   server caps at 200 with no total (9176-9178) and the client shows no "cap reached" hint. Backlog
   >200 or non-pending items are silently absent.
6. **Reload destroys review context.** `reload()` flips `phase="loading"` (35), unmounting the whole
   list into skeleton flash and losing keyboard focus mid-review — risky when the list is long.
7. **No age signal.** `createdAt` is in the view type but never rendered; nothing indicates how old
   (and how stale) a pending row is.

### Invalid submissions

8. **Reviewer approves blind.** No parse/validation detail (ref, subdir, size, duplicate-of-installed)
   is surfaced, so a malformed or duplicate submission is indistinguishable from a valid one before
   approving. Source is safely text-only (133) — keep it that way; never render it as HTML.
9. **Layout / i18n edges on hostile data.** `name ?? source` has no truncation or `break-words` (132);
   a very long name stretches the row on narrow widths. The raw `status` token (134) bypasses i18n.
10. **Latent reply-shape mismatch.** rpc.ts types decide as `Promise<{status:string}>` (rpc.ts:785-790)
    and wire.rs defines `AdminPluginDecideReply{status}` (1746-1752), but the usecase returns `()`
    (9193-9198). The reply is ignored today, so harmless — but any future `reply.status` read would
    throw on a null body.
11. **Silent audit loss.** `audit.append` is `let _ =` (9219-9233): a decision can commit while its
    audit line is dropped; the UI reports unqualified success and cannot prove who decided.
    (Submitter notification is explicitly absent today — usecases.rs:9183-9185 — not a UI defect.)

## Concrete hardening (v0.4.0)

- **H1 — Per-row pending set.** Replace `deciding` with `pendingIds: Set<string>`, added
  synchronously before the first `await` and removed in `finally`. Disables only the in-flight row;
  concurrent decisions proceed; double-click window closes. *Risk:* low — pair with a mounted ref so
  late `finally` updates cannot touch an unmounted tree.
- **H2 — Typed decide errors + reconciliation.** Branch on `RpcError.code`: `Conflict`/`NOT_FOUND`
  → drop the row locally, show a neutral "already decided — list refreshed" notice, trigger one
  background refetch; `FORBIDDEN` → `phase="forbidden"`; validation → server message verbatim (fixed
  safe strings); else generic error. *Risk:* one extra list call per conflict — bounded; reconcile by
  id, never blind `setSubmissions(reply)` or a concurrent decide's removal is clobbered.
- **H3 — Freshness controls.** Add a manual Refresh button in the ready header (reuse `RiRefreshLine`),
  `visibilitychange` refetch on focus, and an optional ≥30s poll gated on
  `document.visibilityState==="visible"`. Keep the list mounted during refetch (no `phase` flip;
  reserve full skeleton for first load). *Risk:* poll widens the stale-decision window slightly and
  adds ~1 list call/min/admin — acceptable, and H2 closes the loop.
- **H4 — Deadlines everywhere.** Compose `AbortSignal.timeout(10_000)` with the unmount controller
  (`AbortSignal.any` where available) for list and decide. Timeout → list `phase="error"`+retry;
  decide → H2 path, which self-heals a committed write. *Risk:* a 10s decide timeout on a slow but
  healthy server yields a Conflict-on-retry that H2 handles gracefully.
- **H5 — Surface truncation and age.** Localize the status badge; render `createdAt` via
  `Intl.DateTimeFormat`; add `break-words` to the name; show "showing first 200 — cap reached" at
  200 rows and document why `unconfirmed` is hidden (or add a status toggle if REQ-058/§8.7 intends
  it reviewable). *Risk:* count is derived from the capped page, not a server total — label it
  accordingly, or add `total` to the reply (wire change → DEVIATION, needs approval).
- **H6 — Read-only review detail.** Before deciding, expand parsed ref/subdir/source host/age and any
  duplicate-of-installed marker the backend already computes; keep `<code dir="auto">` text-only and
  defer any missing detail endpoint rather than parsing submitted code in the client. *Risk:* scope
  creep — a behaviour-neutral UX improvement, not a new capability.
- **H7 — Honesty about the write.** Do not gate approve on the audit append; record the silent-audit
  gap in STATE.md, and only consider a `{status, audited}` reply as a wire change (DEVIATION). Align
  the TS decide reply type with the actual server body (or keep ignoring it) to remove the latent
  null-read. *Risk:* response-shape changes require spec/PARITY.md sign-off.
- **H8 — A11y of async states.** `aria-busy` + `role="status"` on the list region, visually-hidden
  "Loading submissions…"; keep skeletons `aria-hidden`; announce the H2 conflict notice via
  `role="status"`. *Risk:* none.

## Verification plan

- Playwright: two admin contexts decide one row concurrently → loser sees neutral notice, row
  vanishes, zero false error banners; double-click approve fires exactly one RPC (assert request count).
- Offline/throttle: list → error+retry within ~10s; decide → H2 timeout path self-heals after refetch.
- Seed a decide from a second tab, then click in the first → row removed by id, no unhandled rejection.
- axe on loading/ready/error/forbidden states; keyboard-only approve/reject; 360px + RTL pass.

## Explicitly rejected / non-goals

- No change to the atomic server decide, no client "optimistic approve", no auto-install: review
  never executes submitted source. No new dependencies; all fixes ride existing `RpcError`, BoardUI
  tokens and `Ri*` icons. Page-wrapper concerns already filed under wave 1 #0012 are not repeated.
