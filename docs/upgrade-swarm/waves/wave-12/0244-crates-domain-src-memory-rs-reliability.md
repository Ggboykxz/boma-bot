# Worker #244 — `crates/domain/src/memory.rs` — reliability lens — v0.4.0 proposal

## Scope and status

Read-only review of `crates/domain/src/memory.rs` (156 lines) plus its direct consumers
`crates/app/src/ports.rs:1267-1277`, `crates/app/src/usecases.rs:4874-4905`,
`crates/infra/src/stores/memory.rs:16-104`, `crates/api/src/wire.rs:255-291`,
`spec/01-requirements.md:179-185`, `spec/02-data-model.md:89-98` and
`spec/golden/memory-write.json`. No code was changed. All claims below are CONFIRMED from
those files unless marked GUESS.

## What the module does today

`memory.rs` is pure data plus two validators: `MemoryScope` (`bot`/`shared`) with `as_str()` and
`FromStr`; `MAX_MEMORY_PATH_LENGTH = 512`; `validate_memory_path()` (relative POSIX path, no
`.`/`..`/empty segments, no backslash or control characters); and `MemoryDocument` with public
fields `space_id`, `bot_id: Option<BotId>`, `path`, `content`, `revision: u64`, `updated_at:
String`. `scope()` derives the scope from `bot_id.is_some()` (lines 104-113): there is no stored
`scope` field, no constructor and no invariant check on the struct itself. Wire-level scope
consistency is enforced one layer up in `resolve_memory_scope` (usecases.rs:4881-4905). The store
increments `revision` unconditionally on every write and appends one `memory_revisions` copy of the
content in the same transaction (infra/stores/memory.rs:63-104); `updated_at` is a caller-supplied
string from the application clock (ports.rs:1276-1277), cast to `timestamptz` in SQL and rendered
back via `boma.rfc3339`.

## Failure modes

1. **Stale memory.** The header promises content is "the projection of the newest revision"
   (lines 3-4), but nothing in the type keeps that promise. `revision` and `updated_at` are public
   and unvalidated: a row or test can carry revision 0, a decreased revision, a content/revision
   mismatch, or an empty/garbage timestamp. Freshness cannot be judged by time because
   `updated_at` is free-form text from the calling process's clock: clock skew or an out-of-order
   write yields a newer revision with an older timestamp, and string ordering is only meaningful
   if every producer emits canonical RFC3339 (the golden does; nothing enforces it). There is no
   expected-revision / compare-and-set helper in the domain, so a read-then-write can silently
   clobber a concurrent update: the SQL `ON CONFLICT ... DO UPDATE` blindly computes
   `revision + 1`. GUESS: no caller currently supplies an expected revision, so lost updates are
   possible for concurrent writes to the same (space, scope, bot, path).
2. **Unbounded growth.** Only `path` is length-capped (512 bytes, lines 49-50); `content` and
   `updated_at` are unconstrained `String`s. A single write can therefore put arbitrarily large
   content into both `memory_documents` and the append-only `memory_revisions` row, with no
   domain-level body limit. Every write stores a *second* full copy of the content as history
   (infra/stores/memory.rs:93-104), so storage grows without bound and without a retention or
   compaction rule; no document-count or per-space quota exists at this layer.
   `MAX_MEMORY_PATH_LENGTH` counts bytes, not characters, and there is no cap on segment count or
   nesting depth, so deep paths can still stress downstream consumers. `revision: u64` has no
   checked increment; the SQL side eventually errors on bigint overflow instead of failing
   gracefully. GUESS: the DB column is unbounded `text` and no service-level quota is configured.
3. **Dedup.** Repeated content is never detected. Two identical writes bump `revision` twice and
   append two identical revision rows; derived `PartialEq`/`Eq` include `revision` and
   `updated_at`, so they cannot serve as content-based dedup. There is no content hash, no
   idempotency key and no no-op-write rule, so a bot re-saving the same note inflates history and
   advances `updated_at` for no semantic change. The deterministic `document_id`
   (infra/stores/memory.rs:17-19) gives stable document identity but excludes content, so it does
   not help. GUESS: `:` inside `path` is not escaped in `document_id`, so two distinct tuples could
   theoretically collide on one id.

## Concrete hardening for v0.4.0

- **H1 — typed freshness, wire-compatible.** Keep `updated_at: String` for serde parity with the
  golden (`"2026-01-01T00:01:00Z"`), but add `validate_updated_at(&str)` (canonical RFC3339 UTC)
  and `MemoryDocument::is_fresher_than(&self, &other)`. Add
  `MemoryDocument::is_stale(&self, expected_revision: u64) -> bool` so the app layer can express
  optimistic concurrency with a domain-owned rule. Pure functions, no new dependency, no wire
  change.
- **H2 — checked revisions and dedup primitives.** Add `checked_next_revision()` using
  `u64::checked_add` with a typed `DomainError` (coordinate a dedicated variant with the error.rs
  wave). Add `content_hash(&self) -> [u8; 32]` over `content` only via `sha2::Sha256` (record name,
  version and license in `docs/DEPENDENCIES.md`; add with `cargo add`). Add
  `should_append_revision(current, new) -> bool`, false when hashes match, making the no-op
  decision a testable domain rule; app/infra then choose whether to skip the write.
- **H3 — bounds and a single constructor.** Introduce `MAX_MEMORY_CONTENT_LENGTH` and
  `validate_memory_content()`, plus `MemoryDocument::new(...) -> Result<Self, DomainError>` that
  validates path, content, timestamp and the scope/bot_id invariant (`shared` implies
  `bot_id.is_none()`) at one choke point. Keep fields public in v0.4.0 to avoid a source break;
  add `#[must_use]` helpers first and consider private fields plus accessors in a later major
  version.
- **H4 — tests.** proptest that `validate_memory_path` never accepts an escaping path, plus tests
  for content bounds, hash determinism, checked overflow, `is_stale` / `should_append_revision`
  truth tables and `new()` refusing a shared document with a bot id. Carry REQ-022 in test names
  per AGENTS.md.
- **H5 — policy decisions, not silent guesses.** Decide whether no-op writes should be suppressed
  (externally visible revision numbers — likely a DEVIATION) and whether revision history needs a
  cap or TTL. Record both in `docs/STATE.md` under "Decisions needed" and any behavior change in
  `docs/PARITY.md` before touching infra.

## Risks and mitigations

- **Wire/format break:** typing `updated_at` or storing a `scope`/hash field changes serde output
  and breaks `golden/memory-write.json`. Mitigation: H1-H3 keep the wire shape byte-identical; any
  new field goes behind an approved DEVIATION.
- **Dedup semantics:** skipping no-op writes stops revision numbers advancing, which the golden
  and possibly clients read as a write acknowledgement. Mitigation: land the pure helper only;
  enable it in infra after parity approval, or keep appending history and suppress only the
  projection rewrite.
- **Cap choice:** a content limit may reject a legitimate large memory (parity regression).
  Mitigation: pick a generous default, make it configurable, and mark the value GUESS until the
  original's real limit is measured.
- **False safety:** `is_stale` helps only if the store really performs a compare-and-set.
  Mitigation: pair H1 with a contract test proving a stale expected revision is refused; otherwise
  document it as advisory only.
- **Churn:** tightening construction affects testkit/conformance code that builds `MemoryDocument`
  literally (`crates/testkit/src/memory.rs`, `crates/conformance/src/golden_world.rs`). Mitigation:
  keep fields public and add validation as helpers before any constructor-only enforcement.
