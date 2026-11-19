# v0.4.0 architecture proposal — `crates/infra/src/stores/messaging.rs`

Lens: architecture. Scope: `PostgresMessagingStore`, the `MessagingStore` port adapter for
messaging installations and their external threads (REQ-048, REQ-005). Read-only analysis;
no source was changed. Status of every statement below: CONFIRMED by reading the 621-line file
unless marked GUESS.

## Current structure (as read)

- `PostgresMessagingStore { pool: PgPool, secrets: SecretBox }` — credential-sealing vault + DB pool.
- Raw `InstallationRow` = 12-element tuple of `String`/`Option<String>`/`bool`; mapped by free
  function `installation()`; `parse_platform()` falls back silently to `ChannelPlatform::Generic`.
- Private helpers: `installation_by_id`, `sealed_credential` (has unused `_webhook: bool`),
  `reveal`, `thread`; plus second-impl `unwrap_or_fresh_in` for space DEK creation.
- `MessagingStore` impl: `create`, `find`, `find_active`, `find_active_by_platform_workspace`,
  `list`, `mark_removed`, `reveal_token`, `reveal_webhook_secret`, `resolve_external_thread`,
  `list_external_threads`, `store_install_state`, `take_install_state`.
- Every query repeats the `tenant::with_tenant(... Box::pin(async move { ... }).map_err(tenant::unavailable))`
  wrapper; the 12-column SELECT text is duplicated verbatim in two methods.

Architecture read: the file is a flat, correct adapter but carries three structural seams —
untyped rows/stringly state, repeated connection boilerplate with duplicated SQL, and multi-step
writes whose atomicity is implicit rather than transactional. v0.4.0 should tighten these without
changing the public port (any behavior change is a DEVIATION per AGENTS.md).

## Concrete upgrades

1. Replace the tuple row with a typed `sqlx::FromRow` struct and a shared SQL constant.
   Define `struct InstallationRow { id: String, ... platform: String, status: String, ... }` deriving
   `FromRow`, and a single `const INSTALLATION_COLUMNS`/query used by `installation_by_id` and `list`.
   Removes the 12-position tuple and the duplicated column list. Risk: SQLx compile-time checking
   needs offline metadata regenerated (`cargo sqlx prepare`) — GUESS that CI relies on it; verify.
   No externally visible change.

2. Make state and platform typed at the boundary. Introduce `InstallationStatus { Active, Removed }`
   with `as_str()`/`FromStr`, replacing magic strings `"active"`/`"removed"` in `create`, `mark_removed`,
   `find_active`, and the SQL predicate. Change `parse_platform` to reject unknown values instead of
   silently returning `Generic` (keep `Generic` only for a real spec value). Risk: existing rows with
   an unrecognized `platform` would start erroring — audit migrations/data first; the stricter parse
   may be an externally visible DEVIATION, so record in `docs/PARITY.md` before shipping.

3. Collapse connection and key-handling duplication. Add one thin generic helper
   `async fn with_conn<T>(&self, context, query: impl FnOnce(&mut PgConnection) -> ...)` (or a small
   local macro) so the `with_tenant`/`Box::pin`/`map_err` wrapper appears once. Unify the DEK logic
   in `reveal` and `unwrap_or_fresh_in` into a single `space_dek(conn, secrets, space_id)`; drop the
   unused `_webhook` parameter and the `let _ = installation_id;` dead binding in
   `resolve_external_thread`. Risk: over-abstraction can hide error context — keep the helper
   minimal and preserve `tenant::unavailable` mapping; low behavioral risk.

4. Enforce explicit transactional atomicity for multi-step writes. `create` (INSERT ... ON CONFLICT
   DO NOTHING + follow-up SELECT conflict check) and `resolve_external_thread` (SELECT → INSERT →
   SELECT) currently rely on one connection but no explicit transaction; wrap each in one transaction
   and prefer `INSERT ... RETURNING` / single re-read over the second SELECT. Use `find_active`
   filtering in SQL (`status = 'active'`) for consistency with `find_active_by_platform_workspace`.
   Risk: isolation/ON CONFLICT behavior under concurrency must be pinned with an integration test;
   changing conflict semantics is externally visible — verify against spec before shipping.

5. Add observability and bounded reads. Instrument the port methods with `tracing` spans carrying
   `installation_id`/`platform` (never secrets or token ciphertext), per AGENTS.md. Apply a typed
   page parameter to `list` and `list_external_threads` instead of the hardcoded `LIMIT 50`.
   Risk: tracing is low risk; pagination changes the port signature and is an API DEVIATION —
   stage it behind approval, or v0.4.0 can ship tracing alone.

## Suggested sequencing

Upgrades 1 and 3 are internal refactors (safe first, each with `cargo fmt --check`, clippy `-D warnings`,
and existing tests). Upgrade 2 needs a data audit; upgrade 4 needs concurrency tests; upgrade 5's
pagination needs a PARITY decision. Keep each as its own Conventional Commit with the REQ ids.
