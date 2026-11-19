# v0.4.0 reliability proposal — `crates/app/src/account_export.rs` (module 160, lens: reliability)

Scope: REQ-044 (export / import / delete). Status tags follow AGENTS.md: **CONFIRMED** = verified by
reading this file and its ports this session; **GUESS** = inferred, needs a command before shipping.

## 1. Failure modes

### FM-1 — Secret / data leakage
- **CONFIRMED** `render_zip` (189-218) serializes `routines.json`, `memories.json` and `messages.json`
  verbatim. Routine `prompt`, `email_senders` and memory content routinely carry credentials; the
  zip is a portable plaintext artifact handed to the browser. REQ-046 says stored bot secrets are
  "never shown again" — confirm whether any secret field is reachable through `ExportBundle`
  (ports.rs:682-697) before claiming parity. **GUESS** on secret reachability.
- **CONFIRMED** Artifact entry names are `format!("artifacts/{}", artifact.storage_key)` (214-217)
  with no validation. A `storage_key` containing `../` or a leading `/` produces a zip-slip entry:
  third-party unzip tools can write outside the extraction root, and the importer's `allowed_entry`
  (463-478) refuses the archive on round-trip.
- **CONFIRMED** Client-facing strings embed raw errors and identifiers: `refused.push(format!("bots/{}: {}", bot.id, error))`
  (584, 614, 646, 692) and `skipped` entries with `artifact.id` (153, 158, 242). This leaks internal
  error text into the API and into the manifest/DB, against the "never leak internals" rule.
- **CONFIRMED** The import trusts `author_user_id`, `reply_to_message_id` and `reply_quote` straight
  from the archive (680-684). A crafted archive can attribute messages to arbitrary users.
- **GUESS** `download_url` is a stable, non-expiring `/api/v1/me/exports/{id}` capability (301); reads
  are scoped by `find(id, &user_id)` (317-322), so exploitation needs an auth gap, but there is no
  per-download audit event and no rate limit.

### FM-2 — Partial export / inconsistent state
- **CONFIRMED** Budget refusals (`fit_artifact_payloads` 143-166, `load_artifact_bytes` 227-252)
  produce a `ready` archive that is silently incomplete except for `skipped`; the user cannot tell
  a complete export from a truncated one. GDPR/portability exposure.
- **CONFIRMED** One transient non-`NotFound` storage error aborts the *whole* export (248) rather
  than skipping that one blob.
- **CONFIRMED** `mark_ready` runs after bytes are already returned (333-344): if it fails the client
  has the archive but the row stays `pending`. No CAS, so concurrent downloads both render.
- **CONFIRMED** `exported_at` is taken at download time (325) and `status` re-renders the bundle on
  every call (296). Repeated downloads of one id yield different bytes and different counts as the
  source data changes — no snapshot at `start`, no idempotency.
- **CONFIRMED** Import is best-effort and non-transactional: a mid-loop hard error leaves previously
  created bots/memories/routines in place. All messages go to one arbitrary thread via
  `thread_of_bot.values().next()` (662), and old `reply_to_message_id` values are never remapped.
- **CONFIRMED** The doc comment promises "Artifacts only import when they fit" (534) but
  `account_import` writes no artifact bytes at all — docs/code drift.

### FM-3 — Memory growth on the buffered archive
- **CONFIRMED** `ArtifactContentStore::read` (ports.rs:3034) returns an unbounded `Vec<u8>`;
  `load_artifact_bytes` reads **every** blob fully before any budget check (233-240). One oversized
  blob allocates unbounded memory before `fit_artifact_payloads` rejects it — the 8/32 MiB caps are
  applied too late.
- **CONFIRMED** Up to four live copies per artifact: `artifact_bytes`, the `bytes[index].clone()` in
  `fit_artifact_payloads` (163), the `unwrap_or_default` pass (332), and the `ZipWriter<Cursor<Vec<u8>>>`
  that accumulates the entire zip in RAM (175, 219-223) before the final `Vec<u8>` is returned.
