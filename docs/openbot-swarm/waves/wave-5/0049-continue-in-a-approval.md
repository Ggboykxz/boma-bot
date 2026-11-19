# Brief 0049 — continue-in-a through the approval lens (Boma Bot)

- Brief: id 49, area `continue-in-a`, lens `approval` (`docs/openbot-swarm/briefs-1000.jsonl`).
- Status: **PROPOSAL only** — no product code, spec, golden, catalogue or workflow changed.
- Provenance: upstream read as **data** at `/tmp/opencode/openbot-parity` (HEAD `559d5ac`, 2026-11-19;
  inventory pins `386085ff`). No upstream code, asset, wording, identifier or brand reproduced; no Pro;
  all strings stay Boma Bot. **CONFIRMED** = read this session; **GUESS** = recommendation for the owner.

## 1. Upstream capability, in my own words (data only)

CONFIRMED: the upstream product lets an agent present a plan before a long task — a card naming the
steps, with a primary "start" action and a cancel, surfaced through an attention registry and rendered
by a shared confirm block (it also has a press-and-hold mode for danger or explicitly held actions).
When an operator enables the per-agent plan-card option and a wait, the host arms a **server-side
timer** on the start action; the button shows a countdown; if nobody answers before it expires, a
privileged, automatic responder writes the start answer on the operator's behalf, the run proceeds,
and the record later shows an answer with a "(nobody answered in time)" suffix. The wait is an
operator setting: wait-forever, presets 10/30/60/300 s, or a custom value with a 5 s floor
(`DEFAULT_PLAN_START_AFTER_MS` 10 s, `MIN_` 5 s; a settings row and mobile mirror). The surrounding family
is the same "run resumes after a decision" pattern — approvals in chat threads, numbered phone answers,
clipped cards offering only the safe choice.

I **do not** adopt silence-as-consent: a countdown is not a decision, a privileged automatic flag is
not attribution, and "work starts in N seconds unless you cancel" is an implicit approval with extra
steps. Hold-to-confirm is noted, not taken. The literal "continue in a…" fragment starts no UI string
in the clone; the nearest are "Continue in browser" / "Continue in your browser", and the only
`continue in a` hit is mid-sentence in `CHANGELOG.md`. The slug→plan-card mapping is a **GUESS**; the
approval lens is the real anchor.

## 2. Why approval is the deciding lens

