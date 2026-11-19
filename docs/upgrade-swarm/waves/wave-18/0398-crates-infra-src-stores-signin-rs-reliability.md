# 0398 — `crates/infra/src/stores/signin.rs` — reliability lens — v0.4.0 proposal

Worker #398 · module `crates/infra/src/stores/signin.rs` (175 lines) · lens: reliability (replay,
single-use under concurrency, stale capabilities, error classification, unbounded work) · READ-ONLY
review of `rebuild-workspace/.worktrees/before-m5` (v0.3.0-rc5). Spec anchor: REQ-060. CONFIRMED =
read this session; runtime likelihood/impact = GUESS. Proposal only; no code changed here.

## 1. Current behavior (CONFIRMED)

- Three `SignInStore` methods, each one SQL statement wrapped in `tenant::in_space` (a transaction
  that sets `boma.space_id` transaction-scoped, tenant.rs:75-84,107-140). Single-statement writes are
  atomic; no multi-step write exists here.
- `record` (:67-105) is an upsert by id: `ON CONFLICT (id) DO UPDATE` overwrites `space_id`, `bot_id`,
  `requested_by`, `thread_id`, `origin`, `display_name`, `updated_at`, but **preserves** `status`,
  `granted_by`, `created_at` (:77-84). App mints a fresh id per call (usecases.rs:9739).
- `list` (:107-135): `SELECT ... WHERE space_id=$1 ORDER BY created_at DESC LIMIT $2`; limit is
  `i64::try_from(limit.unwrap_or(50)).unwrap_or(i64::MAX)` (:127). Returns the raw id.
- `resolve` (:137-174): conditional `UPDATE ... SET status=$2, granted_by=$3, updated_at=now()
  WHERE id=$1 AND space_id=$4 AND status='pending'`, `fetch_optional`, `None -> AppError::Conflict`
  (:151-169). `into_domain` (:39-63) parses status via a hand match and maps any bad value or bad
  id to `AppError::Unavailable`; all driver errors map to `Unavailable` (tenant.rs:153-156).

## 2. Failure modes

### 2.1 Token replay / consent-integrity corruption (CONFIRMED shape, GUESS likelihood)
Because `record` preserves `status`/`granted_by` but overwrites the grant target, a replayed id for
an already-`granted` row silently rewrites `origin`/`bot_id` while the approval stays. The audit
trail no longer matches what was approved — the exact property REQ-060 says the table guarantees.
Id is minted fresh per call, so external replay is unlikely, but idempotent tool retries are
plausible (GUESS). A concurrent `record` and `resolve` on one id can also interleave: if the upsert
commits while status is still `pending`, `resolve` grants the *new* target; if it commits after
grant, it mutates a terminal row. The current conditional UPDATE alone cannot prevent this.

### 2.2 Single-use races (CONFIRMED: guarded atomically; gap is error fidelity)
Two concurrent resolves on a pending id are correctly serialised: the conditional UPDATE takes the
row lock, one commits, the other sees `status != 'pending'` and returns `None -> Conflict` (:156,169).
So true double-grant is *not* a race here. The reliability gap is that `None` conflates three
distinct states — id absent, wrong space, already resolved — into `Conflict`. That hides tampering
and makes the guard untestable apart from outcome; it also gives callers no way to tell a benign
retry from a cross-tenant probe (ties to 2.4).

### 2.3 Expiry / stale capabilities (CONFIRMED)
No `expires_at`, no TTL column, no age filter. A `pending` request and its id (the answer token)
stay live forever; rows accumulate and `list` has no staleness bound. An old id can be resolved long
after the consent context is gone. The store cannot express expiry because neither the row nor the
domain type carries a deadline.

### 2.4 Enumeration / actor gap at the store layer (CONFIRMED gap, GUESS impact)
`resolve` gates on `id + space_id + pending` only — not `resolved_by == requested_by` and not bot
ownership. `list` returns every space request including the raw id, turning enumeration into a
usable capability: any member can resolve any pending request. This is mostly gated upstream
(usecases.rs:9765-9775 checks only user presence), so the store is the last line of defence it does
not provide.

### 2.5 Reliability extras (CONFIRMED)
- Unbounded `list`: a caller-supplied `limit` collapses to `i64::MAX` (:127) — no ceiling, so an
  oversized request becomes an unbounded scan/allocation (memory/latency risk).
