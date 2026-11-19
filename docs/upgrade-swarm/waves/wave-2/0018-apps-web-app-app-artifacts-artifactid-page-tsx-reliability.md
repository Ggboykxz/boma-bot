# 0018 — `/app/artifacts/[artifactId]/page.tsx` (artifact viewer) — reliability lens

Target: `apps/web/app/app/artifacts/[artifactId]/page.tsx` + the client it mounts,
`components/artifacts/artifact-viewer.tsx` (the page itself is a 23-line static-export shell;
all failure modes live in the viewer). Spec: spec/08-ui-ux.md §8.5, REQ-036, ADR-013.
Status of claims: CONFIRMED = read in this tree; GUESS = inferred, needs verify.

## Current behavior (read, not run)

- Page renders `<AppShell><ArtifactViewer /></AppShell>`; one static shell serves the whole
  `/app/artifacts/<id>` subtree; the id is parsed from `window.location` via `useSyncExternalStore`.
- Viewer fetches `artifactsGet` + `artifactsListVersions` in parallel, phases
  `loading | ready | unavailable | error`; there is no `unavailable` transition — the state is
  declared but never set (CONFIRMED).
- Content arrives inline as base64 in the RPC reply (`contentBase64`); a `data:` URL is built
  unconditionally on every render of a loaded artifact.
- Text preview: `bytesFromBase64` decodes the **entire** payload, then `textPreview` only
  inspects the first 128 KB. `MAX_INLINE_BYTES` (4 MB) is enforced *after* full decode.
- Copy falls back to `artifact.hash` when the payload is not inlinable; download is a
  programmatic `<a download>` against the `data:` URL; delete uses `rootArtifactId`
  (deletes the whole version chain) with a confirm step.

## Failure modes

1. **Missing / unknown artifact.** `artifactsGet` rejects → generic `error` screen with no
   distinction between 404, permission denied, offline, and server fault, and no retry. The
   `unavailable` phase exists in the type union but is dead code, so "gone" and "broken" are
   indistinguishable (CONFIRMED).
2. **Large files.** Whole base64 payload is held in memory twice (RPC reply + decoded bytes),
   plus a third copy inside the `data:` URL string. Browsers are unreliable for very large
   `data:` URLs: `<img>` usually survives, `<embed type="application/pdf">` frequently refuses or
   renders blank past a few MB, and `<a download>` against a multi-MB data URL can silently no-op
   in some engines (GUESS on exact thresholds). `atob` on a huge string can also throw
   synchronously and is outside the try/catch in `bytesFromBase64`'s callers except
   `textPreview` (download path never decodes, so it survives; copy/preview path is guarded).
3. **Render errors.** No error boundary anywhere in the subtree. A throw in render (e.g.
   malformed `createdAt` reaching `shortDate`, unexpected DTO shape after a backend change,
   `atob` via an unforeseen path) blanks the whole `AppShell` with Next's default error screen,
   losing the artifacts nav and any recovery affordance. `<embed>` for PDFs has no fallback when
   the MIME is declared `application/pdf` but the bytes are not.
4. **Download failures.** No error handling, no progress, no completion signal. Failure modes:
   data-URL size limits, blocked navigation, filename sanitization (browser handles, but
   `artifact.name` with path separators should be sanitized anyway), and user denial of the
   download prompt. Silent no-op reads as "button broken."
5. **Version skew / stale state.** `pinnedVersion` set, then artifact deleted/re-created
   elsewhere → `artifactsGet` errors and the whole view collapses to the generic error page even
   though the grid would show the current head. `Number(event.target.value)` can yield `NaN` if
   options are ever rendered from unvalidated data. No `key` on the viewer means a route change
   between two artifact ids reuses state; `useSyncExternalStore` subscription handles path, but
   `data`/`versions`/`pinnedVersion` from the previous artifact linger until the fetch resolves
   (flash of wrong artifact) (CONFIRMED for the state layout; flash is GUESS).
6. **Clipboard.** Denied outside secure context — handled by keeping the label honest, but no
   user-visible hint why "Copy" appeared to do nothing.