Our only client surface for an external effect is the pending `CardBlock` (spec §8.3 "the card is the
state"; §8.9 approvals modal: one click, an explicit "what will happen" sentence). Upstream answers
"what if nobody responds?" with "the work starts anyway"; REQ-017 answers "nothing starts". Parity
means no waiting affordance of ours may answer it otherwise — anything less re-opens the hole ADR-033
closed on the question path.

## 3. Grounding in our contract

- **REQ-017 (anchor, MUST)** — `spec/01-requirements.md:137`: a classified call creates a pending card;
  no effect runs until a member approves; an approved effect runs exactly once under its key.
- **Contextual `CardBlock` + ADR-033** — `docs/adr/033-no-silent-approval-on-empty-answer.md`: an
  empty free-text answer is never consent; submit is disabled until the field is non-empty; the
  approval/purchase path keeps explicit Approve/Deny buttons, the `CONFLICT` already-answered guard,
  the composer gate and the `role="status"` waiting line. No `threads.answer` without a deliberate act.
- **No plan/start timer exists in Boma Bot** (CONFIRMED): no `planStart`/auto-responder setting, no
  countdown on any card, in `spec/`, `apps/` or `crates/`; `thread.card.*` covers only
  `approval|purchase|question|takeover`. `tools/trace_check.py` counts declaration lines only, so the
  new test keeps `REQ-017` in its name.

## 4. Proposed shape — the card states what silence does

GUESS (design; owner approves before code): one new read-only caption in the pending branch of
`CardBlock`, for `approval`/`purchase` only, under the waiting line — "Nothing runs until you choose."
— in our own catalogue (working key `thread.card.noauto`; `en`/`fr`/`he` plus English fallback in the
other eleven locales; wording is the owner's call). Semantic tokens only (`text-text-tertiary`,
`text-caption-1-regular`), no icon, no colour-alone cue, merged with `cx()`, all copy via `t()`, no
`dark:` literals, no Pro. Absent on the answered path; question cards keep their ADR-033 hint; status
line, buttons, `CONFLICT` guard and composer gating untouched. Upstream-style waiting or early-start
behaviour would need a new REQ id, an ADR and explicit sign-off.

**No implicit approvals — hard invariant:** no timer, countdown, hold-to-release, auto-advance,
silence-as-consent, defaulted action or privileged automatic responder on any card kind, ever, without
a REQ + ADR + owner approval; effect keys and the exactly-once path are never weakened.

## 5. One REQ-named Playwright test

In `apps/web/tests/approval-card.spec.ts`, beside the existing `REQ-007`/`REQ-017` cases (reuse
`seedApproval`, `seedRpc` and the "Pending decision" region; distinct from the deny and empty-answer tests):

```ts
test("REQ-017 a pending approval card never continues the run until a member chooses", async ({ page }) => {
  await seedApproval(page); // pending approval card, bot_golden
  const calls: unknown[] = [];
  await page.route("**/rpc/threads/answer", async (route) => {
    calls.push(route.request().postDataJSON());
    await route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify({ ok: true }) });
  });
  await page.goto("/app/bot_golden/");
  const card = page.getByRole("region", { name: "Pending decision" });
  await expect(card).toContainText("Nothing runs until you choose."); // the silence rule, stated
  await expect(card.locator('[role="timer"]')).toHaveCount(0);        // no countdown may exist
  await expect(card.getByRole("status")).toContainText("Waiting for your decision");
  await expect(page.getByRole("textbox", { name: "Message" })).toBeDisabled(); // effect gated
  expect(calls).toEqual([]);                                          // no answer without a deliberate act
  await card.getByRole("button", { name: "Approve" }).click();
  expect(calls).toEqual([{ cardId: "card_approval", answer: "approve" }]); // exactly once, attributable
  await expect(card).toContainText("Approved");
});
```

## 6. First slice (test-first, smallest complete loop)

1. Add the one catalogue key to `en`/`fr`/`he` and the English fallback in the other eleven locales;
   identical key sets, no other strings.
2. In `CardBlock` only, render the caption for pending `approval`/`purchase`; do not touch the
   `CONFLICT` guard, buttons, gating, `question`/`takeover` branches or the answered path.
3. Add the single REQ-017 test above; confirm it fails first, then green.
4. Note the proposal, the refused upstream timed-consent mechanism and the files consulted in
   `docs/STATE.md` as CONFIRMED/GUESS; no new REQ id, no wire/data/permission or golden change.
5. Gates unchanged: web typecheck, lint, build, Playwright approval-card/golden/axe clusters,
   360 px, light/dark/RTL.

**Out of scope:** any plan/start card, timer, auto-answer, countdown, hold-to-start UI, new card kind,
`takeover`, purchase-copy rewording, dependency changes, OTLP, any approval-policy RPC.

## 7. Evidence

CONFIRMED — upstream `559d5ac`: `attention-registry.ts` (`surfaceUiBlock`, `#answerAutomatically`,
`AUTOMATIC_RESPONDER` privileged+automatic, `setTimeout(..., autoAnswer.afterMs)`); `openbot-tool-router.ts`
(`#acknowledgeRequest`); `openbot-tools.ts` (`acknowledgeRequestToolSchema`, actions start/alternative/cancel);
`ipc-agents.ts` (`DEFAULT_PLAN_START_AFTER_MS`/`MIN_`, `planStartAfterMs`); `PlanStartRow.tsx` (presets
10/30/60/300 s, custom units); `UiConfirmBlock.tsx` (hold hint, `confirmHold`); `ui-blocks.ts`
(`UiBlockAutoAnswer`); `en/uiBlock.ts` + `en/mobile/chat.ts` ("(nobody answered in time)");
`en/webClient.ts` ("Continue in browser"); `CHANGELOG.md` ("continue in a new provider session").
Ours: `spec/01-requirements.md` REQ-017; `spec/08-ui-ux.md` §8.3/§8.9; ADR-033 above;
`apps/web/components/thread/message-blocks.tsx` (`CardBlock`, `role="status"`, `CONFLICT` guard);
`apps/web/tests/approval-card.spec.ts` (`seedApproval`, existing REQ-007/REQ-017 tests);
`spec/golden/ui/REQ-007-pending-card.json`; `tools/trace_check.py`.
GUESS — the slug→plan-card mapping, the caption wording and key name, catalogue placement.
