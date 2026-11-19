# Worker 0271 — `crates/infra/src/artifact_files.rs` architecture proposal (v0.4.0)

Lens: architecture. Scope: read-only review of the 166-line file plus targeted reads of
`ports.rs:3019-3042`, `stores/artifacts.rs`, `usecases.rs:1636-1681,1793-1798`,
`migrations/0036_artifacts.sql`, `docs/adr/022-artifacts-pages-board.md` and sibling wave briefs.
No product code changed, no tests run (read-only assignment). "Current" claims are **CONFIRMED by
source inspection**; forward-looking items are GUESS.

## Current storage structure (CONFIRMED)

- One struct, `FileArtifactContentStore { root: PathBuf }`, implementing the `ArtifactContentStore`
  port (`put`/`read`/`remove`). The on-disk layout is hard-coded inside the private `path_for`:
  `root/<first 2 hex of sha256>/<remaining 62 hex>`, key `sha256:<64 hex>`.
- `path_for` (`:39-50`) is both the validator and the path builder: it strips `sha256:`, checks
  64 ASCII-hex digits, splits at 2. The hex-only alphabet is what makes traversal (`../../etc/passwd`)
  impossible — a genuinely good boundary (test `d03_...` at `:157` confirms rejection).
- `put` (`:55-88`) hashes bytes with `Sha256`, formats hex by hand (64 `write!` calls), checks
  `path.exists()` (`:63`), then writes a `.boma-tmp-<uuid>` sibling and `fs::rename`s it into place.
  Idempotence is by content, so a duplicate upload is a no-op *if* the file exists.
- `read` (`:90-97`) materialises the whole blob into `Vec<u8>` and does **no hash verification**;
  a corrupted/truncated final-name blob is served as valid. `remove` (`:99-106`) is best-effort —
  `NotFound` is success, all other IO errors become `AppError::Unavailable`.
- Error taxonomy is inconsistent: `new` logs a `tracing::warn!` on failure (`:32`), but every other
  IO failure is a silent `Unavailable`; `read` maps a missing blob to
  `AppError::Rule(DomainError::InvalidName("the artifact bytes are missing"))` (`:93`), conflating
  storage absence with bad client input. No `tracing` anywhere in `put`/`read`/`remove`.
- Lifecycle is split across crates: `usecases.rs:1663` writes the blob **before** the DB row insert
  at `:1680` (failed insert → orphan blob); family delete (`:1793-1798`) rechecks `storage_key_refs`
  then best-effort-removes each blob. ADR-022 names cross-space orphan GC as a known M11 gap.
- Two unit tests only (`:109-165`): round-trip/idempotence and bad-key rejection. Temp dirs are
  `std::env::temp_dir()` + PID (`:114-118`) — no random suffix, cleanup only on the happy path.
  No concurrency, crash-leftover, permission, or property/fuzz coverage.
- `Path::exists()` at `:63` is a synchronous `stat` on the async task (AGENTS.md: no blocking IO on
  the runtime). `fs::rename` onto an existing destination is Unix-atomic but fails on Windows.

## Target structure for v0.4.0

Keep the store IO-bound and the two-level fan-out (it is parity-visible via `storage_key`), but give
the layout one owner, make writes crash-safe and observable, and make the blob lifecycle explicit
across the port/store boundary:

```
infra/artifact_files.rs   StoreLayout { root, fanout } + FileArtifactContentStore
app/ports.rs              StorageKey newtype; ArtifactContentStore (+ streaming methods)
infra/stores/artifacts.rs DB-side reference count is the authority for reclamation
app/account_export.rs     reuse StorageKey/StoreLayout path builder (no ad-hoc path strings)
```

## Concrete upgrades (with risks)