- Misclassified errors: a corrupt status string or an invalid stored id maps to `Unavailable`
  (:44,48-59), a *transient* class that clients retry. A permanently corrupt row will never heal;
  retries amplify load and mask a data-integrity fault.
- Dual clocks: `record` writes `created_at`/`updated_at` from the app-provided string, `resolve`
  writes `updated_at = now()` from Postgres (:76,155). Ordering and any future expiry check then
  mix two clocks; a drifted or non-UTC string distorts `ORDER BY created_at`.

## 3. Concrete hardening for v0.4.0 (ordered by value/risk)

- **A. Insert-only recording (medium risk, highest value).** Make `record` insert-only: plain
  `INSERT ... ON CONFLICT DO NOTHING`, then `SELECT` the existing row, and reject a resolve-target
  mismatch — or require expected status `Pending` in the conflict path. This kills replay
  overwrites (2.1) and the record/resolve interleave. *Risk:* changes idempotency/retry contract;
  needs ADR + `docs/PARITY.md` DEVIATION and owner sign-off.
- **B. Authorise inside `resolve` (medium).** Add `resolved_by == requested_by` (or an explicit
  approver role) to the SQL `WHERE` and scope `list` to the caller's own requests / return a
  non-actionable reference. Closes 2.4 and improves 2.2 error fidelity. *Risk:* who may approve is
  a REQ-060/STATE question — do not guess; approval UI must still reach the right approver.
- **C. Expiry (medium-high).** Add `expires_at` to the migration + domain, gate `resolve` with
  `AND (expires_at IS NULL OR expires_at > now())` returning a typed `Expired`, and sweep stale
  pending rows. *Risk:* migration + wire-visible change (old ids stop resolving) — ADR + approval.
- **D. Typed error mapping (low).** Distinguish `NotFound` (id absent / wrong space) from
  `Conflict` (already resolved), and map corrupt status/id to a permanent `DataCorruption` variant
  rather than `Unavailable`. *Risk:* new `AppError` variants ripple into API mapping; additive.
- **E. Clamp `list` limit (low).** Cap the effective limit to a documented max (e.g. 100) instead
  of `i64::MAX`. *Risk:* silently truncates very large callers — confirm none exist; DEVIATION if
  observable.
- **F. One clock (low-medium).** Use DB `now()` for both timestamps (or validate the app-provided
  RFC3339/UTC before binding) so `ORDER BY` and expiry share a clock. *Risk:* stored timestamp shape
  changes — ADR if externally visible.
- **G. Single status vocabulary (low).** Give the domain enum `as_str`/`parse` and reuse in
  infra/RPC, surfacing the unknown value via D instead of a silent `Unavailable`. Additive, safe.

## 4. Risks / non-goals / gates

- Do not log the id (bearer token), origin or actor in new warn/error paths; keep tenant.rs's
  debug-level error detail and REQ-060's "never plaintext to any client" guarantee.
- `unsafe_code` forbidden; clippy `-D warnings`; no `unwrap`/`expect`/`panic` outside tests; every
  method keeps its `in_space` transaction (never move `boma.space_id` out of tenant.rs, ADR-002).
- A/B/C/E/F change externally visible behavior — record in `docs/STATE.md` + `docs/PARITY.md`, wait
  for owner approval. D/G are local and reversible. Sequence D+G -> E+F -> A -> B -> C.
- `tools/trace_check.py` counts declaration lines, not comments: hardening tests must carry REQ-060
  in the test name. All likelihood/impact above is GUESS until the tests below run.

## 5. Suggested tests (integration, real PG)

- `req_060_replayed_id_does_not_overwrite_granted_target`
- `req_060_two_concurrent_resolves_yield_one_grant_and_one_conflict`
- `req_060_wrong_space_and_absent_id_are_not_conflicts`
- `req_060_non_requester_cannot_resolve`
- `req_060_expired_pending_id_cannot_be_resolved`
- `req_060_list_limit_is_clamped` and `req_060_corrupt_status_is_permanent_error`

## Bottom line

The store gets the hard part right — single-use under concurrency is an atomic conditional UPDATE —
but `record`'s status-preserving upsert lets a replayed id rewrite an approved grant's target, there
is no expiry, actor authorisation lives only upstream, and error/limit/clock handling misclassify or
unbound work. D/G/E/F are local, reversible wins; A/B/C genuinely harden replay, enumeration and
staleness but change visible behavior and need ADRs plus owner sign-off. Runtime impact is GUESS
until the proposed integration tests run.
