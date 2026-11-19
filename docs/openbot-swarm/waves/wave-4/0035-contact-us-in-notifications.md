# 0035 — contact-us-in × notifications parity proposal (Boma Bot)
- Brief: `id 35`, area `contact-us-in`, lens `notifications` (`docs/openbot-swarm/briefs-1000.jsonl`).
- Status: **PROPOSAL only** — no product code, spec, golden, catalogue or dependency changed.
- Provenance: upstream read this session as **data only** (clone `/tmp/opencode/openbot-parity`, HEAD
  `559d5ac`): `team-client/src/agent-notifications.ts`, `contracts/src/feedback.ts`, the
  report-a-problem tool in `openbot-tool-router.ts`, `SettingsNotificationsTab.tsx`,
  `i18n/src/messages/en/{notification,status/agent}.ts`. No upstream code, asset, wording, brand or
  Pro enters Boma Bot. CONFIRMED = read this session; else GUESS.

## 1. Upstream capability, in my own words

The upstream app has a help-rail door for people who want to report a problem or send an idea from
**inside** the product instead of via the public website. A person picks "report a problem" / "send
feedback"; the assistant collects version/system facts and assembles a report; sending is **not** a side
effect the assistant may take alone — the run parks on an in-chat **approval card** telling the person
in plain words what becomes public (an issue on the project tracker, with app version and system info,
name not shown) and waits for their yes/no. The notification system is what reaches a person who has
walked away at that moment: the event parks as "needs input / needs approval", a desktop notification
names the agent and the wait (never the transcript), levels are per-agent (nothing / needs-me / all,
mute with a duration), there is a test-notification action, and an opt-in "show message preview" toggle
can attach a short redacted excerpt (default off; GUESS — the default was not verified). Clicking a
notification opens the thread holding the card. The through-line for this lens: **a contact-originated
report is an external effect, so the push that stalks the approval card is as content-free as the card
is gated.**

## 2. Parity position for Boma Bot

- **Already ours (CONFIRMED):** REQ-064/065 (both MUST) are delivered end to end — migration
  `0056_notifications.sql` (`notification_subscriptions`/`_policies`/`_log`, tenant RLS) and
  `0057/0058` for destinations (`docs/STATE.md`, ADR-027/ADR-028). `crates/app/src/push.rs` builds
  exactly `{v, kind, title, tag, url}`, the `title` a fixed three-string catalogue keyed on
  `users.locale`; `req_064_payload_is_content_free` (`crates/testkit/src/build.rs`) pins the key
  set. The waiting push fires inside the `if let Some(card)` block of `create_waiting_card`
  (`usecases.rs`) — once per card, replay absorbed, initiator-only, capped per (space, member, bot)
  UTC-day, never fail-able into the run; completed/failed ping from `worker.rs`'s terminal sites.
  `/sw.js` navigates to same-origin `/app/{botId}/` on click. `NotificationsPanel`
  (`notifications-panel.tsx`) renders the per-bot subscribe toggle, policy (`notifyWaiting`/
  `notifyCompleted`, shared cap, destination flags) and Owner/Admin destination CRUD;
  `notifications-panel.spec.ts` round-trips REQ-064/065 against stubbed RPCs. The approval card is
  `CardBlock` (`message-blocks.tsx`, REQ-007/017), the REQ-017 gate any contact-relay external
  effect must park behind (sibling 0029).
- **Gap this lens exposes (CONFIRMED):** (a) the push `url` is bot-wide (`/app/{botId}/`) — it
  cannot deep-link the thread/card that parks, so the click-through contract for a **waiting
  decision about a contact relay** is unproven anywhere; (b) no test ties REQ-064's content-free
  contract to the contact/support-relay path — `notifications-panel.spec.ts` pins the panel
  round-trip, not the relay parking flow; (c) the panel gives no member-facing sentence that
  contact-relay parks are covered by the same content-free, cap-gated push contract, so the ping is
  undifferentiated noise to someone waiting on a support effect; (d) the sidebar **Support**
  secondary entry (`dashboard-sidebar.tsx:533`) is still a stub with no `onClick` — the relay surface
  the notification would carry is undefined, and wiring it is sibling 0029's scope, not this one.
