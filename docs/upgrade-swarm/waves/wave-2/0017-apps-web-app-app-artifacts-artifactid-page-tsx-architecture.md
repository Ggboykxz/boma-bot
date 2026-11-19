# 0017 — app/app/artifacts/[artifactId]/page.tsx — architecture (v0.4.0)

Worker #017 · module `apps/web/app/app/artifacts/[artifactId]/page.tsx` · lens: architecture · READ-ONLY
review of `rebuild-workspace/.worktrees/before-m5` at v0.3.0-era source. Proposals only; no code in
this change.

## What the file is — and where the real mass lives

`page.tsx` is 20 lines and correct for ADR-013 (Next.js static export, single shell per subtree):
a server component that emits `generateStaticParams()` with the one placeholder `artifact_golden`
and renders `<AppShell><ArtifactViewer /></AppShell>`. The id is never baked in; the viewer reads it
from the location via `locSubscribe`/`match` (`lib/loc.ts`), and the Rust static handler maps any
unknown path under `/app/artifacts/` to this shell.

The role for REQ-036 (spec/08 §8.5): this route is the **single static carrier for the artifact
detail screen** — the viewer with version selector (versions are a chain), download, "open in
thread", copy, and the rule that untrusted content never renders as markup. Everything else lives
in `components/artifacts/artifact-viewer.tsx` (266 lines, `"use client"`), which owns four jobs:

- **URL matching** for `/app/artifacts/<artifactId>/` (null → "pick one" empty state).
- **Data load** — `Promise.all([artifactsGet({artifactId, version: pinnedVersion}),
  artifactsListVersions(...)])` inside `useEffect` + `AbortController`, bespoke `Phase` union
  (`loading|ready|unavailable|error`) held in `useState`.
- **Interactions** — pinned-version select, copy-to-clipboard (text or hash), delete-with-confirm
  against the **family root** (`artifactsRemove({artifactId: rootArtifactId})`), `router.replace`
  back to the grid.
- **Markup** — breadcrumb nav, header (name, size · MIME · sha256 short, date), action row, and the
  preview region: `data:` URL image (`max-h-[60vh]`), `<embed>` PDF (`h-[70vh]`), `<pre>` text
  (bounded at 128 KiB decode), or download-only. `MAX_INLINE_BYTES = 4 MiB`.

## Structural findings (CONFIRMED by reading source)

1. **Hand-rolled fetching, not the documented data layer.** `docs/UI.md`/ADR-014 name TanStack
   Query over the generated client as the state/data layer; a repo-wide grep finds **no**
   `useQuery` usage anywhere — both artifact screens hand-roll `useEffect` fetches with no
   cache, no dedup with the grid, no invalidation on delete, and a pinned-version change
   refetches the *version list* too.
2. **One component, no seams.** The viewer mixes URL matching, transport, mutations and markup;
   `renderMessageRow`-style inline closures; no memoized children; nothing is unit-testable
   without a DOM.
3. **Content rides the control plane.** `artifactsGet` returns `contentBase64` in the same JSON
   response as metadata, so a 4 MiB file is decoded twice (atob for text + a second copy inside
   the `data:` URL) and every metadata read over the wire carries bytes.
4. **BoardUI rules bypassed where components exist.** Raw `<select>` for the version chain, raw
   `<embed>` for PDFs, physical `RiArrowLeftLine` for RTL — while the free `select`,
   `icon-button` and `directional-icon` are installed (per docs/UI.md component table) and the
   rules require react-aria-based form components and mirrored glyphs.
5. **No route-level failure surfaces.** No `error.tsx`/`not-found.tsx` for the subtree, no
   per-document `<title>` (impossible from a static placeholder), and the `unavailable` phase is
   dead code — the catch collapses everything to `error`.

## Proposed upgrades for v0.4.0 (in dependency order)

### U1. Keep `page.tsx` a thin server wrapper; add route boundaries and a page title effect
Add `app/app/artifacts/[artifactId]/error.tsx` (resettable client boundary reusing the current
error markup + `.boma-focus` back link) and a small `useDocumentTitle` in the viewer (`document.title =
artifact.name`) so screen readers and history entries carry the artifact name. Split `Phase` into
`loading|ready|error` and surface the API error code (`not_found` → back-to-grid) explicitly.
- Risk: export-time boundary files must stay inside the ADR-013 single-shell contract; goldens
  pin the current error markup. Mitigate: move first, redesign never; Playwright + axe diff.

