# v0.4.0 architecture proposal — `crates/infra/src/stores/destinations.rs`

Lens: architecture · Worker #341 · Read-only review; no code changed.

## Context (CONFIRMED)

`PostgresNotificationDestinationStore` is the infra adapter for `ports::NotificationDestinationStore`
(`crates/app/src/ports.rs:4559-4600`). It seals Slack/Teams webhooks under the space DEK
(migration `0057_notification_destinations.sql`, RLS by `space_id`) and exposes create/list/delete
plus the delivery-only `destination_webhook`. It mirrors `stores/secrets.rs` but diverges structurally.

Traceability: this file, the port and migration 0057 all say REQ-065; REQ-064 is Web Push
(`spec/01-requirements.md:512`). Read "REQ-064" in the assignment as the push-destinations program:
the store implements REQ-065. Fix the heading or record the mapping before release, or the
REQ-named tests and `tools/trace_check.py` drift.

## U1 — Domain owns kind conversion and webhook validation

Facts: `kind_str()` (56-61) and the match in `row_into_domain` (41-45) duplicate
`NotificationDestinationKind::as_str()` (`domain/src/notification.rs:224-229`); a third copy sits in
`api/src/rpc.rs:4448`. create() (86-102) calls `kind.webhook_prefix()` but emits a hard-coded Slack
message for every kind — a Teams rejection cites Slack — and routes URL errors through
`DomainError::InvalidName`.

Change: add `TryFrom<&str> for NotificationDestinationKind` and
`NotificationDestinationKind::validate_webhook(&str) -> Result<(), DomainError>` in the domain; use
`as_str()` on bind, `try_from` on read, the validator in create(). Keep the adapter to sealing, SQL
and error mapping only.

Risk: exact error strings in tests/goldens change (a bug fix — check `testkit/src/build.rs:10241+`);
a new domain API is internal, not a spec-visible DEVIATION. Rejection behavior stays the same.

## U2 — One connection per operation; delete the alias jungle

Facts: create() declares five aliases of the same `space_id` (72-78: `for_scope`, `for_birds`,
`for_insert`, `for_struct`, `for_dek_inner`); destination_webhook() four (205-211). Root cause:
`space_dek()` (`stores/secrets.rs:284-319`) opens its own pool connection while `tenant::in_space`
lends `conn`; the secrets store unwraps the DEK on `conn` (`secrets.rs:46-98`). So DEK read and the
INSERT are not one transaction — a concurrent master-key rekey can seal under one key and open
under another.

Change: add `space_dek_on(&mut PgConnection, ...)` (or thread `conn` into `space_dek`) and use it in
both methods; keep only clones the `async move` closure truly needs.

Risk: `space_dek` is shared — retain the old wrapper or update every caller; verify RLS
`current_space_id()` still covers `space_keys` on the tenant conn; borrow lifetimes inside
`Box::pin(async move ...)` need care (clone ids before the closure). GUESS: the atomicity window is
small but real under rekey.

## U3 — One list query, no ciphertext on the read path

Facts: list() (143-168) duplicates the SELECT, differing only by `kind = $2`; both fetch
`ciphertext, nonce` although only metadata is returned (169, 38-54).

Change: single `SELECT ... WHERE space_id = $1 AND ($2::text IS NULL OR kind = $2)` binding
`Option<&str>`; use a metadata-only row struct so sealed material never enters a read path that
promises no webhook. Keep the `(space_id, kind)` index (0057:16).

Risk: new SQL needs `cargo sqlx prepare` (offline metadata) and an EXPLAIN check that the OR still
uses the index; low functional risk (list tests at `testkit/src/build.rs:10606+`).

## U4 — Honest error semantics and one authority for identity

Facts: corrupt kind / invalid ids map to `AppError::Unavailable` (44, 48, 51), the same variant used
for DB failures (118, 155, 167, 192, 224) — retryable and permanent collapse. create() binds the
context user id (114) but returns `destination.created_by` (124), and ignores `destination.space_id`
(uses context at 70-78, 109): stored and returned rows can disagree and a mismatched caller is
silently accepted.

Change: pick one authority (context wins → overwrite the domain object's `created_by`/`space_id`, or
reject with `AppError::Forbidden`) and bind that same value; give corrupt rows a distinct internal
error plus `tracing::warn!` with ids only, reserving `Unavailable` for transport.

Risk: a new `AppError` variant touches the stable API error shape → DEVIATION per AGENTS.md; prefer
an existing typed variant unless ADR'd. Fixing `created_by` changes stored data for mismatched
callers — audit golden fixtures first.

## U5 — REQ-tagged Postgres tests and secret-safe logs

Facts: no tests in the module; coverage lives in the in-memory testkit and
`infra/tests/notifications.rs`, so sealing round-trips, RLS cross-space isolation and corrupt-row
handling are untested against real Postgres. No tracing on refusals.

Change: `crates/infra/tests/destinations.rs` with `req_065_*` names (plus a note mapping to REQ-064)
using the existing harness (`tests/common/mod.rs:592`): round-trip via `destination_webhook`; list
leaks no ciphertext; cross-space id → NotFound; Slack prefix enforced / Teams https-only; corrupt
kind → non-retryable. Add warn events with ids only — never webhook, ciphertext or nonce.

Risk: testcontainers adds a Docker dependency to CI; reuse the existing harness to avoid a new
moving part (criterion 5: simplicity). GUESS: memory-store tests may mask adapter-only bugs.

## Sequencing

U1 → U4 (domain/errors) → U2 → U3 → U5, each its own commit with `cargo fmt --check`, clippy
`-D warnings` and the REQ-tagged tests. No spec edit needed; the REQ-064/065 label is the only
traceability fix.
