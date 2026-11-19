# Parity proposal — contact-us-in, performance lens

Worker 026 · brief `id: 26` (`area: contact-us-in`, `lens: performance`) · target milestone M10
(`docs/PLAN.md` §5, UI complete). Research only: the upstream clone at `/tmp/opencode/openbot-parity`
was **absent** at read time (CONFIRMED: `ls` returned "No such file or directory"), so the capability
below is rebuilt from the area slug plus `spec/` and `docs/PLAN.md` — no upstream code, asset, wording
or brand is reproduced, and no Pro component is assumed. Only this document is written.

## 1. The capability, in our own words

A "Contact us" surface is the small, inbound support door inside an assistant product: a person who
hits a wall — a failed sign-in, a confusing approval, a desktop that will not start — needs a visible,
trusted way to reach the maintainers without leaving the app or hunting for a URL. In its least
ambitious, most robust form it is one static help entry that opens a prefilled message or a hosted
support page; it carries no personal data, needs no account, and never becomes a place where secrets,
logs or conversation content are pasted. Crucially it is a *durable, low-value-bandwidth* page: almost
nobody opens it, but when someone does it must appear instantly on a cheap phone over a weak link and
must not be the thing that pushes the initial bundle past its budget.

(GUESS: the spec has no contact-us requirement — `grep -rin 'contact|support|feedback'` over `spec/`
finds only a plugin submitter's contact field and a support correlation id. The slug `contact-us-in`
is read as "Contact us in …", one of ~50 upstream help/how-to topics. Per `AGENTS.md`, silence is
recorded here and the simplest safe default — a static, localised, link-only page — is picked; the
owner should confirm or veto before any code.)

For this lens the page is a *pure delivery problem*: it is all bytes-and-layout, so the whole parity
answer is "serve it as static HTML and add nothing to the interactive runtime."

## 2. Parity changes for low-end hardware and poor networks

Grounded in `docs/PLAN.md` §4/§9 budgets: JS initial **≤ 200 KB gzip**, LCP **≤ 2.5 s**, INP
**≤ 200 ms**, CLS **≤ 0.1** on throttled 4G, read p95 **≤ 150 ms**, cold start **≤ 2 s**, RSS idle
**≤ 150 MB**; and `spec/04-nonfunctional.md` §4.3/§4.8 (bounded work, honest degraded state).

1. **Server-rendered static page in the existing export — zero client JS.** Build it as one route in
   `apps/web` with `output: "export"` (ADR-013), emitted as a plain HTML file and served by the Rust
   binary alongside the other static assets. No `"use client"` boundary, no React hydrate, no
   TanStack Query store, no SSE subscription. This is the single most important low-end choice: the
   support door is content, not an app; it adds **0 KB** to the interactive chunk set.
2. **Add no chunk and no dependency.** Current export already ships ~369 KB gzip across 36 chunks;
   the page must not increase that number. Its CTA is a plain `mailto:`/hosted-link `<a>` (no button
   widget with a client runtime), so there is no analytics script, no third-party helpdesk embed, no
   chat bubble, no icon font — each of which would add bytes and a network hop on a weak link.
3. **First-frame friendly on 4G.** Inline the critical CSS for the single screen, use only the
   already-shipped system font stack, and avoid any hero image. The page is a few KB of HTML, so its
   LCP element is text already in the first response — no render-blocking fetch, no layout shift
   (CLS ≈ 0), nothing for INP to measure because there is no interaction script. Read path stays a
   static-file serve inside the 150 ms p95 and never touches Postgres or a provider.
4. **Localised without a client catalogue load.** Route the strings through the shared ICU
   catalogue (`REQ-057`, ADR-014/015) and pre-render each shipped locale (`en`, `fr`, `he`) at build
   time, so the RTL proof exists without any runtime translation bundle. Keep the address/URL in an
   LTR `<bdi>` region; set `dir`/`lang` on the document per locale. A human on a poor link sees their
   language in the first paint, not after hydration.
5. **Honest and inert when offline/degraded.** The page makes zero outbound calls, so it cannot spin,
   show a stale success, or leak a conversation. If the `mailto:`/link target is unreachable, the
   browser handles it natively; we never fake a "message sent" state. No cached authenticated
   response is reused (§4.8), and no telemetry is attached to the click.

## 3. BoardUI tokens (free tier only, no Pro)

Style with the existing semantic tokens only — no raw palette classes or hex: surfaces
`bg-background-full` / `bg-background-primary-default`, text `text-text-primary` /
`text-text-secondary`, hairline `border-separator-border`; composite type utilities
`text-title-2-medium` and `text-body-medium` (never stacked `text-sm font-medium`); merge with
`cx()` from `@/utils/cx`. No Pro component: this is a static, unauthenticated page — one link and a
short paragraph, not a dashboard block. Keep 44 px touch targets, visible focus ring, logical/RTL-safe
spacing, and inherit the app's reduced-motion handling (there is no animation to add).

## 4. One REQ-named gate

`REQ-057_contact_us_static_page_stays_within_js_and_lcp_budget`

Playwright budget check (name carries the id per `docs/PLAN.md` §7 / `tools/trace_check.py`): on
throttled 4G at 360 px, load `/contact` in `en` and `he`; assert initial JS chunk count and gzip total
are unchanged and ≤ 200 KB, LCP ≤ 2.5 s, INP ≤ 200 ms, CLS ≤ 0.1, `dir=rtl` on the `he` route, and no
outbound request beyond the document and its inlined CSS. This is also the i18n/RTL proof for the page,
so one gate covers both the budget and the locale requirement.

## 5. First slice

- `web`: one static `app/contact/page.tsx` (server component, no `"use client"`) with a heading,
  one short paragraph and one LTR `mailto:`/hosted link; strings from the ICU catalogue, pre-rendered
  for `en`/`fr`/`he`; no new dependency, no new JS chunk.
- `web`: tokens-only styling per §3; 360 px and RTL smoke check inline with the budget test.
- Gate: add the REQ-named Playwright budget check above; keep the existing 200 KB gzip budget check
  green with an unchanged chunk count; `axe` zero critical/serious on the page.
- Exit: budget test green, JS budget gate green, no new bundle chunk, no network call beyond the
  document, no upstream wording or brand — user-visible copy uses the Boma Bot name only.

Explicitly out of this slice: any helpdesk/chat embed, analytics, a submission form or backend write,
a searchable help centre, a Pro component, and any upstream code, asset, wording or brand. Opening a
real support destination and whether this surface belongs in-product at all are owner decisions to
record in `docs/STATE.md` (GUESS above) before implementation.
