# Wave 15 — worker 0342 — `crates/infra/src/stores/destinations.rs` — reliability (v0.4.0)

Status: code facts below are CONFIRMED by reading `destinations.rs`, `domain/src/notification.rs`,
`domain/src/lib.rs`, `app/src/usecases.rs`, `app/src/push.rs`, `secret_box.rs`, `tenant.rs` and
`migrations/0057_notification_destinations.sql` in this worktree. Exploitability is GUESS (not
executed). This is a proposal only; no code was changed.

## Scope

The store persists space-scoped incoming webhooks sealed under the space DEK (REQ-065). Reliability
here means: (a) the secret stays a secret, (b) the stored URL cannot aim the server at an attacker
or internal host, (c) bounded fan-out, and (d) failures are observable instead of silent.

## Failure modes

1. Secret leakage / loss (CONFIRMED mechanics, GUESS impact)
   - `NewNotificationDestination.webhook` is a plain `String` (domain line 272); `format_webhook`
     (domain line 132) returns another plain `String`, and line 103 clones it again before wrapping
     it in `SecretString`. Plaintext copies have no explicit zeroization; `secrecy::SecretString`
     does not promise zeroization on drop under default features (GUESS — verify against the pinned
     crate). Minimize copies; prefer a `Zeroizing<String>` at the boundary.
   - `open_with_data_key` (secret_box.rs 126-134) maps *every* failure — wrong/tampered DEK, bad
     nonce, non-UTF8 plaintext — to `AppError::Unavailable`. `push.rs:213` logs a warning and skips,
     so key rotation or ciphertext corruption degrades to silent permanent non-delivery with no
     metric. There is no alarm signal distinguishing "DB down" from "cannot open".
   - `SecretBox::ephemeral()` is accepted whenever `BOMA_MASTER_KEY` is unset (secret_box.rs
     56-61); only a boot-time warning is emitted. A space created with the ephemeral key loses all
     its destinations on restart (GUESS: confirmed by code path, not run).
   - `push.rs:232` calls `expose_secret()` and hands the URL to `post_json`; the transport error is
     logged with `%error` (push.rs 250). If the transport error embeds the URL, the "secret" reaches
     logs. Fix belongs in the transport, but this store should keep the URL out of any log field it
     owns (it currently does not log it — CONFIRMED).
   - `row_into_domain` maps an unknown `kind` or malformed `SpaceId`/`UserId` to `Unavailable`
     (lines 44, 48, 51): corruption is indistinguishable from transient outage, so retries are
     futile and the cause is hidden.

2. SSRF via destination URL (CONFIRMED weak check, GUESS exploitability)
   - Validation is string-only: `format_webhook` strips whitespace, then `starts_with(prefix)` plus
     `starts_with("https://")` and `!contains('@')` (lines 82-102, duplicated in usecases.rs
     9863-9880). No URL parser: scheme, host, port, credentials and redirects are not structurally
     validated.
   - Slack is pinned to the literal prefix `https://hooks.slack.com/`, which does hold the host,
     but nothing checks path/query length or rejects a fragment/userinfo-shaped tail.
   - Teams has `webhook_prefix() == None` (notification.rs 236) and by documented policy accepts
     *any* https endpoint. That admits `https://127.0.0.1/`, `https://169.254.169.254/latest/...`,
     `https://[::1]:8443/` and internal DNS names. `post_json` (push.rs 236) is the sink.
   - `destination_webhook` (lines 199-237) returns the decrypted URL with **no re-validation**.
     A row inserted by migration, backup restore or direct SQL bypasses write-time checks entirely,
     so write-only hardening is not sufficient.

3. Cap bypass / unbounded work (CONFIRMED)
   - The daily cap lives only in `push.rs:187-252`. `count_today` and `record_send` are separate
     calls; `record_send` errors are discarded (`let _ =`, line 242). A failed counter write makes
     the budget look unspent forever (bypass), and concurrent triggers race between read and
     increment (bypass).
   - This store has no per-space row limit (migration 0057 has none) and `list()` has no `LIMIT`
     (lines 144-168): an Owner/Admin can create unbounded destinations, making each trigger load
     and decrypt an unbounded list — a work-amplification/availability issue even with the cap.
   - `create` ignores `destination.space_id` but returns `destination.created_by` while persisting
     `context.user_id()` (lines 114, 124): the returned object can disagree with the stored row.

## Concrete hardening for v0.4.0 (with risks)

- **P0 — Central URL validator in the domain.** Parse with the `url` crate; require https, empty
  username/password, no fragment, port 443 only, length <= 2048; Slack host exactly
  `hooks.slack.com`, Teams host resolved to a non-loopback/non-link-local/non-private/non-reserved
  address via a configurable allowlist. Call it from `create`, `destination_webhook` and
  `usecases::destinations_create`. Risk: Teams policy currently permits arbitrary https hosts —
  an allowlist changes externally visible behavior (DEVIATION, needs approval); provide a config
  knob and document it. DNS rebinding still requires connect-time IP pinning in the transport
  (out of module); disable redirects there.
- **P0 — Re-validate on read.** In `destination_webhook`, run the same validator on the opened
  `SecretString` and return a distinct error on failure; log only the destination id. Risk: a
  poisoned legacy row stops delivering (intended) and needs an operator cleanup path.
- **P0 — Observable decryption failure.** Add a distinct `AppError` variant (e.g. `SecretUnsealed`)
  plus a counter log with no secret material, and surface it in `/readyz` or metrics. Gate prod
  startup on `SecretBox::stable_key()`. Risk: readiness now fails with an unset master key — safe
  but a deployment change.
- **P1 — Bound fan-out transactionally.** In `create`, within the existing `tenant::in_space`
  transaction, `SELECT count(*) ... FOR UPDATE` (or a PG advisory lock) and reject beyond a
  configured per-space max; add a partial unique index on `(space_id, kind, label)` if duplicates
  are not wanted. Risk: count under RLS must use the same setting — it does (transaction scoped,
  tenant.rs 116-135); the new limit may reject existing bulk setups.
- **P1 — Cap atomicity (cross-module note).** Reserve cap slots in one transaction, or make
  `record_send` idempotent with a `(bot_id, day, destination_id)` key and stop swallowing its
  error; this is the real bypass fix and belongs to `push.rs`/`notifications.rs`, not this store.
- **P2 — Consistency.** Return `user_id`/`space_id` from the context rather than the input struct;
  remove the duplicated shape checks from `usecases.rs`; replace `Unavailable` for parse failures
  with a typed corruption error; fold the `for_scope`/`for_birds`/`for_insert` clone chain into one
  value. Risk: none behaviorally beyond error codes.

## Tests to add (REQ-065 names)

`req_065_rejects_ip_literal_teams_webhook`, `req_065_rejects_non443_port`,
`req_065_rejects_fragment_and_userinfo`, `req_065_read_rejects_legacy_poisoned_row`,
`req_065_destination_count_is_bounded`, `req_065_created_by_matches_persisted_row`,
`req_065_tampered_ciphertext_is_distinct_error`, `req_065_ephemeral_key_fails_readiness`.