7. **Delete race.** Double-click confirm is guarded by `deleting`, but a failed delete leaves
   `confirmDelete` open with only an inline message; no focus management to the error, no retry
   affordance other than re-clicking confirm.

## Proposed hardening for v0.4.0

1. **Distinguish failure kinds.** Map RPC errors to `not-found | forbidden | offline |
   server | unknown`. Missing/unknown artifact (incl. 404 and deleted-mid-session) renders a
   dedicated empty state: icon, "This artifact is unavailable or was deleted," link to the grid.
   Wire the dead `unavailable` phase to the 404-like case. Add a **Retry** button on the generic
   error state (re-runs the effect via a `retryNonce` state) — cheap, no new layer.
2. **Error boundary.** Add a route-level `error.tsx` (and/or wrap `<ArtifactViewer>` in a
   boundary) with the same recovery affordances: retry + back-to-grid. Also `not-found.tsx` for
   parity. This is the single highest-leverage change.
3. **Large-file path.**
   - Decode lazily and bounded: decode at most `MAX_INLINE_BYTES` for preview (slice the base64
     string at a 4-aligned boundary before `atob`) instead of decoding-then-slicing.
   - Stop building `dataUrl` unconditionally; compute it only when `bucket` is `image|pdf` or
     when download is requested. Better: ask the backend for a short-lived download URL and use
     `fetch`→`Blob`→object URL for download, revoking on unmount — removes the data-URL ceiling
     entirely. If the RPC must stay base64 for parity, keep data URLs but cap inline render at a
     conservative threshold (e.g. 8 MB) and force "download-only" + explicit size note above it.
   - Replace PDF `<embed>` degradation with a fallback block: iframe/embed onError (or timeout
     heuristic) → "Preview unavailable; download instead." Render text preview with a hard
     256 KB cap and a "truncated" footer note.
4. **Download hardening.** Sanitize `artifact.name` for the `download` attribute (strip `/\\`,
   trim, fallback `artifact-<id>`); surface failure via a transient inline error near the button
   (`role="alert"`); prefer object URLs with `URL.revokeObjectURL` after the click; disable the
   button while a download is being prepared; log a client telemetry breadcrumb on failure.
5. **State hygiene.** `key={artifactId}` on `<ArtifactViewer>` from the page (or reset
   `data/versions/pinnedVersion` on id change); validate `Number()` and fall back to current
   version on `NaN`; clear `deleteError` on retry; move focus to the delete error message when
   shown; `aria-live="polite"` on copy/download feedback.
6. **Skeleton vs. content.** Keep the existing skeleton but add `aria-busy` semantics already
   present — verify focus is not stolen on phase transitions; loading → ready should not move
   focus.
7. **Tests (Playwright, REQ-036).** Mock RPC: (a) 404 → empty state + grid link; (b) 500 → error
   state + retry succeeds; (c) 3 MB PDF → no crash, download works or clean fallback; (d) text
   artifact > inline cap → download-only + copy falls back to hash; (e) clipboard denied →
   honest label; (f) forced render throw → boundary renders, retry recovers; (g) route change
   artifact A → artifact B shows no stale content.

## Risks / trade-offs

- **Object-URL download needs a backend URL endpoint.** If parity forbids new API surface, stay
  on data URLs + conservative cap; the failure mode improves but the ceiling remains (DEVIATION
  risk either way — flag in docs/STATE.md).
- **Error boundaries catch render, not event handlers/effects.** Copy/download/delete already
  use try/catch; keep that pattern and do not rely on the boundary for them.
- **Retry on 404 is pointless.** Only show Retry for transient (offline/5xx) classes; not-found
  gets the grid link only — avoids teaching users to mash a button that cannot succeed.
- **Inline caps are user-visible behavior changes.** Lowering the inline threshold changes what
  renders vs. downloads today; treat as a spec-adjacent UX change, note in PARITY.md before
  shipping.
- **Extra RPC for a signed URL doubles request cost** for the common small-artifact case if made
  unconditional — request it lazily on download click instead, or reuse inline base64 when small.
- **Scope.** This file is a shell; nearly all changes land in `artifact-viewer.tsx`. Page-level
  changes are limited to `key`, maybe `error.tsx`/`not-found.tsx` siblings.
