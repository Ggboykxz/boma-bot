# ExternalAgentPanel — v0.4.0 Architecture Proposal (worker #095)

- **Module:** `apps/web/components/thread/external-agent-panel.tsx` (191 lines, read this session).
- **Lens:** architecture / structure.
- **Requirement:** REQ-059 (external agents over standard agent protocols, MUST); local id `M12.2`.
- **Status convention:** observations below are CONFIRMED by reading the source, RPC types and `spec/01-requirements.md` this session; numbered items are forward-looking PROPOSALs, not verified behaviour.

## Current structure (CONFIRMED)

One client component owns the entire attach flow: seven independent `useState` atoms
(`open`, `status`, `baseUrl`, `agentName`, `secretName`, `busy`, `failed`), a mount `useEffect`
that fetches `botsExternalAgentStatus`, an `attach`/`detach` pair that each mutate then
re-fetch status, and three render branches (loading skeleton / connected pill / attach popover).
The three RPCs in `lib/api/rpc.ts` already accept an optional `AbortSignal`.
The secret *name* is passed through; the secret *value* is never read — that invariant must stay.

## Proposed upgrades

### 1. Replace seven state atoms with one explicit state machine (PROPOSAL)
Collapse the flags into a discriminated union, e.g.
`type PanelState = {kind:"loading"} | {kind:"disconnected"; open:boolean; draft:Draft} | {kind:"attaching"; draft:Draft} | {kind:"connected"; status:ExternalAgentStatus} | {kind:"error"; phase:"attach"|"detach"; message:string}`,
driven by a small `useReducer`. This makes invalid combinations (`busy && failed`, open popover
while connected, `failed` shared by two different operations) unrepresentable, matching the repo
rule "make invalid states unrepresentable".
- **Risk:** largest refactor of the set; the three render branches must be rewritten together, so
  do it as one commit with the Playwright REQ-059 test as the regression gate. Risk of losing the
  current draft text on reconnect — decide explicitly whether `error` retains `draft`.

### 2. Wire the AbortSignal the RPC layer already exposes, and guard on `botId` (PROPOSAL)
The mount effect uses an `active` boolean instead of the `signal?: AbortSignal` the RPC already
accepts; `attach`/`detach` pass no signal at all. Create one `AbortController` per `botId` in an
effect, pass its `signal` to all three calls, and ignore replies whose `botId` no longer matches.
- **Risk:** aborting an in-flight `connectExternal` does not guarantee the server rolled back —
  treat abort as "unknown outcome" and re-fetch status, not as failure. Avoid aborting in `attach`
  cleanup if that would leave `busy` stuck; the reducer from (1) should own the transition.

### 3. Derive status from mutation replies; single refresh path (PROPOSAL)
`BotsConnectExternalReply` already carries `connected`, `baseUrl`, `agentName`, `secretName` and
`connectedAt`, yet `attach` discards it and issues a second `botsExternalAgentStatus` round-trip;
`detach` does the same with an inconsistent `?? {connected:false}` fallback. Build status from the
reply, keep one `refresh()` helper for mount/retry, and use one canonical "disconnected" value.
- **Risk:** reply and status shapes differ (`agentName: string | null` vs optional), so a shared
  normaliser is needed; if the backend later returns a partial reply, the UI could show stale
  detail — keep `refresh()` as the authority when in doubt.

### 4. Adopt BoardUI form/popover primitives; make DOM ids unique (PROPOSAL)
The dialog is a hand-built `role="dialog"` absolutely-positioned `<div>` with no outside-click or
Escape close, no focus return, and three raw `<input>` elements with hardcoded ids `ea-baseurl`,
`ea-agentname`, `ea-secretname`. Multiple panels (one per bot in a thread) would collide on those
ids and break label associations. Use `useId()` for ids; prefer an installed BoardUI
Dialog/Popover and react-aria-backed form components per the always-on rules. Use logical
positioning (`start-0`/`end-0`) and a width that survives 360 px (`max-w` / `min`).
- **Risk:** introduces npm deps (must be recorded in `docs/DEPENDENCIES.md` with license) and may
  need ADR if the required component is Pro. A Popover's focus/portal model can change keyboard
  flow, so re-run the keyboard-only path and axe (WCAG 2.2 AA gate).

### 5. Typed errors and URL validation at the boundary (PROPOSAL)
Both operations collapse into one boolean `failed` rendered as the generic `thread.error.send`.
Store a typed error (`phase`, legible reason) and validate `baseUrl` as an absolute http(s) URL
before calling, sharing the rule with the backend so malformed endpoints fail early and legibly —
directly serving REQ-059's "legible reason" criterion. Add a timeout to the RPC layer (currently
none visible here) so an unreachable endpoint cannot hang the attach button forever.
- **Risk:** client validation must not reject endpoints the server accepts (new DEVIATION); keep
  it permissive and make the backend the authority. Adding a timeout at the shared `call` layer
  affects every RPC, so do it as a separate change with its own tests.

## BoardUI tokens — preserve as-is (CONFIRMED)

Keep every semantic token already used: `bg-background-secondary-default`,
`bg-background-primary-default`, `border-border-separator-border`, `border-border-button-default`,
`text-text-primary/-secondary/-tertiary/-placeholder`, `text-text-error-primary`,
`text-foreground-icon-secondary`, composite types `text-caption-1-semibold` / `text-body-2-regular`,
and `focus-visible:ring-border-focus-ring`. Merge any new classes with `cx()`. No raw palette
classes, no `dark:` overrides, no hand-rebuilt type utilities.

## Preserved invariants / non-goals

- Never render, store or log a secret value; `secretName` is only a pointer to the space vault.
- No new externally visible API/data-format change without a DEVIATION entry and owner approval.
- Pure UI refactors stay inside this one file; shared-layer changes (timeouts, validation) are
  separate changes, not bundled here.
