# Worker 0268 — `crates/domain/src/thread.rs` — reliability lens (target v0.4.0)

Status: proposal only. All claims below marked CONFIRMED (read this session) or GUESS (inferred).

## Evidence read (CONFIRMED)

- `thread.rs`: `Thread` has 8 public fields; `next_message_seq`/`next_event_seq` are bare `i64`;
  `external: Option<ExternalThread>` is the only "kind" signal in the domain; `created_at: String`;
  no constructor, no invariant method, no `deny_unknown_fields`. `validate_page_size` is the sole guard.
- `ids.rs`: ids are opaque newtypes, `new()` rejects only blank strings.
- `infra/src/stores/rows.rs::thread()`: `external` is derived from the DB `kind` column; missing
  external columns become `unwrap_or_default()` → `""`; unknown platform → `ChannelPlatform::Generic`.
- `infra/src/stores/messages.rs` / `events.rs`: seq allocated in-transaction via
  `UPDATE threads SET next_*_seq = next_*_seq + 1 … RETURNING next_*_seq - 1` (first seq = 0).
- `infra/src/stores/threads.rs::for_bot`: `WHERE bot_id=$1 AND kind='bot' ORDER BY created_at ASC LIMIT 1`.
- Spec `01-requirements.md`: REQ-004 (exactly one visible thread per bot), REQ-005 (monotonic
  per-thread seq, backward paging, stream resume), REQ-020 (event resume), REQ-048 (external threads).

## Failure modes

**FM-1 — Invalid state is representable and silently coerced (High).** The domain cannot express the
`kind` invariant that the DB holds. `kind='external'` with missing workspace/conversation yields an
`ExternalThread` with empty strings (CONFIRMED in `rows.rs`); `kind='bot'` with external data silently
drops it; an unknown `kind` falls back to a bot-like thread. Nothing in the domain rejects
`external=Some` with blank ids, or `next_*_seq < 0`. Deserialization can build any `Thread` directly,
so a misspelled field is accepted rather than rejected (GUESS: API DTOs map into these types).

**FM-2 — Orphan / duplicate threads (High).** `space_id`, `bot_id`, `user_id` are opaque and never
checked for existence at the domain boundary; cross-tenant safety rests entirely on infra RLS
(CONFIRMED: store relies on `tenant::with_tenant`). If two `kind='bot'` rows exist for one bot — racy
creation, bad import, migration — `for_bot` silently returns the oldest, so writes and reads can
diverge into two histories. A thread whose bot/user was deleted is a live orphan that still accepts
messages. REQ-048 threads can orphan when an installation is removed: history is kept by design, but
a blank `conversation_id` makes it undeliverable.

**FM-3 — Ordering / sequence misuse (Medium).** Two independent `i64` counters are publicly mutable;
callers can set, reuse, or skip seqs, and nothing checks `checked_add` overflow or negative cursors.
Message seq and event seq are allocated independently, so cross-stream causality (a `message` event
for message seq N must not be observed before an event for N-1) is not expressed anywhere. `created_at`
is an unparsed `String` used as a tie-breaker in `for_bot` (CONFIRMED), so malformed timestamps from
imports corrupt that ordering.

## Hardening proposal (ordered by value/risk)

**H-1. Checked sequence newtypes.** Add `MessageSeq`/`EventSeq` (private `i64`, `#[serde(transparent)]`
to keep the wire/DB format) with `new` rejecting negatives, `next() -> Result<Self, DomainError>` using
`checked_add`, and `PartialOrd`. Replace the two public fields and add `Thread::allocate_message_seq()`
/ `allocate_event_seq()` returning a reserved seq. Wire format unchanged (CONFIRMED: seq columns are
plain integers); call-site churn in `rows.rs`, `usecases.rs`, `app` ports. Add `DomainError::SequenceExhausted`.
*Risk:* medium churn, low behavioral risk; reserve-then-insert must stay in the infra transaction or a
rolled-back insert can burn a number (a gap is acceptable, a reuse is not — document this).

**H-2. Domain-level kind invariant, without a breaking field change in v0.4.0.** Keep `external:
Option<ExternalThread>` but add `Thread::validate() -> Result<(), DomainError>` enforcing: `Some(e)`
⇒ non-blank `workspace_id`/`conversation_id`; `None` for bot threads. Add `ExternalThread::new()`
checked constructor (blank ids rejected, same rule as `ids.rs`). Call `validate()` in `rows::thread()`
and at every construction site, replacing `unwrap_or_default()` with a typed
`DomainError::InvalidThreadState` (no silent `""`, no silent `Generic`). Treat unknown `kind` as an
error, not a default. *Risk:* unknown column data that previously "worked" now fails reads — intended,
but must be gated by a data-quality check on staging; log the offending thread id, never its payload.

**H-3. Orphan/duplicate defense.** In infra, fetch 2 rows in `for_bot` and return a typed conflict
when >1 bot thread exists instead of `LIMIT 1`. Add a migration with a partial unique index on
`(space_id, bot_id) WHERE kind='bot'`, preceded by a dedupe/repoint migration. In the domain, add
`Thread::belongs_to(&SpaceId, &BotId)` and a `ThreadRef` carrying space/bot/user so the app layer can
assert ownership before appending. *Risk:* the unique index fails on existing duplicates — hence the
dedupe step; failing loudly on corrupt installs is a deliberate behavior change → record in `docs/PARITY.md`.

**H-4. Ordering contract.** Document (rustdoc + `docs/PARITY.md`) that clients may trust only the
per-stream seq, never cross-stream order. Add a domain `EventPayloadRef` check so a `message`/`message-update`
event carrying a message seq can only be appended after that message is committed (app-port contract).
Reject negative/out-of-range seq in paging helpers alongside `validate_page_size`. *Risk:* port signature
change; streamed replies may still interleave across streams — acceptable if the documented contract holds.

**H-5. Strict deserialization & timestamp hygiene.** Add `#[serde(deny_unknown_fields)]` to stored-format
types (not public API DTOs unless a DEVIATION is filed). Add `Thread::created_at_parsed()` returning a
validated RFC3339 instant used by ordering; keep the `String` field for compatibility. *Risk:* stricter
parsing can reject legacy stored rows; run a replay over existing fixtures first.

**H-6. REQ-tagged tests.** `req_004_duplicate_bot_thread_is_rejected`, `req_005_sequence_never_reused_or_renegative`,
`req_020_event_cannot_precede_its_message`, `req_048_external_thread_requires_conversation_id`; plus
proptest over arbitrary allocation sequences asserting strict monotonicity, and a property that
`validate()` rejects all blank-id/`None` combinations. Trace_check counts declaration lines — put REQ ids in `fn` names.

## Decisions needed (GUESS defaults applied)

- Is the first seq 0 (CONFIRMED in SQL) treated as valid by newtypes? Default: yes, `new(0)` ok.
- Does the spec allow a bot thread with an external platform id? Default: no — `external=Some` means `kind='external'`.
- Dedupe policy for pre-existing duplicate bot threads: default keep oldest, repoint newer messages, archive rather than delete.

No code was changed. Only this proposal file was written.
