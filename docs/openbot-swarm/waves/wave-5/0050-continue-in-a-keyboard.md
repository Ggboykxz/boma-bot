# 0050 — continue-in-a × keyboard parity proposal (Boma Bot)

- Brief: `id 50`, area `continue-in-a`, lens `keyboard` (`docs/openbot-swarm/briefs-1000.jsonl`).
- Status: **PROPOSAL only** — no product code, spec, golden, catalogue or dependency changed.
- Provenance: upstream read as **data only** (`/tmp/opencode/openbot-parity`, HEAD `559d5ac`):
  `CHANGELOG.md:9432` (literal source of the area fragment),
  `docs/architecture/channels-and-messaging.md:262-265,334,521`, `docs/architecture/agents.md:941-944,1049`,
  `packages/i18n/src/messages/en/queue.ts:25`. No upstream code, asset, wording, brand or Pro
  enters Boma Bot. CONFIRMED from files read this session; else GUESS.

## 1. Upstream capability, in my own words (inspiration only)

When the host's link to a provider session breaks mid-conversation, the app keeps the saved
conversation and continues it in a fresh provider session, so the thread stays usable. Same family:
a turn that a failed sign-in or a silent provider ended after work had started continues on the
backup model; restart loses no policy and work resumes; typed "continue"/"resume"/"go on"/"keep
going" re-dispatches stopped root tasks; a paused subtask offers an explicit Continue/Reassign/Cancel
affordance. Generic rules (restated, not copied): the continue affordance is a named, natively
operable control (Tab/Enter/Space; no pointer, no custom key handling); activation produces a visible
state change and announces it; typed text stays text; nothing dead-ends silently; no chord is added to
text-field focus and no keystroke waits on an animation.

Transferable fact: resuming interrupted work is a first-class keyboard path, not a leftover button.
Mirror risk: the chord or the typed word leaking into every text input.

## 2. Parity position for Boma Bot

- **REQ-012** is the only continue transition: `failed → queued` (`spec/03-api-contract.md`). The one wired
  surface is the Activity screen (`components/activity/activity-screen.tsx`): the failed row alone shows
  retry (`isRetryable`, `lib/run-status.ts`), calling `runsRetry` (`lib/api/rpc.ts:538`). After success it
  updates nothing and announces nothing; the `role="status"` `<p>` only re-renders on a refetch retry never
  triggers. A keyboard-only member activates retry and gets silence.
- The transcript has **no** failed-run continue row; §8.3's "Failed — error row … retry action
  (REQ-021)" is not implemented (GUESS: later milestone; no component/key exists).
- Pause/resume for queued/paused runs (REQ-067) ships in the same screen, same pattern, same gap.
- No chord for retry exists anywhere (§8.9: send, new line, stop, focus composer, next/previous bot,
  close panel, search, `?`) and none is proposed. BoardUI `Button` carries
  `focus-visible:ring-2 ring-border-focus-ring`; product links use `.boma-focus`; thumb-reached
  controls use `.boma-target` (44 px).

## 3. Keyboard-path parity, grounded in spec/08-ui-ux.md

- §intro "**Keyboard first.** Every action in a menu is reachable without a pointer." Retry/resume are
  tabbable labelled buttons — keep them so; never hover-only or unlabelled icon-only.
- **§8.4** (Activity view): rows are list items whose one recovery action is retry; after retry the
  row updates in place without remounting.
- **§8.9 Keyboard** — shortcuts "never fire while a text field has focus except send/new-line." Retry
  adds no chord, leaves the composer guard intact, and typed "continue" is free text (upstream's typed
  keyword refused deliberately).
- **§8.9 Announcements** — status changes announce politely; the transcript is not a live region.
  Reuse the existing `role="status"` line: the catalogue flip announces once; failure renders one
  inline `role="alert"` sentence.
- **§8.9 Error copy / Density / §4.7** — one sentence, no ids; targets ≥ 32 px (Button `small` =
  32 px); keyboard reachable; colour never sole signal (word + icon).
- **§8.10** — no golden: retry is a click path, not a seeded state (`REQ-012-run-statuses.json`
  stays authoritative for chips).

## 4. Focus rings and no stolen shortcuts

- Ring: BoardUI `Button` keeps `focus-visible:ring-2 focus-visible:ring-offset-2
  ring-border-focus-ring`; product links keep `.boma-focus` (two-tone, `styles/boma.css`). No
  hand-rolled outline, raw palette hex or `dark:` literal; dark mode rides the token; ring never
  depends on hover. Focus after activation stays on the control (it re-enables) — never dropped to
  `<body>`, never moved mid-request; the live region carries the outcome, not focus.
