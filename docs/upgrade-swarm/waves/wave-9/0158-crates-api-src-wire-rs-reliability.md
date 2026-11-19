# 0158 — `crates/api/src/wire.rs` reliability proposal (v0.4.0)

Module: `crates/api/src/wire.rs`
Lens: reliability
Spec anchor: the module header's two golden rules (always-present fields serialize even when empty;
genuinely-absent fields are omitted), REQ-005 / REQ-001 / REQ-010 / REQ-011 (the four pins in the
local `tests` module), REQ-045 (redaction), and the AGENTS.md mandate for a generated-OpenAPI vs
`spec/openapi.yaml` contract test.

Status of claims: "Current behavior" is CONFIRMED by reading this file and grepping
`deny_unknown_fields` across `crates/api` this session. Failure modes and hardening are GUESS
(design inference, not implemented, not tested).

## Current behavior (CONFIRMED)

- Every view derives `Serialize + Deserialize` with `#[serde(rename_all = "camelCase")]`. One field is
  explicitly renamed: `EventView.kind -> "type"`. `GroupMessageView` flattens a `MessageView`.
- Option/Vec/string fields use three inconsistent omission styles: `#[serde(default,
  skip_serializing_if = ...)]` (e.g. `MessageView.run_id`, `reactions`), `skip_serializing_if` alone
  (e.g. `ComputerStatusReply.started_at`, `RunView` has neither, `GroupMessagesReply.next_cursor`),
  or plain `Option<String>` that serializes as `null` (e.g. `RunView.prompt_snippet`,
  `MessagesReply.next_cursor`, `BotReply.archived_at`).
- `deny_unknown_fields` appears only on Deserialize-only request structs (9 in this file, e.g.
  `ExportIdRequest`, `AccountDeleteRequest`, the `Admin*Request`/`Notifications*Request` types). No
  reply/view struct rejects unknown fields.
- Four untyped blobs pass through untouched: `EventView.payload`, `AuditEventView.metadata`,
  `PagesGetReply.source`, `PluginInstallView.skipped` (`serde_json::Value`).
- Only `LocalDiscoveredModelView` and `ModelsDiscoveredReply` derive `utoipa::ToSchema`; the other
  ~100 public views do not, so they are not contributed to the generated OpenAPI document here.
- Golden coverage is four unit tests (`req_005`, `req_010`, `req_011`, `req_001`) asserting exact key
  sets / null-vs-absent for a handful of views. The rest have no local pin.

## Failure modes

1. **Serde drift.** The camelCase contract lives only in attributes; renaming a Rust field changes
   the wire key with no shared type-check between server and client. Untyped `serde_json::Value`
   bodies can change inner shape with zero compile-time signal. With almost no `ToSchema`
   registration, the generated OpenAPI document can silently diverge from these shapes, so the
   mandated contract test would either pass on a stale document or fail as an unexplained DEVIATION.
   Anomalies to verify: `ComputerSettingsReply` derives `Eq` while holding `f64` (which does not
   implement `Eq`), and `#[serde(flatten)]` on `GroupMessageView` alters deserialization behaviour
   versus plain structs.
2. **Optional/null mismatch.** `skip_serializing_if` without `#[serde(default)]` means a view that
   omits a field cannot be deserialized from its own output (missing-field error) — a real
   round-trip defect for `ComputerStatusReply.started_at`, `RoutineWriteReply.timezone`,
   `GroupMessagesReply.next_cursor`, and the `ExternalAgent*`/`Notifications*` Option fields. The
   same concept is sometimes `null`, sometimes absent (`MessagesReply.next_cursor` vs
   `GroupMessagesReply.next_cursor`), so clients must special-case each reply. A future edit that
   drops `skip_serializing_if` from an always-absent field would break goldens; one that adds it to
   an always-present field breaks the "serialize even when empty" rule.
3. **Unknown fields.** Request structs reject extras, but view/reply structs silently accept them.
   Any code that deserializes a reply (golden replay, a mocked client, a future echo endpoint) will
   hide typos and stale keys instead of failing, so drift is detected late. Untyped blobs accept any
   shape at all, including secret-shaped keys the redaction rule (REQ-045) was meant to remove.
4. **Golden drift.** The header's two rules are enforced for only four views. Adding a field with
   `default + skip` to a covered view is invisible; adding an always-present field to any of the
   ~100 uncovered views changes the wire contract without a test. Because field names are the
   contract, a rename is a DEVIATION that requires `docs/PARITY.md`, yet nothing here flags it.

## Concrete hardening for v0.4.0 (GUESS)

1. Audit every `skip_serializing_if` Option/Vec and add `#[serde(default)]` wherever it is missing,
   so all views round-trip (`serde_json::from_value(to_value(v)) == v`). Add one proptest/table test
   asserting this for all exported view structs. *Risk:* `#[serde(default)]` on a field currently
   treated as required could loosen deserialization; confirm none of these views are inbound before
   applying (GUESS — this file is the only evidence consulted).
2. Codify the two header rules as reusable helpers/tests rather than per-field convention: one test
   enumerating "always-present" fields that must serialize as `null`, one enumerating "absent"
   fields that must be omitted. Extend golden pins beyond the four current views to every reply
   named in a spec golden. *Risk:* more fixtures to maintain; a wrong classification bakes the bug
   into a test — derive each case from `spec/golden/*.json`, not from intuition.
3. Register the view structs with `utoipa::ToSchema` (or generate them from one source) so the
   contract test compares real shapes instead of a hand-written document. Replace `serde_json::Value`
   with typed structs (or a validated newtype) at least for `EventView.payload` and
   `AuditEventView.metadata`. *Risk:* large mechanical change; typed payloads can be a breaking
   wire change if they reject shapes the original tolerated — stage behind a DEVIATION entry.
4. Decide a consistent policy for unknown fields on views: keep permissive for server output (needed
   for forward compatibility) but document it, and add a dedicated strict deserializer used only by
   golden replay so unexpected keys fail the test. Verify `EventView.kind -> "type"` and the flattened
   `GroupMessageView` still decode correctly under that strict path. *Risk:* strict golden decoding
   may fail on original fixtures containing extra keys; triage each as DEVIATION before enforcing.
5. Resolve the `Eq`/`f64` anomaly (replace with an integer/decimal minor-unit type or drop `Eq`) and
   add a compile-time assertion for camelCase key names via a small serialization test per reply
   family. *Risk:* changing `cpu_limit` representation is an externally visible DEVIATION.

## Constraints

- Field names, camelCase casing, and null-vs-absent behaviour are the wire contract; any change is a
  DEVIATION recorded in `docs/PARITY.md` and awaits approval (AGENTS.md).
- Keep secret-shaped values out of logs and replies (REQ-045); do not let typed payloads reintroduce
  fields the redactor removes.
- Claims here are GUESS until backed by a command/test run; mark them CONFIRMED only after the
  round-trip test and OpenAPI contract test are actually executed.
