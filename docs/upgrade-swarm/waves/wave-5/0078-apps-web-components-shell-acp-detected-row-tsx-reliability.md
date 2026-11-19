# Worker #078 — Reliability proposal for `acp-detected-row.tsx` (v0.4.0)

Target: `apps/web/components/shell/acp-detected-row.tsx` (76 lines). Lens: reliability.
Status: proposal only — no code changed. Confidence: **[C]** = verified by reading this file,
`lib/api/rpc.ts` (l.152-179, 245-272, 492-503) and `lib/roster.ts` (l.1-46) this session;
**[G]** = inferred, needs a test to confirm. Spec anchor: REQ-061 (`01-requirements.md` l.620-634);
contract `03-api-contract.md` l.59-60.

## 1. Failure modes

### 1.1 Executor probe failure
- **[C] Probe failure is indistinguishable from "none installed".** The mount effect (l.23-29)
  catches every rejection and sets `executors = []`; the early return (l.31-33) then renders nothing.
  A 500, a dropped connection, a 401 (`RpcError.code/status`, rpc l.169-179), or a timeout all look
  exactly like a healthy remote deployment with no local executor. No retry, no operator signal.
- **[C] Unbounded probe.** `botsDetectedExecutors(controller.signal)` (l.25) is aborted only on
  unmount. A hung request leaves the row permanently empty with no loading state; no timeout is set.
- **[C] Malformed reply can crash the shell.** `call()` blindly casts `response.json()` to `T`
  (rpc l.271); `botsDetectedExecutors` does no normalization (rpc l.493-495). The component trusts
  `reply.executors` and reads `executors.length` (l.31) / `.map` (l.55). If the server ever returns
  `{}`, `null`, or a non-array field, `setExecutors(undefined)` throws on the next render and takes
  down the rail — not just this row.
- **[G] Duplicate/stale entries** are rendered verbatim as React keys (l.56); a duplicate executor
  string produces duplicate keys and a React warning.

### 1.2 Stale detection
- **[C] Fetched once and never refreshed.** `useEffect` has `[]` deps (l.29). An executor installed
  or removed after mount is never reflected. Detection is server-side and best-effort by contract
  (`03-api-contract.md` l.59-60): on a load-balanced deployment "the deployment machine" can differ
  between calls, so the list is inherently point-in-time — but the UI presents it as current truth.
- **[C] Attach targets a moving, unnamed bot.** `target` is recomputed each render as the *first*
  non-archived, non-snoozed roster bot (l.35). Between render and click the roster can change
  (`subscribeRoster`, l.21), so `attach` (l.37-51) may bind the executor to a different bot than the
  one implied. While `roster` is still `null` the button is disabled and "no-bot" is shown even if
  bots exist — a false dead state, not a real absence.
- **[C] `attached` is local-only and resets on remount.** It is set from `setAttached(executor)`
  (l.45) and never derived from server truth. After reload/reset the same executor shows "attach"
  for an already-attached bot, inviting a duplicate `connectAcp`. `refreshRoster()` is called
  (l.44) but `attached` is not reconciled with its result.
- **[G] No connection-status read-back.** No API call confirms an attachment after connect; the UI
  cannot know that the server accepted it beyond the resolved promise.

### 1.3 Connect errors
- **[C] Every connect error is swallowed.** The `catch {}` (l.46-47) gives the operator nothing: a
  transient network error, `NOT_FOUND`/`BAD_REQUEST`, `CONFLICT` (already attached), rate limit, or
  executor-gone-at-connect all produce the same silent re-enable. `RpcError.code/status` exists and
  is discarded.
- **[C] Hung connect wedges the row.** `botsConnectAcp({botId, executor})` (l.43) passes no signal
  though the RPC accepts one (rpc l.498-503). If it never settles, `working` stays `true` (l.41,
  l.49 in `finally` never runs) and every attach button is disabled forever.
- **[C] False "attached" on refresh failure.** `refreshRoster()` swallows its own errors
  (`roster.ts` l.36-46) and returns `void`. `setAttached(executor)` runs unconditionally after it
  (l.44-45), so the row can claim success while the roster is stale — inconsistent state until reload.
