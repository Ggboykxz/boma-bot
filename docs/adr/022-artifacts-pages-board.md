# ADR-022 — Artifacts, shared pages, board (M6: REQ-036/037/038/039, D-03)

Status: Accepted (2026-11-14, decisions recorded by the rebuild; the owner delegated them)

## Context

`spec/01-requirements.md` REQ-036 (artifacts, MUST), REQ-037 (share a page by link, MUST),
REQ-038 (ticket board, COULD, feature-flagged), REQ-039 (bot-built pages; SHOULD), defect D-03
(attachment type trusted from the client). The data model (`spec/02-data-model.md`, "Artifacts,
pages, boards") already names the entities; the goldens pin the wire shape of `artifacts.create`,
`artifacts.listVersions`, `GET /api/v1/pages/shared/:token` (200 with `noindex, nofollow` +
`referrer-policy: no-referrer` when valid; uniform 404 otherwise) and the artifact reply carrying
metadata only.

## Alternatives

1. **Artifact bytes inside Postgres.** Rejected: blobs in the OLTP store undermine the retention
   story (spec §4.6 reclaims agent homes) and bloat every dump. Spec says out-of-band.
2. **Artifact bytes on the local filesystem, content-addressed (chosen).** `storage_key =
   "sha256:<hex>"`; rows reference the hash; blobs are written once (idempotent for identical
   bytes) and removed on family delete when unreferenced. A shared deployment mounts a volume at
   `BOMA_ARTIFACT_DIR`; unset means a per-process ephemeral directory with a startup warning —
   never silently lost between processes.
3. **Version per row, ordered by a sequence (chosen).** `rootArtifactId` groups the family; `version`
   is `max(version in family) + 1`; `artifacts.get` returns the latest by default; versions list is
   newest first (golden). Family delete removes the rows and the unreferenced blobs; a version
   deleted individually is the same operation scoped to the family.
4. **Shared page read is unauthenticated JSON only.** Rejected: REQ-039 demands an interactive page
   in a sandboxed viewer. The same route content-negotiates: `Accept: application/json` → golden
   body; `Accept: text/html` → a static HTML document whose only script is the vendored
   `/api/v1/assets/page-runtime.js`, served with `Content-Security-Policy: default-src 'none';
   script-src 'self'; style-src 'unsafe-inline'; img-src data: https:; connect-src 'none';
   form-action 'none'; base-uri 'none'`. No inline script ever exists in the payload. The app's
   own CSP is untouched (scripts never execute outside the sandbox document).
   Mermaid is **not** vendored in this build; `chart` blocks render as bar charts drawn by the
   runtime from table data. Named gap, not a compromise on the policy.
5. **Bots publish pages over RPC.** Rejected: bots don't hold sessions. Bots use the builtin
   Write-class tool `page.save` (approval-gated exactly like `skill.teach`); sharing is a member
   act (`pages.share` over RPC) — a bot never mints a public link silently.
6. **Board boots with background runs on assignment.** Rejected for M6 (named gap, not a silent
   half-feature): a `ticket` run trigger touches `RunTrigger` exhaustively and the worker claim
   path; it ships with its own design in M11 planning. M6 ships the flagged board: flag off →
   every board route/procedure answers 404 even when signed out (the spec's acceptance); flag
   on → tickets, comments, assignment recorded.

## Decision

- **Artifacts (REQ-036, D-03).** `migrations/0036_artifacts.sql` table: space/bot/group/user/run
  attribution, `name`, `declared_mime`, sniffed `mime_type`, `size`, `hash` (`sha256:<hex>`),
  `root_artifact_id`, `version`, `client_nonce` with a partial unique index on
  `(space_id, user_id, client_nonce)`, `standalone`. Content lives out of band.
  **D-03 policy:** bytes are sniffed with magic bytes (PNG/JPEG/GIF/PDF/ZIP; HTML detection by
  doctype/`<html`/`<head`/`<script` in the first 4 KiB; UTF-8 text otherwise). Mismatch rules:
  *declared specific, sniffed specific different → rejected; declared generic
  (`application/octet-stream`) → corrected to sniffed; declared text, bytes are HTML → rejected;
  declared anything, sniff fails → rejected.* The stored `mime_type` is always the sniffed one.
- **Versions.** `artifacts.create` accepts optional `rootArtifactId`; a re-upload of bytes for a
  known root mints `max(version)+1`; the create reply carries metadata only (golden); versions
  list is newest first; `artifacts.remove` deletes the family.
- **Pages (REQ-037, REQ-039).** `migrations/0037_pages.sql`: `pages` (space, bot, token hash,
  visibility, current version id) + `page_versions` (immutable `source` JSON: `title` + `blocks`,
  `version`, `compiled_at`). Public read at `GET /api/v1/pages/shared/:token`: matched by token
  hash, never by id; `cache-control: no-store`, `x-robots-tag: noindex, nofollow`,
  `referrer-policy: no-referrer`; body is either the golden JSON (`accept: application/json`) or
  the sandboxed HTML document; unknown/revoked/malformed share one 404 body (`Not found.`) and
  the same headers set. 120/min/IP through the D-01 counter. `pages.share` mints a 256-bit
  URL-safe token; only the hash is stored. `pages.revokeShare` returns the row to private.
- **Board (REQ-038).** `migrations/0038_tickets.sql`: `tickets` (space, key, title, description,
  state, assignee bot, created_by) + `ticket_comments`. `BOMA_BOARD` unset/`false` (default)
  removes the surface entirely — `boards.get` and `tickets.*` answer 404, including when signed
  in; the route absent, not locked. `true` enables `boards.get`, `tickets.create|move|comment`;
  assignment records the bot; comments carry author (member or bot). Named gap: assignment does
  **not** queue a background run yet (needs a `ticket` run trigger; M11 planning owns it).
- **Bot tools.** `artifact.create` and `page.save` are builtin Write-class tools: dispatch returns
  `{ approval: "required" }`, the approval gate applies, `execute_approved_tool` runs them, and
  effects are exactly-once via the existing `pending_tool_calls` machinery.
- **Storage guard.** Uploads cap at 4 MiB raw (`contentBase64`, ≈5.46 MB base64); per-procedure
  body caps stay per route (D-07). `contentBase64` that does not decode is a 400 before any byte
  touches disk or the vault.

## Consequences

- An artifact is never stored under a lie: the type a space can trust is the sniffed type, and a
  declared/actual mismatch is loud (rejected at upload), never silent.
- A shared page link is a capability: guessability is 2^-256 by construction, revocation is one
  row flip, and the public read leaks nothing (uniform 404, `no-store`, `noindex`).
- The app never renders page scripts: they execute only inside the sandbox document with a CSP
  that admits nothing but the pinned same-origin runtime.
- The board exists exactly when the operator asks for it; a deployment without the flag has no
  board surface to audit.
- Named, accepted gaps (not hidden): no mermaid vendoring; no `ticket` run trigger (assignment is
  recorded, not executed); artifacts have no cross-space garbage collection of orphaned blobs
  (family delete removes unreferenced blobs; a periodic sweeper is an M11 item).
