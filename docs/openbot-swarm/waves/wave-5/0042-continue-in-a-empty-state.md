# 0042 — "continue in a new provider session" through an empty-state lens (parity proposal)

- Brief: `id 42`, area `continue-in-a`, lens `empty-state` (`docs/openbot-swarm/briefs-1000.jsonl`, slug `0042-continue-in-a-empty-state`).
- Provenance: upstream (HEAD `559d5ac`, 2026-11-19) read as **data only** in `CHANGELOG.md` (0.34.0, 2026-10-09, "Fixed": the saved conversation is used to continue in a new provider session when a provider reports its session missing as an "Invalid params"-class error) and in the ACP restore path (`src/backend/acp-client.ts`: a missing provider session is a first-class outcome, a fresh session is re-established and reseeded from the saved conversation rather than dead-ending the chat; `scripts/provider-fakes/agents/session-errors.ts` covers the not-found shapes). Product code stayed read-only; no upstream code, asset, wording, brand, provider name or URL enters Boma Bot; no BoardUI Pro, no second design system. Claims are CONFIRMED unless marked GUESS. Language: English (disk convention).

## 1. Upstream capability, in my own words (inspiration only)

An agent's handle on the provider-side conversation can vanish — the provider reports it gone or unreadable. The app treats that as recoverable, not fatal: it opens a brand-new provider session, reseeds it from the persisted conversation and lets the person keep talking. Nothing is replayed into a void and no raw provider string reaches the screen; the invisible invariant is *continuity survives provider forgetfulness*. The failure this fixes is the dead end — a raw error where the thread used to be, or a resume that silently starts a blank context and forgets the prior conversation.

## 2. The parity target in Boma Bot

Boma Bot is already stricter: every run is a fresh provider context assembled from the saved thread, with REQ-069 window-aware compaction before the provider sees it — there is no long-lived provider session to lose. The gap is on the screen, not the backend. In `apps/web/components/thread/thread-screen.tsx` (`phase === "ready" && messages.length === 0`, line ~501) zero messages renders `EmptyThread` — the new-bot intro and two suggested prompts — **even when zero is the symptom of a resume that came back empty or a run failed before its first token**. The screen cannot distinguish "genuinely a new bot" from "the conversation is not visible but continuity survived" — precisely the ambiguity spec/08 forbids (state never ambiguous; a blank panel is a bug; the interface must survive latency). This lens closes that gap.

## 3. Empty-state implications grounded in our own sources

- `spec/08-ui-ux.md` intro: named state out loud, no spinner that outlives its work, skeletons everywhere, no hardcoded strings. §8.3 thread states: `Empty (new bot)` (intro + two prompts, composer focused) and `Failed` ("Error row with a plain-language cause and a retry action (REQ-021)") — but **no named state for "resume lost, conversation intact"**. §8.9: one sentence of what happened and one of what to do; announcements via a polite live region, never the transcript; 120–180 ms motion, `prefers-reduced-motion`; RTL mirrors. §8.10: a state pins a REQ-named golden. A §8.3 row for the new state is a spec edit only with a `docs/STATE.md` note (GUESS: owner approves).
- `docs/UI.md` Foundations 1–10 and token map: semantic tokens only (`bg-background-full` ground, `bg-background-secondary-default` panel, `border-separator-border` hairline, `text-text-primary/-secondary/-tertiary`, `text-foreground-icon-primary/-secondary`), composite type (`text-title-2-medium`, `text-body-medium`, `text-caption-1-semibold`), `cx()`, 44 px `.boma-target`, token focus ring; every string via `t(locale, key)` in `lib/i18n.ts`, English source, missing key falls back to English (REQ-057, 14 locales, `he`/`ar` RTL).
- Real RPCs (no new endpoints): `threads/messages` (the saved conversation is readable exactly when the provider forgot it — REQ-005), `threads/head` (REQ-009), `threads/send` (the next message continues in a fresh provider context by construction — REQ-001/002 idempotency), `runs/retry` (only `failed` may retry — REQ-012; the call is written in `lib/api/rpc.ts`, the M1 note says it may answer not-found and is reported like any other error), `runs/list`, `threads/subscribe`, `bots/get`, `bots/list`. Existing `thread.error.load` / `thread.error.send` / `run.status.failed` keys show the house pattern for failure copy.

