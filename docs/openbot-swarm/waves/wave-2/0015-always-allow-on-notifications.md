# 0015 — always-allow-on × notifications (parity proposal, Boma Bot)

- Brief id 15, area `always-allow-on`, lens `notifications`. **PROPOSAL only** — no product code,
  spec, golden, catalogue or traceability row changed.
- Provenance: upstream read as **data** (HEAD `386085ff`) in `/tmp/opencode/openbot-parity/docs/
  architecture/{agents,channels-and-messaging}.md`; our side in REQ-064/065, `crates/domain/src/
  notification.rs`, `crates/app/src/{push,usecases}.rs`, `apps/web/components/thread/notifications-panel.tsx`,
  `apps/web/public/sw.js`, `apps/web/tests/notifications-panel.spec.ts`. CONFIRMED from files read
  this session unless GUESS. No upstream code, asset, wording or brand; no Pro, no second design system.

## 1. Upstream capability, in my own words

Some hosts let a person record a standing "always allow" choice for a class of tool call so later
calls of that class proceed without a card. The notification corollary is what this lens cares about:

- A client notifies when an agent **waits for the user** (question, approval, browser takeover) and
  when a turn finishes or errors. One shared decision path combines a server level (`all` vs
  `needs-me` for wait-events), a per-agent member level, the agent's switch and quiet routines; a
  failed turn does not notify at `needs-me`.
- A request notification is **closed when answered**, even from another device. With previews on its
  text is a short redacted lead of the action; with them off, a fixed generic sentence — a
  deliberately content-light channel.
- "Always allow" is stored with notification/approval preferences; an import from a less-trusted
  source downgrades an on-state to a **warning**, never a silent grant.

Key insight is negative: an always-allowed call never *waits*, so it produces no `needs-me`
notification; completion/error notifications are a separate axis. GUESS: no notification is
manufactured for the mode switch. **Permissive defaults stay out** — no client-side stored consent,
free-text tool keys, preview text in a push or downgrading-by-import. Always-allow is a named,
reversible policy; pushes stay content-free.

## 2. Notification parity, grounded in REQ-064/065 and approval cards

REQ-064: a member subscribes per space/bot; the server may push **content-free** notifications when a
run parks waiting on a person (approval/question card) and when it completes or fails; follows the
per-bot policy and a per-(space, member, bot) daily cap; a dead subscription is deleted after repeated
failures. REQ-065: space-scoped Slack/Teams webhooks receive **the same content-free text** the browser
would, behind the same policy and cap, managed by Owners/Admins only.

CONFIRMED coupling: `create_waiting_card` (`crates/app/src/usecases.rs`) is the *only* caller of
`notify_run_waiting` — card and waiting push are one event. `push_one` (`crates/app/src/push.rs`)
gates on `allows(policy, kind)` (`Waiting`→`notify_waiting`, `Completed`→`notify_completed`), VAPID
and the shared cap; `build_push_payload` emits exactly `{v, kind, title, tag, url}`; `push_destinations`
fans the same title out under the cap; `sw.js` shows only title/tag/url. Parity invariants:

1. **No card ⇒ no waiting push.** An auto-approved write creates no card, so `notify_run_waiting` is
   never reached — the "needs your answer" push (browser *and* destinations) is absent: nothing waits.
2. **Never fake it via the policy.** Enabling always-allow must NOT set `notify_waiting=false`; that
   conflates axes and would silence genuine cards elsewhere. `notifications.policySet` stays independent.
3. **Completion/failure pushes survive.** `notify_run_outcome` is orthogonal; `notify_completed` still
   applies and the cap is shared across channels.
4. **Content-free stays content-free.** The auto-approval reason must not enter title, tag, url or
   destination body — no thread id, tool name or arguments; our constant text is the stricter
   counterpart to upstream's optional preview.
5. **No push as a mode-change signal.** A flip is a UI/i18n state (REQ-017/064), not a fabricated
   notification. GUESS: no push-dismiss RPC exists; "close-on-answer" parity needs a new REQ.
No new externally visible API/data behaviour, so no `DEVIATION` entry.

## 3. Gap and BoardUI surface (one design system, semantic tokens only)

