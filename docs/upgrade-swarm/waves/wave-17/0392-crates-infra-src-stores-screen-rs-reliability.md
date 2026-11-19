# Worker #392 — `crates/infra/src/stores/screen.rs` — LENS: reliability — v0.4.0 proposal

- **Status:** PROPOSAL (read-only review; no source changed).
- **Evidence:** CONFIRMED by reading this module plus `crates/app/src/ports.rs`
  (`ScreenCapability*`), `crates/app/src/usecases.rs` (`issue_screen_capability`,
  `resolve_screen_capability`, `screen_frame`), `migrations/0018_screen_capabilities.sql`,
  `crates/infra/src/tenant.rs`, `crates/domain/src/ids.rs` and the companion architecture
  proposal #391; items needing a test/oracle run are marked GUESS.
- **Scope:** reliability of the two-method Postgres adapter (REQ-030). Structural/ADR-level
  upgrades are #391's job; this proposal stays on failure modes and containment.

## Summary

The adapter is small and its SQL is parameterized, but three silent conversion/parsing shortcuts
turn bad data into either a generic outage or a wrong capability. The generation round-trip is
lossy in a way that can break the exact-equality revocation check, arbitrary-size byte strings are
accepted straight into a btree PK and into an allocation on every auth call, and a single malformed
row makes `find_by_hash` return an error where its caller promises a uniform 403/`None`.

## Failure modes

### A. Stale frames — generation and lifetime integrity

- **A1 (HIGH, CONFIRMED):** `issue` stores `screen_generation` as
  `i32::try_from(u64).unwrap_or(i32::MAX)` (line 41) and `find_by_hash` reverses it with
  `u64::try_from(i32).unwrap_or(u64::MAX)` (84–85). The revocation mechanism is the *exact*
  comparison `computer.screen_generation != capability.screen_generation`
  (`usecases.rs:6329`). Silent saturation can therefore mint a capability whose stored generation
  is not the one issued against: normally a false rejection, but a false acceptance if the
  computer's generation ever reaches the clipped boundary. The column is `integer`
  (`0018:12`), so values above `i32::MAX` cannot even be represented.
