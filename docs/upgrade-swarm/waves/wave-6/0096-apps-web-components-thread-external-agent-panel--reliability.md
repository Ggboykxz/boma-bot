# 0096 — `apps/web/components/thread/external-agent-panel.tsx` — reliability lens — v0.4.0 proposal

Worker #096 · module `apps/web/components/thread/external-agent-panel.tsx` (191 lines) · lens:
reliability · READ-ONLY review of `rebuild-workspace/.worktrees/before-m5` (v0.3.0-rc5). Proposal
only; no code changed here. Spec anchor: REQ-059 (external agents over standard protocols,
`spec/01-requirements.md §486`), `spec/08-ui-ux.md`; wire in `lib/api/rpc.ts §M12.2`
(`botsExternalAgentStatus` / `botsConnectExternal` / `botsDisconnectExternal`, lines 1488-1530) and
transport `call()` (245-272). CONFIRMED = read this session; runtime/server behavior = GUESS.

## Current behavior (read, not run)

- One status fetch on mount keyed on `[botId]` with an `active` guard; any error is coerced to
  `{connected:false}`. `attach`/`detach` each do a write RPC then a *second* status read, all errors
  → `failed=true`; state is `busy`/`failed` booleans. Fetchers accept `AbortSignal`, but the component
  never passes one and `call()` sets no timeout (`fetch` alone has no timeout). CONFIRMED.

## Failure modes

1. **ACP connect failure (CONFIRMED 50-71, 112, 183-185).** Every fault — hung ACP handshake,
   unreachable endpoint, malformed protocol, bad `secretName`, 401/403, 5xx — collapses into the
   same `failed=true` plus `t("thread.error.send")`, a *send* string used for a connect fault; no
   classification, no actionable detail, no auto-retry. Held-open request → `busy` stuck `true`
   ("Attaching…" + disabled buttons, latch leak) because no signal/timeout reaches `call()`.
   `busy` is a boolean and the only guard is DOM `disabled`, so a same-tick double click can issue
   two connects. Two-step connect→read is non-atomic: if the read fails after a successful connect,
   the catch reports failure while the session is live, and because form values are only cleared on
   the success path, the user's next attempt reconnects — duplicate/live session drift. `baseUrl` is
   checked non-empty only (174), so malformed URLs round-trip to a generic error. Worse, the button
   labelled `common.retry` (179) merely does `setOpen(false); setFailed(false)` — it closes the panel
   and does **not** retry, so a user who just failed believes a retry ran.

2. **Stale state (CONFIRMED 32-48, 60-61, 78).** Status is fetched once and refreshed only as a
   side-effect of a successful attach/detach: no poll, no focus/visibility refresh, no SSE. An agent
   going offline, a rotated vault secret, a connection replaced by another tab/session, or a backend
   restart never updates the pill — "connected" can be shown for a dead endpoint indefinitely. The
   failed first load is silently rewritten to `{connected:false}`, so an auth/network outage looks
   like a clean "not connected" with no error and no retry; `failed` is cleared only by the
   mislabeled Retry or a new attempt.

3. **Session drift (CONFIRMED).** (a) On `botId` change the effect refetches, but `open`, `baseUrl`,
   `agentName`, `secretName`, `busy`, `failed` are never reset: a dialog opened for bot A stays open
   for bot B prefilled with A's `secretName`, and `attach()` then sends `{botId:B, secretName:A}` —
   cross-bot mis-binding of a vault secret *name* (no value ever renders, per the 15-21 contract).
   (b) `attach`/`detach` have no mounted/current guard, so a late response writes another bot's
   status after unmount or switch. (c) The write→read pairs have no request epoch: attach racing
   detach (or two attaches) resolves last-*response*-wins, not last-action-wins. (d) Fixed ids
   `ea-baseurl`/`ea-agentname`/`ea-secretname` (137-167) collide if two panels mount on one page →
   `<label htmlFor>` binds to the wrong input. (e) `role="dialog"` has no Escape/outside-click close,
   no focus trap or focus return after `autoFocus`, so keyboard users get stranded on close.

