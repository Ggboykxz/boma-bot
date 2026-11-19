# Reliability proposal — `crates/infra/src/stores/rows.rs` (v0.4.0)

- Worker: 388 · Lens: reliability · Module: row → domain mappers (`crates/infra/src/stores/rows.rs`)
- Status: proposals only (CONFIRMED = read in this session; GUESS = inferred). No code changed.

## Current behaviour (CONFIRMED)

This file is the single decode boundary for every read: ~15 `#[derive(sqlx::FromRow)]` structs
(one per table) plus free functions that convert them to `boma_domain` / `boma_app::ports` types.
ID newtypes (`UserId::new`, `ThreadId::new`, …) and enum parsers (`RunStatus::from_str`,
`MessageRole::parse`, `EventType::parse`, `ToolKind::parse`, `SpaceRole::parse`) are the main
validators. All failures surface as `AppError`. Three mapping policies coexist: strict (error on
unknown enum / invalid ID), silent coercion (bool/fallback), and silent saturation (`unwrap_or(MAX)`).

## Failure modes

- **F1 — NULL silently becomes an empty ID (HIGH, CONFIRMED).** `bot()` (l.267) and `bot_summary()`
  (l.296) do `ThreadId::new(row.thread_id.unwrap_or_default())`. The column is `Option<String>`; a
  NULL is turned into `""` and fed to the newtype. Either the newtype rejects it (whole read fails
  with an opaque ID error) or accepts it (bot carries a bogus thread id). GUESS: `ThreadId::new`
  rejects empty strings, so a NULL row takes down the entire roster/space read. NULL is masked,
  never represented.
- **F2 — Corruption reported as outage (HIGH, CONFIRMED).** `message()` (l.326-329) maps a failed
  `serde_json::from_value` of `blocks` to `AppError::Unavailable` (503). One corrupt row then looks
  transient: clients retry, breakers trip, and a deterministic data bug is invisible to operators.
  The `debug!` log carries the serde error but no row/thread/seq id, so the bad row cannot be located.
  `prompt_snippet()` (l.523) swallows the same decode with `.ok()?` — zero observability (LOW; it is
  only an activity-list hint).
- **F3 — Unvalidated JSON passthrough (MEDIUM, CONFIRMED).** `event()` (l.387) and `effect()` (l.505)
  assign `row.payload.0` straight to the domain with no structural check. A non-object or drifted
  payload is accepted here and fails at a distant consumer, making root-cause slow. Contrast with
  `message()`, which at least attempts a typed decode.
- **F4 — Silent signed→unsigned saturation (MEDIUM, CONFIRMED).** `run()` (l.377) `attempt`,
  `bot_summary()` (l.298) `unread_count`, `memory_document()` (l.548) `revision`, `routine()` (l.594)
  `account_failures` all use `try_from(...).unwrap_or(MAX)`. A negative/overflowing value (bad
  migration, manual edit, type drift) becomes `u32::MAX`/`u64::MAX`: e.g. `attempt = MAX` can
  suppress retries or warp scheduling, `unread_count = MAX` renders an absurd badge. The failure is
  silent and far from its cause.
- **F5 — Inconsistent enum-string policy (MEDIUM, CONFIRMED).** `thread()` (l.312) `row.kind ==
  "external"` turns any unknown kind into a bot thread; `invitation()` (l.400) `status == "pending"`
  turns any unknown status into not-pending; (l.316) `_ => ChannelPlatform::Generic` erases unknown
  platforms. Drift/corruption is hidden, while `card()` (l.410-430) and the `parse()`/`from_str()`
  calls error on unknown values. One module, three policies.
- **F6 — Misleading error variant (LOW, CONFIRMED).** `card()` (l.416, l.427) reports an unknown card
  kind/status as `DomainError::UnknownEventType` and omits the row id — misleading telemetry.
