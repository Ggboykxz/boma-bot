# 0047 — continue-in-a × error-state parity proposal (Boma Bot)

- Brief: `id 47`, area `continue-in-a`, lens `error-state` (`docs/openbot-swarm/briefs-1000.jsonl`).
- Status: **PROPOSAL only** — no product code, spec, golden, catalogue or dependency changed.
- Provenance: upstream read as **data only** (clone `/tmp/opencode/openbot-parity`, HEAD `559d5ac`, verified):
  `CHANGELOG.md` ("OpenBot now uses its saved conversation to continue in a new provider session"),
  `src/renderer/src/features/updates/whats-new-releases.json` ("An OpenCode agent continues in a new session
  when OpenCode cannot read its old session"), `src/backend/acp-client.ts` (`isSessionNotFound`,
  `MissingAcpSessionError`, failed-turn `error` notification carrying `causeCode`),
  `src/backend/agent/thread-lifecycle.ts` (`rotateProviderSession`, `ensureThread` with reason `"missing-session"`),
  `src/backend/agent/handoff-history.ts` (bounded, redacted history handoff),
  `src/backend/agent/thread-items.ts` (`isMissingProviderSessionError`),
  `packages/i18n/src/messages/en/error/provider.ts` ("started a new {provider} session with the chat history"),
  `docs/architecture/providers.md`, `scripts/provider-conformance-e2e.ts`. No upstream code, asset, wording or brand
  enters Boma Bot; no Pro, one design system. The slug is a truncated sentence fragment; the reading below is CONFIRMED
  from those entries. CONFIRMED = read this session; GUESS = owner decision.

## 1. Upstream capability, in my own words (inspiration only)

The provider keeps its own session per thread; the saved conversation lives on the host side. When the provider can no
longer read that session (unreadable file, missing history item, foreign reasoning, over-size request, or a
"session not found"/"Invalid params" reply for a dead session id), OpenBot classifies the failure as "the saved session
is gone", retires the stored provider session, opens a fresh provider session bound to the same public thread, and seeds
the first turn with a bounded, redacted handoff of the saved conversation (recent messages, the latest user/assistant
pair, captured work steps, pointers to recall older history). The room, files and messages stay; only the provider's
session rotates. The person sees one plain sentence — a new session was started with the chat history; try again —
and the recovery is logged as "replaced" and conformance-tested. Lossy by design, bounded, visible, retryable.

## 2. Parity position for Boma Bot, grounded in REQ-021

- **REQ-021 (MUST)** — failures surface as run status plus a reason, never a silent stop; a retryable classification;
  "the chat shows why" (`spec/01-requirements.md:170`). Trace: one testkit test
  (`req_021_a_run_that_fails_carries_a_legible_reason`, `crates/testkit/src/build.rs:802`) pins the stored `runs.error`;
  no web test names REQ-021 (CONFIRMED, `apps/web/tests/`).
- **The spec error envelope is the binding rule** (`spec/03-api-contract.md` §3.1): machine code + message safe to
  display (`UNAUTHORIZED|FORBIDDEN|NOT_FOUND|BAD_REQUEST|CONFLICT|TOO_MANY_REQUESTS|INTERNAL`); internal detail never
  crosses; golden `unauthorized.json` pins `{code, error}`. Today the failure travels `Failure{reason, detail}`
  (`crates/app/src/service.rs`; reason is a closed union: `rule_violation|conflict|screen_taken|invalid_credentials|
  stdio_disabled|rate_limited|dependency_unavailable|not_found|forbidden`) into `runs.error` — and nowhere else:
  `RunView` (`crates/api/src/wire.rs:117`) has no error field; the `run` SSE event carries `{runId,status}` only
  (`crates/app/src/usecases.rs:6557`); `AppError::is_retryable()` (only `Unavailable`) feeds routine backoff only.
- **Current screens.** Thread (`components/thread/thread-screen.tsx`): a failed run renders as a bare "Failed" caption
  (`run.status.failed`) — no cause sentence, no in-room retry — although spec §8.3 demands "Error row with a
  plain-language cause and a retry action (REQ-021)". Activity (`components/activity/activity-screen.tsx`): the failed row
  offers Retry but a failed `runs.retry` RPC is swallowed silently. Push exists (`notify_run_outcome`), content-free.
- **Recovery today.** Boma's ACP path (M23, REQ-061) spawns one subprocess per turn — fresh 0700 workspace, full thread
  context re-rendered each turn (`crates/app/src/acp_provider.rs`). There is no persistent provider session to lose, so
  rotate-and-continue is structurally unavailable, not merely unimplemented. `failed → queued` is the only exit from
  terminal (REQ-012; golden `run-retry.json`, CONFLICT otherwise); §4.4: retries bounded, count user-visible.

Gap: nothing on the wire or in the room names why a run failed, nothing says whether retrying helps, no copy exists for
a rotated/replaced executor session once ACP grows persistence.

## 3. Error-state parity rules (grounded in our own sources)

1. **One envelope discipline.** Project only a safe closed-union `errorCode` on `RunView` and the `run` SSE payload; the
   web maps codes → catalogue strings. Raw `detail`/provider text never crosses (§3.1; golden discipline). Ids behind
   "details" only (§8.9).
2. **The §8.3 Failed row ships in the room.** `RiErrorWarningLine` + one sentence of what happened (per `errorCode`) +
   one of what to do + Retry — REQ-021, REQ-012; `completed`/`cancelled` offer none (golden `run-retry.json`).