## Concrete hardening (v0.4.0), with risks

- **A. Bounded, cancellable actions.** A fresh `AbortController` per load/attach/detach, signal passed
  to all three RPCs, abort on unmount, on `botId` change and when superseded; wrap with
  `AbortSignal.timeout(...)` (feature-detect + timer fallback) so no latch can hang. Guard every
  state write with mounted + monotonic request epoch; replace the `busy` boolean with an operation id
  so double-submit is impossible and `finally` clears only the matching op. *Risk:* aborting after the
  agent accepted the connect may leave a live server-side session — requires idempotent,
  botKeyed connect or authoritative status reconciliation; verify handshake latency budget (GUESS).
- **B. Reset on bot switch.** A keyed effect clears all form/UI state and aborts in-flight work before
  refetching when `botId` changes; use `useId()` (or botId-scoped ids) to kill cross-instance id
  collisions. *Risk:* discards in-progress typing on switch — acceptable, document it.
- **C. No partial-success lie.** Prefer `BotsConnectExternalReply` (rpc.ts 1510-1516, carries
  connectedAt/baseUrl/agentName/secretName) to update `status` directly instead of a second read; if
  a verification read is kept, its failure means "connected, verify pending", never total failure;
  treat "already connected" as a no-op refresh. *Risk:* fields are nullable in practice — check
  `spec/03-api-contract.md` and server idempotency (GUESS) before relying on the reply alone.
- **D. Legible, classified errors.** Branch on `RpcError.code`/status: 401/403 → permission, 404 →
  bot missing, 400 → bad baseUrl/secret, 5xx/network/timeout → retryable. Replace
  `thread.error.send` with dedicated i18n keys, keep inputs on failure, clear `failed` at the start
  of each attempt, and never render raw server text. *Risk:* new keys × locales + parity/golden
  updates; no enumeration/secret leakage via error copy.
- **E. Honest Retry.** Make the control actually retry the last failed action, or relabel it Close and
  add a distinct Retry. *Risk:* visible copy change → `docs/PARITY.md`.
- **F. Freshness.** Refetch status on mount, on window focus/visibility and after a retryable failure,
  with backoff; expose a subtle "verifying/reconnecting" state. *Risk:* extra requests; do not invent
  an SSE channel — GUESS whether one exists; reuse existing poll conventions.
- **G. Input hygiene.** `type="url"`, `maxLength`, client-side scheme validation; disable attach until
  valid. *Risk:* enforcing https-only may break local bring-up if the server allows http/localhost —
  check the ADR before making it a DEVIATION.

## Risks / non-goals

- **R1** Server-side ACP session lifecycle, connect idempotency and secret rotation are out of this
  component's scope — flag in `docs/STATE.md`, do not patch here.
- **R2** No wire changes and no new dependency; keep the "bearer secret name only, never a value,
  never logged" contract (15-21) intact.
- **R3** Epoch guards must not drop a *successful* write's status just because a later read lost the
  race — reconcile against authoritative status, not blind last-write-wins.

## Success criteria / tests (REQ-named, route-mocked Playwright)

- `REQ-059 attach failure shows a classified alert, keeps inputs, and its Retry issues exactly one reconnect`.
- `REQ-059 hung connect times out, busy clears, buttons re-enable`.
- `REQ-059 connect succeeds but verification read fails → pill shows connected, not a generic failure`.
- `REQ-059 switching botId mid-attach aborts the write and resets form/dialog/ids`.
- `REQ-059 two panels on one page bind labels and secret-name input to their own instance`.
- `REQ-059 rapid double attach issues one RPC; last-action-wins across attach/detach`.

## Bottom line

The panel is correct on the happy path but brittle: timeouts/double-submit/partial-success can hang or
lie, status is fetched once and goes stale, and a bot switch can carry one bot's secret name into
another. A-G are local, reversible and i18n/parity-checkable; the server-side session semantics in R1
are the only part this lens cannot close.

CONFIRMED = code read this session; all outcomes above are GUESS until the Playwright tests run.
