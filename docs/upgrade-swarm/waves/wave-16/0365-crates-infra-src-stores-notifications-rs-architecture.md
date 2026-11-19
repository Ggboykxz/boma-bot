# v0.4.0 architecture proposal — `crates/infra/src/stores/notifications.rs`

Lens: architecture. Scope: `PostgresNotificationStore`, the Postgres adapter for
`ports::NotificationStore` (REQ-064 Web Push, REQ-065 destination flags). Read-only analysis;
no source was changed. Every statement below is CONFIRMED by reading the 404-line module, the
`NotificationStore` port (`crates/app/src/ports.rs:4446-4540`), the domain types
(`crates/domain/src/notification.rs`), migrations 0056/0057/0058, and
`crates/infra/tests/notifications.rs`, unless marked GUESS.

## Current structure (as read)

- `PostgresNotificationStore { pool: PgPool }` — a thin pool wrapper (no `SecretBox`; unlike the
  sibling `destinations.rs` and `messaging.rs`, it seals nothing).
- Two `sqlx::FromRow` structs, `SubscriptionRow` (10 fields) and `PolicyRow` (8), converted by
  free functions `subscription_into_domain` / `policy_into_domain`.
- Port impl: `subscribe`, `unsubscribe`, `list`, `get_policy`, `set_policy`, `record_send`,
  `count_today`, `note_failure`, `note_success` — nine methods, three backing tables.
- All nine repeat the same shape: pull `space_id`/`user_id` from `TenantContext` (missing space →
  `NotFound`, missing user → `Forbidden`), clone them into `for_*` locals for the `'static`
  closure, then `tenant::in_space(&self.pool, &space_id, |conn| Box::pin(async move { ... })
  .map_err(tenant::unavailable))`. CONFIRMED: `tenant::in_space` already opens a transaction
  (`tenant.rs:107-129`), so multi-statement bodies are atomic — this is not a missing-transaction
  defect; it is duplicated ceremony.
- `subscribe` upserts on `(space_id, user_id, endpoint)`, resets `fail_count`, and binds one
  client-supplied stamp (`$8`) to both `created_at` and `updated_at` on insert.
- `get_policy` does `fetch_optional`, then on a miss runs a **second** query
  `SELECT boma.rfc3339(now())` to stamp `NotificationPolicy::defaults`.
- `set_policy` is a COALESCE upsert with `i32::try_from(...).unwrap_or(i32::MAX)` on `daily_cap`.
- `note_failure` does `UPDATE ... RETURNING fail_count` then deletes at `count >= 3`, but the
  DELETE filters on `id` alone; the magic threshold lives only in this adapter's source.
- `record_send` is a bare INSERT on a PK `id`; the app mints a fresh id per delivery
  (`app/src/push.rs:241-245`), so retries are not idempotent today.
- No `tracing` instrumentation in this module.

## Architecture read

The adapter is correct and consistent with the repo's tenant pattern, but its seams are: a row→domain
boundary that mislabels corrupt data as infra failure, nine copies of context/connection
boilerplate with inconsistent error mapping, a two-round-trip default, and write paths whose
idempotency/scoping/threshold rules live implicitly in SQL and call sites. v0.4.0 should tighten
these **without changing the port**: any externally visible change is a DEVIATION per AGENTS.md.

## Concrete upgrades

1. **One decoding boundary, honest error taxonomy.** Both converters map an invalid stored id to
   `AppError::Unavailable`, i.e. corrupt data is reported as "store down"; `fail_count` and
   `daily_cap` use `unwrap_or(0)`, silently changing values on overflow/negative. Route all
   row→domain conversion through a single private `decode` layer that uses the domain's
   `TryFrom<&str>` parsing (proposed in the wave-12 domain proposal) and returns a typed
   data-corruption error, never a silent clamp. Factor the two SELECT/RETURNING column lists into
   shared `const` strings so `list` and `subscribe` cannot drift. Risk: a new `AppError` variant is
   an API-visible change — reuse an existing variant or record a DEVIATION; SQLx offline metadata
   must be regenerated if SQL text changes (GUESS that CI checks it — verify).

2. **Collapse the repeated tenant/context wrapper.** Introduce one helper, e.g.
   `async fn in_context<T, F>(&self, context: &TenantContext, f: F) -> Result<T, AppError>`
   (HRTB over the connection, mirroring `tenant::TenantFuture`) that extracts space+user once,
   maps both missing fields to one documented error, and applies `Box::pin`/`tenant::unavailable`
   once. Keeps RLS and the transaction exactly as today. Risk: HRTB lifetimes are fiddly and an
   over-broad helper can hide which method failed — keep spans/context at call sites; changing
   `NotFound` vs `Forbidden` for a missing user is externally visible, so preserve current mapping.

3. **Make the missing-policy default a single query.** Replace `fetch_optional` +
   `SELECT rfc3339(now())` with one statement (COALESCE of row columns against defaults, or a
   scalar `now()` in the same SELECT), returning a complete `PolicyRow` even when absent. This
   removes a round trip and makes the default consistent with the transaction snapshot. Risk:
   `created_at`/`updated_at`, `notify_waiting=true`, `notify_completed=false`, `cap=20` and both
   destination flags=false must stay byte-identical or REQ-064/065 clients see a DEVIATION;
   verify against `req_064_policy_round_trip_and_the_missing_row_defaults`.

4. **Harden the write paths structurally.** Make `record_send` `ON CONFLICT (id) DO NOTHING` so a
   retried delivery record cannot poison cap accounting with a unique-violation mapped to
   `Unavailable`; carry `space_id`/`user_id` into `note_failure`'s DELETE as defense-in-depth; lift
   the `3` into a named constant shared with the domain `MAX_PERMANENT_FAILURES` from the wave-12
   proposal. Risk: `DO NOTHING` could mask a genuine id-collision bug (ids are freshly minted —
   low risk); the threshold must stay exactly three permanent failures or the deletion test flips.

5. **Add non-sensitive observability.** Wrap the nine methods in `tracing` spans/instruments
   carrying `space_id`, `bot_id`, `kind` and subscription id only — never `endpoint`, `p256dh`,
   `auth_secret` or webhook values (AGENTS.md: no secrets in logs). Risk: low; the discipline is to
   avoid logging whole `NewNotificationSubscription`/row structs, whose `Debug` includes keys.

## Suggested sequencing

1 and 2 are internal refactors (first, with the existing REQ-064/065 integration tests green);
3 and 4 need the byte-identical/pinned-behavior checks above; 5 is safe any time. Each step:
tests named with REQ-064/REQ-065, then `cargo fmt --check`, clippy `-D warnings`, tests, and one
Conventional Commit.

## Open questions (GUESS where noted)

- Is a new `AppError` data-corruption variant acceptable, or should decode failures keep mapping
  to an existing variant? (Decision needed.)
- Should stored timestamps move to DB `now()` rather than the client stamp in `subscribe`? That is
  externally visible via `list`/`get_policy` — log as a proposal in `docs/STATE.md` and seek owner
  approval before treating it as v0.4.0 scope.
