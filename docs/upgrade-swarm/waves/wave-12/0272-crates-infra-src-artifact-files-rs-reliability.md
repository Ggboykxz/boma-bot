# artifact_files.rs — reliability hardening proposal (v0.4.0)

Worker #272 · MODULE `crates/infra/src/artifact_files.rs` · LENS reliability · READ-ONLY audit.
Claims read from the tree this session are CONFIRMED; inferred impacts/proposals are GUESS.

## Current state (CONFIRMED)

- `new` does blocking `std::fs::create_dir_all` on a caller-supplied root; it logs a warning only
  there and stores the path verbatim — no canonicalisation, no writability check.
- `path_for` accepts only `sha256:<64 ascii hex>`, then splits the first two hex chars into a
  directory. Traversal via the *key* (`/`, `..`, NUL) is already well defended.
- `put` does a blocking `Path::exists()` inside the async fn, writes to a UUID-named sibling
  `.boma-tmp-<uuid>`, then `fs::rename`s onto the final path. All I/O errors collapse to
  `AppError::Unavailable` with the `io::Error` discarded and no log line.
- `read` maps *every* error to `Rule(InvalidName("the artifact bytes are missing"))`, returns an
  unbounded `Vec<u8>`, and never re-hashes the bytes against the key.
- `remove` deletes only the file, never prunes the empty 2-hex directory, and is called with the
  result discarded at both GC sites (`usecases.rs:1798`, `account_export.rs:436`).
- `storage_key_refs` is checked *before* `remove` in a separate, unlocked call
  (`usecases.rs:1795-1799`). `Unavailable` is the only `is_retryable()` error (`error.rs:50`);
  `Rule(_)` is a `is_client_fault()` error (`error.rs:58`).

## Failure modes

### FM-1 — symlinked / non-canonical root (CONFIRMED mechanism, GUESS impact)

Key validation is strong, but the root is not. If `root/ab` is a symlink outside the store,
`path.exists()` follows it: `put` can report success for a blob never written under the root, and
`read` can return bytes from an arbitrary location. A symlinked `root` is accepted outright.

### FM-2 — partial writes and orphan temp files (CONFIRMED)

Temp-and-rename correctly prevents a half blob at the final path, but any error between temp
creation and rename — failed write, failed rename, cancellation at an `.await`, crash — abandons a
`.boma-tmp-<uuid>` file with no cleanup or sweep. The "no half blob" comment hides a temp leak.

### FM-3 — disk full becomes a self-amplifying retry loop (CONFIRMED mechanism, GUESS impact)

A full disk makes `fs::write` fail with `StorageFull`/`WriteZero`, mapped to retryable `Unavailable`,
so callers retry with backoff. Each retry creates a new temp file; a mid-write failure leaves the
orphan from FM-2 consuming the last free sectors. The loop worsens its own cause, and the real
error is swallowed, so operators get no disk-full signal.

### FM-4 — GC race creates dangling references (CONFIRMED mechanism, GUESS impact)

`storage_key_refs == 0` and `remove` are not atomic. A concurrent identical upload is idempotent:
`put` sees `path.exists()` and returns the key *without rewriting* the blob while a new row
references it. Interleaved between ref check and unlink, the file is deleted under a live row —
a permanently broken artifact. Removal errors are discarded, so this is silent. Two concurrent
puts of the same bytes merely race two renames (benign, bytes identical).

### FM-5 — error classification and observability (CONFIRMED)

Missing blob, permission error, corrupted file and transient I/O fault all surface from `read` as
`Rule(InvalidName)`: a client fault, non-retryable, mapped to a 4xx-style envelope. A deployment
fault is blamed on the caller and never retried, while `put` faults are all retryable and retried.
Every `io::Error` is dropped and only `new` logs; there is no metric for disk-full, orphan temps or
read failures.

### FM-6 — no size bound, no integrity check (CONFIRMED)

`read` trusts file length, loading a corrupted/oversized blob into one unbounded `Vec<u8>` (same
class as wave-9 worker #160), and never verifies `sha256(bytes) == key`. Bit rot or an accidental
overwrite is served as valid content; the content-addressed contract is assumed, not checked.

### FM-7 — blocking and cancellation hygiene (CONFIRMED)

`Path::exists()` is a blocking stat in an async fn, against the workspace no-blocking rule. The
temp lifecycle is not cancellation-safe: dropping the future mid-write leaks the temp file (FM-2).

## Concrete hardening (ordered, small steps)

1. Canonicalise the root in `new`; fail fast if it is a symlink or unwritable (probe write+remove).
2. In `path_for`/`put`/`read`, use `symlink_metadata` to refuse any symlink component in the path
   and re-canonicalise after creation, asserting the result stays under the root. (FM-1)
3. Replace `path.exists()` with `fs::try_exists(&path).await` (or skip the check and let rename win):
   removes the blocking call and narrows the TOCTOU window. (FM-7)
4. Wrap the temp path in a drop guard that unlinks on any error/cancellation, and add a bounded
   startup sweep of `.boma-tmp-*` files older than a configured age. (FM-2)
5. Classify disk-full explicitly: match `StorageFull`/`WriteZero`/`PermissionDenied`, clean the temp
   file, log the real `io::Error`, emit a `disk_full` counter, and map to a non-retryable error so
   callers stop looping. (FM-3, FM-5)
6. Fix `read`/`remove` mapping: `NotFound` → `AppError::NotFound`, permission/I/O → `Unavailable`;
   reserve `Rule(InvalidName)` for a malformed key from `path_for`. Trace the source error. (FM-5)
7. Stat the file, enforce the upload max-blob size, stream into a capped buffer, and re-hash to
   confirm the key — a mismatch is corruption (`Unavailable` + alert), not success. (FM-6)
8. Make the GC check-and-unlink atomic (process-wide lock keyed by storage key), and have `put`
   rewrite the blob when the final path is missing rather than trusting a prior `exists()`. Stop
   discarding `remove` results; log failures. (FM-4)
9. Traceable tests: `req_036_rejects_symlinked_blob_path`, `req_036_temp_removed_on_rename_failure`,
   `req_036_disk_full_not_retryable`, `req_036_read_maps_not_found`, `req_036_read_rejects_corrupt_digest`,
   `req_036_read_enforces_size_limit`, `req_036_gc_keeps_referenced_blob`, plus a cancellation test
   asserting no temp files survive a dropped `put`.

## Risks and mitigations

- Step 7 adds a full SHA-256 pass per read; benchmark read-heavy paths and consider making
  verification per-deployment opt-in if it breaches a latency budget.
- Steps 1-2 may reject a deployment that intentionally uses a symlinked root — call out as an
  operational DEVIATION and verify the configured root before enabling.
- A typed storage error touches `boma_app::AppError` and the API mapping (outside this module);
  coordinate with app/api owners and record the status-code change in `docs/PARITY.md`.
- Step 8's process lock only helps within one process; multi-instance deployments need a DB-backed
  claim or an age-thresholded sweep (GUESS — confirm topology). The startup sweep must not delete a
  temp file another process is writing: use an age threshold and per-process temp prefixes.

## Recommendation

Do steps 3, 4, 6 and 9 first — low risk, high payoff, no external behaviour change. Treat steps 1,
2, 5 and 7 as a gated batch (fail-fast root, error taxonomy, integrity) with a DEVIATION/ADR for the
new status codes. Step 8 needs the deployment-topology answer before choosing an in-process lock
vs. a GC sweep.
