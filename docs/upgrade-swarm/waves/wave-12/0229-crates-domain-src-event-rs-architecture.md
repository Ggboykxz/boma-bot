# 0229 — `crates/domain/src/event.rs` — architecture (v0.4.0)

- Worker 229 · Lens: architecture · Target: v0.4.0 · Read-only review; no code edited.
- Module: `crates/domain/src/event.rs` (141 lines) at `before-m5` (v0.3.0-rc5).
- Contract read for context: `spec/01-requirements.md` REQ-005/REQ-020,
  `spec/03-api-contract.md` (`threads.subscribe`), `spec/02-data-model.md` (Event row),
  `spec/golden/thread-subscribe.json`, `spec/06-decisions-needed.md` (DEC-01/DEC-07),
  and the producers/consumers `crates/app/src/ports.rs` (`NewEvent` :216),
  `crates/app/src/worker.rs` (:412), `crates/infra/src/stores/events.rs` (:45),
  `crates/infra/src/stores/rows.rs` (`event()` :382), `crates/api/src/realtime.rs`
  (`frame()` :138, `heartbeat()` :150), `crates/api/src/wire.rs` (`EventView` :88),
  `crates/domain/src/lib.rs` (:72, :75), `crates/domain/src/ids.rs` (`EventId` :97),
  `crates/domain/src/thread.rs` (`next_event_seq` :32).

## What the event domain model is today

One vocabulary enum (`EventType`, 7 variants), one stream frame (`Event`), four *typed payload
structs* (`RunState`, `MessageDelta`, `MessageRef`, `Roster`), a parse error, and one unit test.
The stream is keyed by `(thread_id, seq)`; ordering comes from `Thread::next_event_seq`, which the
store increments with a `RETURNING next_event_seq - 1` (`stores/events.rs:45`). REQ-005/020 ride
entirely on that seq: a reconnect resumes by last-seen seq and a heartbeat keeps the gap filled.

The architecture debt is a split brain between the typed and untyped views:

- `Event.payload` is a free-form `serde_json::Value` (`:89`). Every producer builds it with
  `serde_json::json!` and every consumer forwards it as `Value` — the four payload structs in this
  very file are **never constructed anywhere** and are **not re-exported** (`lib.rs:72` exports only
  `Event, EventType`). They are orphaned documentation, not a contract.
- The wire has a *second* event model, `EventView` (`wire.rs:88`), built `From<&Event>`; the SSE
  frame encodes the view, not the domain type. So the shape clients see is defined twice.
- `EventId` (`ids.rs:97`) is exported but no `Event` field uses it; the row has no id. The stream's
  identity and its newtype disagree.
- `created_at` is a `String` and `seq` is a bare `i64` — neither carries the meaning the domain
  assigns them.

Pinned invariants for v0.4.0 (do not break): the golden `thread-subscribe.json` fixes the flat
payloads (`{runId,status}`, `{messageId,delta}`, `{}`) and the exact wire strings
`message`/`message-update`/`run`/`typing`/`card`/`roster`/`heartbeat`; `EventType::ALL.len() == 7`
is asserted; `EventType::parse("messageupdate")` must stay an error.

## Target structure for v0.4.0 (GUESS)

Keep `EventType` as the single vocabulary and `Event` as the single domain frame, but make the
payload a first-class, typed part of the *domain* (not a second wire model): a `Payload` sum keyed
by `EventType` whose serde representation is byte-identical to today's JSON. Reduce `EventView`
to a thin projection of `Event`, and make each type (`seq`, time, id) say what it means. The four
orphaned structs become the typed variants instead of dead code.

## Concrete upgrades, each with a risk

