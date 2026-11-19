# Worker #080 — `apps/web/components/shell/app-entry.tsx` — reliability (v0.4.0)

Scope: the `/app` static-export dispatcher only (33 lines). Claims marked CONFIRMED were read
from source this session; GUESS marks runtime behaviour inferred from code comments, not tested.

## What the component does

`AppEntry` is the client body of the `/app/` shell. Because the Rust handler maps unknown
`/app/*` paths to one `index.html` (ADR-013; CONFIRMED as the design intent in comments in
`app-entry.tsx` and `lib/bot-directory.ts`), it reads the path with `botIdFromPath()` in a
`useSyncExternalStore` getter and renders `ThreadsHome` (null) or `ThreadScreen` (id).
`subscribe` is a no-op and the server snapshot is always `null`.

## Failure modes

1. **Unknown route silently becomes home** (CONFIRMED from regex). `botIdFromPath` uses
   `/^\/app\/([^/]+)\/?$/`, which accepts exactly one segment. In the fallback shell:
   - `/app/artifacts/<id>/` and `/app/g/<groupId>/` have two segments → no match → `null` →
     `ThreadsHome`. The user lands on the threads placeholder instead of the artifact or group
     room they asked for; no error, no navigation, no log.
   - `/app/board`, `/app/activity`, `/app/artifacts` (one segment) → matched as a *bot id* →
     `ThreadScreen botId="board"` → `botsGet` rejects → renders `thread.error.load` + Retry,
     forever unrecoverable (no bot with that id exists). A directory-looking reserved word is
     mistaken for data.
   - A truly bogus single segment (`/app/typo/`) also becomes an error screen, not a not-found.
   Severity: high — wrong content and a dead retry loop on every non-thread deep link that is
   not one of the few `generateStaticParams` fixtures (`bot`, `bot_golden`, `group_golden`,
   `artifact_golden`).

2. **Capability gating is bypassed in the fallback shell** (CONFIRMED gating exists elsewhere;
   GUESS on exact runtime). `/app/board` is flag-gated (`BOMA_BOARD`): `board/page.tsx` and
   `BoardScreen` render a not-found shape when `boards.get` is absent (REQ-038). The fallback
   `AppEntry` path has no gating at all, so when the flag is off a deep link served by the
   fallback renders either the threads home or a bogus-thread error instead of the required
   not-found state. Same risk for group rooms if they gain a capability gate later.

3. **No error boundary around the dispatch** (CONFIRMED). `app/` has `not-found.tsx` but no
   `error.tsx` / `global-error.tsx`. Any render throw in `ThreadsHome` or `ThreadScreen` (e.g. a
   malformed message block reaching `MessageBlocks`) unmounts `AppProviders` + `AppShell`: the
   user loses navigation and the rail, not just the screen.

4. **Uncaught `decodeURIComponent`** (CONFIRMED). `botIdFromPath` calls `decodeURIComponent`
   with no try/catch inside the `useSyncExternalStore` getter. A malformed escape such as
   `/app/%E0%A4%A/` throws `URIError` during render; with no boundary (mode 3) the whole app
   blanks. The getter must be total.

5. **Stale dispatch after client-side path changes** (GUESS). `subscribe` is a no-op on the
   premise the path only changes on reload. `router.push/replace` to `/app/...` exists elsewhere
   (`onboarding-screen`, `bot-create-form`, `accept-screen`). Those targets are real route pages,
   so today they re-render through their own page; but any future in-place path change under this
   shell would not re-dispatch. At minimum the no-op assumption needs a regression test.

## Concrete hardening (v0.4.0)

- **H1 — total route classifier.** Extract a pure `routeFromPath(pathname): AppRoute` into
  `lib/` returning a discriminated union: `home | thread(id) | group(id) | board | activity |
  artifacts | artifact(id) | notFound`. Reserved first segments (`board`, `activity`,
  `artifacts`, `g`) are checked *before* the bot regex so they can never be mistaken for ids.
  `decodeURIComponent` is wrapped: malformed encoding → `notFound`, never a throw. `AppEntry`
  switches on the union. Unit-test the classifier (Vitest) for every shape above, including
  `%`-malformed ids, trailing slashes and extra segments.
- **H2 — render the right screen.** `home` → `ThreadsHome`; `thread(id)` → `ThreadScreen`;
  `group/board/activity/artifacts/artifact` → reuse the exact components the route pages use
  (`GroupThreadScreen`, `BoardScreen`, …) so the fallback shell matches the dedicated shell.
  Use `next/dynamic` for the non-home screens to keep the home JS bundle within the
  `docs/PLAN.md` budget; import them lazily, not eagerly.
- **H3 — not-found, not home.** `notFound` renders the existing `app/not-found.tsx` shape (i18n
  via `t`) with a link back to `/app/`, instead of the threads home. This is an externally
  visible change for bogus deep links: record it in `docs/PARITY.md` and flag for operator
  approval before shipping (parity-first rule).
- **H4 — error boundary inside the shell.** Add `app/app/error.tsx` (client component) so a
  screen throw keeps the header/nav/rail mounted and offers `reset()` + Retry; use the catalogue
  for all text and BoardUI error tokens (`text-text-error-primary`,
  `border-border-error-default`). Optionally a `global-error.tsx` for provider-level throws.
  Do not auto-recover silently: surface a retry, keep the failure visible.
- **H5 — honour capability gating.** For the `board` route in the fallback shell, reuse the
  existing `boards.get` probe (as `BoardScreen` does) and render not-found when absent; do not
  invent a second client-side flag source. Keep the server flag authoritative.
- **H6 — subscription correctness.** Either switch to `usePathname()` (already used in
  `app-shell.tsx`) as the client source of truth while keeping the `null` server snapshot, or add
  a `popstate` subscription and document that client-side pushes target route pages. Add a
  Playwright test that navigates within the shell and asserts the screen changes.

## Risks / trade-offs

- Eagerly importing five screens in the home shell would inflate the initial bundle and could
  breach the JS budget; mitigate with `next/dynamic` and measure with `/ui-verify`.
- The error boundary can hide defects if it swallows errors; require a visible retry and keep
  console reporting in dev.
- Capability probing in the client can flash the wrong screen for one frame; keep the SSR
  snapshot `null`/loading until the probe settles to avoid hydration mismatches.
- H1/H3 change fallback behaviour (home → not-found, bogus thread → not-found): these are
  DEVIATION candidates needing `docs/PARITY.md` + approval, despite being bug fixes.
- A routing/dispatch decision (H1/H2) warrants an ADR; test-first per AGENTS.md (unit tests for
  the classifier, Playwright + axe for the shell flows), REQ ids in test names.
- No change to the Rust static handler is assumed here; if the fix requires the handler to map
  known routes to their own shells instead of `index.html`, that is a separate ADR.
