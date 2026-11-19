# Worker 0230 — `crates/domain/src/event.rs` reliability proposal (v0.4.0)

Module: `crates/domain/src/event.rs` (141 lines). Lens: reliability (ordering, loss, replay, drift).
Evidence: source, `error.rs`, `thread.rs`, `run.rs`, spec §4.2/§4.4, DEC-01/DEC-07 and the
`thread-subscribe.json` golden were read in `before-m5` on 2026-11-19. CONFIRMED = read here;
GUESS = inferred, needs a test. Read-only task; this document is the only write.

## 1. What the module does (CONFIRMED)

- `EventType` (7 variants) with a manual `ALL` array, `as_str()` and `parse()`; `parse()` returns
  `DomainError::UnknownEventType` (event.rs:13-76; error.rs:46,85).
- `Event` is a plain data bag: `thread_id`, `seq: i64`, `kind`, `payload: serde_json::Value`,
  `created_at: String`, all `pub`, with derived serde and no constructor or invariant check (:78-92).
- Typed companions exist (`RunState`, `MessageDelta`, `MessageRef`, `Roster`, :94-126) but nothing
  ties them to `kind` or to `payload`; the module doc (:1-4) promises gapless resume by `seq`.
- Only one test: `parse`/`ALL` happy path, including `parse("messageupdate").is_err()` (:128-141).
- The golden frame uses `"type": "message-update"` and `seq` 4,5,6 (spec/golden/thread-subscribe.json).

## 2. Failure modes

### 2.1 Two wire vocabularies inside one enum (GUESS, highest)

`#[serde(rename_all = "lowercase")]` (:12) makes serde emit and accept `MessageUpdate` as
`"messageupdate"`, while `as_str()`/`parse()` and the golden use `"message-update"` and the test at
:139 explicitly rejects `"messageupdate"`. Derived `Deserialize` of a golden frame would therefore
fail, and derived `Serialize` would produce a value `parse()` rejects. No round-trip test exists.
This is drift between two encoders of the same type, and it strikes the stream that reconnect replay
depends on. Serde's `rename_all` semantics are documented; confirm with a CI test before coding.

### 2.2 `seq` is an unchecked `i64` (CONFIRMED)

Zero, negative, duplicate, skipped and backwards sequences are all constructible; `i64::MAX + 1`
overflows silently in naive `prev + 1`. Spec §4.2 requires no observable gaps and no reordering, and
`Thread.next_event_seq` (thread.rs:31) is the allocator, but the domain offers no `Seq` newtype,
no `checked_next`, and no stream validator. DEC-01's "resume from last acknowledged seq" has no
domain-level type to express "last applied" (compare `parse_cursor` in message.rs).

### 2.3 Replay of `message-update` is not idempotent (CONFIRMED code, impact GUESS)

`MessageDelta` (:105-110) has a `message_id` and free text but no delta index/offset. Replaying from
a stale cursor re-applies deltas already rendered, duplicating streamed text; applying a frame twice
after a transport retry has the same effect. Nothing in the module gates "apply only `seq > last`".

### 2.4 `kind` ↔ `payload` shape drift (CONFIRMED)

Any kind can carry any JSON. `RunState.status` is a raw `String` (:100) although `RunStatus` owns a
closed vocabulary and parser (run.rs:20-69), so a stale/unknown status token can ride in a `run`
frame. Consumers discover corruption late, mid-stream, when they try to decode a payload.

### 2.5 `created_at` is an unvalidated `String` (CONFIRMED, low-medium)

No RFC3339/UTC validation or normalisation; `"now"`, local offsets and lexical mis-orderings are
representable. Audit export and goldens require stable, comparable timestamps (spec §02, §4.6).

### 2.6 Unbounded error echo and `ALL` drift (CONFIRMED, low)

`parse()` allocates the raw wire value into `UnknownEventType` (:71-73) and `Display` quotes it
verbatim (error.rs:85): an attacker-controlled 10 KiB control-character string becomes an error and
a log line. `req_040` (error.rs:137-143) already establishes the opposite rule for e-mail. `ALL` is
a manually maintained array: adding a variant breaks the exhaustive `as_str`/`parse` matches at
compile time but can silently leave `ALL` short.

