# Worker 0159 — `crates/app/src/account_export.rs` — architecture proposal (v0.4.0)

- Lens: architecture. Requirement: REQ-044 (export / import / delete account), buffered in this wave.
- Status: PROPOSAL only. No product code changed; read-only except this file.

## What I inspected (CONFIRMED)
- Full `crates/app/src/account_export.rs` (741 lines), including the module doc, budgets and helpers.
- `crates/app/src/ports.rs:676-739` (`ExportBundle`, `ExportRow`, `AccountExportStore`) and
  `ports.rs:3019-3042` (`ArtifactContentStore`).
- Callers: `crates/api/src/rpc.rs:3485-3546`, `public.rs:1367-1394`, `router.rs:99-105`.
- `spec/01-requirements.md:356-363`, `spec/02-data-model.md:170-176`,
  `spec/04-nonfunctional.md:138-139`, `spec/golden/export-start.json:17`.
- Two sibling wave-9 architecture briefs for house format. Forward-looking items are GUESS.

## Current archive structure (CONFIRMED)
- One in-memory pass: `load_artifact_bytes` awaits each blob serially into `Vec<Vec<u8>>`,
  `fit_artifact_payloads` copies it into a second `Vec`, `render_zip` writes the whole zip into a
  `Cursor<Vec<u8>>`. Peak ~= 2x artifact bytes + the zip, all on the request task; the 32 MiB
  budget is checked only *after* bytes are in memory.
- Entries are flat JSON (`profile.json`, `bots.json`, `threads.json`, `messages.json`,
  `memories.json`, `routines.json`, `artifacts.json`) plus `artifacts/{storage_key}` payload files;
  `manifest.json` carries `{version:1, exportedAt, userId, counts{...}, skipped}` in camelCase.
- The spec describes the layout as `bots/`, `threads/`, `messages/`, `routines/`, `memory/`,
  `artifacts/` entries referenced by the manifest. Flat JSON vs directories, and `memories.json`
  vs `memory/`, are an unresolved divergence, not a tested choice (CONFIRMED as text difference).
- **Writer/reader disagree on the payload path**: `render_zip` writes `artifacts/<storage_key>`,
  but `allowed_entry` whitelists only exact top-level names, so `read_archive` refuses any
  `artifacts/...` entry as "not part of an account export archive" — an export carrying artifact
  bytes cannot be imported at all (CONFIRMED by code).
- `allowed_entry` re-lists the names `render_zip` writes; there is no shared layout type.
  `IMPORT_BYTE_BUDGET` is declared but unused, while `router.rs:105` hardcodes the same
  33 MiB limit as a literal (CONFIRMED by grep).
- `export.start` promises rendering "on the next status/dump", but `status` only rebuilds a
  *live* bundle for counts and returns the stored `row.status`; `mark_ready` and rendering happen
  only in `download`. Status counts can drift from the frozen manifest written at download.
- `account_import` is a 176-line function (`#[allow(clippy::too_many_lines)]`) calling
  `usecases::{create_bot, memory_write, routines_create}` and stores directly. It parses
  `threads.json` into `_threads` and drops it; every message is inserted into
  `thread_of_bot.values().next()` regardless of source bot; artifacts are never imported and never
  named in `refused`, despite the module's "None of them is silent" contract.

## Guiding principle for v0.4.0
Make the archive a versioned schema with one owner: a buffered render bounded in memory, a layout
shared by writer and reader, and a snapshot whose status and bytes agree. Keep wire names,
`version: 1` and refusal strings byte-stable unless a DEVIATION is approved (AGENTS.md).

## Upgrades (5, with risks)

### U1 — Buffered, bounded render (core "buffered" item)
Replace `Cursor<Vec<u8>>` + double-cloned payloads with a streaming writer into a buffered sink
(`spawn_blocking` for the sync `zip::ZipWriter`, or `tokio_util::io::SyncIoBridge`): stream one
artifact blob at a time, apply entry/archive budgets before writing, and hold O(one entry) of
artifact bytes plus a small zip buffer. `download` returns bytes or a blob key from the same
pipeline; no second full copy.
*Risk:* zip container bytes may change (ordering/metadata) and blob storage adds a dependency.
Mitigate with an entry-name-to-bytes golden (not raw container bytes), `spawn_blocking` cancellation
tests, `cargo add` + docs.rs verification and a `DEPENDENCIES.md` entry; treat any public format
change as a DEVIATION.

### U2 — One `ArchiveLayout` as the single source of truth
A typed module (section names, `artifacts/<key>` path builder/parser, `allowed_entry`, budget
constants) used by both `render_zip` and `read_archive`; make the import path a `ArchivePath` newtype
that cannot be constructed outside it. Use `IMPORT_BYTE_BUDGET` in the router instead of the
33 MiB literal. This also fixes the `artifacts/...` refusal bug.
*Risk:* the layout is externally visible. Decide first whether spec directories or current flat
JSON win: write the conflict in `docs/STATE.md`, pick the safe default, and never change entry
names silently; a real layout change needs `version: 2` plus a v1-compatible reader.

### U3 — Split the render into pure, testable stages
Extract `build_manifest(bundle, at, skipped) -> ExportManifest` and a section serializer from the
55-line `render_zip`; derive the manifest/counts from one `counts_of` call so status and archive use
the same code. Keep the public structs, camelCase keys and `skipped` wording unchanged.
*Risk:* low, but duplicating counts/manifest in two places again would silently reintroduce drift —
lock it with a round-trip test asserting manifest counts equal the entries actually written.

### U4 — One coherent snapshot contract for start/status/download
Choose and document one semantics: render once on first download, persist the buffered bytes, then
serve status (snapshot counts + `ready`) and later downloads from that same artifact, with a
retention/expiry policy because archives hold personal data. Add a narrow store port rather than
reusing `AccountExportStore::bundle` for status.
*Risk:* new store port and retention policy are design decisions (ADR + `STATE.md`); making status
report frozen instead of live counts is wire-visible — record as DEVIATION and wait for approval.

### U5 — Decouple import and restore per-section routing
Turn the monolith into an `ImportPlan`/`ImportReport` pipeline: a typed id map, one small importer
per section over narrow ports (wrapping the existing usecases), each message routed to its source
bot's *new* thread, threads and artifact sections either imported or explicitly named in `refused`.
Keep `ImportOutcome` and best-effort semantics.
*Risk:* message routing and a longer `refused` list change observable behavior (DEVIATION + golden
tests); correct thread routing may need a source-thread id the flat format lacks — split "fix with
v1 data" from "needs manifest v2" and ask before the latter.

## Suggested order (small steps, test-first)
U2 → U3 (layout/stage refactors, parity-locked) → U1 (buffered render) → U4 → U5 (behavior, needs
operator sign-off). Each step: `cargo fmt --check`, `clippy -D warnings`, REQ-044 integration tests.

## Verification for v0.4.0
- `fn req_044_export_payload_round_trips()` — export with artifacts then import; currently fails on
  the `artifacts/...` refusal (CONFIRMED defect, not yet a run test).
- `fn req_044_manifest_counts_match_written_entries()` and `fn req_044_status_matches_download_snapshot()`.
- `fn req_044_import_routes_messages_to_source_bot_thread()` and a `refused` completeness test.
- Property/fuzz test for the external-format path parser per AGENTS.md; golden `export-start` unchanged.
- All "current" claims above CONFIRMED from named files; forward items GUESS until implemented.