- **[G] Double-submit race.** `working` is React state; two clicks before re-render can both pass
  `if (working) return` (l.38) and issue two connects. No ref lock or idempotency.
- **[C] State updates after unmount** (l.26, 44-49) are unguarded; benign in React 18 but wasteful.

## 2. Concrete hardening (prioritized)

**P0 — never crash, never lie by silence**
1. **[C] Validate the probe reply at the boundary:** `Array.isArray(reply.executors)`, filter to
   non-empty strings, dedupe (then sort). *(Risk: minimal; dedupe changes ordering — keep the
   server order if the spec treats it as preference order.)*
2. **[C] Add `probeState: "loading" | "ready" | "error"`** plus a bounded timeout (e.g. 8 s) via a
   `setTimeout` that aborts the controller; on failure render a neutral, i18n'd notice with a Retry
   button instead of nothing. *(Risk: the row is no longer invisible on transient failure — check
   `docs/PARITY.md`; two new translation keys; axe-visible `role="status"` needed.)*
3. **[C] Surface connect errors per executor:** keep `attachError` keyed by executor, branch on
   `RpcError.code` (`CONFLICT` → "already attached", `NOT_FOUND`/`BAD_REQUEST` → "no longer
   available" and drop that executor from the local list, other → retry). *(Risk: new i18n keys;
   mapping assumes the backend uses the documented codes — confirm with a contract test.)*

**P1 — cancellation and truth**
4. **[C] Ref-based in-flight lock + per-attach `AbortController`:** a `useRef` guard checked and set
   synchronously before the call; pass the signal to `botsConnectAcp`; abort controllers on unmount
   cleanup; clear the lock in `finally`. *(Risk: must not abort on ordinary re-render; keep one
   controller per in-flight executor.)*
5. **[C] Only claim "attached" after confirmation.** Make `refreshRoster()` return a success flag
   (additive change to the shared store — coordinate with its other callers), and set `attached`
   only on success; otherwise leave the row retryable with an error. Ideally reconcile `attached`
   from roster/server state on mount and on roster change. *(Risk: roster.ts is shared; if connection
   state is not exposed there, a read-back endpoint is needed — DEVIATION, request operator approval
   and record in `docs/PARITY.md`.)*
6. **[C] Pin the target at click time:** capture `target.id` when the click fires, re-read the current
   roster entry, and abort/re-prompt if the id changed; show the target bot name next to the row so
   the operator knows which bot is affected. *(Risk: extra UI text; keep to semantic tokens and
   `useTranslation`; keep keyboard flow intact.)*

**P2 — reduce staleness**
7. **[G] Re-probe on window focus / visibility change** (and optionally a manual refresh control),
   with a staleness timestamp; do not poll aggressively. *(Risk: extra RPC load; multi-instance
   detection can still disagree across requests — refresh reduces but cannot eliminate staleness.)*
8. **[G] Reconcile "gone at connect" by dropping the offending executor locally** (P1-3) and
   surfacing a one-time notice, rather than leaving a permanently failing button. *(Risk: relies on
   the documented `NOT_FOUND`/`BAD_REQUEST` codes; message sniffing is fragile — avoid.)*
9. **[C] Accessibility:** all new notices use i18n keys and `role="status"`/`role="alert"`; retry
   stays keyboard reachable with visible focus. *(Risk: must pass the WCAG 2.2 AA / axe gate.)*

## 3. Verification (test-first, REQ-061-tagged Playwright)
- **[C]** Probe rejects / returns `{}` / returns duplicate names → error+retry visible, no shell
  crash, no duplicate React keys (P0-1/2, 1.1).
- **[C]** `connectAcp` rejects with `CONFLICT` / `NOT_FOUND` / network error → correct notice,
  retry works, no false "attached" (P0-3, P1-5).
- **[G]** Connect never settles → timeout releases `working`, buttons re-enable (P1-4).
- **[G]** Two rapid clicks → exactly one `connectAcp` (P1-4); executor removed after detection →
  executor dropped with a legible notice (P2-8). Cover light/dark, 360 px, and reload/remount
  showing truthful attach state (1.2).
