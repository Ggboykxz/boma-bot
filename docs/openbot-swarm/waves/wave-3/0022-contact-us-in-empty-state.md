# 0022 — "contact us in" through an empty-state lens (parity proposal)

- Brief: `id 22`, area `contact-us-in`, lens `empty-state` (`docs/openbot-swarm/briefs-1000.jsonl`).
- Provenance: upstream (HEAD `386085ff`, 0.73.0) read as **data only** in `CHANGELOG.md:1625-1629`, `app/src/routes/_authed/onboarding.tsx:152-215`, `app/src/components/onboarding/booking-calendar.tsx:60-160`, `app/src/components/layout/self-host-banner.tsx`, `app/src/lib/openbot-for-business.ts`. Product code stayed read-only; no upstream code, asset, wording, brand, provider name or URL enters Boma Bot; no BoardUI Pro component or second design system. Claims are CONFIRMED unless marked GUESS. Language: English (disk convention).

## 1. Upstream capability, in my own words (inspiration only)

The product gives a solo, self-hosting user several ways to *initiate contact from inside the app*: a "Book a call" link on its self-host banner and "Talk to us" / "Book a demo" links in onboarding and bot setup, all resolving to one business destination. One onboarding step goes further: it embeds a booking calendar in place of the illustration, so a call can be booked without leaving the flow. That embed is a real control with a deliberate state model: **before contact** an explanatory intro, picture and primary demo action, with "Not now" the only way forward (a "Continue" would read as finishing the booking); **loading** — a spinner, and "ready" only when the provider posts a message from its own origin (so an error page never masquerades as loaded), with height following the provider's reported height rather than scrolling inside the step; **ready** — the secondary action becomes "Continue" once a booking is reported; **failed** — offline, an iframe error or a timeout restores the picture and a plain fallback button that opens the destination in a new tab beside a `role="status"` sentence saying the calendar could not load and the button opens it in the browser. Only solo/self-host users see any of it; organisation members do not. The lesson for this lens is discipline: a contact affordance is a *named, explained state*, never a blank panel, a spinner that outlives its work, or a dead link.

## 2. The parity target in Boma Bot

Boma Bot has no vendor, sales desk or demo booking — adding one is a new feature and a DEVIATION (AGENTS.md), and no upstream copy or URL is ours to reuse. The in-spec analogue of "contact us in-app" is the surface where a person is **reached through a channel**: the notification and messaging destination panel (`apps/web/components/thread/notifications-panel.tsx`), inside `/app/:botId` (no settings page — §8.1). REQ-065 (notification destinations), REQ-064 (Web Push) and REQ-048 (messaging platforms) own it. Its empty state today is one `text-text-tertiary text-xs` line, "No destinations yet." (i18n key `thread.notifications.destinations.empty`), with no accessible name on the region, no explanation, no next action, and an immediate empty render while `destinationsList` is still in flight. This lens closes that gap: give the contact-channel empty state the same named, explained, latency-tolerant contract as the upstream contact embed — without its business meaning or a single upstream string.

## 3. Empty-state implications grounded in our own sources

- `spec/08-ui-ux.md` intro: "State is never ambiguous", "a blank panel is a bug", "the interface must survive latency. Skeletons and optimistic states everywhere", "no hardcoded strings". An empty destination list must be a named region with a status line the user can say out loud, not a bare tertiary paragraph, and must hold a state-shaped skeleton while the RPC is pending.
- §8.9 "Announcements": state changes go through a polite live region, not the transcript; "Error copy": one sentence of what happened, one of what to do, with ids and URLs off screen. The failed-fetch fallback follows that pattern, never a vendor URL. §8.10: a screen state pins a REQ-named golden, not a click path.
- `docs/UI.md` Foundations 1–10 and token map: semantic tokens only (`bg-background-full`, `bg-background-secondary-default`, `border-separator-border`, `text-text-primary`, `text-text-secondary`, `text-text-tertiary`, `text-foreground-icon-primary`), composite type (`text-title-2-medium`, `text-body-medium`), `cx()`, 44 px `.boma-target`, token focus ring.
- `docs/UI.md` "Copy conventions" / i18n and RTL: every string via `t(locale, key)` in `lib/i18n.ts`; a missing key falls back to English, never to blank; 14 locales ship (REQ-057), `ar`/`he` RTL.

### Real RPCs we build on (no new endpoints)

