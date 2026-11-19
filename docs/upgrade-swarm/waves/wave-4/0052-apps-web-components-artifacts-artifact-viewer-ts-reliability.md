# 0052 — `apps/web/components/artifacts/artifact-viewer.tsx` — reliability (v0.4.0)

Scope: the client viewer only (`"use client"`, 266 lines). Spec §8.5 / REQ-036, ADR-013.
Complements brief 0018 (same viewer seen from the `[artifactId]/page.tsx` shell); this brief stays
on the component. Status: CONFIRMED = read in this tree; GUESS = inference needing verification.

## Current behavior (read, not run)

- `artifactId` comes from `useSyncExternalStore(match("/app/artifacts/", pathname))`; a
  `Promise.all([artifactsGet, artifactsListVersions])` fills `data`/`versions` behind one
  `AbortController`; phases are `loading | ready | unavailable | error`.
- `unavailable` is declared but never assigned, and the `catch` discards the `RpcError`
  (`code`, `status`): 404, FORBIDDEN, offline and 5xx all collapse to one generic screen
  with a back-to-grid link and no retry (CONFIRMED).
- `call()` is a bare `fetch` with no timeout; only unmount aborts (CONFIRMED). A half-open
  connection leaves the `aria-busy` skeleton up forever (GUESS on likelihood, CONFIRMED on code).
- `dataUrl = data:\`<mime>\`;base64,<payload>` is built unconditionally on every loaded render and
  drives `<img>`, `<embed type="application/pdf">`, and the `<a download>` (CONFIRMED).
- `textPreview()` calls `bytesFromBase64()`, which `atob()`s the **whole** payload, then checks
  `MAX_INLINE_BYTES` (4 MB) *after* decode, then slices to 128 KB for the `<pre>` (CONFIRMED).
- `download()` builds/clicks a programmatic anchor with no try/catch, no name sanitization, no
  completion or failure signal. `copyContent()` guards clipboard denial but gives no reason.
  `doDelete()` guards double-click via `deleting`, sets `deleteError` inline only (CONFIRMED).

## Failure modes

1. **Missing / deleted / forbidden artifact.** `artifactsGet` rejects → generic error screen;
   a deleted-mid-session artifact or a 404 is indistinguishable from a server fault. Dead
   `unavailable` phase means "gone" has no state (CONFIRMED).
2. **Large-file decode.** The base64 string is held in the RPC reply, a full decoded
   `Uint8Array`, and the `data:` string — three copies. `atob` on a huge string can throw
   synchronously (caught only inside `textPreview`, not on the image/pdf/download paths);
   `MAX_INLINE_BYTES` is enforced too late to prevent the allocation. Browser `data:` limits
   make big `<embed>` PDFs render blank and multi-MB `<a download>` silently no-op (GUESS on
   exact thresholds, CONFIRMED on the unconditional construction).
3. **Download failure.** No error handling, progress, or success signal; an oversized data URL,
   blocked navigation, or `/` / `\` in `artifact.name` yields a silent no-op read as "button
   broken" (CONFIRMED on absence of handling).
4. **Error-boundary gaps.** No `error.tsx` / `not-found.tsx` in the artifacts route (CONFIRMED
   by glob: only `page.tsx` files). A render throw (malformed `createdAt` in `shortDate`, DTO
   shape drift, unforeseen `atob` path) or a non-PDF payload declared `application/pdf` blanks
   the whole `AppShell`, losing nav and recovery (GUESS on triggers, CONFIRMED on no boundary).
5. **Related (keep in scope).** No timeout → permanent skeleton. `pinnedVersion` plus
   `data`/`versions` are not reset when `artifactId` changes and the viewer has no `key`
   → stale-artifact flash after client route change. `Number(event.target.value)` can be `NaN`
   (CONFIRMED on layout; flash is GUESS).

## Concrete hardening for v0.4.0

1. **Error taxonomy.** In the `catch`, check `err instanceof RpcError`; map `NOT_FOUND` and
   deleted-mid-session to the `unavailable` phase (dedicated "unavailable or deleted" copy +
   grid link); `FORBIDDEN` to its own message; `UNAUTHORIZED` delegates to the global handler
   and does not render `error`. Add a `retryNonce` state so the effect re-runs from a Retry
   button on the transient (`INTERNAL`/offline/timeout) screen. Keep Retry only for transient
   classes — never teach users to mash 404.
2. **Bounded decode.** In `bytesFromBase64`/`textPreview`, slice the base64 string at a
   4-aligned boundary to `MAX_INLINE_BYTES` *before* `atob`, and cap the `<pre>` preview at a
   smaller hard limit with a "truncated" note. Make every `atob` call try/catch so a corrupt
   payload yields download-only, never a render throw.
3. **Conditional media URL.** Compute `dataUrl` only for `bucket === image | pdf`, and only
   under a conservative size ceiling; above it render "Preview unavailable — download."
   Preferred: a backend short-lived download URL → `fetch`→`Blob`→`URL.createObjectURL`,
   revoked on click/unmount, removing the ceiling entirely. Give `<embed>` a fallback block
   for bytes that are not really PDF.
4. **Download hardening.** Wrap `download()` in try/catch; sanitize `artifact.name`
   (strip `/\\`, trim, fallback `artifact-<id>`); on object URLs revoke after click; surface
   failure via transient `role="alert"` copy near the button and `aria-live="polite"` feedback;
   disable while preparing.
5. **Transport + state hygiene.** Compose the unmount signal with `AbortSignal.timeout(10–15s)`
   (`AbortSignal.any`) so a hung call reaches the error phase with Retry. Reset
   `data`/`versions`/`pinnedVersion` (or require `key={artifactId}` from the page) on id
   change; validate `Number()` and fall back to the current version on `NaN`; move focus to
   the delete error and clear it on retry.
6. **Boundary.** Add route-level `error.tsx` and `not-found.tsx` siblings with retry +
   back-to-grid, so render throws degrade locally. (Boundaries catch render, not handlers —
   keep the existing try/catch pattern in copy/download/delete.)
7. **Tests (Playwright, REQ-036).** Mock RPC: (a) 404/deleted → unavailable state; (b) 500 →
   error + Retry succeeds; (c) corrupt base64 → download-only, no throw; (d) oversized PDF →
   no inline render / clean fallback; (e) download with `/` in name → sanitized, failure
   surfaced; (f) forced render throw → boundary renders; (g) hung call → timeout error;
   (h) route change A→B → no stale content.

## Risks / trade-offs

- Object-URL download needs a new backend endpoint — API/DEVIATION decision for the owner; if
  forbidden, stay on capped data URLs (failure improves, ceiling remains). Note in STATE.md.
- Inline caps change what renders vs. downloads today: user-visible, spec-adjacent — record in
  PARITY.md before shipping; keep golden/screenshot churn in mind.
- Retry on 404 is pointless and busy-state focus must not be stolen on loading→ready.
- `AbortSignal.any` needs a modern baseline — verify browserslist target.
- i18n: every new state/copy string (taxonomy, truncated, download error) needs all locales or a
  clean fallback; axe 2.2 AA and keyboard paths must be re-checked after new controls.
- No change to untrusted-content rules (never render artifact bytes as markup) or artifact
  RPC semantics is proposed here.
