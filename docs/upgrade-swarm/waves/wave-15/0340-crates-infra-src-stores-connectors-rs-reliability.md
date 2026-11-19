# Wave 15 / worker 0340 — `crates/infra/src/stores/connectors.rs` (LENS: reliability)

Target: v0.4.0. Status: **proposal only** (read-only pass; nothing in `crates/` was changed).
Claims marked CONFIRMED were verified by reading the file, `migrations/0034_connector_installs.sql`,
`crates/infra/src/{tenant,space_keys,secret_box}.rs`, `crates/app/src/{ports,usecases}.rs`, and the
in-memory adapter `crates/testkit/src/memory.rs`. Reachability notes are GUESS where not traced.

## Summary

The store is small and mostly correct: parameterized SQL, a real PK, a CHECK-constrained status,
RLS via `with_tenant`, and per-space envelope encryption. Reliability gaps cluster in three areas
below, plus one cross-adapter divergence that is the highest-priority item.

## Failure modes

### 1. Secret handling

- **FM-1 — revoked secrets stay at rest (CONFIRMED).** `revoke` only flips `status` (lines 148–156);
  `sealed_credential` + `sealed_credential_nonce` are untouched. `connected_credential` filters
  `status = 'connected'` (line 194), so the secret is unreachable through the API but remains
  encrypted in the row and in backups forever. No crypto-shredding, no retention/erasure story.
- **FM-2 — System context bypasses tenant scoping (CONFIRMED divergence).** Migration 0034's policy
  is `space_id = current_space_id() OR is_system()`. `install` rejects System (line 79) and the
  memory adapter returns empty/false/None for System in `list`, `revoke`, `is_connected`,
  `connected_credential` (memory.rs:5813–5947), but the Postgres `list`, `revoke`, `is_connected`,
  `connected_credential`, `catalogue_tools` accept System. `connected_credential` uses
  `fetch_optional` with **no `space_id` predicate and no ORDER BY** (lines 191–197) and decrypts with
  the row's own `space_id` (203–209): a System-context call can return an arbitrary tenant's
  credential, and a System `revoke` can revoke every tenant's install of a connector. Parity break
  plus cross-tenant disclosure/blast radius.
- **FM-3 — read path can mint the wrong key (CONFIRMED).** `open_for_space` calls
  `load_or_create_dek`; if the `space_keys` row is missing, a fresh DEK is created
  (space_keys.rs:40–57) that cannot open the old ciphertext, yielding `AppError::Unavailable`
  while `status` stays `connected` — a "healthy" install that silently cannot dispatch
  (usecases.rs:1041–1045). A rotated/wrong master key fails the same way. No quarantine/repair.
- **FM-4 — plaintext lifetime (CONFIRMED, low).** The `Option<SecretString>` lives across validation
  (lines 75, 85–94, 102–107); `secrecy` zeroizes on drop, but there is no explicit drop before
  early returns and no audit event for install/revoke/credential-read (`audit.rs` exists).

### 2. Stale status

- **FM-5 — TOCTOU in dispatch (CONFIRMED).** `is_connected` (169–179) and `connected_credential`
  (189–214) are two separate `with_tenant` transactions. A revoke committed between them lets one
  final tool call run with a freshly read credential; `connected_credential` re-checks status,
  narrowing but not closing the window.
- **FM-6 — install/revoke last-write-wins (CONFIRMED).** `install` upsert sets `connected` (line 114)
  and `revoke` sets `revoked` (line 150) with no row lock or version guard; an in-flight install can
  resurrect a revoked connector. `updated_at` records time, not causality.
- **FM-7 — stringly-typed status (CONFIRMED).** Literal `"connected"`/`"revoked"` comparisons occur
  in six places (59, 114, 150, 171, 194, 223), constrained only by the migration CHECK. Adding a
  third state would silently mean "not installed" everywhere, with no central match to update.

### 3. Duplicate upserts

- **FM-8 — upsert is correct but not idempotent (CONFIRMED).** `ON CONFLICT (space_id, connector_id)`
  relies on the PK (0034:16) — good — but concurrent installs both seal with fresh nonces
  (secret_box.rs:92–101), both return success, and last write wins; ciphertext/nonce/`updated_at`
  churn with no idempotency key. A retried request after a timeout silently re-keys the connector,
  invalidating any holder of the previous credential.
- **FM-9 — upsert can overwrite a concurrent revoke (CONFIRMED).** `install` unconditionally writes
  a new credential even when a revoke is interleaving, so a `revoked` row can carry a live secret —
  amplifying FM-1.

## Concrete hardening for v0.4.0 (with risks)

1. **Close the System-context gap (highest priority).** Reject `TenantContext::System` in the five
   read/write methods as the memory adapter does, *and* add an explicit `space_id = $1` predicate
   to every query for defence in depth. Risk: any legitimate System path would start failing —
   grep shows callers in `usecases.rs` pass member context only (GUESS: other future callers may not).
2. **Crypto-shred on revoke.** In the same UPDATE set `sealed_credential = NULL,
   sealed_credential_nonce = NULL`. Risk: revoke becomes irreversible (re-install needed), which
   matches its documented semantics; verify no rollback path reads a revoked credential.
3. **Collapse check-then-read.** Have `dispatch_connector_tool` call `connected_credential` once and
   require `Some` for api-key connectors instead of calling `is_connected` first (usecases.rs:1029–1045).
   Risk: cross-crate change; surfaces "connected but no credential" as an explicit error instead of a
   silent tool failure — check against REQ-032 parity before shipping.
4. **Never mint a DEK on the read path.** Split `open_for_space` so a missing key row returns a
   distinct "credential unavailable" `AppError`, and emit a `tracing::warn` + audit event on decrypt
   failure (no secret values). Risk: touches shared `space_keys.rs`/secrets primitives; internal
   error-kind change only, no wire-format change.
5. **Add a generation guard for the races.** Add `install_generation` (or use `updated_at` as a CAS
   token) and scope `revoke`/`install` with a compare-and-set so a stale install cannot resurrect a
   revoked row. Risk: new migration; an idempotency key exposed on the API would be a DEVIATION
   requiring `docs/PARITY.md` and the operator's approval — keep it internal for v0.4.0.
6. **Type the status.** Introduce a domain enum plus an exhaustive match helper used by all six call
   sites; keep the DB CHECK as backstop. Risk: low; DTO churn only if status is ever exposed (today
   `ConnectorSummary` exposes only `installed: bool`).
7. **Audit install/revoke/credential-read** as structured events with actor/space/connector id, never
   the secret. Risk: new event names may need a spec/`docs/PARITY.md` note.

**Test-first plan (REQ-032 name tag):** real-DB integration tests for (a) System context yields
empty/false/None and never decrypts, (b) revoke NULLs ciphertext and blocks `connected_credential`,
(c) concurrent install/revoke converges to a single deterministic final state, (d) tampered/missing
DEK returns a typed error and does not leave the install advertised as connected, (e) re-install
rotates the nonce. Risk: testcontainers cost/build time; use the existing `crates/infra/tests/common`.

**Suggested order:** 1 → 2 → 3 → 4 → tests, then 5–7 as follow-ups; items 1 and 2 are the only ones
with security rather than purely reliability impact.
