# 0083 — apps/web/components/shell/bot-create-form.tsx — architecture (v0.4.0)

Worker #083 · module `apps/web/components/shell/bot-create-form.tsx` (102 lines) · lens: architecture · READ-ONLY.
Requirement REQ-004 (one continuous thread per bot, MUST) + spec/08 §8.3 "New bot" action.
Proposals only; no code in this change. Line refs against `before-m5`.

## Current structure (CONFIRMED)

One `"use client"` component owns the whole inline flow: five independent `useState` atoms
(`open`, `name`, `description`, `submitting`, `failed`), a hand-rolled async `submit` with
try/catch/finally, and one conditional form branch. It calls `botsCreate()` (rpc.ts:485, accepts an
optional `AbortSignal`), then `await refreshRoster()` (roster.ts:36, a fire-and-forget fetch that
swallows its own errors), clears local state and `router.push`es to `/app/<bot.id>/`. `BotReply`
already carries `id`, `name`, `threadId`, `createdAt`. Both RPCs are signal-capable; neither is
given a signal here. The repo's `RpcError` with its typed `code` (rpc.ts:169) is caught into a single
boolean. The sibling group-create form in `bot-rail.tsx:120-150` duplicates the same submit pattern
but with raw `<input>`/`<label>` and its own `open`/`submitting`/`failed` trio.

Architecturally the form conflates three concerns: disclosure UI, the create transaction, and roster
cache reconciliation. That is where the upgrades below aim.

## Proposed upgrades (v0.4.0)

### U1. Replace boolean soup with one discriminated state machine
Collapse `open`/`submitting`/`failed` into a union driven by `useReducer`, e.g.
`{kind:"closed"} | {kind:"editing"; name; description} | {kind:"submitting"; name; description} |
{kind:"error"; name; description; code}`. Invalid combinations (`submitting && failed`, an open
form whose submit already navigated) become unrepresentable, matching the repo rule "make invalid
states unrepresentable".
- **Risk:** the render branch is rewritten wholesale; do it as one commit gated by the existing
  Playwright REQ-004 tests. Decide explicitly whether the error arm retains the draft text.

### U2. Own the request lifecycle: signal + in-flight lock
Create one `AbortController` per open form, pass its `signal` to `botsCreate`, and abort on close or
unmount so a stale create cannot `router.push` after the rail is gone. Guard double-clicks with a
`useRef` lock, not only the `submitting` state (React state updates are async, so two rapid submits
can both pass the current `submitting || ...` check).
- **Risk:** aborting an in-flight create has an unknown server outcome (the thread may already exist).
  Treat abort as "unknown", not as a clean failure — do not auto-retry in a way that could create a
  second bot. Never leave the reducer stuck in `submitting`; the abort path must transition too.

### U3. Single source of roster truth — derive from the reply, refresh only to reconcile
`BotReply` is enough to upsert a `RosterBotView` into the roster store (`lib/roster.ts` is a module
singleton with `useSyncExternalStore`). Add a small `upsertBot(bot)` action, publish the created bot
from the reply, then navigate; keep `refreshRoster()` as an optional reconciliation, not a blocking
step before the UI closes. This removes the failure mode where the new bot is missing from the rail
because the refresh fetch failed — the error is swallowed and the user lands in a thread with no
rail row.
- **Risk:** reply→view mapping needs a normaliser (`archivedAt: null`, no `unreadCount`/
  `snoozedUntil` in `BotReply`); keep `refresh()` as authority if a later backend reply is partial.
  Touching the shared store is a separate change with its own tests, not bundled silently here.

### U4. Honour the typed error; fix the dead validation branch
`RpcError.code` is discarded. Branch on `CONFLICT` (duplicate name), `BAD_REQUEST`,
`TOO_MANY_REQUESTS` and a generic fallback via new `bots.create.error.*` i18n keys; `UNAUTHORIZED`
already triggers the global handler, so do not show a misleading "try again" for it. Two CONFIRMED
copy/paste defects: `errorMessage={t("bots.create.name")}` renders the *label* as the error, and the
`isInvalid={name.trim().length === 0 && failed}` branch is unreachable from the UI (empty name
disables the submit button, so submit never fires to set `failed`). Drive invalidity from the typed
error and give the field a real message.
- **Risk:** new keys must land in every locale catalogue (en/fr/he + fallbacks, `lib/i18n.ts`); keep
  a generic fallback. Never surface raw server internals in the field. A client-side duplicate-name
  check must not reject names the server accepts (would be a DEVIATION).

### U5. Align form architecture and focus management with the sibling flow
Extract the shared inline-create pattern (disclosure + submit + typed error) used by this form and
`bot-rail.tsx`'s group form rather than maintaining two divergent handlers. On open, focus the name
field (the group form already uses `autoFocus`); support Escape-to-close and return focus to the
`IconButton` trigger on close; let the BoardUI `Input` wire `aria-errormessage` via its `errorMessage`
slot and add `aria-live` to the alert.
- **Risk:** focus changes alter the keyboard-only path — re-run axe (WCAG 2.2 AA) and the 360 px /
  light+dark / RTL checks. Keep BoardUI react-aria-backed form components; do not hand-build a second
  form system or add a component library. Any new dependency must be recorded in
  `docs/DEPENDENCIES.md`.

## BoardUI tokens — preserve as-is (CONFIRMED)

Keep every token already in use: `text-text-secondary`, `text-text-error-primary`,
`border-border-button-default`, composite types `text-caption-1-semibold` / `text-body-2-medium`,
`rounded-md`, Tailwind spacing (`gap-1`, `gap-2`, `p-2`), and `@remixicon/react` refs passed as
components (`icon={RiAddLine}`). Merge any new classes with `cx()` from `@/utils/cx`. No raw palette
classes or hex, no `dark:` overrides, no hand-rebuilt type utilities; dark mode and RTL ride the
tokens and `DirectionProvider`.

## Preserved invariants / non-goals

- REQ-004 contract stays: bots.create yields one bot with one thread; the client never synthesizes a
  thread id or duplicates a bot on a retry.
- No externally visible API/data-format change without a DEVIATION entry and owner approval.
- The `boma-target` / `boma-focus` class contract and the roster-vs-directory split are out of scope.
- Shared-layer changes (timeouts at `call()`, roster store API) are separate changes with their own
  tests, not smuggled into this one file.

CONFIRMED by reading `bot-create-form.tsx`, `bot-rail.tsx`, `lib/api/rpc.ts`, `lib/roster.ts`,
`lib/i18n.ts`, `tests/bot-create.spec.ts`, `spec/01` REQ-004, `spec/03` §bots, `spec/05`, and the
bot-create golden this session. GUESS: whether the desired end state lifts disclosure state into the
rail or keeps it local — verify against the group-form refactor before implementation.