- **CONFIRMED** `render_zip`, `serialize_json` and zip I/O are CPU/allocation-heavy work executed
  inline on the async runtime (169-224) — blocks Tokio workers, against AGENTS.md.
- **CONFIRMED** `read_archive` checks per-entry size (500) but has no aggregate decompressed cap and
  no entry-count cap; many compressed 8 MiB entries exhaust RAM despite `IMPORT_BYTE_BUDGET`.
- **CONFIRMED** `to_vec_pretty` (127) inflates JSON and wastes the byte budget.

## 2. Concrete hardening
1. **Bound the read, not the result.** Add `read_limited(key, max)` / `metadata` to
   `ArtifactContentStore`; in `load_artifact_bytes` accumulate a running total and skip oversized
   blobs *before* allocation. Hard-cap artifact count. Removes the FM-3 precondition for FM-2 skips.
2. **Stream the archive.** Write to `ZipWriter<File>` in a 0600 temp file under a dedicated,
   size-quota'd directory; return a stream/path and delete on TTL. Do rendering and compression in
   `spawn_blocking` with a bounded concurrency semaphore. Use `to_vec` (compact JSON) for sections.
3. **Freeze the snapshot.** Persist `exported_at`, `counts` and the payload at first successful
   render; make `mark_ready` a compare-and-set; serve identical bytes on repeat downloads and have
   `status` read the stored counts instead of re-rendering.
4. **Safe entry names.** Derive artifact names from an index or a validated hash
   (`artifacts/{index}`), and reuse the importer's component rules to validate every name before
   writing. Never interpolate `storage_key` into a path.
5. **Stable errors.** Map `AppError` to a small fixed code + short public message in `refused`/
   `skipped`; log the detail with `tracing` and the request id. Never put ids or space names in
   client strings.
6. **Trust boundary on import.** Ignore archive-supplied `author_user_id`, `reply_to_message_id`
   and `reply_quote` (set to caller/none); remap replies through an old→new id map; route each
   message to its own bot's thread; wrap writes in one transaction (or roll back created rows on a
   hard failure) and reconcile the artifact wording in the docs.
7. **Import bomb guard.** Sum `entry.size()` against a decompressed ceiling (e.g. 64 MiB) and a max
   entry count in `read_archive`, in addition to `IMPORT_BYTE_BUDGET`.
8. **Secret hygiene + audit.** Explicitly decide and document whether prompts/memory are redacted
   (pattern pass + user confirmation) or exported as-is; add an audit event and rate limit on every
   download, with a short-lived signed URL if the API layer allows it.

## 3. Risks / deviations
- R1 — Streaming to a temp file changes the `download` response shape: **DEVIATION**, needs owner
  approval (docs/PARITY.md). The in-memory fallback only cuts peak from ~4× to ~2×.
- R2 — Freezing counts/bytes at first render changes current fresh-render behavior: **DEVIATION**;
  record it and update the golden tests.
- R3 — Dropping author/reply fields changes imported data: **DEVIATION** with a parity note.
- R4 — Earlier oversize rejection changes `skipped` contents and count golden files.
- R5 — Temp files add disk quota, cleanup, permissions and secure-deletion obligations.
- R6 — `spawn_blocking` can starve the pool without a semaphore bound; cap and benchmark.
- R7 — Redaction may remove data a user legitimately exported; prefer confirmation over silent
  mutation until REQ-046 scope is confirmed.

## 4. Acceptance tests (REQ id in name)
- `req_044_export_oversized_blob_never_allocates_past_budget`
- `req_044_export_entry_names_are_zip_slip_free`
- `req_044_export_download_is_idempotent_and_snapshot_frozen`
- `req_044_export_errors_are_stable_codes_with_no_internal_text`
- `req_044_import_decompression_bomb_is_rejected`
- `req_044_import_remaps_replies_and_preserves_authorship`

Mark R1-R3 as DEVIATION in docs/PARITY.md and wait for approval before shipping them.