CONFIRMED: `NotificationsPanel` shows a `notifyWaiting` row (`thread.notifications.waiting`) and a
`notifyCompleted` row, but when always-allow suppresses the card a member sees a checked toggle and
*no* push — an unexplained silence that reads as a broken subscription. `spec/08-ui-ux.md` §8.3/§8.9
set the contract: a read-only explanation is the fix, no policy control (GUESS: only
`notifications.policyGet/Set` and `mail.policyGet/Set` exist in `crates/api/src/rpc.rs`). So, beside
the `notifyWaiting` row, render a **read-only, translated note**:
auto-approved calls create no waiting card and send no "needs your answer" push, while completion
pushes continue. Reuse `gap-2`, `rounded-3xl`, `bg-background-secondary-default`, hairline
`border-separator-border`, `cx()` from `@/utils/cx`; composite type `text-body-2-regular` /
`text-caption-1-semibold` in `text-text-secondary` / `text-text-tertiary`; one `Ri*` icon
(e.g. `RiInformationLine`), `aria-hidden`, `text-foreground-icon-secondary`. No raw palette literal
(do not copy the existing `accent-accent-600` into new code), no `dark:`, no Pro. Copy via `t()`:
one key in `en`/`fr`/`he` in `apps/web/lib/i18n.ts` (REQ-057 fallback). Name the note via a
`region`/`note` role; not a live region. Focus `.boma-focus`, target `.boma-target` (44 px),
`prefers-reduced-motion` respected.

## 4. One REQ-named Playwright test

New `apps/web/tests/notification-parity.spec.ts`, using `seedRpc` and the fake-pushManager pattern
from `notifications-panel.spec.ts`. Add **exactly one** test:

```ts
test("REQ-064 an auto-approved run sends no waiting push and keeps completion pushes content-free", async ({ page }) => {
  const policySets: unknown[] = [];
  await seedRpc(page, { answers: {
    "bots/get": { id: "bot_golden", name: "Boma Bot", threadId: "thr_golden" },
    "threads/messages": { messages: [] },          // no card: write call auto-approved
    "runs/list": { runs: [{ id: "run_auto", status: "completed" }] },
    "notifications/status": { enabled: true, vapidPublicKey: "BBfake" },
    "notifications/policyGet": { botId: "bot_golden", notifyWaiting: true, notifyCompleted: true, dailyCap: 20 },
  } });
  await page.route("**/rpc/notifications/policySet", async (route) => {
    policySets.push(route.request().postDataJSON());
    await route.fulfill({ status: 200, contentType: "application/json", body: "{}" });
  });
  // fake serviceWorker/Notification init script — as in notifications-panel.spec.ts
  await page.goto("/app/bot_golden/");
  const note = page.getByRole("note", { name: "Auto-approved runs and notifications" });
  await expect(note).toContainText("no waiting notification"); await expect(note).toContainText("completion");
  await expect(note).not.toContainText("thr_golden");         // no thread/tool content
  await expect(page.getByLabel(/Waiting on your answer/)).toBeChecked(); // policy untouched
  expect(policySets).toEqual([]);                             // never silence notifyWaiting
});
```

It pins the silence, no policy mutation, completion enabled and no content leak; the name carries
`REQ-064` so `tools/trace_check.py` counts it (a TRACEABILITY row follows only after the test exists).

## 5. First slice

1. Write the failing REQ-064 test; add the single note key to `en`/`fr`/`he`.
2. In `NotificationsPanel` only, render the read-only named note; leave checkboxes, cap, destination
   form and RPC calls untouched.
3. **Do not** add an always-allow control, policy write RPC, `notify_waiting=false` side effect or
   push dismiss/close — those change permission semantics and need a new REQ id, an ADR and owner
   approval (`AGENTS.md`). Record them in `docs/STATE.md` as a later slice.
4. Gates: web typecheck, lint, build, this test plus notifications-panel/golden/axe clusters
   (light, dark, RTL, 360 px); update `docs/STATE.md` and `docs/UI.md`; no dependency change.

## Guardrails

No upstream code, asset, wording, brand or preview text reproduced; inspiration only. No Pro; one
design system, BoardUI free/installed components only. CONFIRMED only after a command actually ran;
this document ran no build or test.