## 4. Proposed surface (BoardUI, one design system)

Split today's single empty state into two mutually exclusive, named regions in `thread-screen.tsx`:

1. **`thread.empty.new`** — unchanged new-bot state (intro + suggested prompts), wrapped in `role="region"` with an accessible name.
2. **`thread.empty.resume_lost`** — shown when zero messages coincides with a failed run or an empty reply from the history fetch after a subscribe failure: `text-title-2-medium` heading, one `text-body-medium` / `text-text-secondary` sentence saying the saved conversation is intact and that sending continues it in a fresh context, plus one `.boma-target` action ("Retry this turn", `RiRefreshLine` as a component reference, `aria-hidden`) calling `runs.retry` for the failed run, and the composer focused. Panel on `bg-background-secondary-default`, `rounded-3xl`, `border-separator-border`; icon `text-foreground-icon-primary`; state never colour- or icon-only. No vendor name, no diagnostics, no ids (§8.9). Copy ships as new i18n keys in all 14 locales.

## 5. Accessibility, RTL, motion

- **a11y** — WCAG 2.2 AA, zero critical/serious axe in light, dark and RTL. The region carries an accessible name so a screen reader hears *why* it is empty; the explanation is static text, state changes go through the polite live region (§8.9), targets ≥44 px, keyboard reaches the action, focus is the token ring.
- **RTL** — locale drives `DirectionProvider`; logical styles (`ms`/`me`, `ps`/`pe`, `start`/`end`, `text-start`/`text-end`); the retry glyph stays LTR-agnostic; ids/URLs in `<bdi>`/`dir="auto"`.
- **motion** — 120–180 ms cross-fade between the two states, `prefers-reduced-motion` respected, no animation on streamed content (BoardUI `composer-loader` untouched).

## 6. One REQ-named Playwright test

In `apps/web/tests/thread-ui.spec.ts`, reusing `seedRpc` from `./fixtures`:

```ts
test("REQ-021 a lost resume shows a named, explained empty state with a working retry", async ({ page }) => {
  await seedRpc(page, {
    answers: {
      "bots/get": { id: "bot_golden", name: "Boma Bot", threadId: "thr_golden", archivedAt: null, snoozedUntil: null, unreadCount: 0 },
      "threads/messages": { messages: [] },
      "runs/retry": { ok: true },
    },
  });
  await page.route("**/rpc/runs/list", (r) =>
    r.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify({
      runs: [{ id: "run_failed", threadId: "thr_golden", botId: "bot_golden", status: "failed", trigger: "user", promptSnippet: "Plan my week", createdAt: "2026-11-19T00:00:00Z" }] }) }));
  const region = page.getByRole("region", { name: "This conversation is saved" });
  await expect(region).toBeVisible();
  await expect(region).toContainText("continues");       // our own copy, not the new-bot intro
  await expect(page.getByRole("button", { name: "Retry this turn" })).toBeEnabled();
  await expect(page.getByRole("button", { name: /Plan my week/ })).toHaveCount(0); // not the new-bot state
});
```

GUESS: how the thread learns the failed run's id (a `runs/list` for the thread, or the run id carried on the stream) — smallest honest wiring, recorded in `docs/STATE.md` before implementation.

## 7. First slice

One vertical, reversible slice: the two mutually exclusive named empty regions in `thread-screen.tsx` (+ `runStatus === "failed"` detection), the new i18n keys (14 locales, English source, fallback intact), the REQ-021 test above, and a §8.3 spec row noted in `docs/STATE.md` (row itself only with owner approval; `golden/ui/REQ-021-…` optional). No new RPC, no new dependency, no behaviour change to send/retry paths. Gates: `npm run lint`, the axe gate (light, dark, RTL), the REQ-021 test.

## 8. Boundaries

- Inspiration only: no upstream code, brand, wording, icon, colour, provider name or URL; no BoardUI Pro; no vendored-file edit; one design system.
- No new externally visible API/data behaviour — a named UI state inside REQ-021/012/004/005 — so no DEVIATION; the §8.3 spec row and any golden are owner-gated via `docs/STATE.md`.
- This file is the only artefact written; product code and the upstream clone stayed read-only.