3. **Retryable is visible.** Project `retryable` (or fold it into `errorCode`); the row says whether Retry is likely to
   help and shows the attempt when > 1 (§4.4); the retry RPC's failure announces inline, never a silent no-op.
4. **Rotation honesty (GUESS).** When persistent ACP executor sessions land, a session-lost-shaped failure follows the
   upstream pattern (retire, rotate, bounded history handoff, disclose "a new session was started with the chat history")
   as one named catalogue key; until then no copy may claim a rotation that did not happen.
5. **Bounded, never silent.** No automatic re-queue outside routine backoff; the person decides (REQ-006, REQ-012). ACP
   spawn/timeout failures classify as `dependency_unavailable` (REQ-061, second criterion; never an empty bubble).
6. **Announce, don't interrupt.** The failed row rides a `role="status"` polite region; the transcript stays non-live
   (§8.9); focus is never yanked for a passive failure.
7. **RTL and catalogue.** Copy via `t()` in every shipped locale; logical `ps/pe`, `ms/me`; codes stay LTR in
   `<bdi dir="auto">`; `he`/`ar` mirrored, identical roles (§4.7; REQ-057).

## 4. BoardUI tokens, focus and motion

- **One system, free tier.** Error row: `border-border-error-default`, `bg-background-primary-default` /
  `-secondary-default`, `text-text-error-primary` (light `red-600`, dark `red-400`, per `docs/UI.md` re-tune),
  `text-foreground-icon-error`, `RiErrorWarningLine` (installed) + `RiRefreshLine` for Retry; composite
  `text-body-medium` / `text-caption-1-semibold` / `text-title-3-medium`; merged with `cx()`; never hex or raw palette;
  dark mode rides the tokens; the free `notification` card is the optional BoardUI block for the same row — prefer it
  over a third hand-rolled block.
- **A11y.** `.boma-target` ≥ 44×44 px on Retry (32 px floor, §8.9 density); `boma-focus` /
  `focus-visible:ring-2 focus-visible:ring-border-focus-ring`; state by text + icon, never colour alone (1.4.1);
  polite live region; entrance ≤ 120–180 ms cross-fade only, dropped under reduced motion (§8.9).
- **Layout.** Reuse the existing `rounded-xl border-border-error-default` error idiom (`thread.error.send`/`load`); one
  shared `ThreadFailureRow` component for thread + activity; `ModuleView` untouched.

## 5. One REQ-named Playwright test

Add to `apps/web/tests/thread-ui.spec.ts` or a new `run-failure.spec.ts` (no web spec names REQ-021 today):

```ts
const failedRun = { id: "run_failed", threadId: "thr_golden", botId: "bot_golden", status: "failed",
  trigger: "user", promptSnippet: "Plan my week", errorCode: "dependency_unavailable", retryable: true,
  createdAt: "2026-11-18T00:00:00Z" };

test("REQ-021 a failed run names its cause in the room and offers a retry", async ({ page }) => {
  await seedRpc(page, {
    answers: { "threads/messages": { messages: [], nextCursor: null } },
    stream: [{ type: "run", threadId: "thr_golden", seq: 41, payload: failedRun,
               createdAt: "2026-11-18T00:00:01Z" }],
  });
  await page.route("**/rpc/runs/retry", async (route) => route.fulfill({
    status: 200, contentType: "application/json",
    body: JSON.stringify({ runId: "run_failed", status: "queued", attempt: 2 }) }));
  await page.goto("/app/bot_golden/");
  const row = page.getByRole("status").filter({ hasText: /unavailable|try again/i });
  await expect(row).toBeVisible();
  await row.getByRole("button", { name: /try again/i }).click();
  // runs/retry fired; row re-announces "Queued"; axe light/dark/he at 390×844 = 0 critical/serious
});
```

Pins the §8.3 contract: named cause, one next action, Retry reaches `runs.retry`, never a silent no-op.

## 6. First slice (test-first, small, reversible)

1. Wire: `RunView` + the `run` SSE payload gain optional `errorCode`/`retryable` (omitted → goldens byte-identical);
   map `Failure::reason` → the union; never `detail`. Spec note in `spec/03` §3.1 additive — owner sign-off, no new REQ.
2. Catalogue: `run.error.<code>` keys (what happened + what to do) in every shipped locale; `<bdi>` for codes.
3. Web: `ThreadFailureRow` in thread + activity; Retry via `runsRetry`, in-flight disabled, inline failure surfaced;
   attempt count when > 1.
4. ACP: classify executor spawn/timeout as `dependency_unavailable` (REQ-061); no session persistence — record that
   rotate-and-continue stays out of scope until executor resumes exist (owner call: persist executor sessions, or
   keep stateless per turn).
5. Gates: typecheck, lint, REQ-021 + run-controls + a11y + motion specs; axe light/dark/he at 390/1280 px; record in
   `docs/STATE.md`; no new dependency, endpoint or DEVIATION.
6. Out of scope: provider-session rotation and handoff packets, golden updates (a `run-list` variant only with the
   owner), M32/Expo, any new REQ/ADR.

## 7. Boundaries

Inspiration only: no upstream code, asset, wording or brand; no Pro; no second design system; no new transport or REQ
without the owner; failure rows never animate focus or streamed content; this file is the only artefact written —
product code and the upstream clone stayed read-only.