- Not stolen: no ⌘/Ctrl-Enter, no `r` for retry, no typed-word interception. A future chord lands in
  §8.9 and the `?` overlay (absent; GUESS) — owner decisions, proposals only.
- **No animation on high-frequency actions.** Retry/resume repeat: activation gives immediate pressed
  feedback, no transition gating the request; the row updates in place and does not re-trigger
  `animate-row-slide-in`; `.boma-target:active` scale stays `hover:hover`+`pointer:fine`-scoped, so
  keyboard activation never waits on 100–150 ms. `prefers-reduced-motion` already drops row animation
  to 1 ms — keep it.

## 5. One REQ-named Playwright test (`apps/web/tests/run-controls.spec.ts`; REQ-012)

```ts
test("REQ-012 a keyboard-only member retries a failed run and the row says so", async ({ page }) => {
  let retried = 0;
  await seedRpc(page, { answers: { "runs/list": { runs: [failedRun] } } });
  await page.route("**/rpc/runs/retry", async (route) => {
    retried += 1;
    await route.fulfill({ status: 200, contentType: "application/json",
      body: JSON.stringify({ id: "run_failed", status: "queued" }) });
  });
  await page.goto("/app/activity/");
  const retry = page.getByRole("button", { name: "Retry" });
  await retry.focus();
  await expect(retry).toBeFocused();
  await page.keyboard.press("Enter");
  expect(retried).toBe(1);
  await expect(page.getByRole("status").filter({ hasText: "Waiting to start" })).toBeVisible();
});
```

Pins: keyboard-only reach and operation; `failed → queued` the only offered transition; a once-announced
outcome. Fails on today's code (no state update, no announcement). Mirrors the REQ-067 tests. No golden.

## 6. First slice (test-first, small)

1. Add the failing test; record the red.
2. `activity-screen.tsx`: on `runsRetry` success map the row in place to `status: "queued"` (id
   preserved; no remount / animation replay); on failure render one `role="alert"` sentence
   (`activity.retry.failed`), focus stays on the button. Grid, RPCs, goldens untouched.
3. i18n: add `activity.retry.failed` to `en`/`fr`/`he` (English fallback covers the rest, REQ-057);
   no English literals; RTL unchanged.
4. `docs/STATE.md`: log proposals — in-thread REQ-021 failed-run row (§8.3 vs shipped), §8.9 `?`
   overlay absent, no retry chord (deliberate). `docs/PARITY.md` untouched (no wire change);
   `docs/UI.md` gains an entry if the component changes.
5. Gates: typecheck/lint/build; Playwright run-controls + a11y + locale clusters, light/dark/RTL,
   360 px; goldens replayed unchanged; axe zero critical/serious.

## 7. Non-goals / evidence

No new REQ/RPC/field/golden; no `?` overlay; no chord; no typed-keyword interception; REQ-017 untouched;
no animation on the retry path. Upstream is an oracle, not a source.

CONFIRMED — spec/08 intro/§8.3/§8.4/§8.9/§8.10; spec/01 REQ-012/-021/-057/-067; spec/03 `runs.*`
(retry only from `failed`); spec/04 §4.7; `components/activity/activity-screen.tsx` (retry; no
refetch/announce; `role="status"` p), `lib/api/rpc.ts:532-546`, `lib/run-status.ts` (`isRetryable`),
`components/thread/thread-screen.tsx` (no failed-run row), `components/base/buttons/button.tsx` (ring),
`styles/boma.css` (`.boma-focus`, `.boma-target`), `styles/globals.css` (`animate-row-slide-in`),
`tests/run-controls.spec.ts`, `tests/fixtures.ts` (`seedRpc`, `runs/retry` seed),
`tests/motion.spec.ts` (frequent actions get no animation), `lib/i18n.ts` (14 locales, English
fallback). Upstream HEAD `559d5ac`: CHANGELOG:9432; channels-and-messaging.md:262-265,334,521;
agents.md:941-944,1049; queue.ts:25. No build or test run for this document.

GUESS — whether the in-thread REQ-021 row lands later (milestone); copy/placement of a richer
retry-outcome announcement; the `?` overlay shape; upstream's changelog-only signal that
provider-session continuation is user-invisible (no control to mirror).