- **A2 (MED, CONFIRMED):** `created_at` is stored but never read by this module or the usecase;
  there is no TTL, no purge path (also #391 item 2). Capabilities stay resolvable until the FK
  cascade deletes the computer/space, so stale bearer rows accumulate indefinitely.
- **A3 (MED, CONFIRMED):** no timeout bounds either query. A slow/locked database hangs the
  authentication path with no deadline, contradicting the AGENTS.md rule of a timeout on every
  outbound call.
- **A4 (LOW, GUESS):** `resolve_screen_capability` re-reads the computer after the hash lookup;
  a generation bump between the two reads is inherently racy. The current post-check closes it,
  but only if the stored generation is faithful (A1).

### B. Unbounded blobs — resource exhaustion

- **B1 (HIGH, CONFIRMED):** `token_hash: Vec<u8>` is bound with no length validation (line 50) and
  `token_hash bytea PRIMARY KEY` has no `octet_length` CHECK (`0018:9`). An oversized hash either
  grows the table without bound or trips Postgres' ~2704-byte btree index-entry limit; the driver
  error is returned as a blanket `Unavailable`. GUESS: confirm the real length emitted by
  `app.services.tokens.hash()` before fixing a constant.
- **B2 (MED, CONFIRMED):** `find_by_hash` copies caller input with `token_hash.to_vec()` (65) on
  every authentication, then binds the copy. The port accepts any `&[u8]`, so allocation size and
  index-lookup work scale with untrusted input.
- **B3 (MED, CONFIRMED):** `created_at: String` is cast to `$5::timestamptz` (48); arbitrary content
  is parsed by Postgres and a malformed value surfaces as generic `Unavailable`, conflating bad
  input with an outage.
- **B4 (LOW, CONFIRMED):** `issue` clones both newtype IDs into owned `String`s (38–39); minor
  avoidable work on the hot issuance path.

### C. Decode errors — malformed rows and misclassified failures

- **C1 (HIGH, CONFIRMED):** row reconstruction uses `ComputerId::new(...)?` / `SpaceId::new(...)?`
  (82–83). `opaque_id::new` rejects only blank strings (`ids.rs:29–35`), so an empty/corrupt id in a
  row propagates a `DomainError` → `AppError` instead of `Ok(None)`. The usecase contract
  (`usecases.rs:6300`) promises `None` for "anything that does not check out", answered with one
  uniform 403; one poisoned row breaks that and can be replayed indefinitely.
- **C2 (MED, CONFIRMED):** a negative or overflowing stored generation silently becomes
  `u64::MAX` (84–85). That fails closed at the equality check today, but it hides corruption and
  would become a false accept if boundary semantics change.
- **C3 (MED, CONFIRMED):** every driver error, including a unique-violation from re-issuing the
  same token hash, is mapped by `tenant::unavailable` to `AppError::Unavailable`. The reusable
  `tenant::is_unique_violation` helper (`tenant.rs:159`) is unused here; replay/collision is
  misreported as a database outage and no metric distinguishes the causes.
- **C4 (MED, GUESS):** `tenant::unavailable` logs `error = %error` at warn (154); sqlx errors may
  embed the connection target. Verify by test that no bound token hash or secret reaches logs
  (REQ-054).

## Concrete hardening (prioritized)

- **P0 — fail closed on decode (low risk).** Treat any row that cannot be reconstructed (blank id,
  out-of-range generation, malformed timestamp) as `Ok(None)` plus a `tracing::warn!` counter with
  no parameter values; replace both `unwrap_or` saturations with checked conversions (C1, C2).
  No behavior change on healthy data.
- **P1 — bound the blobs (low/medium).** Validate `token_hash` length at the port/API edge and add
  `CHECK (octet_length(token_hash) = <confirmed hash length>)`; parse `created_at` into a typed
  value before binding (B1, B3); stop copying the hash by borrowing it in the tenant closure where
  the HRTB signature allows, otherwise cap the copy (B2). *Risk:* a wrong hash-length constant
  blocks all issuance — confirm with the token service first.
- **P2 — bound the wait (low).** Wrap both tenant closures in `tokio::time::timeout` (or set a
  Postgres `statement_timeout`) and map the expiry to `Unavailable` (A3). *Risk:* too-tight a
  deadline turns slow-but-valid auth into 503s; tune from observed p99.
- **P3 — classify and observe (medium).** Use `tenant::is_unique_violation` to give PK collisions
  a distinct typed outcome (no unbounded retry), and emit failure-cause counters for stale
  generation, decode failure, oversize rejection and timeout (C3, C4). *Risk:* a new `AppError`
  variant / different status is a DEVIATION needing PARITY approval — keep the wire format stable
  in v0.4.0 and add metrics only.
- **P4 — lifetime (deferred to ADR).** Widen `screen_generation` to `BIGINT` and reject generations
  above `i64::MAX` at issuance; add a conservative purge of dead rows. Both are covered by #391
  items 1–2; they alter schema/behavior, so they need an ADR and operator sign-off, not a quiet
  fix here (A1, A2).

## Acceptance tests (test-first, REQ id in name)

- `req_030_stale_generation_capability_is_rejected`: a row minted at generation N fails once the
  computer advances to N+1 (guards A1/A4).
- proptest: generations at `i32::MAX`, `i32::MIN`, `u64::MAX` never round-trip to a different
  value; out-of-range issuance returns an error or is rejected at the boundary (A1, C2).
- `req_030_malformed_row_resolves_to_none`: blank ids, negative generation and bad timestamp each
  yield `Ok(None)` with a warning counter, never `Err` (C1, C2).
- `req_030_oversized_token_hash_rejected`: an over-length hash never reaches the index; verify the
  CHECK and that no hash bytes appear in logs (B1, C4).
- `req_030_hash_lookup_does_not_copy_unbounded_input`: allocation bounded by the confirmed hash
  length (B2).
- Forced slow query (testkit/pg sleep) returns `Unavailable` within the configured deadline (A3).
- Duplicate `issue` of the same hash yields the classified outcome, not a masked "outage" (C3).

Bottom line: A1, B1 and C1 are the v0.4.0 reliability blockers — silent generation loss, unbounded
byte strings and decode errors that break the uniform-403 contract. P0–P2 are safe, contained wins;
P3/P4 need ADR/PARITY approval because they change observable errors or the schema.
