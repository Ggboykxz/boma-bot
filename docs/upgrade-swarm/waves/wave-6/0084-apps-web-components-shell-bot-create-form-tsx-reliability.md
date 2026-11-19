# 0084 · apps/web/components/shell/bot-create-form.tsx · LENS=reliability · v0.4.0 proposal

Scope: `bot-create-form.tsx` (102 lines). Status: proposal only — no source changed. Evidence legend:
**[C]** = read this session from the file, `rpc.ts`, `roster.ts`, `acp-detected-row.tsx`, `bot-create.spec.ts`,
`crates/{api,app,domain}/src` and the 2026-11-18 UI plan; **[G]** = inferred, needs a test to confirm.
Complements the architecture-lens brief for this module; no structural duplication.

## Current behavior (C)

- `submit` (l.30-53): `event.preventDefault()` → trim → guard `trimmed.length === 0 || submitting` →
  `setSubmitting(true)` → `botsCreate` → `await refreshRoster()` → close/reset → `router.push(/app/<id>/)`.
  Any rejection sets one boolean `failed`; `finally` clears `submitting`.
- Guard is a React state read (l.33), not a ref; button `disabled={submitting || blank}` (l.95). No
  `AbortSignal`/timeout is passed to `botsCreate` or `refreshRoster` though `call()` accepts one
  (`rpc.ts:245-260`); `refreshRoster` catches errors internally (`roster.ts:36-46`) but cannot be cancelled.
- Name validation is `length === 0` only (l.33/81/95); server also caps at 120 **chars** (`bot.rs:11,61-71`,
  `usecases.rs:359`) and has no uniqueness rule. `errorMessage={t("bots.create.name")}` reuses the label.
- Single catch (l.48) drops the error object: `RpcError.code/status` (`rpc.ts:169`) is ignored; all faults
  map to one `bots.create.error` string (l.90-94).
- Executor probe is **not** in this form: sibling `AcpDetectedRow` fetches `botsDetectedExecutors` on mount,
  catches failure as `[]` and renders nothing (`acp-detected-row.tsx:23-33`); server probe is a serial 2 s
  PATH check per command (`rpc.rs:4011-4043`); attach targets `roster.find(first active bot)` (l.35).

## Failure modes

**FM1 — double submit / stuck busy (C).** Two rapid Enter or click events before the state re-render both
pass the l.33 guard, firing two `bots.create` calls; the `+` toggle (l.64) and inputs stay live mid-flight,
so close/reopen can strand a submit. There is no idempotency key on the wire (`BotCreateRequest`, `rpc.rs:854`),
so a second tab or a client-only retry can create duplicate bots. A hung create or hung `refreshRoster` leaves
the button disabled forever with no error. `router.push` (l.47) is not awaited or guarded: on a failed/aborted
push the form is already closed and cleared, so the user is told nothing and sees no new bot.

**FM2 — name validation (C).** A 121-char name passes every client check, travels to the server, is rejected
by `Bot::validate_name`, and surfaces as the same generic "could not create" message — indistinguishable from
a network fault. `isInvalid` only marks empty-after-failed; the label is reused as the error text, so screen
readers get no real validation message. The plan's "duplicate name rejected before RPC" review focus
(2026-11-18 plan l.25) has no backing server rule (C: `bot.rs:61-71` checks only empty/length) — a client-only
uniqueness list would be a fabricated invariant and a DEVIATION; flag as a spec/plan conflict in `STATE.md`.

**FM3 — executor probe failure (C/G).** A failed/timeout probe leaves no row and no signal, so the bot just
created silently has no local executor; the create form reports unconditional success. The server probe is
serial (2 s × N executors) with no single overall client deadline, so the mount promise can stay pending for
many seconds on a remote deployment. After `refreshRoster()` publishes the new bot, `AcpDetectedRow` still
attaches to the *first* active bot, not the one just created and routed to — the "Attach" affordance can act
on the wrong bot. Coupling create success to probe success would make an unrelated best-effort check able to
fail the create — explicitly out of scope (G: product may want "no executor" feedback; needs an owner call).

## Concrete hardening (v0.4.0)

1. **FM1 ref lock (C).** Add `inFlight = useRef(false)`; check-and-set synchronously at the top of `submit`,
   clear in `finally`; gate both `onSubmit` and the button on it and on `submitting`. Disable the `+` toggle
   and inputs while busy (do not unmount — keep focus/a11y). *Risk:* a stale ref if a path returns before
   `finally` — keep a single exit; verify keyboard-only flow with axe.

