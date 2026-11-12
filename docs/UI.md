# UI

The design system, the token map, the component plan and every local deviation from the
vendored BoardUI source. `/ui-kit` writes this file; `/ui-verify` adds the evidence;
/`/ui-parity` adds the comparison with the original.

## Plan

| Question | Answer | Reference |
| --- | --- | --- |
| Design direction | Fresh design. Nothing visual is taken from the analysed product: no logo, font, icon set, colour value or copy. | ADR-015 (DEC-12) |
| Framework and rendering | Next.js 16 App Router with `output: "export"`, React 19, Tailwind v4. Node is a build-time dependency only; the Rust binary serves the export. | ADR-013 |
| Design system | **BoardUI** (free tier), vendored into `apps/web/`: `styles/`, `utils/`, `components/`. Theme: BoardUI's own `styles/theme.css` (light + dark). **Pro licence: none on this machine** — no key in any file, no Pro component or template in the tree. | ADR-015, `docs/DEPENDENCIES.md` |
| State and data | TanStack Query over the generated client; SSE frames written into the cache by `(threadId, seq)`. | ADR-014 |
| Icons | `@remixicon/react`, passed as component references (`trailingIcon={RiArrowRightLine}`), never as elements. | BoardUI rules |

## Foundations in ten lines

1. **Tokens, not palette.** Every colour is a semantic token (`text-text-primary`,
   `bg-background-secondary-default`, `border-separator-border`, `text-foreground-icon-primary`);
   raw palette classes and literals are forbidden, so dark mode is automatic.
2. **Type is composite.** `text-title-2-medium`, `text-headline-medium`, `text-body-medium` set
   size, weight and line-height together; stacking `text-sm font-medium` is forbidden.
3. **Dark mode** is the `.dark` class on `<html>`; components never carry `dark:` overrides.
4. **Classes are merged with `cx()`** from `@/utils/cx` (tailwind-merge aware of the composite
   type utilities), never by concatenation and never with plain `clsx`.
5. **Focus** is `focus-visible:ring-2 focus-visible:ring-border-focus-ring`; our own links carry
   `.boma-focus`, which draws the same ring from the same token.
6. **Targets** are at least 44 px (`.boma-target`), above the 24 px WCAG 2.2 AA minimum.
7. **Direction** comes from the locale through BoardUI's `DirectionProvider`, which also gives
   React Aria the right keyboard behaviour and portal placement.