1. **Typed payload sum with wire-identical serde.** Replace `payload: Value` with an enum
   (`Run(RunState | RunState-like)`, `MessageDelta`, `Message`, `Roster`, `Heartbeat`, `Typing`,
   `Card`) that serializes flat — reuse `#[serde(untagged)]` on per-variant structs or a custom
   `Serialize` so the golden bytes do not change. Delete `EventView` or make it a direct
   `From<&Event>`. Risks: (a) the migration surface is wide — `NewEvent` in `ports.rs`, the worker,
   `realtime.rs::heartbeat`, and `rows.rs::event()` all touch `Value` and must move together;
   (b) an enum that *rejects* an unknown kind would break the gap-free stream on a forward-compatible
   producer — keep an `Unknown(Value)` catch-all at the storage boundary; (c) any changed byte is a
   DEVIATION against `thread-subscribe.json` and needs `docs/PARITY.md`.

2. **One source of truth for the vocabulary.** Derive `as_str`, `parse`, and `ALL` from a single
   declaration (one match table or a small macro) instead of hand-maintaining three parallel 7-way
   lists (`:32`, `:44`, `:63`). Risks: (a) a dependency (e.g. `strum`) needs an ADR plus a
   `docs/DEPENDENCIES.md` row — prefer a local macro to avoid that; (b) `as_str` is `const fn`, so
   the generated impl must stay const-compatible; (c) the exact wire strings, including
   `message-update`, must survive unchanged or clients break.

3. **Make seq / time / id mean what they say.** Introduce a typed sequence (a newtype checked
   against `Thread::next_event_seq`), a real timestamp type instead of `String`, and settle
   `EventId`: either add an `id` field to the frame and persist it, or remove the unused newtype.
   Risks: (a) changing public fields or adding an id changes the persisted schema and wire — a
   DEVIATION requiring approval; (b) a timestamp must format exactly as the golden's RFC3339
   `createdAt` values and needs a dependency decision (ADR); (c) `seq` is produced by SQL
   `RETURNING`, so the newtype must still coerce to `i64` at the store boundary.

4. **Give the event its own error type.** `DomainError::UnknownEventType` is shared with card
   kinds and statuses (`rows.rs:416`, `:426`), conflating three vocabularies under one error. Move
   event parsing to an `event`-scoped error (or at least a distinct variant). Risks: (a) the API
   error envelope must stay stable (one stable format, no internal leakage), so the new error must
   map to the same client-visible code; (b) `DomainError` is `PartialEq` and pattern-matched in
   tests — a variant move has a small but non-zero blast radius.

5. **Fail-closed encoding without losing the stream.** `frame()` (`realtime.rs:138`) silently
   substitutes `data("{}")` when `json_data` fails, which hides a broken payload as a corrupt frame;
   pair with `deny_unknown_fields` on the typed payload structs so malformed input is caught at the
   boundary. Risks: (a) strict fields can reject forward-compatible clients — apply them on the
   domain/storage deserialize path only, not to the public event stream; (b) the current `{}`
   fallback keeps the stream alive — preserve REQ-005's gap-free behavior by logging and skipping a
   bad frame rather than closing the connection; (c) more typed structs mean more parser tests
   (property/fuzz per repo standard) and a re-run of the golden.

## Explicit non-goals

No change to the wire strings or payload JSON, no new event types beyond the spec's seven, no
second event model alongside `Event`, no new runtime dependency without an ADR, no edit to the SSE
resume semantics (DEC-01/DEC-07), no change to `threads.subscribe`.

## Evidence

CONFIRMED — read this session: `event.rs` :1–141; `lib.rs` :72/:75; `ids.rs` :97;
`thread.rs` :32; `error.rs` :46/:85; `rows.rs` :382/:416/:426; `stores/events.rs` :45;
`realtime.rs` :138/:150; `wire.rs` :88/:102; `ports.rs` :216; `spec/golden/thread-subscribe.json`;
`spec/01-requirements.md` REQ-005/020; `spec/03-api-contract.md` :38–40.
GUESS — that the four payload structs are unused outside this file (grep across `crates/` found no
constructors) and that no external client relies on tolerant unknown fields; verify both before
proposals 1 and 5 land.
