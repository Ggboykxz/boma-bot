# 0093 — apps/web/components/thread/computer-panel.tsx — architecture lens (v0.4.0)

Target: `before-m5` (v0.3.0-rc5), module `apps/web/components/thread/computer-panel.tsx` (192 lines,
`"use client"`). Read-only review; nothing edited. Sister files read: `apps/web/lib/api/rpc.ts`
(`computerStatus/Takeover/Release/OpenScreen/OpenScreenFrame`, all accept an optional `AbortSignal`;
`ComputerStatusReply.status` is a plain `string`), `spec/08-ui-ux.md` §8.3 (right panel,
REQ-028/REQ-029), `spec/01-requirements.md` REQ-026/029/030,
`spec/golden/ui/REQ-026-computer-panel.json`, `apps/web/AGENTS.md` BoardUI block.
Status of claims: CONFIRMED = read in this tree; GUESS = inferred, needs verify.

## Structure today

One client component owns five concerns: presence detection, status polling, frame polling plus
one-time capability minting, takeover/release mutations, and rendering. It returns `null` unless
`presence !== "absent"` and the status is in `LIVE_STATUSES` (`queued|running|interrupted`). All
user-visible strings go through `useTranslation`, and the markup already uses BoardUI semantic tokens
(`border-border-button-default`, `bg-background-primary-default`, `bg-background-secondary-default`,
`text-text-primary/secondary/tertiary`, `text-text-error-primary`,
`text-foreground-icon-secondary`, composite `text-body-2-*`). That token set and the `<Button>`
import are the contract for v0.4.0 — no raw palette class, no new design system.

What the structure gets wrong (all CONFIRMED):

- **One-shot polling bootstrap.** The `Promise.resolve().then(loadStatus)` effect starts the frame
  timer from the *initial* status only. A later `stopped → running` transition never starts frame
  polling; a `running → stopped` transition never stops it, even while the component renders `null`.
  The interval callbacks also ignore the `cancelled` flag (cleanup only clears the timers).
- **No cancellation.** Every RPC accepts an `AbortSignal`; the component passes none. In-flight
  `loadFrame` can `setFrame` after unmount or after `botId` changes, and `computerOpenScreenFrame`
  is awaited without `try/catch` inside a `void` call — a network rejection becomes an unhandled
  rejection.
- **Capability leaks across bots.** `frameCapability` is a ref keyed by nothing but lifetime. If
  `botId` changes, a capability minted for bot A is reused for bot B; the effect teardown does not
  reset it, nor the displayed `frame` or `screenGeneration`.
- **Presence conflates "no backend" with "request failed."** The `code === "COMPUTERS_UNAVAILABLE"`
  branch and its `else` both set `presence = "absent"`; the computed `code` is effectively dead. Any
  transient status error hides the panel until a later 5 s poll succeeds.
- **Stale frame is shown as a frozen image.** On a null frame the ref is dropped but `frame` is kept,
  so the old PNG stays on screen. The REQ-026 golden explicitly requires "a stale screen capability
  must surface as a status, not a frozen image" (CONFIRMED).
- **Reduced motion is asserted, not implemented.** The comment on `FRAME_POLL_MS` claims reduced
  motion is honoured, but this file never calls `matchMedia` (CONFIRMED); the golden requires that
  reduced motion not animate the stream.
- `takeOver` and `release` duplicate the same takeover-field merge; `busy` collapses both actions.

## Concrete upgrades for v0.4.0

### 1. Extract `useComputerSession(botId)`; make the panel presentational

Move presence, status, frame, capability, action error, and `busy` into one hook backed by a pure
reducer (`state, event -> state`). `ComputerPanel` becomes `<section>` + hook output, with no RPC
imports. Risks: (a) behavior-preserving only — the DOM/role map must stay byte-compatible with the
REQ-026 golden (one region, one status, one img, disabled takeover) and the axe suite; (b) do not
over-abstract into context providers or a state library — one hook, props, one reducer; (c) reducer
and polling effect must be unit-testable without a live RPC (use the `tests/fixtures.ts` screen mock).