## 3. Concrete hardening for v0.4.0 (ordered by value/risk)

- **H1 — one encoder (medium risk, highest value).** Put `#[serde(rename = "message-update")]` on
  the `MessageUpdate` variant (keep `rename_all` for the rest) so serde, `as_str`, `parse` and the
  golden agree on one vocabulary. Do **not** change `parse`/goldens to `messageupdate`. Add a
  proptest `parse(as_str(v)) == v` over all variants, a serde round-trip test, and a test that
  deserializes the golden's `message-update` frame into `Event`.
- **H2 — sequence invariants (low-medium).** Add pure `validate_event_seq(seq)` (require `>= 1`)
  and `is_next_event_seq(prev, next)` using `checked_add`, plus `validate_event_stream(&[Event])`
  that reports duplicate/gap/backwards with a new `DomainError` variant. Keep it additive and opt-in
  at first; make construction strict only after producers are proven gapless. Tests named
  `req_005_*` for gap, duplicate, backwards, zero and overflow.
- **H3 — replay gate (low).** Add `should_apply(last_applied_seq: Option<i64>, &Event) -> bool`
  (`Some(s) => seq > s`) and use it in the subscriber so a replayed or retried frame cannot re-apply
  a delta. For true delta idempotency, propose an additive `#[serde(default)] index: u64` on
  `MessageDelta` (wire-shape change: ADR + DEVIATION entry first). Test `req_020_*` for replay twice.
- **H4 — typed payload validation (medium).** Add `Event::validate(&self)` checking payload shape
  per `kind` (run_id/status; message_id/seq; message_id/delta; user_ids; empty heartbeat) and parse
  `RunState.status` through `RunStatus`. Call it at the adapter boundary, log a metric, then reject.
- **H5 — timestamp (low-medium).** Add pure `validate_created_at(&str)` enforcing RFC3339 with a
  trailing `Z`; prefer `chrono::DateTime<Utc>` behind an ADR + `docs/DEPENDENCIES.md` entry rather
  than a hand-rolled date parser. Test the golden value passes and a local-offset value is refused.
- **H6 — error hygiene and exhaustiveness (low).** Truncate/sanitise the unknown value (cap ~64
  bytes, strip control chars) before building `UnknownEventType`, and add a test that a 10 KiB
  CRLF-laden value cannot echo into a log line. Add a test that `ALL` covers every `as_str()` value.

## 4. Risks and gates

- H1 and H3's `index` alter the observable wire contract: record each in `docs/PARITY.md` as a
  DEVIATION with justification and wait for owner approval before shipping (AGENTS.md).
- H2 strict construction can reject streams that producers emit with legitimate gaps (e.g. an
  allocated seq whose event was dropped): roll out as a validator + metric first; confirm against
  §4.2 before making `Event` unrepresentable-invalid. `Event.seq` and `Thread.next_event_seq` must
  stay consistent across domain/app/infra.
- Domain crate is pure: no IO, clock or async (lib.rs:1-8). H5 needs either a pure shape check or a
  dependency justified by ADR; do not add IO here. No new deps otherwise; `cargo add` + docs.rs if so.
- `error.rs` changes are outside this write scope; propose them, do not edit them here. Keep
  `#![deny(missing_docs)]` satisfied and lints green (`clippy -D warnings`, no `unwrap`/`expect`/
  `panic` outside tests).
- Traceability: `tools/trace_check.py` counts declarations, not comments — put REQ-005/REQ-020 in
  test names. Mark DEC-01/DEC-07 follow-ups in `docs/STATE.md` as GUESS until verified against the
  running original; never point verification at third-party hosts.

## 5. Verification for this wave

1. `cargo fmt --check` and `cargo clippy -p boma-domain --all-targets -- -D warnings`.
2. `cargo test -p boma-domain` including the new `req_005_*` / `req_020_*` tests and the golden
   deserialization test; add `proptest` round-trips for `EventType` and `validate_event_stream`.
3. Differential: deserialize `spec/golden/thread-subscribe.json` frames into `Event`, then reserialize
   and byte-compare the `type` fields; record results in `docs/PARITY.md` as CONFIRMED only after
   running them.
