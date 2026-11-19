# 0075 — SharedPageScreen (public share read view): architecture (v0.4.0)

Worker #075 · module `apps/web/components/shared-page/shared-page-screen.tsx` · lens: architecture · READ-ONLY
Proposal only; no source changed. Read at `rebuild-workspace/.worktrees/before-m5` (v0.3.0-era tree).
Scope: REQ-037 (spec/01-requirements.md §305), spec/08-ui-ux.md §8.8, both
`spec/golden/ui/REQ-037-shared-page{,-invalid}.json`, `spec/openapi.yaml` §`/api/v1/pages/shared/{token}`,
ADR-013 static shell (`lib/loc.ts`).

## What the file is (CONFIRMED by file read)

- 220 lines, `"use client"`. One exported screen, one local `Block` renderer, three guards
  (`safeLink`, `safeImage`, `clampText`), and `pathToken()` at the bottom.
- Token read via `useSyncExternalStore(locSubscribe, () => pathToken(), () => null)`; the API is
  fetched anonymously (`credentials: "omit"`) with an `AbortController`; every non-2xx and every
  rejection fold into the same `unavailable` document. `document.title = body.title` on success.
- `Phase = "loading" | "unavailable" | "ready"`. Block union: `text|table|chart|link|image`, with
  caps `TEXT_CAP=4096`, `COLUMNS_CAP=16`, `ROWS_CAP=64`, `POINTS_CAP=64`; link/image schemes are
  regex-allowlisted; content is always text/SVG, never markup.
- Loading skeleton and unavailable screen are both `<main className="bg-background-full min-h-dvh …">`
  on semantic tokens only. No nav, button, or form — matching both UI goldens.

## Architecture findings (CONFIRMED unless marked GUESS)

1. `phase === "ready"` is never tested: the final branch renders for any non-loading/non-unavailable
   phase, using `page?.title` / `page?.blocks`. A 200 response whose body lacks `blocks` throws inside
   render; a null page yields an empty `<article>` — two distinct states hidden behind one branch.
2. A token change never resets `phase`/`page`: content and title from share A stay visible while B
   loads. GUESS: today reachable only on full document change, because `locSubscribe` is a no-op.
3. `lib/loc.ts:10` defines `locSubscribe = () => () => {}` — no `popstate`/router subscription;
   `getSnapshot` is a fresh inline arrow each render; `pathToken()` duplicates `pathSegment("/p/")`.
   Client-side navigation to another `/p/<token>` would not update the view at all.
4. No runtime parse at the boundary: `await response.json() as SharedPage` is an unchecked cast, and
   OpenAPI's `updatedAt` is absent from the TS type. Safety is spread across three helpers plus caps
   inside `Block`, so a new block type is not forced through validation.
5. Hand-rolled fetch, no timeout, no dedup/retry, no cache. ADR-014 names TanStack Query over the
   generated client; sibling reviews found no `useQuery` in the tree. This screen is an island.
6. `document.title` is mutated inside the promise callback with no cleanup, so the tab title leaks
   into whatever screen follows an unmount/route change.
7. Chart `viewBox` width is `points.length * 48 + 8`, coupling geometry to an untrusted count
   (64 points → ~3k px squeezed into `max-w-md`); `<th key={column}>` collides on duplicate columns.
8. Spec §8.8 permits "a small footer"; the screen renders none (CONFIRMED on both sides).

## Concrete upgrades for v0.4.0 (in dependency order)

**U1 — Make the state machine honest.** Key the state by token and reset to `loading` when it changes;
render the article only when `phase === "ready" && page`, with a `ReadyState` whose `page` is non-null.
- *Risk:* an extra skeleton flash on token change and a different failure shape for malformed 200s.
  *Mitigation:* reuse the existing skeleton markup unchanged; replay both REQ-037 UI goldens and keep the
  uniform "not available" wording byte-identical.

**U2 — Add a typed boundary parser.** `parseSharedPage(value: unknown): SharedPage` validates `title`,
`blocks`, each block's discriminant and fields, and the caps; fold `safeLink`/`safeImage`/`clampText`
into it so `Block` becomes dumb and exhaustive. Accept unknown keys (or model `updatedAt`) so a valid
backend payload is never rejected.
- *Risk:* an over-strict parser silently converts a valid share into the unavailable screen — visually
  indistinguishable, so easy to miss. *Mitigation:* replay `shared-page.json` plus the contract test
  against `openapi.yaml`; never surface parse detail in the UI (uniform failure is REQ-037).

**U3 — Give the data load one seam.** Extract `useSharedPage(token)`, or adopt ADR-014 TanStack Query with
the generated client; add a bounded timeout to the `AbortController`, keep `credentials: "omit"` and the
single uniform non-2xx mapping (429 stays unavailable — no retry that could distinguish revoked tokens).
- *Risk:* a new dependency (`docs/DEPENDENCIES.md`) and wire-shape drift; the static export has no
  session. *Mitigation:* keep the endpoint, headers, and accept type byte-identical; if Query is too
  wide, land the hook first and record the deferral in `docs/STATE.md`.

**U4 — Own title and location side effects.** Replace the in-callback assignment with a
`useDocumentTitle(title)` effect that restores the prior title on cleanup; reuse `pathSegment("/p/")`
from `lib/loc.ts` instead of local `pathToken()`.
- *Risk:* a real subscriber added to `locSubscribe` could loop or go stale via `getSnapshot` identity
  churn. *Mitigation:* keep snapshot values as plain strings/null, memoize `getSnapshot`, and add an
  unmount-title test. GUESS default: document that `/p/` transitions are full loads (ADR-013) rather
  than adding `popstate` inside this PR.

**U5 — BoardUI/accessibility tidy and the §8.8 footer.** Normalize chart bars to a fixed `viewBox`
(percentage x, independent of point count), replace `th key={column}` with composite keys, give tables a
`<caption>`/row scope, align link focus with `focus-visible:ring-border-focus-ring`, and add the small
footer with the Boma Bot name if the owner wants explicit §8.8 parity.
- *Risk:* DOM/text changes can shift future screenshot goldens; a footer string needs the i18n catalogue
  in every locale. *Mitigation:* keep the heading count and absence of nav/button/form assertions
  intact; add the footer string per locale together, or log it under "Decisions needed" first.

## BoardUI tokens (unchanged, no raw palette/hex)

Keep `bg-background-full`, `bg-background-secondary-default`, `text-text-primary/-secondary`,
`border-border-table`, `border-separator-border`, `fill-accent-400`, `stroke-separator-border`,
`accent-600`/`accent-700`, composite `text-title-2-medium` / `text-body-regular` / `text-body-medium` /
`text-body-2-medium`, `rounded-xl`/`rounded-md`, and `cx()` for any merge. Dark mode stays token-driven.

## Verification and gates

Web typecheck/lint; Playwright REQ-037 suite (valid, revoked, unknown, 360 px); both UI goldens; axe
WCAG 2.2 AA in light/dark; bundle budget in `docs/PLAN.md`. No cargo step. Mark risk notes GUESS until
replayed on the candidate build.

## Decisions needed (GUESS defaults recorded here)

- Real `/p/` location subscriber vs documented full-load contract — default: full load.
- Add the optional §8.8 footer now — default: defer, no new visible string.
- Adopt TanStack Query here vs keep the isolated hook — default: hook in v0.4.0, Query deferred with ADR.
