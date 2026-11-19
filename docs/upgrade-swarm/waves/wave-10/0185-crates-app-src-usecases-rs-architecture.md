# v0.4.0 architecture proposal — `crates/app/src/usecases.rs` (use-case orchestration)

Worker #185 · LENS=architecture · target v0.4.0 · READ-ONLY analysis (no code changed).

Status of claims: all "current state" statements below are CONFIRMED by a direct read of the
10,080-line `usecases.rs` and by cross-reads of `service.rs`, `lib.rs`, `ports.rs` and the sibling
wave-10 proposals in this worktree. Forward-looking items are proposals (GUESS until implemented
and tested).

## Current structure (CONFIRMED)

- One flat god module: 344 KB, 163 `pub async fn`, 5 `pub fn`, 60 `pub struct`, 34 private
  `async fn`, 22 private `fn`. Every use case is a free function taking `&App` (+ usually
  `&TenantContext`) and returning `Result<_, AppError>`; there is no type that groups a family.
- Organization is by milestone accretion, not by concern: only 10 informal section markers
  (`M8` `:3516`, `M9` `:8446/:8575`, `M11.1` `:9237`, `M11.3` `:9382`, `M12.1` `:9430`,
  `M13.1` `:9697`, `M15.2` `:9811`). Skills, MCP, connectors, marketplace, pages, artifacts and
  tickets sit interleaved at `:462-2050`; mail/webhook/messaging at `:1945-3415`.
- Cross-cutting helpers are buried beside unrelated flows: `decode_base64` `:654`,
  `audit_private_use` `:926`, `constant_time_eq` `:1932`, `normalize_optional` `:4170`,
  `duration_ms_between` `:3416`, `unix_seconds` `:3480`, `model_choice_for` `:4196`,
  `member_context_for` `:4225`, `room_thread_for_bot` `:4241`, `thread_of` `:2386`.
- Context derivation is duplicated and semantically loose: 12 `TenantContext::System` sites;
  eight use `SystemReason::WebhookDelivery` for mail (`:2169`), Slack (`:2683`), messaging
  (`:3081`) and billing (`:4062`) webhooks — one reason stands for four different transports.
- Nonce replay is hand-rolled twice: `pages_save` `:1493-1502` and `artifact_create` `:1651-1661`
  repeat the same length check + `find_by_nonce` short-circuit; `send_message` instead relies on
  store-level dedup (34 `client_nonce` references total).
- The `NewMessage` 14-field literal is repeated verbatim in `webhook_wake` `:2018`,
  `mail_inbound`'s nested `user_message` `:2231`, `mail_held_release` `:2529` and
  `reassign_delegation` `:10030+`.
- Tool dispatch and the tool catalogue are two sources of truth: `tool_catalogue`/`connector_catalogue_tools`/
  `mcp_catalogue_tools` (`:8210-8299`) decide what is *allowed*, while `execute_approved_tool`
  (`:7482-7601`) and `dispatch_read_tool` (`:8368-8444`) are hand-written
  `match call.name.as_str()` with a duplicated `string(key)` closure and hand-built
  `{"ok":false,...}` envelopes.
- Exactly-once effects are derived ad hoc: `effects.record` with `approve:{card_id}` at `:7686-7709`,
  another key scheme in `apply_pending_tool_calls` `:7108`; several audit/effect failures are
  swallowed with `let _ =` (`:807`, `:831`, `:2342`).
- Eight `#[allow(clippy::too_many_lines)]` and four `too_many_arguments` suppressions mark the
  real orchestration hotspots (`mcp_server_add` `:581`, `build_plugin_plan` `:1116`,
  `webhook_wake` `:1965`, `mail_inbound` `:2135`, `marketplace_install` `:1328`, `answer_card` `:7659`).

## Guiding principle for v0.4.0

Keep the flat `pub use` surface for callers, but give each use-case family one cohesive module, one
context/derivation helper layer, and one source of truth per dispatch decision. Invalid
orchestrations (wrong system reason, missing replay check, missing effect key) should be
unrepresentable or shared, not re-implemented per function.

## Upgrades (5 concrete items)

