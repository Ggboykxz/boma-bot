# Connector domain model — architecture proposal (v0.4.0)

Module: `crates/domain/src/connector.rs` (REQ-032 connector catalogue).
Lens: architecture. Scope: proposal only — no code was changed.

## Current shape (CONFIRMED — read this session)
- `ConnectorAuth` enum (`None`, `ApiKey`) with `as_str()` for the `auth_kind` DB column.
- `ConnectorToolSpec` { `name`, `description`, `input_schema: serde_json::Value` }.
- `ConnectorSpec` { `id`, `name`, `description`, `auth_kind`, `tools: Vec<ConnectorToolSpec>` }.
- Free fn `validate_connector_id(&str) -> Result<(), DomainError>`; private `CONNECTOR_ID_MAX = 64`.
- `builtin_connectors() -> Vec<ConnectorSpec>` rebuilds two owned entries (`rss`, `http-json`) per call.
- `ConnectorSpec::tool_key(&tool) -> String` formats `connector.<id>.<name>`; there is no inverse.
- Three unit tests; module is pure (no IO/async), correct for a domain crate.

## Guiding principles
Keep the module pure; make invalid states unrepresentable; keep the wire contract
(`connector.<id>.<name>` keys, `auth_kind` strings) byte-stable. Any externally
visible change is a DEVIATION needing `docs/PARITY.md` and approval before shipping.

## Upgrade 1 — Newtype `ConnectorId` / `ToolName`, self-validating at construction
- Current: `id`/`name` are bare `String`; `validate_connector_id` is a free function that
  callers must remember to invoke (confirmed in `usecases.rs`, three separate call sites).
  Nothing forces a `ConnectorSpec` to hold a valid id.
- Proposal: add `ConnectorId` and `ToolName` newtypes following `ids.rs`'s `opaque_id!`
  convention, with `try_new` that runs the kebab check once. Store them in `ConnectorSpec` /
  `ConnectorToolSpec`. Keep `validate_connector_id` as a thin compatibility wrapper.
- Why: moves validation into the type; eliminates the "validated but stored as String" gap.
- Risks: touches constructors in `infra/stores/connectors.rs`, `testkit/memory.rs`, `lib.rs`
  re-exports (CONFIRMED callers). Mitigate with staged `From<&str>`/accessor shims and by
  keeping the DB/wire text identical. Effort: medium; highest blast radius of the five.

## Upgrade 2 — `ConnectorToolKey` newtype with paired encode/decode
- Current: `tool_key()` formats a key, but inbound parsing is duplicated ad hoc in
  `usecases.rs::dispatch_connector_tool` (`strip_prefix("connector.")` + `splitn(2, '.')`,
  validates only the id and "tool is non-empty") and filtering in `infra/stores/tools.rs`.
- Proposal: a `ConnectorToolKey { connector: ConnectorId, tool: ToolName }` with `parse(&str)`
  and `Display`/`as_str` producing the canonical key — one implementation of both directions.
- Why: removes key-parity drift and a whole class of routing inconsistencies.
- Risks: parse must reject dots/spaces in parts (id validation already does); must preserve
  the exact error text clients rely on. Mitigate by porting the current behavior verbatim
  into `parse` and adding round-trip + rejection unit tests.

## Upgrade 3 — Symmetric `ConnectorAuth` wire contract
- Current: `as_str()` exists but no inverse; each consumer re-derives `ApiKey` from text
  (e.g. `auth_kind == ConnectorAuth::ApiKey` checks are fine, but DB row decoding has no
  single home in the domain).
- Proposal: `impl TryFrom<&str> for ConnectorAuth` with a typed parse error (reuse
  `DomainError`), plus a `const ALL`/`as_str` pairing. Add a round-trip test over all variants.
- Why: keeps the `auth_kind` contract in one place as OAuth/webhook kinds are added.
- Risks: unknown future values must be rejected, not defaulted; needs a new error variant —
  confirm `DomainError` shape first (CONFIRMED to exist, variant set unverified).

## Upgrade 4 — Named `ConnectorCatalogue` abstraction, built once
- Current: `builtin_connectors()` returns a fresh owned `Vec` with heap strings and JSON
  schemas on every call; grep confirms repeated calls in `infra/.../connectors.rs` and
  `testkit/memory.rs` (lookup, connect check, tool enumeration).
- Proposal: introduce `ConnectorCatalogue` (newtype over a static/`LazyLock` slice) with
  `get(id)`, `tool_keys()`, `find_by_tool()`. Keep `builtin_connectors()` as a cloning shim.
- Why: centralizes lookup logic currently re-implemented by callers and removes per-call
  allocation on a hot dispatch path.
- Risks: `LazyLock` needs non-`const` `serde_json::Value` construction and a `std` domain
  crate — verify no `no_std` requirement (GUESS: none, but unconfirmed). Static init and
  lifetime changes can leak into public signatures; keep the shim to contain churn.

## Upgrade 5 — Validated tool specs + additive capability metadata
- Current: `input_schema` is an unchecked `Value`; `name`/`description` have no length or
  format limits; no version/capability field exists to host the OAuth, webhook-push and
  community-marketplace connectors the module docs promise for later milestones.
- Proposal: validate each tool spec at catalogue construction (non-empty name, unique tool
  keys per connector, object-shaped schema); add additive `version: u16` and
  `capabilities: ConnectorCapabilities` bitset to `ConnectorSpec` so v0.4.0 can express
  auth modes without reshaping the model again.
- Why: preserves invariants at build time and gives a stable extension seam.
- Risks: adding public struct fields breaks every literal constructor (CONFIRMED sites in
  `connector.rs`, `infra`, `testkit`); do it as one atomic commit with defaults. "Capability"
  fields are proposal-only, not spec-derived — log in `docs/STATE.md` and get approval before
  treating them as contract.

## Suggested sequencing
1 → 2 → 3 (type/key/wire hardening, low functional risk), then 4, then 5 (extension seam).
Each step: add tests first with REQ-032 in the name, then `cargo fmt --check`, clippy `-D warnings`, tests.

## Open questions / decisions needed (GUESS where noted)
- Is the domain crate `std` (for `LazyLock`) — GUESS yes, confirm before Upgrade 4.
- Should `DomainError` gain a parse variant or reuse `InvalidName` — decision needed.
- Are capability/version fields in scope for v0.4.0 or deferred to the OAuth milestone — ask owner.
