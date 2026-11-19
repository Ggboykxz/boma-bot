# 0094 — ComputerPanel reliability proposal (v0.4.0)

Module: `apps/web/components/thread/computer-panel.tsx`
Lens: reliability
Spec anchor: REQ-026/027/029/030, `spec/08-ui-ux.md` §8.3–8.4 (panel states, takeover banner),
goldens `ui/REQ-026-computer-panel.json` and `computer-takeover-conflict.json`, `computer-status.json`.

Status of claims: "Current behavior" is CONFIRMED by reading this component, `lib/api/rpc.ts:1285-1372`,
`spec/01-requirements.md:213-253` and the two goldens this session; all proposals and risks are GUESS
(design inference, not implemented or tested).

## Current behavior (CONFIRMED)

- `status`/`frame`/`presence`/`actionError`/`busy` are plain state; `frameCapability` is a ref that is
  **never reset on `botId` change**. The effect depends on `botId` but only sets a `cancelled` flag.
- `loadStatus` maps *every* error (`COMPUTERS_UNAVAILABLE`, 401, 5xx, timeout, offline) to the same
  `presence="absent"`; the extracted `code` is dead — both branches are identical. Spec says "no
  computer" is `stopped`, never an error, so this conflates transport failure with an unsupported deployment.
- `LIVE_STATUSES = {queued, running, interrupted}`; `waiting_takeover`/`waiting_input` are absent, yet
  §8.4 says the panel is raised on "Waiting on takeover". Frame polling is started **once** in the initial
  `.then` if that first status is live; later live/stopped transitions never add/remove `frameTimer`.
- `computerOpenScreenFrame` is called with **no** try/catch, timeout or `AbortSignal`, despite every RPC in
  `rpc.ts` accepting `signal?`. `frameCapability` is cleared on a `null` frame but no status surfaces;
  the last `frame` stays rendered. The golden requires a stale capability to show as a status, not a frozen image.
- `screenGeneration` exists on both status and openScreen replies and is ignored; overlapping 2.5 s frame
  polls and 5 s status polls have no generation/sequence guard, so older responses can overwrite newer ones.
- `takeOver`/`release` merge the reply into `status` optimistically, but a concurrent poll can overwrite it;
  `busy` is a single boolean checked before React batches the disable, so a fast double-click can double-submit.

## Failure modes

1. **Session state (GUESS).** On bot switch the component keeps the previous bot's `status`, `frame`,
   `actionError`, `busy` and `frameCapability`, so stale screen bytes and a bot-scoped bearer capability can
   render on the new bot until they happen to fail. An in-flight `loadStatus`/`loadFrame` from the old bot
   resolves after cleanup (the interval is cleared, but the promise is not aborted) and writes stale state.
   A transient status error permanently hides the panel (`presence="absent"`), and a later poll recovering
   flips it back with a flash; there is no "degraded/retrying" state distinct from "unsupported". The takeover
   merge has no version token, so a 5 s poll can clobber a just-completed action and show the wrong holder.

2. **Stream gaps (GUESS).** A rejected `computerOpenScreenFrame` becomes an unhandled rejection (callers use
   `void loadFrame()`), silently ending that tick; a hung request stacks a new fetch every 2.5 s with no
   timeout, leaking promises on poor networks. Out-of-order responses freeze an old frame. When the computer
   starts *after* mount, the panel appears (status poll is reactive to render) but no frame timer exists, so
   the stream never starts; when it stops, frame polling continues pointlessly. A stale capability yields
   `null` and is only re-minted on the next tick, leaving a visible multi-second gap with a frozen image.

3. **Permission gating (GUESS).** The panel always renders "Take over"; there is no check of the caller's
   right or of whether the current user already holds control. A 403 is rendered as the same
   `computer.error.takeover` string as an offline/network fault, inviting repeated clicks on a forbidden
   action. `release` is offered whenever `isHeld`, including when someone else holds it — likely a server-side
   conflict (see `computer-takeover-conflict.json`) surfaced as a generic error. `heldBy` is shown as raw text
   with no "you / someone else" distinction.

## Concrete hardening for v0.4.0 (GUESS)

1. **Reset session on `botId` change (P0).** At the top of the effect, reset `status`, `frame`, `presence`,
   `actionError`, `busy` and set `frameCapability.current = null`; create an `AbortController` per effect run
   and pass its `signal` to all five RPC calls, aborting in cleanup. Guard every `setState` with `!cancelled`.
   *Risk:* resets cause a one-tick blank panel on legitimate remounts — assert no flicker in a bot-switch test.
2. **Split error classes (P0).** Keep `COMPUTERS_UNAVAILABLE` / 503 → `absent`; add a `degraded` presence for
   401/5xx/offline that keeps the last known status visible behind a retry affordance rather than removing the
   panel. Remove the dead `code` branch. *Risk:* new UI state needs a translated string and a golden update;
   mark any visible change as a DEVIATION in `docs/PARITY.md`.
3. **Make polling reactive and bounded (P0).** Derive frame polling from the *current* status via a second
   effect keyed on `isLive`; use recursive `setTimeout` (schedule next only after the previous settles) instead
   of `setInterval`, with a per-call timeout (e.g. 10 s) and try/catch around frame fetch. *Risk:* slower poll
   cadence under load; keep FRAME_POLL_MS/STATUS_POLL_MS constants and verify cadence with a mocked clock.
4. **Guarantee freshness (P1).** Track the latest `screenGeneration` from status/openScreen and ignore frame
   replies whose generation is older; on a `null` frame set a `stale-capability` status (not a frozen image)
   and re-mint immediately rather than waiting a full tick. *Risk:* generation may jump between polls — compare
   with `<=` and re-mint on mismatch, and confirm semantics against `computer-status.json`.
5. **Reconcile actions (P1).** Store a monotonically increasing local revision on takeover/release; on poll
   result, trust the local holder value until a reply with a newer generation arrives, or briefly suspend
   status polling while `busy`. Disable buttons synchronously with a ref-based `inFlight` guard to stop
   double-submit. *Risk:* suspension can hide a genuine conflict — cap it at one poll interval.
6. **Gate permissions (P2).** If status exposes a `canTakeover`/holder field, derive "you hold it" vs "another
   member holds it", disable takeover accordingly and use a distinct 403 translation. Otherwise keep the
   server as the authority but classify auth errors explicitly. *Risk:* the field may not exist in the DTO —
   confirm against the spec/OpenAPI before adding; do not invent UI-only authorization.

## Constraints

- Keep BoardUI semantic tokens / composite type utilities, `useTranslation` for every new string, and the
  existing `cx()`-free class strings unchanged.
- Every behavior change here is externally visible: record DEVIATIONs in `docs/PARITY.md` and await approval.
- Verification needs REQ-named Playwright cases (bot switch, transient 500, 403 takeover, late computer start,
  stale generation) plus a no-unhandled-rejection assertion; none were run in this session.