### 2. Derive the polling lifecycle from current status

Replace the one-shot bootstrap with an effect keyed on `botId` plus a status-driven rule: status
polling always runs every `STATUS_POLL_MS` (needed for recovery); frame polling starts only when the
latest status enters `LIVE_STATUSES` and stops on exit. On exit, clear `frame`, the capability ref,
and the generation. Use a per-session generation guard so rapid `stopped → running → stopped` flips
cannot leave two timers alive. Risks: (a) extra re-renders if status is set unconditionally —
compare status strings before `setStatus`; (b) golden fixtures that flip status must be added to
prove start/stop; (c) if the server's status string set grows, `LIVE_STATUSES` must stay in sync —
prefer a typed union in `rpc.ts`.

### 3. Thread `AbortSignal` through every call; reset session state on `botId` change

Create one `AbortController` per `(botId, session)`, pass its `signal` to status, openScreen, frame,
takeover, and release calls (the signatures already support it, CONFIRMED), and abort on cleanup or
bot change. Treat `AbortError` as a silent cancellation, never as `COMPUTERS_UNAVAILABLE` or an
action error. Wrap `computerOpenScreenFrame` in `try/catch`. Reset `frame`, capability, status, and
presence when `botId` changes. Risks: (a) a single shared controller means takeover/release started
just before a bot switch are aborted mid-flight — acceptable, but the UI must not show a stale
error; (b) aborting the status poll on every status change would kill recovery — keep one controller
per bot session, not per request; (c) unverified assumption that the RPC layer propagates the signal
to `fetch` for all five calls (GUESS — read `call()` before implementing).

### 4. Separate absence from failure; make stale frames honest

Reserve `presence: "absent"` for the real `COMPUTERS_UNAVAILABLE` code; keep the mounted panel on
other failures and expose a dedicated error/retry line. On a null frame from a stale capability (or a
generation bump), clear `frame` and surface `t("computer.status.stale")` in the existing `role=status`
element instead of keeping the frozen PNG. Fold the duplicated takeover-field merge into one helper
in the reducer. Risks: (a) new i18n keys across 14 locales (fallback rule applies); (b) this changes
visible failure behavior — record in `docs/PARITY.md` as a defect fix against REQ-026, not a silent
DEVIATION; (c) the absent path must still poll, or a backend that comes up later is never detected;
(d) keep the error line on `text-text-error-primary` / `border-border-error-default` only.

### 5. Accessibility, RTL, and reduced-motion integrity

Add `aria-busy={busy}` on the section, wrap the holder name in `<bdi>` (mixed-language user data),
keep `ms-auto` and all spacing on logical properties so the panel mirrors under `dir=rtl`, and
honour `prefers-reduced-motion`: when reduced motion is on, fetch one frame and do not schedule the
frame interval (the comment already promises this; the golden requires it). Subscribe to the media
query with cleanup, and restart/stop the frame timer if the preference changes at runtime. Risks:
(a) freezing the stream is a visible behavior change — confirm with the owner before shipping
(REQ-026 golden already encodes `reducedMotion: reduce`); (b) `matchMedia` listener must be removed
on unmount or it leaks; (c) re-run axe and the golden suite — adding a live region or `aria-busy`
can change the accessible role counts the golden asserts.

## Explicit non-goals

No new component library, no BoardUI Pro components, no hand-built lookalike buttons, no spec edit,
no change to the RPC wire contract, no realtime/WebSocket stream (polling stays per spec), no new
hardcoded user-facing text. Verify with the REQ-026 golden, `tests/motion.spec.ts`, the axe suite,
and a status-flip unit test of the extracted hook; mark every claim in `docs/STATE.md` CONFIRMED or
GUESS as today.
