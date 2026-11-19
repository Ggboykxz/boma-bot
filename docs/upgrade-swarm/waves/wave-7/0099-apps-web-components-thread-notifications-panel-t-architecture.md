# 0099 — NotificationsPanel architecture proposal (v0.4.0)

Module: `apps/web/components/thread/notifications-panel.tsx` (372 lines, read this session).
Lens: architecture / structure.
Spec anchor: REQ-064 (Web Push notifications, MUST) + REQ-065 (notification destinations, MUST),
`spec/01-requirements.md` §REQ-064/065; local id M15.1.
Status: "Current structure" is CONFIRMED by reading the file, `lib/api/rpc.ts` and the spec this
session; numbered upgrades and their risks are PROPOSAL/GUESS (design inference, not implemented).

## Current structure (CONFIRMED)

One client component co-locates two distinct concerns:

- **Browser push lifecycle (REQ-064):** capability probe (`isPushSupported`), a 7-value `phase`
  (`loading|unsupported|disabled|off|on|busy|failed`), a mount effect that chains
  `notificationsStatus` → `getRegistration().pushManager.getSubscription()` →
  `notificationsPolicyGet`, and a `toggle()` that requests permission, registers `/sw.js`,
  subscribes with the VAPID key and posts the p256dh/auth keys.
- **Space webhook destinations (REQ-065):** `destinations[]`, a create/delete form, and a
  `creatingDestination` flag shared by both add and remove operations.

The policy form and the destination panel are rendered together behind `phase === "on"`; the
policy checkboxes and `dailyCap` number input call `patchPolicy` on every change (keystroke included).

Observed structural smells (CONFIRMED):

- `toggle()`'s off-branch has an identical `if (policy) setPhase("off") else setPhase("off")`
  (lines 163-167) — a leftover branch that conveys no information.
- Two separate failure channels (`phase: "failed"` and `destinationEnabledFlags: "ready"|"failed"`)
  render the same generic `thread.error.send`; the operation that failed is lost.
- The mount effect uses an `active` boolean, while every RPC in `lib/api/rpc.ts` already accepts
  an optional `AbortSignal`; no signal is passed. The initial destinations load is a nested
  fire-and-forget block duplicating `refreshDestinations()`.
- `refreshDestinations()` turns any error into `[]`, so a transient failure is indistinguishable
  from "no destinations" (violates REQ-065's "failure is logged"-in-spirit observability).
- The destination create/delete UI is rendered for every member, while REQ-065 states destinations
  are "managed by Owners and Admins only" — the component has no capability input.
- The remove button's `aria-label={`Remove destination ${destination.label}`}` is hardcoded English;
  a visible `https://` placeholder is a scheme hint, not user copy.

Invariants to preserve: pushes carry **no conversation content** (no thread id, tool args or text);
the webhook URL is sealed at rest and never echoed to this view; the secret/key bytes are only
ever transformed for the subscribe call, never rendered or logged.

## Proposed upgrades (v0.4.0)

### 1. Split into a data hook and thin view; separate the two requirements (PROPOSAL)
Extract `usePushSubscription(botId)` (REQ-064: capability, permission, register/subscribe/unsubscribe)
and `useNotificationPolicy(botId)` + `useNotificationDestinations()` (REQ-065). Keep
`NotificationsPanel` as the exported thin view that composes them, so route imports do not change.
Destinations are **space-scoped, not bot-scoped** — a dedicated hook avoids refetching them on bot change.
*Risk:* more files and a larger diff than a single-file refactor; hook boundaries must keep the
"nothing renders when unsupported/disabled" behaviour byte-identical. Gate on the REQ-064/065
Playwright tests and replay `spec/golden/ui/*.json`.

### 2. Replace ad-hoc flags with one discriminated state machine (PROPOSAL)
Use `useReducer` with a union such as
`{kind:"loading"}|{kind:"unsupported"}|{kind:"disabled"}|{kind:"off"}|{kind:"on";policy;destinations}|
{kind:"busy";op:"subscribe"|"unsubscribe"|"policy"|"destination"}|{kind:"error";op;message}`,
folding `phase`, `creatingDestination` and `destinationEnabledFlags` into it. This makes the dead
off-branch unrepresentable and keeps the failed *operation* for a legible, translated error.
*Risk:* every render branch must be rewritten together in one commit; a shared `busy` can block
independent policy and destination actions — model per-operation pending state or accept serialized
actions deliberately.

### 3. One coordinated load path with AbortSignal and botId guarding (PROPOSAL)
Create an `AbortController` per `botId` in the effect, pass its `signal` to `notificationsStatus`,
`notificationsPolicyGet` and `notificationsDestinationsList`, drop the `active` flag and the nested
fire-and-forget load, and reuse a single `refreshDestinations()` on mount and after mutations.
Ignore policy replies whose `botId` no longer matches the current one.
*Risk:* aborting a subscribe/unsubscribe leaves the server outcome unknown — on abort, re-fetch
status rather than assuming failure, and ensure `busy` is always cleared in a `finally`. Never let
a failed destinations refresh masquerade as an empty list: log it and keep the previous list.

### 4. Gate destination management on space role (fail closed) (PROPOSAL)
Accept a `canManageDestinations` capability (or read it from an existing space-role context) and
render the add/delete form only for Owners/Admins; members get the read-only list. This closes the
REQ-065 permission gap without changing the wire contract.
*Risk:* the call site must supply the capability; if it cannot, default to read-only (safer than an
unauthorized affordance). Confirm whether the OpenAPI/role surface already exposes this signal —
if not, record a STATE question and a possible DEVIATION before adding any new endpoint.

### 5. Commit `dailyCap` deliberately; validate at the boundary (PROPOSAL)
Keep the input controlled for responsiveness but send `policySet` on blur/Enter (or a short
debounce), clamp to the documented `0..10000` range and reject `NaN`/negative before the call.
Translate the remove-button `aria-label` through `useTranslation` and keep `dir="ltr"` on the URL
input for RTL correctness.
*Risk:* debounce adds perceived lag; keep local optimistic value until the reply confirms it.
Client clamping must not diverge from the Rust-enforced cap — the server stays the authority, and
any new user-visible string needs i18n keys, not a literal.

## BoardUI tokens — preserve as-is (CONFIRMED)

Keep `text-text-primary/-secondary/-tertiary/-error-primary`, `bg-background-secondary-default`,
`border-border-separator-border`, `border-border-button-default`, `accent-accent-600`, `boma-focus`,
`rounded-full/rounded-md/rounded-xl` and the existing layout tokens. Migrate the bare `text-xs` /
`text-[10px]` sizes toward composite type utilities (`text-caption-1-*` / `text-body-2-*`) without
introducing raw palette or hex classes. Merge new classes with `cx()` from `@/utils/cx`; no `dark:`
colour overrides, no second design system.

## Non-goals / constraints

- No push payload, policy value or webhook URL may ever contain or surface conversation content.
- All user-visible text and accessible names go through `useTranslation`.
- No new externally visible API/data-format change without a DEVIATION entry and owner approval.
- Shared-layer changes (timeouts in the RPC `call`, role capability source) are separate changes,
  not bundled into this refactor.