- **Out of scope / proposal only:** an upstream-style redacted "show preview" toggle or per-event
  bodies carrying the question/command — that is a **DEVIATION** from REQ-064's content-free
  mandate; rejected, not proposed. A thread-deep-link `url` in the payload, per-agent nothing/
  needs-me/all levels, timed mutes and a "test notification" action — each needs a new REQ/ADR and
  owner approval; recorded in `docs/STATE.md` as proposals, implemented in none. Wiring the Support
  entry and the contact-relay effect itself — sibling 0029's slice; no new transport here.
- **In scope now:** one Playwright test pinning the client half of the content-free contract on the
  contact-relay waiting path, plus catalogue phrasing on the existing panel sentence if missing.
  No wire, data-format, permission or policy change ⇒ **no DEVIATION**.

## 3. The parity invariants (REQ-064, REQ-065, REQ-017)

A parked relay: the card mints once; the push goes only to the initiator via `notify_waiting`; payload
keys are exactly `{v, kind, title, tag, url}` (content-free, by construction); destinations ship the
same three-string title, never tool names or message text; the member's per-bot `notifyWaiting` policy
and the shared (space, member, bot) cap decide delivery; 404/410/401/403 ×3 deletes a dead subscription;
a failed or capped send leaves the run unaffected. The push is a **pointer**, never a **payload**: the
card is the only place a member authorises the relay, exactly-once across retries (`threads/answer` +
`CONFLICT` guard). Click-through lands on the bot's thread holding the pending card — that is the whole
contract; more would leak.

BoardUI only: semantic tokens (`bg-background-full`, `border-separator-border`, `text-text-*`, `text-foreground-icon-*`), composite type utilities, `cx()`, 44 px `.boma-target`, token focus ring, icon +
word never colour alone, dark mode rides `.dark` tokens, `prefers-reduced-motion` kept. No raw palette,
no `dark:` literals, no Pro, no second design system, no `REQ-057` string outside the catalogue (14
locales incl. en/fr/he; RTL mirrors; codes/URLs stay LTR in `<bdi>`).

## 4. One REQ-named Playwright test

In `apps/web/tests/notifications-panel.spec.ts`, following that file's `page.route` stubs and the
`addInitScript` fake service worker / push manager pattern, add exactly one test:

```ts
test("REQ-064 a contact-relay waiting run pings the member with no conversation content and the push lands on the bot thread", async ({ page }) => {
  // GIVEN the seeded pending contact/support-relay approval card (seedApproval shape) with
  //      notifications/status enabled; WHEN the member enables push in the panel;
  // THEN subscribe carries exactly {endpoint, p256dh, auth, userAgent} and none of the card
  //      title, thread text or tool arguments; the pending card stays the response surface;
  //      /sw.js's notificationclick contract (golden-pinned) points at /app/bot_golden/.
});
```

It pins what the browser can prove: the subscription carries zero conversation content and the waiting
decision surface is the thread holding the card. The payload key set stays pinned server-side by
`req_064_payload_is_content_free`; do not re-pin it in the browser.

## 5. First slice (test-first, small)

1. Write the failing REQ-064 test above (seed the relay card; stub `status`/`subscribe`/`policyGet`;
   capture the subscribe body).
2. If it exposes client-side leakage or a missing member-facing sentence on the panel ("waiting
   decisions, including support relays, stay content-free and capped"), fix it in
   `notifications-panel.tsx` using existing tokens, `Button`, icons by reference and new
   `thread.notifications.*` keys in en/fr/he plus English fallback in the other eleven (identical
   key sets across `SUPPORTED_LOCALES`).
3. Keep the REQ-065 panel round-trip test and the approval-card REQ-007/017 tests green; touch no
   wire, data format, policy or golden.
4. Record the payload deep-link `url`, the per-agent level/mute matrix, the test-notification
   action and the preview toggle (rejected) in `docs/STATE.md` as proposals/owner notes.
5. Gates: web typecheck, lint, build, Playwright notifications-panel/approval-card/shell/golden
   clusters, 360 px and dark mode, axe light/dark/RTL. Mark CONFIRMED/GUESS in `docs/STATE.md`;
   update `docs/UI.md` only if the component changes; `docs/DEPENDENCIES.md` untouched.

## Guardrails

No upstream code, asset, wording, brand or Pro. Content-free pushes are normative (REQ-064): any preview
would be a DEVIATION, escalated to the owner, never shipped quietly. No new REQ, wire field or
permission change without the owner. Every claim marked CONFIRMED only after a command actually ran; the
upstream clone stayed read-only; this file is the only artefact written.
