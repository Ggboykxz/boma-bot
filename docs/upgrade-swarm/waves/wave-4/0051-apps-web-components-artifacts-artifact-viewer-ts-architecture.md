# v0.4.0 architecture proposal — `apps/web/components/artifacts/artifact-viewer.tsx`

- Worker: 051 · Lens: architecture · Target: v0.4.0
- Scope: `/app/artifacts/<artifactId>` viewer (spec §8.5, REQ-036, ADR-013 single shell).
- Legend: **CONFIRMED** = read from source this session; **GUESS** = proposed design, not implemented.

## 1. Current shape (CONFIRMED)

One client component owns four concerns: routing (`useSyncExternalStore` + `match`), a fetch effect
(`Promise.all` of `artifactsGet` + `artifactsListVersions`), a 4-phase view switch, and all actions
(copy, download, delete). Data is held as `ArtifactGetReply | null` plus a parallel `ArtifactVersionView[]`.
`Phase = "loading" | "ready" | "unavailable" | "error"`; `"unavailable"` is declared but never set,
and `error`/`unavailable`/`!data` collapse into one render branch. `dataUrl` is a full `data:` string
built from `contentBase64` and reused by `<img>`, `<embed>`, and the download anchor. Preview is
`textPreview()` (base64 → `Uint8Array` → `TextDecoder`, 128 KiB decode cap, 4 MiB inline cap).

## 2. Target architecture (GUESS)

Split by responsibility but keep one exported `ArtifactViewer` shell (ADR-013 route contract unchanged):

- **`useArtifactViewer(artifactId)`** hook — owns a discriminated state, not loose booleans:
  `type ViewerState = { status: "loading" } | { status: "unavailable" } | { status: "error"; message: string }
  | { status: "ready"; data: ArtifactGetReply; refreshing: boolean }`. Derive the view from one value so
  impossible combinations (`error` + stale `data`) cannot render.
- **Version chain**: fetch `artifactsListVersions` **once** per `artifactId` (it is independent of the
  pin), and `artifactsGet({ artifactId, version: pinnedVersion })` on pin change only. Sort descending in
  the hook; expose `versions`, `pinnedVersion`, `setPinnedVersion`, `isLatest`. During a pin change keep
  the last `data` visible and set `refreshing: true` (`aria-busy` on the preview) instead of flashing the
  skeleton — today the effect re-runs but `phase` stays `"ready"`, so stale bytes render with no signal.
- **Download controls**: build a `Blob` + `URL.createObjectURL` from decoded bytes (octet-stream for
  download, typed blob only for the bounded image/PDF frame) and `revokeObjectURL` on change/unmount.
  Centralize `download()`, `copyContent()`, and `doDelete()` in the hook so the view stays declarative.
- **View components**: `ViewerSkeleton`, `ViewerUnavailable`, `ViewerError`, `ViewerReady` (header +
  preview) receive state and callbacks only — no fetch logic in JSX.

All colors/type stay on BoardUI semantic tokens and composite type utilities; compose with `cx()`;
install/extend BoardUI form components rather than a raw `<select>`; icons via `@remixicon/react`.

## 3. Concrete upgrades, 3–5, with risks

**U1 — Discriminated phase state + distinct `unavailable` (GUESS).** Replace `Phase` flags with the
`ViewerState` union; map a `404/410` RPC result to `unavailable` (own i18n string) rather than generic error.
*Risk:* distinguishing "not found" requires the RPC layer to expose a typed error code — currently
`call()` rejects generically (CONFIRMED). *Mitigation:* add an error-kind field to `call()` first; if not
in scope, keep `unavailable` mapped from a specific status and document the GUESS in `docs/STATE.md`.

**U2 — Decouple version list from version fetch (GUESS).** Split the `Promise.all` effect into two
effects with independent abort controllers; add `refreshing` so pin changes do not flash stale content.
*Risk:* two effects + a stale pin could render a version pair out of sync. *Mitigation:* carry the
requested version in the ready state and ignore replies whose version differs from the active pin.

**U3 — Replace data-URL transport with managed blob URLs (GUESS).** Decode once to bytes, create a typed
blob for the image/PDF frame and an octet-stream blob for download; revoke on unmount/change. *Risk:*
`URL.createObjectURL` lifetime bugs leak memory or revoke a still-visible URL. *Mitigation:* a small
`useObjectUrl(bytes, mime)` hook with cleanup in its effect return. Side benefit: removes an attacker-
controlled `mimeType` from a navigable `data:` URL scheme (CONFIRMED current code interpolates it).

**U4 — Streamlined text preview (GUESS).** Decode only the first 128 KiB of base64 instead of allocating
a full `Uint8Array` before `subarray` (CONFIRMED current `bytesFromBase64` decodes everything), and add a
"truncated" hint when preview < `artifact.size`. *Risk:* a truncated preview shown without a label
misleads; *Mitigation:* gate the hint on `textContent.length < artifact.size` and keep full download.

**U5 — BoardUI-native version selector + RTL-safe affordances (GUESS).** Swap the raw `<select>` for the
installed BoardUI select (react-aria-components) and mirror `RiArrowLeftLine` in RTL; wrap hash/MIME in
`<bdi dir="auto">`. *Risk:* BoardUI select API/slot naming may differ; *Mitigation:* read
`get_component`/`get_usage_examples` from BoardUI MCP before editing, and check `dir=rtl` in Playwright.

## 4. Token discipline (binding)

Keep `text-text-primary/-secondary/-tertiary/-error-primary`, `bg-background-primary-default/
-secondary-default`, `border-border-button-default`, `text-foreground-icon-*`, composite type utilities,
`rounded-2xl`/`rounded-md`, and `outline-none focus-visible:ring-2 focus-visible:ring-border-focus-ring`.
No raw palette/hex, no `dark:` literals, no hand-built type stacks.

## 5. Verification (GUESS)

Playwright + `@axe-core/playwright`: loading→ready, pin change shows `refreshing`, 404→unavailable,
blob download filename = `artifact.name`, keyboard-only select/delete, 360 px, light/dark, `dir=rtl`.
Traceability: name tests with REQ-036.