### U1 — One `StorageKey` + `StoreLayout` owns the on-disk layout
Move validation and path building into a `StorageKey` newtype (`try_from(&str)`, `hex()`, `key()`)
and a `StoreLayout { root, fanout: 2 }` with `path_for(&StorageKey)`. `path_for` becomes a pure
function that cannot produce a path outside `root`, and the same builder is reused by account
export's `artifacts/<key>` entry (wave-9/0159 found the writer/reader disagree and a raw key allows
zip-slip) and by the reclamation sweep. Keep `sha256:<64 hex>` and the 2-char fan-out byte-stable.
*Risk:* the layout is externally visible (a deployed volume already holds blobs); changing fan-out
or prefix requires a one-off rehome or a dual-read shim. No wire change if the key format is frozen —
otherwise it is a DEVIATION (`docs/PARITY.md` + approval).

### U2 — Crash-safe, observable writes
Drop the blocking `path.exists()` stat: attempt the temp write + `fs::rename` directly (rename is
idempotent by content on Unix) and treat `AlreadyExists` as success at the temp stage. `sync_all`
the temp file before rename, and sweep stale `.boma-tmp-*` at `new`/startup. Add `tracing::warn!`
with key/root context to every IO failure and map missing blobs in `read` to `AppError::NotFound`,
not `InvalidName`.
*Risk:* per-put `sync_all` costs latency on low-end disks — benchmark before enabling. Blocking
sync must run in `spawn_blocking` (or accept a documented durability gap instead); Windows rename
onto an existing file needs an explicit overwrite path. Error-string changes are wire-visible only
if clients branch on them — verify against the REQ-036 goldens.

### U3 — Bound memory at the store boundary
The port's `&[u8]` → `Vec<u8>` shape forces each blob fully into memory (API caps at 4 MiB, but N
concurrent uploads = N buffers). Add streaming `put_reader`/`read_to_writer` (Tokio `AsyncRead`/
`AsyncWrite`) with the 4 MiB cap enforced *in the store*, and keep the existing byte methods as thin
buffer adapters so no caller/test double breaks. Hash incrementally while streaming.
*Risk:* port change touches every `ArtifactContentStore` impl (in-memory test doubles included) and
`usecases.rs`/`account_export.rs` call sites — land as one mechanical commit with default methods.
Changing the cap or partial-write semantics is a DEVIATION; keep the REQ-036 artifact reply
metadata-only.

### U4 — Make the blob lifecycle explicit (write-once + recheck + repair)
Document and enforce the contract: blobs are write-once, the DB row is authoritative, and reclamation
is a sweep that rechecks `storage_key_refs` under the same transaction that deletes the row (fixing
the write-before-insert orphan at `usecases.rs:1663` and the best-effort delete window). Persist a
pending-reclamation row on delete failure (wave-10/0186 item 4), and give `read` a repair path when a
referenced blob is missing.
*Risk:* a concurrent `put` can reference a key an old sweep is about to delete — requires a generational
or "recently written" guard and a new table/migration. This is cross-crate work already scoped in
wave 10; coordinate rather than duplicate, and do not delete a blob a live row may reference.

### U5 — Integrity verification and real tests
Verify the sha256 on `read` (or, to protect low-end latency, on a periodic audit sweep) so bit rot
and torn writes are detected rather than served. Add a proptest for `StorageKey` parse/format
round-trip and path containment, plus tests for concurrent identical puts, a crash-leftover temp
file, and a permission-denied write; use unique temp dirs (PID + random) with `Drop` cleanup.
*Risk:* read-time hashing is O(blob) CPU on every download — prefer audit-mode and benchmark; keeping
`read` unbounded is a known debt. proptest/fuzz are dev-only (no runtime cost), but AGENTS.md requires
fuzz for external-format parsers and REQ ids in test names — `tools/trace_check.py` counts declaration
lines, so never weaken an existing test.

## Sequencing / cross-cutting risks

1. U1 first (pure type/layout refactor, parity-locked); it unblocks the account-export path fix.
2. U2 next (write safety + observability); U5's tests can ride alongside to lock behavior.
3. U3 then U4 — both touch the port and multiple crates; land each as one commit, keep idempotence.
4. After every step: `cargo fmt --check`, `clippy -D warnings`, REQ-036 integration/trace tests.
5. No UI/i18n impact. Every "current" claim above is CONFIRMED by named source only; re-run the
   suite in the implementing session before flipping any status to verified-by-test.