- `notifications/destinationsList` → `{ destinations: [] }` — the empty case we name; `notifications/destinationsCreate` / `destinationsDelete` — the next action and its reversal; `notifications/status`, `notifications/policyGet`, `notifications/policySet` — surrounding panel states (loading/ready/failed already modelled in `notifications-panel.tsx`).
- `messaging/list`, `messaging/installUrl`, `messaging/conversations` (REQ-048) — the in-spec home of contact-through-a-channel; a future outbound affordance writes here, never a bespoke endpoint (GUESS: confirm the intended surface with the owner before adding any contact UI).

## 4. Proposed surface (BoardUI, one design system)

Wrap the destination block in a named `region` ("Contact destinations") with a `text-title-2-medium` / `text-text-primary` heading, a `text-body-medium` / `text-text-secondary` sentence saying no channel is connected yet and what connecting one does, and one `.boma-target` action ("Add a destination") that focuses the existing form. Use `gap-*`, `rounded-xl`, `border-separator-border`, surface `bg-background-secondary-default` on page `bg-background-full`; a small `@remixicon/react` glyph (component reference, `text-foreground-icon-primary`) backs the text — state is never icon- or colour-only. While `destinationsList` is pending show a state-shaped skeleton (installed `skeleton` / `composer-loader` pattern), not a blank region; on failure show the §8.9 one-sentence + one-action fallback in `text-text-error-primary`, not a dead link. All copy is new i18n keys.

## 5. Accessibility, RTL, motion

- **a11y** — WCAG 2.2 AA, zero critical/serious axe in light, dark and RTL. Name the region so a screen reader hears *why* it is empty; keep the explanation static and route load/failure changes through the polite live region (§8.9), never make the transcript one. Keyboard reaches the action and the existing form; focus is the token ring, targets ≥44 px.
- **RTL** — locale drives `DirectionProvider`; logical styles (`ms`/`me`, `ps`/`pe`, `start`/`end`, `text-start`/`text-end`); mirror the action chevron; keep URLs, codes and ids LTR in `<bdi>` / `dir="auto"`.
- **motion** — 120–180 ms token cross-fade for empty↔populated, `prefers-reduced-motion` respected, no animation on streamed values and no spinner that outlives its RPC.

## 6. One REQ-named Playwright test

In `apps/web/tests/notifications-panel.spec.ts`, add exactly one test, following that file's `page.route` stubs and `fixtures.ts` patterns:

```ts
test("REQ-065 no contact channels shows a named, explained empty state with a skeleton under latency", async ({ page }) => {
  await page.route("**/rpc/notifications/status", r => r.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify({ enabled: true, vapidPublicKey: "BBP_test..." }) }));
  await page.route("**/rpc/notifications/policyGet", r => r.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify({ botId: "bot_golden", notifyWaiting: true, notifyCompleted: false, dailyCap: 20 }) }));
  await page.route("**/rpc/notifications/destinationsList", async r => {
    await new Promise(res => setTimeout(res, 300)); // skeleton visible here
    await r.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify({ destinations: [] }) });
  });
  await page.goto("/app/bot_golden");
  // enable push via the existing stubbed subscribe path, then:
  const region = page.getByRole("region", { name: "Contact destinations" });
  await expect(region).toBeVisible();
  await expect(region).toContainText("No destinations yet");
  await expect(region).toContainText("No channel is connected");
  await expect(region.getByRole("button", { name: "Remove" })).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Add a destination" })).toBeEnabled();
});
```

It pins the accessible name, the explanation, the absence of removal controls and the next action — the contract above. The 300 ms window is where the skeleton assertion belongs once it lands; it stays in this one test.

## 7. First slice

One vertical, reversible slice: turn the existing "No destinations yet." paragraph into a **named, explained, tokened empty region** on the notification/destination panel, add the state-shaped loading skeleton and the §8.9 failure fallback, ship the new i18n keys for all 14 locales (English source, fallback elsewhere), the REQ-065 test above and a matching §8.10 golden. No new RPC, no contact feature, no policy change and no vendor URL. Gate on `npm run lint`, the axe gate (light, dark, RTL) and the REQ-065 test.

## 8. Boundaries

- Inspiration only: no upstream code, brand, wording, icon, colour, provider name or URL; no BoardUI Pro item, no second design system, no hand-edited vendored source.
- No new externally visible API/data behaviour, so no DEVIATION entry — a named UI state inside REQ-065/064/048. GUESS (AGENTS.md): whether a true outbound "contact us" affordance is wanted at all; record it in `docs/STATE.md` and seek owner approval before any new RPC or surface.
- This file is the only artefact written; product code and the upstream clone stayed read-only.
