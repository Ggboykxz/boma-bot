# 0526 — `migrations/0057_notification_destinations.sql` — reliability (v0.4.0)

Worker #526 · module `migrations/0057_notification_destinations.sql` · lens: reliability · READ-ONLY
review of `rebuild-workspace/.worktrees/before-m5` (v0.3.0-era tree). Proposals only; no source changed.
Claims tagged CONFIRMED (read this session) or GUESS (inferred); no tests were run (read-only).
**Additive only:** every item adds a column, constraint, index, function, policy, view or comment in
*new* forward-only migrations; nothing in 0057 is edited, dropped or destructively rewritten.
0057's bytes are checksum-verified by `sqlx::migrate!` (`crates/infra/src/db.rs:20`).

## Current structure (CONFIRMED — read this session)

- 25 lines. `notification_destinations`: `id text PK`, `space_id text NOT NULL REFERENCES spaces(id)
  ON DELETE CASCADE`, `kind text NOT NULL CHECK IN ('slack_webhook','teams_webhook')`, `label text
  NOT NULL`, `ciphertext bytea NOT NULL`, `nonce bytea NOT NULL`, `created_by text NOT NULL`,
  `created_at timestamptz NOT NULL DEFAULT now()`, named `CHECK (length(btrim(label)) > 0)` (:3-14).
- Index `(space_id, kind)` (:16-17); RLS `ENABLE` + `FORCE` and one permissive policy
  `USING/WITH CHECK (space_id = boma.current_space_id() OR boma.is_system())` (:19-25). Standard
  tenant shape, same as 0025/0030/0040 — nothing destination-specific.
- Boundaries: `boma.is_system()` is any non-empty `boma.system_reason` GUC (0002_m1.sql:38-42);
  `boma.current_space_id()` is `boma.space_id` (0002:27-31). App sets both transaction-local via
  `set_config(..., true)` (`crates/infra/src/tenant.rs:120-125`) and restricts reasons to the
  8-variant `SystemReason` enum plus a CI scan (`crates/domain/src/tenancy.rs:15-83`,
  `crates/infra/tests/policy.rs:28-54`, D-09). Sealing: AES-256-GCM under the space DEK
  (ADR-028:25-28, ADR-020, `space_keys` in 0026). REQ-065: `spec/01-requirements.md:610-618` —
  "refuses or times out → run unaffected, failure logged". Sibling briefs 0342 (store) and 0527
  (0058 flags) own the app-side and flag vocabulary respectively.

## Failure modes

**Secret storage.** `ciphertext`/`nonce` are unconstrained `bytea`: a truncated blob, a wrong-sized
GCM nonce (12 bytes expected — GUESS, confirm against `secret_box.rs`) or a corrupted tag is
accepted at write time and only fails at unseal, which the app maps to `Unavailable` and skips
(0342:20-24) — silent permanent non-delivery with no schema-level signal (CONFIRMED shape).
No FK ties `space_id` to an existing `space_keys(space_id)`: a restore or direct write can create a
destination whose DEK does not exist, so unseal can never succeed (CONFIRMED absence; GUESS on
frequency). `id` is a bare text PK with no client nonce/idempotency key, so a retried Owner RPC
can insert a second row (GUESS on retry policy). `created_by text` has no FK to `users`, so a stray
or cross-space value cannot be rejected at write time (CONFIRMED; tenant coherence GUESS). Only
`btrim(label)` is checked; label length and `(space_id, kind, label)` uniqueness are unbounded, so
duplicates and fan-out amplification accumulate (CONFIRMED; `list()` has no LIMIT per 0342:56).

**SSRF.** The URL is sealed, so the DB cannot parse or re-validate it: write-time checks are
string-only in the app and the read path does not re-check (0342:37-49, CONFIRMED there). A
poisoned row (migration, backup restore, direct SQL) can therefore point the server at loopback,
link-local `169.254.169.254`, RFC1918 or internal DNS; the sink is `post_json`. The migration layer
can only hold *derived* metadata and diagnostics, never enforce host safety, and cannot see DNS
rebinding (transport concern).

