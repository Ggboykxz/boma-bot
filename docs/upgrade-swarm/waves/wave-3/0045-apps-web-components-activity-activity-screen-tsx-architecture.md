# 0045 — apps/web/components/activity/activity-screen.tsx — architecture lens

- Target: `before-m5` (v0.3.0-rc5), module `apps/web/components/activity/activity-screen.tsx` (166 lines, one `"use client"` component).
- READ-ONLY review. Spec anchors: `spec/08-ui-ux.md §8.4` (grouped-by-day list with prompt snippet, bot name, status chip, duration, link to originating message; filter by bot and status; working runs sort to top and update in place; ticket runs appear here, not in the transcript), REQ-011 (Activity replaces the whole chat list), REQ-012 (named states, `failed → queued` only), REQ-067 (pause/resume queued runs), BoardUI rules (components first, semantic tokens, composite type, `cx`).

## Structure today

One `ActivityScreen` owns all state: `runs: RunView[]`, `phase: "loading"|"ready"|"error"`, `reloadToken`, `retrying: string | null`, `busyRun: string | null`. `reload()` bumps the token and forces the skeleton; one `useEffect` fetches `runsList({ limit: 50 }, signal)` under an `AbortController` with aborted-aware success/failure and cleanup; `retry(id)` calls `runsRetry` swallowing errors with a comment. Render is header → skeleton / error+retry / empty / `<ul>` of inline `<li>` rows; each row derives `runStatusKey`/`runTriggerKey`, shows a written status (`role="status"`), a pause/resume button for `queued|paused`, and a retry button for `failed`. All colors are semantic tokens, type is composite utilities, all user strings go through `useTranslation`. Keep all of that.

## Confirmed observations (read this session)

- **Footer-to-header indentation defect**: `control()` (lines 105–118) is declared *inside the `.map()` callback*, at the wrong indent, and re-created for every row on every render. It is a pure behavioral closure over `setRuns`/`setBusyRun` with no per-row captured state — hoistable as-is. CONFIRMED.
- **Spec gap vs §8.4**: no day grouping, no bot name, no duration, no link back to the originating message, no bot/status filters, and no in-place/live update for working runs (single mount fetch only). The header comment frames the screen as complete; the divergence is implicit, not recorded in `docs/PARITY.md`. CONFIRMED (spec read).
- **Silent mutation failures**: `runsRetry` and `runsPause/Resume` swallow errors with comments; on retry the row stays `failed` with zero user feedback — yet `runsRetry` returns `{ ok: boolean }` (rpc.ts:538) and `runsPause/Resume` return a server-echoed `{ id, status }` (rpc.ts:546/554). The echo is used for pause/resume but ignored for retry. CONFIRMED.
- **Skeleton flash on reload**: `reload()` sets `phase="loading"`, discarding a good list for a 3-bar skeleton on every manual retry; mutations are not abortable and do not trigger revalidation. CONFIRMED.
- **Duplicated fetch machine**: same `phase + reloadToken + AbortController + retry` pattern as `admin/access-screen.tsx` (see 0047); two copies already, no shared hook. GUESS: more screens duplicate it.
- **Unbounded run list**: hardcoded `limit: 50`; `runsList` supports `cursor` (rpc.ts:524), so >50 runs are silently truncated with no pagination. CONFIRMED.
- **Accessibility gaps**: error box has no `role="alert"`; loading phase has no announcement; `animate-row-slide-in` has no `prefers-reduced-motion` guard (WCAG 2.2 AA / AGENTS.md gate). Status text is spelled out (REQ-012, not color-only) — keep. CONFIRMED.
- Two in-flight flags hold a single id; buttons disable by id match, so concurrent actions on different rows are unreachable by UI but the state model does not express that invariant. Minor; CONFIRMED.

## Proposals (v0.4.0)

1. **Split by responsibility: `useActivityRuns()` + `RunRow`.** Extract a hook owning runs/phase/token/in-flight flags and the three RPC mutations; extract `activity-run-row.tsx` receiving `{ run, busy, onPause, onResume, onRetry }`. Hoist the in-map `control()` closure to a `useCallback(id, action)` in the hook so row identity is stable. `ActivityScreen` keeps only the header and phase switch. Risk: prop-drilling and behavior drift — return one hook object, keep markup byte-identical, gate on existing REQ-011/012/067 Playwright coverage (GUESS: confirm those test names exist).

2. **Share the phase/fetch machine; revalidate in background.** Lift `loading|ready|error + abort + reloadToken` into `lib/use-async-list.ts` parameterized by fetcher, consumed here and by `admin/access-screen.tsx`; add a typed `isRpcError` guard instead of ad-hoc error casts elsewhere. Make `reload()` keep `ready` with a spinning refresh and `aria-live="polite"` announcement rather than a skeleton flash. Risk: coupled error wording across screens — keep per-screen catalogue keys (`activity.error`) at call sites; test the network-down path explicitly.

3. **Failure honesty anchored to server echo.** Surface `runsRetry`/pause/resume failures in an inline `role="alert"` with a retry affordance; on `runsRetry` success (`ok === true`) patch the row to `queued` from the server echo or revalidate instead of leaving it `failed`; make mutations accept an `AbortSignal` and abort in-flight work on unmount. Risk: optimistic divergence on partial failure — never invent status locally, only apply echoed values and reconcile on revalidate; abort semantics must be explicit.

4. **Reconcile §8.4 deliberately, not silently.** Either implement day grouping, bot name, duration, origin-link, bot/status filters, and live in-place updates for working runs as separate REQ-011 slices, or record a DEVIATION in `docs/PARITY.md` stating what ships later. Before choosing (a), verify `RunView` (rpc.ts) and `spec/03-api-contract.md` actually carry the needed fields — bot name, started/finished time, originating message id; missing fields mean a contract change (spec edit + trace + maintainer sign-off). Risk: scope explosion and contract churn; recommend (b) now, (a) behind the v0.4.0 gate.

5. **Accessibility and completeness polish.** Add `role="alert"` to the error box, an accessible loading announcement, a `motion-reduce:` guard on the row animation, and cursor-based pagination (or a documented 50-run cap) for the `runsList` call. Risk: golden snapshot churn from class changes — re-record goldens intentionally and re-run axe at 360 px in light/dark.

## Explicit non-goals

No live WebSocket stream (polling/revalidate is enough until §8.4 live-update is spec-gated). No change to REQ-012 transition rules or to written-out status labels. No new filters or fields beyond §8.4. No BoardUI Pro components. No client-side authorization.

## Suggested sequencing

1 (mechanical split) → 3 (one-line failure-honesty fixes) → 5 (accessibility) → 2 (shared hook) → 4(b) DEVIATION immediately, 4(a) as its own REQ-gated slice. Every step gated by `lint`, `typecheck`, axe, and the REQ-011/012/067 Playwright tests.