8. **Spacing** stays on Tailwind's scale; `gap` before per-child margins.
9. **Motion** is CSS-first and respects `prefers-reduced-motion` (BoardUI's globals.css).
10. **One system.** No second component library, no hand-written lookalike of an installed
    component.

## Semantic token map

| Role | Token | Where it is used |
| --- | --- | --- |
| Page ground | `bg-background-full` | `<body>` on every route |
| Bar / inactive surface | `bg-background-secondary-default` | the application header |
| Hover on a bar | `bg-background-secondary-hover` | navigation items on hover |
| Primary text | `text-text-primary` | headings, navigation labels, the not-found title |
| Secondary text | `text-text-secondary` | the product label, the body copy, the placeholder |
| Accent action | `bg-button-primary` + `text-text-white` | the landing call to action (BoardUI `ButtonLink`) |
| Hairline | `border-separator-border` | the header's bottom border |
| Focus ring | `border-border-focus-ring` | `.boma-focus` and every BoardUI component |
| Icon | `text-foreground-icon-primary` | icons inherit the text colour of their context |

## Component table

Installed today (M0): `direction` (foundation), `button` (base), `cx`, `directional-icon`.

| REQ | Screen | Components | States | Milestone |
| --- | --- | --- | --- | --- |
| REQ-057 | every route | `DirectionProvider`, `useTranslation` | locale, direction, dark mode | M0 (done) |
| REQ-001, REQ-002 | `/app/:botId` thread | `textarea` (composer), `button` (send, stop) | idle, sending, streaming, error | M1 |
| REQ-003, REQ-004 | thread | transcript list, `divider` (unread) | streaming, steering, empty | M1–M2 |
| REQ-007 | thread | `card` (pending question, approval) | pending, answered, timed out | M2 |
| REQ-009 | thread | `divider`, `badge` (unread count) | read, unread, snoozed | M2 |
| REQ-012 | thread and activity | `chip` (run status) | queued, leased, running, waiting, completed, failed, cancelled | M4 |
| REQ-026 | `/app/:botId` computer panel | `drawer`, `spinner`, `skeleton` | starting, running, taken over, released | M5 |
| REQ-037 | `/app/x/:linkId` shared page | `table`, `divider` | shared, invalid, expired | M6 |
| REQ-040, REQ-041 | `/sign-in`, `/sign-up` | `input`, `button`, `social-button` | signed out, invalid, locked | M1 |
| REQ-042 | `/onboarding/:step?` | `input`, `button`, `select` | first run, invited | M1 |
| REQ-038 | `/app/board` | `table`, `chip` (feature-flagged) | board on, board off | M7 |
| REQ-058 | `/admin` (operator only) | `table`, `badge` | operator, non-operator | M2 |

Every screen in that table has a Playwright test whose name carries the REQ id, and every UI
golden in `spec/golden/ui/` is replayed by `apps/web/tests/golden.spec.ts`. A golden that is
marked `pending` in `golden-status.json` is skipped with its milestone and its reason printed.

## Local deviations from the vendored source

The vendored files (`styles/theme.css`, `styles/typography.css`, `styles/globals.css`,
`utils/*`, `components/*`) are never hand-edited. The product's own additions live in
`apps/web/styles/boma.css`:

| Deviation | Why | Where it is enforced |
| --- | --- | --- |
| `--color-text-secondary` is re-mapped to `neutral-400` in dark mode | BoardUI leaves it at `neutral-500`, which reaches 3.95:1 on the dark page ground — under the 4.5:1 WCAG 2.2 AA floor. `neutral-400` plays the same role in dark (7.4:1). A token is re-mapped, never a literal. | `styles/boma.css`; the axe gate found it |
| `.boma-target` (44 px minimum block size) | WCAG 2.2 AA asks 24 px; this product asks 44 px on touch controls. Scoped to a class so it cannot change a vendored component's metrics. | `styles/boma.css` |
| `.boma-focus` (focus ring for our own links) | BoardUI components carry their own ring; our navigation links do not. Same token, same width, same offset. | `styles/boma.css` |
| The navigation labels use `text-text-primary` | `text-text-secondary` on the secondary surface reaches 4.42:1 in light mode, under the AA floor. | `components/shell/app-shell.tsx`; the axe gate found it |

No other file was edited. `npm run lint` and the axe gate are what keep the deviations honest.

## Copy conventions

- **Every user-visible string goes through the catalogue** (`lib/i18n.ts`, `t(locale, key)`).
  A hardcoded label is a bug waiting for the locale switch. The catalogue is ICU-shaped from the
  start; the full set of locales ships in M1.
- **A missing string falls back to English**, never to the key and never to a blank. The key
  itself is returned as a last resort, because a key is visible in a screenshot and an empty
  string is not.
- **Direction follows the locale, not the text.** `direction("he")` is `rtl`, `direction("fr")`
  is `ltr`; the document's `lang` and `dir` are written together by the boot script.
- **Tone**: plain, factual, second person, no exclamation marks. The product speaks like a tool
  that is on the user's side.
- **English is the source catalogue.** French and Hebrew are translations; a key that has no
  translation falls back rather than inventing one.

## i18n and RTL

- The static export ships the English locale. The boot script reads `localStorage["boma-locale"]`
  before the first paint and writes `lang` and `dir` on `<html>`; the direction provider reads them
  back with `useSyncExternalStore` and hands the locale to React Aria.
- The first render always uses the server snapshot, so React never compares two different trees:
  the exported HTML is English, and the page switches to the stored locale right after hydration.
- Logical properties everywhere (`ms`/`me`, `ps`/`pe`, `start`/`end`, `text-start`/`text-end`).
- Email addresses, URLs, code and OTP digits stay LTR, isolated with `<bdi>` or `dir="auto"`.
- The RTL proof is a test: `apps/web/tests/a11y.spec.ts` stores the Hebrew locale, loads
  `/app/`, and asserts `dir="rtl"`, the Hebrew navigation label and a clean axe run.

## Accessibility target and known limits

Target: **WCAG 2.2 AA**, zero critical or serious axe finding on every screen, in light, dark and
RTL, with a keyboard path for every primary flow.

| Check | State |
| --- | --- |
| axe, light and dark, `/` and `/app/` | zero critical/serious |
| axe, RTL | zero critical/serious |
| Focus ring | token-driven, visible on every interactive element |
| Target size | 44 px on the shell's controls |
| Keyboard | navigation and the CTA are reachable and operable; the full path lands with the M1 screens |
| `prefers-reduced-motion` | BoardUI's animations fall back to a static state |
| 360 px width | **not verified yet** — the shell is fluid, but no narrow-viewport test exists until M1 |
| Screen reader | **not verified yet** — axe checks the markup, not the announcement |

## Reference captures and their licenses

`../re-workspace/notes/ui/` is empty: `/ui-probe` never ran in the analysis workspace, so there
is no capture of the original to compare against. Nothing visual was taken from the analysed
product, and no asset is vendored at M0. `/ui-parity` will compare behaviour and structure, not
pixels, and will list the intentional differences in `docs/UI-PARITY.md`.

## Screenshots

`apps/web/test-results/` holds the failure screenshots Playwright writes; they are gitignored.
The captures that matter for a review are produced by `/ui-verify`, which writes them under
`tests/ui/screenshots/` with the origin, the viewport, the theme and the date in the name.
