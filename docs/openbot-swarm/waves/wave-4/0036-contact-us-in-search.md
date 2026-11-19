# 0036 — contact-us-in × search parity proposal (Boma Bot)

- Brief: `id 36`, area `contact-us-in`, lens `search` (`docs/openbot-swarm/briefs-1000.jsonl`).
- Status: **PROPOSAL only** — no product code, spec, golden, catalogue or dependency changed.
- Provenance: upstream read this session as **data only** (`/tmp/opencode/openbot-parity`, HEAD `559d5ac`):
  `ContactPage.tsx`, `v1/contact.ts`, `server/contact-message.ts`, `server/content-feed.ts` (sitemap),
  `contracts/src/feedback.ts`, `global-search-sources.ts`, i18n strings, `ANALYTICS.md`, `CHANGELOG.md`.
  No upstream code, asset, wording, brand or Pro enters Boma Bot. CONFIRMED = read this session; else GUESS.

## 1. Upstream capability, in my own words (inspiration only)

- **A public "talk to the team" page** — a short form (name, e-mail, company, a topic menu, a hard-capped
  message) validating before any request; same-origin submit, rate-limited per source IP and globally per
  15-minute window, delivered by mail. Success swaps the form for a status notice with a send-another
  action; failure/rate-limit show one inline sentence with a `mailto:` fallback; analytics record topic and
  outcome, never the message. Linked from header, footer, FAQ, Teams and Pricing in three languages — and
  **absent from the sitemap**: the site has no search index, so the channel is found by links, not search.
- **An in-app report path that searches before it files.** Before a report goes out, the client asks the
  server to find **similar existing reports**: the title is reduced to keywords (lower-cased, split on
  non-alphanumerics, short words and stop words dropped, the few longest kept, de-duplicated) and the public
  tracker is searched. Sign-in required, rate-limited, failures degrade silently to "none found". A match can
  be marked as the same report instead of filed twice; the send itself is a previewed, approval-gated
  external effect; the sent report lands in the timeline as a card.
- **A global palette whose action group lists destinations — and lists no contact entry**; in-app help
  lives on the rail Help menu. Transferable rule: a contact channel must be findable from the search
  surface the product already has, and filing should search what exists first.

## 2. Where Boma Bot stands (CONFIRMED unless marked)

- No contact/support screen in spec/08 §8.1 (unlisted means absent), no contact RPC/field; capabilities
  advertise auth/billing/`localFirst` only (spec/openapi.yaml:81-106).
- The only **shipped, wired** search is `/app/artifacts`: `artifacts.search` narrows the grid client-side
  over `artifacts.listSpace` (spec/08 §8.5, REQ-036; `artifacts-grid.tsx`; `apps/web/lib/api/rpc.ts:1045`;
  `artifacts.spec.ts` already pins narrowing).
- Two other "search" filters are dormant template material — `dashboard-sidebar.tsx` (⌘/Ctrl-L over local
  labels; "Support" row is a dead `href="#"`) and `settings-storage.tsx` ("Search files" over mocked rows);
  nothing in the shipped shell imports either (grep clean).
- spec/03 §3.1 (line 29) declares `search` among the 52 namespaces but nothing is wired: no
  `crates/api/src/rpc.rs` procedure, `search.*` wrapper in `apps/web/lib/api/rpc.ts`, or `spec/golden/search*.json`;
  `Message.searchTokens` (spec/02:42) is unreferenced. Inventing `search.*` is out of scope (GUESS).
- The **artifacts library is the only RPC-backed corpus searchable in-app** — name, type, bot, size, updated
  time, version chain (REQ-036 MUST); the honest analogue of upstream's similar-report search.

## 3. Search-parity change (read-only first)

1. **Search first, then contact.** When the artifacts search matches nothing, replace the bare
   `artifacts.empty` tertiary line with a named region stating the scope (this space) and — only when the
   deployment configures a support target — one action linking to it ("search before you contact" mapped
   onto the one search surface that exists; it files, sends and gates nothing). Fires only on zero matches,
   unlike sibling 0016's always-on disclosure.
2. **Unconfigured means absent.** With no operator-configured target (GUESS: a deployment config, e.g.
   `supportUrl` surfaced via capabilities), the region adds nothing — never a dead affordance.
3. **No new wire surface.** No `search.*` procedure, message search, similar-report backend,
   autocomplete/indexing, change to `artifacts.listSpace`/`listVersions`, or golden. A similarity endpoint
   or `searchTokens` use is a new REQ + ADR + PARITY DEVIATION + owner approval — logged in `docs/STATE.md`,
   built nowhere here.
