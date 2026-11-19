# 0226 — `crates/domain/src/connector.rs` — reliability proposal (v0.4.0)

Worker #226 · module `crates/domain/src/connector.rs` · lens: reliability · READ-ONLY review of
`rebuild-workspace/.worktrees/before-m5` (v0.3.0-era source). Proposals only; no code changed.
Claims: **CONFIRMED** = read in this tree this session; **GUESS** = inference needing a test.

Scope: this 192-line pure-domain module (REQ-032) — `ConnectorAuth`, `ConnectorSpec`,
`ConnectorToolSpec`, `validate_connector_id`, `builtin_connectors`, `tool_key` — plus its consumers:
`crates/app/src/usecases.rs` (`connections_*`, `dispatch_connector_tool`),
`crates/app/src/ports.rs` (`ConnectorStore`, `ConnectorSummary`),
`crates/infra/src/stores/connectors.rs`, `crates/infra/src/connectors.rs`. Spec anchors: REQ-032,
spec/02-data-model.md (Connection status `pending/connected/revoked/error`; "a connector install
owns its credentials"), spec/06-decisions-needed.md DEC-10 (secrets, open), module header promise
"revoking the install removes the tools everywhere and every later call is refused".

## What is already right (keep; do not re-propose)

- Domain is pure: no IO, no async, no secrets held in the catalogue (CONFIRMED). `ConnectorAuth` is a
  small `Copy` enum with a stable `as_str` wire value (CONFIRMED).
- `validate_connector_id` is strict lowercase-kebab with a 64-char ceiling (CONFIRMED), and it is
  reused at the app boundary for `connections_install` / `connections_remove` and inside
  `dispatch_connector_tool` (CONFIRMED, usecases.rs:985, :1002, :1022).
- `builtin_connectors()` is code, unique by id, and covered by a `req_032_*` test (CONFIRMED).

## Failure modes

### F1. Invalid catalogue config is "reviewable" but not checked (high)

The catalogue is hand-built `Vec<ConnectorSpec>` with plain `String` fields; nothing validates a spec
at construction or in `builtin_connectors()` (CONFIRMED). Only *ids* are validated, and only in tests
(CONFIRMED). Tool names (`ConnectorToolSpec.name`) are unvalidated yet concatenated into the tool key
`connector.<id>.<name>` (`tool_key`, :144-146): a name containing `.`, uppercase or a dash-leading
value yields an ambiguous or unparseable key, and dispatch splits with `splitn(2, '.')` and validates
only the id portion (CONFIRMED, usecases.rs:1013-1022). Two tools with the same name, or a tool name
that collides after a future id change, are not detected (GUESS — no current duplicate). The JSON
`input_schema` is an untyped `Value` with no assertion that it is an object with
`additionalProperties: false`, so a malformed schema ships silently (CONFIRMED structure, GUESS on a
real bad schema). Net: a catalogue typo becomes a runtime "tool not found", not a build/test failure.

### F2. Connection state drift — no domain status and no consistent call snapshot (high)

The module models no connection state at all, while spec/02-data-model.md lists four statuses
(`pending/connected/revoked/error`) (CONFIRMED). The Postgres store encodes status as a raw `String`
and repeats the literal `"connected"` in `list`, `is_connected`, `connected_credential`,
`catalogue_tools` and in the `install`/`revoke` writes (CONFIRMED, stores/connectors.rs:59, 110-115,
150, 171/179, 194, 223). Adding an `error` status would silently make a connector "not installed"
in some paths and stale in others (GUESS — needs a fault-injection test).
Dispatch reads state in two separate queries: `is_connected()` (:1029) then
`connected_credential()` (:1041) then `runner.run()` (:1053) (CONFIRMED). A revoke or a re-key
(`install` uses `ON CONFLICT DO UPDATE` to rotate the credential, :108-124) between those reads lets
an in-flight call use a revoked/old credential — the header's "every later call is refused" is a
promise the type cannot keep (GUESS on exploitable timing; TOCTOU window is CONFIRMED). A connector
removed from the catalogue in code leaves an orphan DB row that `list()` hides entirely (CONFIRMED
shape, GUESS on a present orphan).

### F3. Secrets — rules enforced in the adapter, and errors echo attacker input (high)

The domain has no credential policy type. The pure rule "an `ApiKey` connector needs a key; a `None`
connector takes none" lives only in `PostgresConnectorStore::install` (CONFIRMED, :84-94); the
memory/testkit double may drift (GUESS — not diffed this pass). Empty/whitespace keys are never
rejected at the domain or install boundary (GUESS on the RPC layer), so a `connected` connector can
be permanently unusable (looks installed, every call 401s) — a state-drift/secrets hybrid.
`validate_connector_id` embeds the raw id in its error (`format!("... `{id}` ...")`, :89-91) and
`DomainError::InvalidName` Display echoes that reason verbatim (error.rs:77), so a crafted id
(newlines/control chars) can become reflected text in RPC errors and log injection (CONFIRMED data
flow, GUESS on a reachable logging sink). DEC-10 is still open, so any schema/rotation decision here
must not pre-empt it (CONFIRMED).

## Concrete hardening (v0.4.0)

1. **P1 — `validate_connector_spec(&ConnectorSpec)` in domain.** Require: valid id; tool names also
   lowercase-kebab and non-empty; unique tool keys; `input_schema` is an object with
   `additionalProperties: false`. Call it from `builtin_connectors()`'s `req_032` test and assert
   `tool_key()` round-trips. Risk R1.
2. **P2 — typed `ConnectorStatus`** (`Pending|Connected|Revoked|Error`) with `as_str`/`parse` in this
   module; migrate the store's raw `"connected"`/`"revoked"` comparisons to it so a new status cannot
   silently fall through. Risk R2 (schema/check constraint).
3. **P3 — one consistent snapshot per call.** Add a port method returning a `ConnectorGrant` newtype
   (status + credential read together, or a recheck after credential fetch); have `dispatch_connector_tool`
   use it instead of the two-query sequence. No wire change. Risk R3.
4. **P4 — credential policy in domain.** Add `needs_credential(auth_kind)` and a pure
   `validate_credential(auth_kind, Option<&str> /* view only */)` rejecting blank keys; move the
   adapter's :84-94 checks here so memory and Postgres stores share one rule, and never retain or log
   the value. Risk R4.
5. **P5 — sanitise validation errors.** Stop echoing the raw id (or cap/escape control chars) in
   `validate_connector_id`; give `DomainError::InvalidName` a redacted Display for untrusted reasons.
   Risk R5 (fixture text).
6. **P6 — credential version on the grant** so a re-key invalidates in-flight dispatches; gate on
   DEC-10. Risk R6.
7. **P7 — catalogue reconciliation.** Pure `reconcile_catalogue(builtins, installed_ids) -> {known,
   orphaned}`; surface orphans as `Error`/log instead of dropping them from `list()`. Risk R7.
8. **P8 — property tests.** `proptest` for `validate_connector_id` (never panics, rejects non-kebab,
   accepts only the documented grammar) and a generated-catalogue test for tool-key uniqueness —
   AGENTS requires property coverage for boundary validators. Risk R1.

## Risks / trade-offs

- R1. P1/P8 may reject a future tool name using a dot; default to kebab and record the decision.
- R2. P2 touches domain + infra + DB check constraint; canonicalise existing rows and verify the
  five status call sites agree before merging.
- R3. P3 changes an internal port signature (app/infra/testkit together); keep the RPC error body
  byte-identical, or record a DEVIATION in docs/PARITY.md.
- R4. P4 keeps secrecy out of domain by validating a borrowed view only; do not store `SecretString`
  here.
- R5. P5 changes error text; update golden/RPC fixtures. No wire *shape* change.
- R6. P6 adds a column and awaits DEC-10 — defer to v0.5 if key rotation lands later.
- R7. P7 needs an `Error` status consumers can see; ship as log/metric first to avoid a wire change.
- All edits in English; keep `clippy -D warnings`, `cargo fmt --check`, and REQ-032 tests green after
  each step. No secret value enters docs, logs or fixtures.

## Explicit non-goals

No OAuth/webhook connectors, no adding/removing builtins, no change to the `connector.<id>.<tool>`
key format, no secret-manager integration (DEC-10's decision), no weakening of tenant scoping.

## Suggested rollout

Wave A (domain-only, revertable): P1, P4, P5, P8 + `req_032_*` tests. Wave B (cross-crate): P2, P3,
P7 with differential store tests and a revoked-mid-dispatch fault test. Wave C: P6 after DEC-10.
Acceptance: the catalogue is validated by a test, a connector call sees one consistent state, and no
validator can reflect attacker-controlled text. All facts CONFIRMED by reading; timing/impact marked
GUESS needs the tests above. Line refs against `.worktrees/before-m5`; nothing executed this pass.