1. **Split into submodules behind a re-export facade.** Move `usecases.rs` to `usecases/mod.rs`
   that only declares modules and `pub use`s the same names, then migrate section by section:
   `messaging::{webhook, mail, channel}`, `runs::{lifecycle, fanout, routines}`,
   `tools::{catalogue, dispatch, approval}`, `marketplace` (skills/MCP/connectors/plugins),
   `resources::{pages, artifacts, boards, tickets}`, `external::{models, billing, scim, admin, acp}`.
   Signatures keep `&App`/`&TenantContext`, so the API crate does not change.
   *Risk:* real cross-family calls (`marketplace_install` → `teach_skill`/`mcp_server_add`,
   `answer_card` → `set_run_status`) create cycles. *Mitigation:* extract shared helpers first
   (item 2) and keep the facade importing across siblings; land one family per commit.

2. **An orchestration kernel for context, replay and message building.** Put `member_context_for`,
   `room_thread_for_bot`, `thread_of`, a `NewMessage::user(...)` builder and a
   `replay_nonce(...)` helper in `usecases::shared`. Replace the eight `WebhookDelivery` system
   contexts with a typed `SystemContext::{Webhook, Mail, Channel, Billing, Scheduled, Auth}` that
   derives the existing `SystemReason`.
   *Risk:* a wrong reason mapping could alter audit/event payloads (wire-visible). *Mitigation:*
   assert identical reason strings in a test per transport before switching; keep the replay
   helper byte-identical to the two current implementations.

3. **One source of truth for tools.** Derive `tool_catalogue` from the same table that drives
   `dispatch_read_tool`/`execute_approved_tool` (e.g. `ToolHandler { key, effect, parse, execute }`),
   appending dynamic MCP/connector tools from one `DynamicTool` source.
   *Risk:* the catalogue is permission-filtered per bot and dynamic tools are runtime-discovered,
   so a fully static registry cannot express it. *Mitigation:* keep the dynamic `Vec` as a separate
   contributor and add a conformance test that every dispatched key is in the catalogue and every
   static catalogue key has a handler (REQ-016/017).

4. **Typed inputs at the stringly-typed boundaries.** Generalize the existing `PageSaveInput`/
   `ArtifactInput`/`SendMessage` pattern to the 4-9-argument functions (e.g. `McpAddInput`,
   `TicketCreateInput`) and parse tool `serde_json::Value` argument bags into typed structs once,
   at dispatch.
   *Risk:* tool-argument JSON and RPC wire DTOs must stay byte-identical; `deny_unknown_fields`
   may reject clients today accepted. *Mitigation:* accept the same field set via `serde` defaults,
   keep a wire snapshot test, and record any strictness change as a DEVIATION in `docs/PARITY.md`.

5. **An explicit exactly-once effect primitive.** Centralize effect-key derivation and the
   "record → execute once" sequence used by `answer_card` and `apply_pending_tool_calls`, and
   replace swallowed `let _ =` audit/effect writes with a counter/metric.
   *Risk:* changing an effect key breaks replay idempotency for in-flight runs (externally visible).
   *Mitigation:* keep keys stable, keep the existing `effects.record` conflict semantics, add a
   property test "same card answered twice → one effect", and REQ-017 in test names
   (`tools/trace_check.py` counts declarations, not comments).

## Cross-cutting risks / sequencing

- Sequence 2 → 1 → 3 → 5 → 4: the shared kernel must exist before modules are split, catalogue
  unification needs the tools family in one place, and typed inputs are orthogonal (can land last).
- The sibling proposal for `service.rs` (worker #183) owns the composition root and feature-config
  validation; this proposal deliberately does not touch `App`/`Stores`/`Services` or `AppError`.
- Every change is intended to be behavior-preserving; any wire/audit difference is a DEVIATION per
  AGENTS.md — record in `docs/PARITY.md` and wait for approval before shipping.
- Verification gate per step: `cargo fmt --check`, `clippy -D warnings`, unit + `tests/` integration,
  golden + contract suites; mark claims CONFIRMED only after running them in this worktree.

## Out of scope for this module

Port trait definitions (`ports.rs`, worker #179), composition/config (`service.rs`, worker #183),
HTTP/DTO mapping (`api/`), and adapter implementations (`infra/`). This proposal only restructures
use-case orchestration and its internal contracts.