- **F7 — Flattened row coupling (LOW, GUESS).** `ActivityRow` (l.217) `#[sqlx(flatten)] run: RunRow`
  pulls all ~26 `RunRow` columns into the activity query; a SELECT/shape mismatch becomes a runtime
  `FromRow` error. Whether call sites use compile-time-checked `query_as!` is GUESS (depends on the
  stores that invoke it).

## Concrete hardening (v0.4.0)

- **H1 — Make NULL explicit (F1).** Stop `unwrap_or_default()`; validate non-empty and return a
  typed `AppError::Rule` carrying the bot id, or model thread-less bots explicitly in the domain.
  *Risk:* `Bot.thread_id` is currently non-Option, so a real Option is a wide API/DEVIATION change;
  cheap v0.4.0 path is the explicit error. GUESS no active bot has NULL thread_id — audit first.
- **H2 — Separate corruption from outage (F2).** Add a typed non-retryable
  `AppError::Corrupt`/`DataIntegrity` for decode failures; log row id + thread id + seq at `debug`
  (never PII: `email`, `password_hash`, `token_hash` stay out of logs). Reserve `Unavailable` for
  genuine sqlx pool/IO errors. Give `prompt_snippet()` a `debug!` on decode failure.
  *Risk:* new variant touches the error envelope — keep the external mapping unchanged or record a
  DEVIATION in `docs/PARITY.md`; confirm no client depends on today's 503 here.
- **H3 — Validate JSON payloads (F3).** Deserialize `event.payload` / `effect.payload` into a typed
  envelope (or assert object) at the row boundary, mirroring `message()`. *Risk:* stricter parsing
  may reject historically loose rows; GUESS risk is low — audit distinct stored shapes first.
- **H4 — Fallible numeric conversions (F4).** Replace every `unwrap_or(MAX)` with
  `try_from(...).map_err(...)` carrying field name + row id; fail loudly with a typed error.
  *Risk:* previously "working" out-of-range rows become read errors; run an audit query confirming
  no such values exist before shipping.
- **H5 — One strict enum policy (F5, F6).** Route `thread.kind`, invitation `status` and platform
  through `parse()`-style validation with row id, instead of bool coercion / `Generic` fallback;
  add card-specific `UnknownCardKind`/`UnknownCardStatus` variants. *Risk:* unknown legacy values
  that currently render as bot thread / Generic begin erroring → DEVIATION-gated; audit values first.
- **H6 — Observability + parser tests (F2, F7).** Structured `debug!` on every mapping failure with
  identifiers only; add property/fuzz tests for the JSON decode paths and round-trips for each
  mapper. *Risk:* id cardinality in logs — keep at `debug`, do not export as a metric label.

## Constraints / non-goals

- These are read-path mappers: do not change stored formats or column names (public interface
  compatibility, AGENTS.md). Any externally visible error/behavior change is a DEVIATION needing
  approval — do not silently ship H1/H2/H3/H5 envelope changes.
- No raw/hex logging of secrets or personal data; keep detail at `debug` (REQ-054).

## Tests to add (traceability: REQ id in name)

- `req_009_null_bot_thread_id_is_typed_error` (H1); `req_009_negative_unread_count_is_error_not_max` (H4).
- `req_010_corrupt_blocks_json_is_data_error_not_unavailable` and
  `req_010_decode_error_logs_row_id` (H2); `req_010_event_payload_must_be_object` (H3).
- `req_048_unknown_thread_kind_is_rejected_not_silently_bot` (H5);
  `req_022_negative_revision_is_error_not_u64_max` (H4).
- Property/fuzz test for `message()` / `prompt_snippet()` JSON decode; keep `crates/infra/tests/m1.rs`
  round-trips green.

**Recommendation:** ship H2 + H4 + H6 first (observability + fail-loud after an audit; no stored-data
change); treat H1/H3/H5 as DEVIATION-gated for v0.4.0 approval. CONFIRMED by reading the module in
this session; downstream call sites and newtype validation strictness are GUESS pending inspection.
