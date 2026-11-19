# 0015 — apps/web/app/app/activity/page.tsx — architecture (v0.4.0)

Worker #013 · module `apps/web/app/app/activity/page.tsx` · lens: architecture · READ-ONLY
of `rebuild-workspace/.worktrees/before-m5`. Proposals only; no code in this change. The
module itself is a 10-line server-component wrapper (`AppShell` > `ActivityScreen`) and is
structurally fine; the architecture that matters is the surface it mounts, so this review
covers `components/activity/activity-screen.tsx` and its collaborators
(`lib/api/rpc.ts`, `lib/run-status.ts`).

## What already exists (keep, don't re-propose)

- Thin page + screen split: the route file owns nothing, `AppShell` owns landmarks, the
  screen owns the list. Correct composition; should survive any refactor.
- `isRetryable`, `runStatusKey`, `runTriggerKey` are already extracted to `lib/run-status.ts`
  (pure, testable, unknown-string tolerant) — the right precedent for a policy module.
- Fetch path aborts on unmount/reload (`AbortController` + `reloadToken`); retry button
  resets via event handler, effect only fetches; `role="status"` on the status text so state
  is never colour-only (REQ-012); failed is the only retryable row.
- Tests pin the contract (`tests/run-controls.spec.ts`, REQ-067).

## Structure under pause/resume/reassign — why it should split by responsibility

`ActivityScreen` today conflates four responsibilities in one 190-line client component:

1. **Data access** — `runsList({limit: 50})`, abort wiring, `Phase` state machine,
   reload-token plumbing.
2. **Mutation orchestration** — two independent pending-state variables (`retrying`,
   `busyRun`) for the same class of thing, a retry path that on success shows *nothing* and
   never refetches (the row stays `failed` until a manual reload — arguably a live bug), and
   a pause/resume path that optimistically patches the row from the reply.
3. **Action policy** — which control is offered per status, currently inline JSX
   conditionals (`status === "queued" || "paused"`, `isRetryable(...)`) plus a `control()`
   closure re-created per row per render, inside the `.map()`, with broken indentation that
   signals the seam is overdue.
4. **Presentation** — row markup, skeleton, error and empty states, all inline.

Under pause/resume (REQ-067, shipped) this already produced two parallel pending flags and a
duplicated try/catch/finally. Reassign (REQ-068, shipped in the room via `groups.reassign`,
ADR-031) is the next pressure on the same seam: if the activity surface ever offers it for
group-delegation runs (the §8.4 spec explicitly wants room/background runs here), it arrives
as a third mutation path targeting `{groupId, runId, toBotId}` — it cannot be bolted on
without a third pending flag, a third inline branch, and a third silent-catch handler. The
Rust transition table (`crates/domain/src/run.rs`, REQ-012) is the single source of truth for
what may leave which state; the web layer currently mirrors that policy ad hoc in JSX. Split
by responsibility:

- `useRunsList` — fetch/abort/reload-token/optimistic patch; the only place that knows the
  wire.
- `lib/run-actions.ts` — pure `actionsFor(status, run)` returning the offered transitions,
  mirroring the domain table; reassign slots in here as data, not as JSX.
- `RunRow` / `RunActions` — dumb components; pending keyed by one `pending: {runId, action}`.
- `ActivityScreen` — composition only: phase → skeleton/error/empty/list.

This matches the thin-page/screen pattern and keeps role-query tests stable.

## Spec drift worth naming (not to fix here)

§8.4 wants day-grouping, bot name, duration, a link back to the source message, bot/status
filters, working-runs-on-top updating in place — none of which the screen renders; `RunView`
lacks `botId`, `threadId`, `messageId`, `parentRunId`, `duration`. `runs.list` supports
`cursor` but the screen hard-caps `limit: 50` with no paging. Header comments cite REQ-011
where REQ-012/067 are meant; `spec/TRACE.md` points REQ-012 at `packages/core/src/run-state.ts`
which does not exist in this tree (it is `crates/domain/src/run.rs`). Architecture work in
v0.4.0 is the right moment to reconcile these.

## Concrete upgrades

U1. **Extract `useRunsList` + `lib/run-actions.ts` + `RunRow`; collapse `retrying`/`busyRun`
    into one `pending: {runId, action} | null`; hoist `control()` out of the map.** Pure
    refactor, same DOM roles/names. Risk: selector drift breaks `run-controls.spec.ts` —
    low; mitigated by keeping `role="status"`, button names and row structure unchanged.

U2. **Make retry honest: on `runs.retry` success, patch the row to `queued` (or refetch via
    the existing reload token) and surface a failure toast/message on catch.** Today a
    successful retry is indistinguishable from a no-op. Risk: wire reply is `{ok: boolean}`
    with no status — refetch is the safer branch; slight extra RPC; low risk.

U3. **Define the action-policy seam for reassign now, implement it after REQ-068 lands on
    `runs.*`.** Extend `RunView`/`runs.list` with `parentRunId`/`threadId`/`botId` and add
    `runs.reassign` only via an ADR (contract + export-hash churn, same shape as ADR-031).
    UI must not invent targeting fields. Risk: contract/spec churn and scope creep into
    REQ-068's room semantics (boundary stop, hand-off `System` message); do not reuse
    `groups.reassign` from this surface — it requires room membership the activity projection
    does not express. Gate on the ADR; moderate risk.

U4. **Replace manual `reloadToken` reloads with a bounded live signal** (SSE frame or poll
    only while any row is non-terminal, paused in `waiting_*`, with `visibilitychange` gating
    and backoff). Delivers §8.4's "working runs update in place". Risk: reconnect storms and
    double-apply against the optimistic patch from pause/resume — needs a version/seq check
    per row; medium-high risk, schedule after U1–U2.

U5. **Align skeleton/error/empty with the phase machine and add `aria-busy` on the list
    region + a polite live region for action results.** Small, self-contained a11y win.
    Risk: minimal; copy additions (`activity.retry-failed`, etc.) touch i18n catalogues in
    every shipped locale — keep keys additive.

Explicit non-goals for v0.4.0: day-grouping, filters, durations and "link to source
message" (all blocked on the `RunView`/`runs.list` projection, not on this component);
replacing `AppShell` composition; touching the route file beyond comment fixes.

## Risks and trade-offs (summary)

- R1. Refactor-only moves (U1) can regress the one shipped golden path; land it behind the
  existing Playwright spec, unchanged assertions.
- R2. Optimistic patching (U2/U4) can diverge from server truth if pause/resume reply and
  list disagree; always reconcile on next fetch, never trust the patch indefinitely.
- R3. Reassign (U3) without the contract ADR leaks room-scoped authz into a global surface;
  the activity list must offer it only for delegations it can attribute.
- R4. Splitting into more files raises import indirection; keep the four modules in
  `components/activity/` (plus one `lib/run-actions.ts`) rather than a new package.
- R5. Comment/TRACE fixes imply spec export hashes may need re-recording — coordinate with
  whoever owns `spec/EXPORT.md` before touching spec files.

## Suggested sequencing

Wave 1 (v0.4.0): U1, U2, U5. Wave 2: U3 ADR + contract, then U4. Non-goals stay until the
projection lands. Acceptance: `run-controls.spec.ts` green unchanged, no new `any`.

_Line references are against `.worktrees/before-m5`; this document makes no code, spec-hash or wire changes._