4. **§8.9 honesty.** "Search" remains listed as a keyboard shortcut but is unimplemented (the only ⌘-filter
   is dormant template code) and the `?` overlay is absent. Out of scope; noted in `docs/STATE.md`. Wiring
   Support into the real shell is sibling 0029/0035 territory with its own REQ — do not fold it in.

## 4. BoardUI surface (one design system, semantic tokens)

In `artifacts-grid.tsx`'s `filtered.length === 0` branch, render a `role="region"` block: `gap-2`,
`rounded-md`, `border-separator-border`, on `bg-background-secondary-default`; copy in `text-body-regular`
/ `text-caption-1-semibold` over `text-text-secondary` / `text-text-tertiary`; one `@remixicon/react`
glyph passed by reference (`RiCustomerServiceLine`, the sidebar's support glyph), `aria-hidden`,
`text-foreground-icon-secondary`; the action is a catalogue-labelled link with `.boma-target` (44 px) and
`focus-visible:ring-2 focus-visible:ring-border-focus-ring`; merged with `cx()` from `@/utils/cx`. Strings
via `t()` with English fallback (`artifacts.empty.support*`), 14 locales incl. RTL (REQ-057). No raw
palette/hex, no `dark:` literals, no Pro, no second design system. The target is a configured URL, never
rendered inline; RTL mirrors with `ms`/`me`, ids/URLs stay LTR in `<bdi>`.

## 5. One REQ-named Playwright test (`apps/web/tests/artifacts.spec.ts`; reuse `seedGrid`)

```ts
test("REQ-036 the artifacts empty result offers the support link only when the deployment configures one", async ({ page }) => {
  await seedGrid(page);
  await page.goto("/app/artifacts/");
  const search = page.getByRole("searchbox", { name: t("en", "artifacts.search") });
  await search.fill("zzz-no-match");
  await expect(page.getByRole("link", { name: t("en", "artifacts.empty.support") })).toHaveCount(0);
  // A later route shadows the earlier one.
  await page.route("**/api/auth/capabilities", (route) => route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify({ ...CAPABILITIES, supportUrl: "https://support.example.test/contact" }) }));
  await page.reload();
  await search.fill("zzz-no-match");
  const help = page.getByRole("link", { name: t("en", "artifacts.empty.support") });
  await expect(help).toBeVisible();
  await expect(help).toHaveAttribute("href", "https://support.example.test/contact");
});
```

Pins the contract: zero matches show nothing extra unconfigured; a configured target yields exactly one
named, keyboard-reachable link; search still narrows. No golden needed (state, not click path).

## 6. First slice (test-first, small)

1. Write the failing REQ-036 test above; record the red.
2. Add `artifacts.empty.support*` keys to `en`/`fr`/`he` plus English fallback in the other eleven (REQ-057).
3. Render the region only in the zero-match branch; leave grid, filters, viewer and RPCs untouched.
4. `docs/STATE.md`: log config shape (GUESS), Support rail entry + §8.9 shortcut, `search` namespace and
   `searchTokens` intent, similar-report analogue — proposals only, each with REQ/ADR/DEVIATION need.
   `docs/PARITY.md` untouched (no wire change); `docs/UI.md` gains an entry if the component changes.
5. Gates: web typecheck/lint/build; Playwright artifacts + a11y + locale clusters, light/dark/RTL, 360 px;
   goldens replayed unchanged; axe zero critical/serious.

## 7. Non-goals / evidence

No upstream code, asset, wording, brand or Pro; no phone-home, no silent send, no support transport; no
new REQ/field/migration; REQ-017 untouched; no approval semantics. Upstream is an oracle, not a source.

CONFIRMED — spec/08 §8.1/§8.5/§8.9; spec/01 REQ-036/REQ-057; spec/02:42; spec/03 §3.1 (line 29);
spec/openapi.yaml:81-106; `artifacts-grid.tsx` empty-state, `artifacts.spec.ts` (`seedGrid`), dormant
`dashboard-sidebar.tsx`/`settings-storage.tsx` (unimported), `crates/api/src/rpc.rs` and
`apps/web/lib/api/rpc.ts` (no `search`), no `spec/golden/search*.json`; upstream HEAD `559d5ac`: contact
page + `/v1/contact` + per-IP/global rate limits, sitemap omits `/contact` (`content-feed.ts`),
similar-report search (`feedback.ts`), palette without a contact action (`global-search-sources.ts`),
Help-rail destinations (`RailHelpMenu.tsx`). No build or test was run for this document.
GUESS — the support-target config shape, whether Support becomes a rail/nav entry, copy tone, and any
similar-report analogue without a public tracker (owner decisions).