### U2. Extract `useArtifactDetail` (hook + reducer) or migrate to TanStack Query
One hook owns `{ phase, artifact, versions, pinnedVersion }`; Query keys `["artifact", id, version]`
and `["artifact-versions", id]`; delete invalidates and `router.replace`s; the version list does
not refetch on pin. Grid and detail share the cache instead of double-fetching. If the Query
migration is too wide for one PR, ship the hook first behind the same call sites.
- Risk: behavior drift vs the REQ-036 Playwright suite and any replay of the (currently pending)
  `artifact-*` goldens; double-fetch on mount. Mitigate: golden replay + forced-abort test before
  merge; keep network shapes byte-identical.

### U3. Split the content plane from the control plane
Propose (DEVIATION, needs ADR + owner sign-off) a bounded content read: `GET
/api/artifacts/{id}/content?version=` streaming bytes with `Content-Type` from the proven MIME,
keeping `artifactsGet` metadata-only; `<img>`/`<embed>`/`<a download>` point at it; the 4 MiB
inline-base64 path remains only as a small-file fast path. Until approved, gate behind a capability
flag and ship zero wire changes by default.
- Risk: spec/API contract breach if shipped unapproved; auth on a raw byte route (cookie/token,
  no referrer leak); base64 fallback divergence. Mitigate: ADR + PARITY.md entry; flag default-off;
  content never logged; `Referrer-Policy: no-referrer`.

### U4. Swap raw elements for installed BoardUI components; fix RTL and focus
Replace `<select>` with the vendored `select` (react-aria-components), keep option labels ICU
(`artifacts.detail.version`), and render the PDF in a bounded `<iframe sandbox="">` (or keep
`<embed>` only if the axe gate objects to the iframe). Replace `RiArrowLeftLine` with the
`directional-icon` mirror; audit `ms/me`, `ps/pe`, `start/end` across the viewer; all colors stay
on semantic tokens (`text-text-error-primary`, `border-border-button-default`), all type on
composite utilities, all merges via `cx()`, targets ≥ 44 px (`.boma-target`), icons as component
references.
- Risk: native-`<select>` golden assertions and keyboard behavior change; sandboxed PDF still needs
  a download escape hatch. Mitigate: Playwright keyboard path (Tab/Enter/Escape), axe AA at 360 px,
  light/dark, RTL locale run.

### U5. Make the version chain a first-class derived model
Derive `VersionChain` (sorted desc, 1..N, latest flag, current) once from `artifactsListVersions`;
the header select, the "latest by default" rule (REQ-036) and the delete confirmation all consume
it. Make the destructive scope explicit in types and copy: `artifactsRemove` targets the
`rootArtifactId` family — the confirm label must say the whole chain is deleted, not one version
(or split per-version delete if the API allows it; that is an owner question, logged as GUESS).
- Risk: user-visible wording change = i18n catalogue churn (en source, fr/he translations);
  deleting a family vs a version is ambiguous in spec §8.5. Mitigate: owner decision or GUESS note
  in docs/STATE.md; no behavior change until decided.

## Execution order & gate
U1 → U2 → U4 land in one v0.4.0 theme (structure, then tokens); U3 needs owner sign-off (spec);
U5 follows U2. Every step gated by: Playwright REQ-036 suite green, axe WCAG 2.2 AA clean, bundle
budget (docs/PLAN.md) not exceeded, `page.tsx` diff stays a thin server wrapper, no new dependency
without docs/DEPENDENCIES.md.

## What not to touch
`generateStaticParams` + the ADR-013 fallback (contractual); `lib/loc.ts`'s single-segment id
contract; the untrusted-content posture (never markup, blob/data URLs, `<pre>`, `TextDecoder`);
`AppShell`; the `artifact_golden` placeholder; the Rust-pinned contract in
`crates/api/tests/rpc_surface.rs::req_036_*`.

CONFIRMED by reading the tree at v0.3.0-era source; all risk and timing notes are GUESS until the
REQ-036 Playwright suite and goldens are replayed on the candidate build.
