# 0002 — Always-allow-on, empty-state lens (parity proposal)

- Brief: `id 2`, area `always-allow-on`, lens `empty-state` (`briefs-1000.jsonl`).
- Upstream read as **data only** from the parity clone at HEAD `386085ff` (2026-11-18). No upstream
  code, asset, wording, icon, colour, font or brand enters Boma Bot. No Pro component, no second
  design system. All claims below are CONFIRMED from the files read this session unless marked GUESS.

## 1. Upstream capability, in my own words (inspiration only)

The analysed product separates an agent's *permission level* (restricted / ask-first / full access)
from a per-tool **"always allow"** consent decision. That choice is kept on the user's side, scoped
to the tool and provider account, persisted in a file-backed layer; later calls raise no card. A
full-access agent likewise does not pause — the host answers its own approvals, so a card never
reaches a client. Grant changes can clear approvals already waiting; importing downgrades a
previously-on "always allow" to a warning so a safe default wins. Two consequences matter: (a)
"always allow on" means *no card is created*, and (b) that is a deliberate, named, reversible policy
state — not the absence of activity.

## 2. What "always allow on" means in Boma Bot

Boma Bot already owns the concept: **REQ-017** says "a policy per space and per user decides which
tool calls need approval." Always-allow is one value of that policy — a classified call produces no
pending card and its effect runs under the recorded idempotency key. We reuse that contract, invent no
new permission vocabulary and copy no upstream level names. The empty-state lens asks what the spec
leaves implicit: **what does the approvals surface show when the policy is on and no card is pending?**

## 3. Empty-state implications, grounded in our spec and surface

- `spec/08-ui-ux.md` §intro: "State is never ambiguous", "a blank panel is a bug", "the interface
  must survive latency". An empty approvals list must therefore be a *named, explained state*, never
  a blank region, a spinner that outlives its work, or a disabled-looking void.
- §8.3 "Waiting on user — the card is the state" and the thread-state table: when no card is pending,
  the composer is not blocked (only modal approvals block it). An always-allow thread must not show
  "Waiting for your decision"; it shows the ordinary idle/streaming composer.
- §8.9 approvals are modal and blocking *for the effect only*; announcements go through a polite live
  region. A transition into always-allow must announce the mode change, not stay silent.
- `docs/UI.md` component table: REQ-007 card states are pending / answered / timed out. There is no
  "none because auto-approved" state today — that is the gap this proposal fills.

### Real RPCs we build on (no new endpoints invented)

- `threads.messages` — no `card` block is the per-thread empty case; `threads.answer` is the action
  *not* offered when no card is pending (REQ-017).
- `runs.list` — `waiting_input` / `queued` / `running` (§8.9, REQ-012) shows the run is progressing,
  not stalled; reuse `lib/run-status.ts`.
- `GET /api/v1/automation/approvals` → `{ "approvals": [] }` and `POST .../approvals/{id}/answer` (REQ-072) — the inbox whose empty state we name.
- Precedent for a persisted per-bot policy panel: `notifications/policyGet` / `policySet`. The
  always-allow control follows that shape; assume no new RPC yet (GUESS — check `crates/api/src/rpc.rs`).

## 4. Proposed surface (BoardUI, one design system)

An empty-approvals state is a named `region` ("Pending decisions") with a status line in
`text-title-2-medium` / `text-text-primary`, one sentence in `text-body-medium` / `text-text-secondary`
saying no action is waiting because the bot is set to always allow, and one control (a `switch` or
`select` for the policy) plus a link to the bot's settings panel. Layout uses `gap-*`, `rounded-3xl`,
`border-border-button-default`; page ground `bg-background-full`, surface
`bg-background-secondary-default`, hairline `border-separator-border`.
Status chips reuse the installed run-status `chip` pattern (text + icon, never colour alone). Small
icon glyphs come from `@remixicon/react` as component references; classes merge with `cx()` from
`@/utils/cx`. Focus is `focus-visible:ring-2 focus-visible:ring-border-focus-ring` (`.boma-focus` for
our own link), targets `.boma-target` (44 px). A long inbox list uses the installed `ScrollFade`
(`@/components/foundations/scroll-fade/scroll-fade`) for the softened top edge. No copy is hardcoded:
new keys go through `t(locale, key)` in `lib/i18n.ts`; a missing key falls back to English.

## 5. Accessibility, RTL, motion

- **a11y** — WCAG 2.2 AA, zero critical/serious axe in light, dark and RTL. Name the empty region so
  a screen reader announces *why* the list is empty; announce mode changes through the polite live
  region (§8.9), never by making the transcript a live region. Keyboard path reaches the policy
  control and settings link; focus is visible and token-driven.
- **RTL** — mirror via `DirectionProvider` and logical styles (`ms`/`me`, `ps`/`pe`, `start`/`end`,
  `text-start`/`text-end`); flip arrow glyph and drawer motion. Keep ids, URLs and code LTR in
  `<bdi>` / `dir="auto"`.
- **motion** — 120–180 ms, `prefers-reduced-motion` respected, no animation on streaming content,
  never animate a value the user reads. Empty→populated is a plain token cross-fade, not a spinner.
- **Connectivity** — while the RPC is in flight show a state-shaped skeleton, not a blank panel.

## 6. One REQ-named Playwright test

In `apps/web/tests/approval-card.spec.ts`, seeded through `seedRpc`, add exactly one test:

```ts
test("REQ-017 always-allow policy shows a named empty approvals state, not a blank panel", async ({ page }) => {
  await seedRpc(page, {
    answers: {
      "bots/get": { id: "bot_golden", name: "Boma Bot", threadId: "thr_golden" },
      "threads/messages": { messages: [] },          // no card block
      "runs/list": { runs: [{ id: "run_1", status: "running" }] },
    },
  });
  await page.goto("/app/bot_golden/");
  const region = page.getByRole("region", { name: "Pending decisions" });
  await expect(region).toBeVisible();
  await expect(region).toContainText("No action is waiting");
  await expect(region).toContainText("always allow");
  await expect(region.getByRole("button", { name: "Approve" })).toHaveCount(0);
  await expect(page.getByRole("textbox", { name: "Message" })).toBeEnabled();
});
```

It pins an accessible name and the absence of an Approve button, and asserts the composer is *not*
blocked — the exact empty-state contract above. Extend the REQ-007 golden pattern if this state
needs a `spec/golden/ui/REQ-017-always-allow-empty.json` (state, not click path).

## 7. First slice

One vertical, reversible slice: a **read-only named empty state** on the thread's pending-decisions
region when `threads.messages` returns no card and the policy is always-allow. Ship the copy keys,
the BoardUI-tokened empty component, and the one REQ-017 Playwright test above. Do not yet add the
policy write RPC or the settings control — those are a later slice once the policy endpoint is
confirmed. Gate the slice on `npm run lint`, the axe gate, and the REQ-017 test passing.

## 8. Boundaries / decisions

- No upstream wording, brand, asset or code is reproduced; the analysis is inspiration only. No Pro
  component and no second design system; BoardUI free tier and installed components only.
- No new externally visible API/data behaviour, so no DEVIATION entry — a UI state inside REQ-017.
- GUESS (per AGENTS.md): the policy control/RPC name is unconfirmed; keep the first slice read-only
  and record the question in `docs/STATE.md` before coding.