**RLS.** `boma.is_system()` grants a full cross-tenant bypass to *any* non-empty `system_reason`
at the DB level (CONFIRMED 0002:38-42); the closed enum + CI scan protect app paths only, so a
direct `psql`/restore caller can read or write any space's sealed destinations. Pooled connections
leak a stale `boma.space_id`/`system_reason` only if a path fails to use `set_config(..., true)`
(CONFIRMED app path is local; GUESS on other paths). `FORCE RLS` applies to the owner too, so a
`NOT VALID → VALIDATE` run without `system_reason = 'data_migration'` can scan zero rows and
false-pass (CONFIRMED pattern, cf. worker #492). Adding a non-tenant SELECT/UPDATE carve-out to
"fix" delivery would silently break isolation (DEVIATION, forbidden).

## Concrete hardening for v0.4.0 (additive, with risks)

1. **AEAD shape CHECKs.** New file adds named `CHECK (octet_length(nonce) = 12)` and
   `CHECK (octet_length(ciphertext) BETWEEN 16 AND 8192)`, `NOT VALID` → audit → `VALIDATE`.
   Risk: medium — legacy rows with a different nonce length block VALIDATE; confirm the GCM nonce
   size first and never truncate ciphertext (record offenders as DEVIATION).
2. **Space-key presence diagnostic + FK.** Add view `notification_destinations_without_space_key`
   joining `space_keys`; optionally `FOREIGN KEY (space_id) REFERENCES space_keys(space_id) NOT
   VALID` after audit. Risk: medium — a legacy orphan set blocks VALIDATE; ship the view first,
   keep the FK deferred until an operator cleanup path exists.
3. **Bounded label + dedupe.** Add `CHECK (length(label) <= 200)` (keeping the existing btrim
   check) and `CREATE UNIQUE INDEX CONCURRENTLY ... ON notification_destinations (space_id, kind,
   label)`. Risk: medium — duplicate legacy labels collide; CONCURRENTLY cannot run inside the
   migration transaction and needs a staged step; the index changes Owner RPC behavior if dupes
   are relied upon (GUESS).
4. **Write idempotency.** Add nullable `client_nonce text` plus partial `UNIQUE (space_id,
   client_nonce) WHERE client_nonce IS NOT NULL`. Risk: low/medium — legacy rows stay NULL and are
   excluded; requires the app to send a stable key (GUESS that Owner retries exist).
5. **created_by integrity.** Add `FOREIGN KEY (created_by) REFERENCES users(id) NOT VALID`; only
   after audit consider a tenant-coherent composite FK to tie it to `space_id`. Risk: medium —
   stray legacy ids block VALIDATE; users/space coherence is GUESS, so defer the composite FK.
6. **SSRF metadata + read-only diagnostic.** Add nullable `url_scheme text`, `url_host text`,
   `url_port int`, `validated_at timestamptz`, populated by the sealed-write path; add view
   `notification_destinations_untrusted_host` flagging `url_host::inet` inside loopback
   (`127.0.0.0/8`, `::1/128`), link-local (`169.254.0.0/16`, `fe80::/10`), RFC1918 and reserved
   ranges. Risk: medium — metadata is only as honest as the writer and cannot catch rebinding;
   keep the view diagnostic, add no blocking CHECK, never store plaintext URL in a log column.
7. **RLS defence in depth.** Add immutable `boma.is_known_system_reason()` checking the GUC against
   the 8 wire values (`boot`, `authentication`, `webhook_delivery`, `scheduled_job`,
   `data_migration`, `shared_page_read`, `scim_directory_sync`, `account_export`), plus an `AS
   RESTRICTIVE` policy
   `USING (space_id = boma.current_space_id() OR boma.is_known_system_reason())` with matching
   `WITH CHECK`. This ANDs onto the existing policy: member access is unchanged,
   `webhook_delivery` fan-out still works, arbitrary GUC strings are denied. Risk: high if any
   legitimate path uses an unknown reason — enumerate from `SystemReason::ALL`, keep the tenant
   predicate byte-for-byte, and regression-test both member and `webhook_delivery` paths.

## Additive-only guarantee

Items 1/5 add constraints via `NOT VALID → VALIDATE`, 3/4 add indexes or nullable columns, 6 adds
columns plus a view, 7 adds a function and a restrictive policy, 2 adds a view (FK deferred). No
`DROP`, no `ALTER COLUMN ... TYPE`, no RLS weakening, no destructive UPDATE of user data. Ship as
new numbered files (e.g. `0060_notification_destinations_shape.sql`,
`0061_notification_destinations_dedupe.sql`, `0062_notification_destinations_host_audit.sql`,
`0063_notification_destinations_rls_restrict.sql`), never by editing 0057.

## Verification before merge (owner to run; not run in this read-only pass)

- Apply to a fresh DB and a seeded copy via `boma migrate`; assert existing rows unchanged.
- Run audit queries for items 1/2/3/5; record offender counts in `docs/STATE.md`.
- Run every `VALIDATE` under `system_reason = 'data_migration'` (`tenant::as_system`) so FORCE RLS
  sees all rows.
- Exercise REQ-065: Owner create/delete, `webhook_delivery` fan-out under FORCE RLS, and a
  non-system role with no space context denied both read and write; test an unknown reason GUC.

Status: proposal only — no file outside this report was modified.