2. **FM1 bound and reconcile (C).** Pass `AbortSignal.timeout(...)` to `botsCreate` and `refreshRoster`
   (`botsList`); map timeout to a retryable message that says the create *may* have succeeded and directs the
   user to the roster rather than blind resubmit. Await/guard `router.push`: on failure keep the form with a
   "created — open bot" link. *Risk:* a slow-but-valid create on a poor link could be cut off — tune to p95;
   no server state is rolled back, so never present Retry as a guaranteed clean slate.

3. **FM1 server idempotency (C/G, DEVIATION).** Propose a `clientNonce` on `bots.create` deduped server-side;
   a client ref lock cannot stop two tabs. *Risk:* wire-contract change — record in `docs/PARITY.md` and
   request operator approval before shipping; coordinate with the backend slice.

4. **FM2 mirror server rules (C).** Reject `[...name.trim()].length > 120` client-side with a dedicated
   `bots.create.name.length` key (add to en/fr/he); gate `onSubmit` on the same validity predicate as the
   button, not only on `disabled`. Replace `errorMessage={t("bots.create.name")}` with real required/length
   messages. *Risk:* the 120 constant is duplicated across Rust/TS — share a single value where possible or
   add a drift test; 2-3 new keys × 3 locales and an `aria-invalid`/`role="alert"` association check.

5. **FM2 classify errors (C).** Keep the `catch` argument and branch on `RpcError.status`/`code`:
   `BAD_REQUEST`/`InvalidName` → field error; `CONFLICT`/`TOO_MANY_REQUESTS` → specific retryable copy; 5xx
   and network → generic retryable. Do **not** invent duplicate-name rejection. *Risk:* over-differentiation
   can leak internals — keep one generic string per class and never echo server text verbatim.

6. **FM3 bound the probe, keep it best-effort (C).** In `AcpDetectedRow`, wrap `botsDetectedExecutors` with
   one overall `AbortSignal.timeout` (e.g. 5 s) and treat timeout as empty; never block shell render and never
   make `BotCreateForm` await the probe. *Risk:* a slow-but-real local executor may be missed — acceptable
   for a best-effort row; log the code for operators instead of showing an error to the user.

7. **FM3 post-create target (C/G).** Scope the detected row to the bot on the current route (`/app/[botId]/`)
   so attach follows the newly created bot, or pass the created id through the shell after `router.push`.
   On empty probe, at most one neutral i18n'd "no local executor" note in the row. *Risk:* route-param coupling
   between modules; an externally visible attach-target change needs a test and a `PARITY.md` entry if shipped.

## Test coverage (REQ-tagged Playwright, extend `bot-create.spec.ts`)

- `REQ-004`: two rapid Enter/clicks → exactly one `bots/create` POST; busy button/toggle disabled (FM1-1).
- `REQ-004`: 121-char name → no RPC, dedicated length message; whitespace-only still rejected (FM2-4).
- `REQ-004`: hung `bots/create` → button released + retryable copy; retry does not double-create (FM1-2).
- `REQ-004`: 400 vs 500 → distinct messages; label no longer used as the error text (FM2-5).
- `REQ-061`: `bots/detectedExecutors` 503/timeout → shell renders, row absent, create still works; after a
  second creation attach targets the routed bot, not the first active one (FM3-6/7). *Risk:* route-mock timing
  brittleness — reuse `seedRpc` fixtures and fake timers for timeouts.

## Explicitly deferred / owner calls

- Server idempotency key (FM1-3), any new wire field, and attach-target change (FM3-7): DEVIATIONs — approval.
- Duplicate-name policy and "new bot has no executor" feedback: no server rule / new behavior — `STATE.md`.
- Auto-attach on create: forbidden by the plan; keep an explicit click only.

## Success criteria

- One user action can never produce two create RPCs; no request can strand the form in `submitting`.
- Every server/client validation or fault class has its own honest message; no label-as-error, no generic hole.
- A failed executor probe never blocks creation or the shell, and never attaches to the wrong bot.
- New states stay accessible (`role="alert"`, visible focus, no color-only signal) across en/fr/he, light/dark, 360 px.
