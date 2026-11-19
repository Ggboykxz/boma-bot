# 0262 — `crates/domain/src/signin.rs` — reliability lens — v0.4.0 proposal

Worker #262 · module `crates/domain/src/signin.rs` (62 lines) · lens: reliability (state-machine
correctness, exactly-once, stale capabilities, canonicalisation, information leak) · READ-ONLY
review of `rebuild-workspace/.worktrees/before-m5` (v0.3.0-rc5). Spec anchor: REQ-060. CONFIRMED =
read this session; runtime impact = GUESS. Proposal only; no code changed here.

## 1. Current behavior (CONFIRMED)

- Three plain public-data structs: `SignInRequest` (:7-30), `SignInRequestStatus` {Pending, Granted,
  Refused} (:34-41), `NewSignInRequest` (:45-62). All fields `pub`, derived serde, no constructor,
  no `validate_*`, no invariant method — unlike `mcp`/`routine`/`external` neighbours this module has
  zero domain rules.
- `id`, `thread_id`, `origin`, `display_name`, `created_at`, `updated_at` are raw `String`, not the
  opaque newtypes `ids.rs` provides; only `space_id`/`bot_id`/`requested_by`/`granted_by` are typed.
- App mints `id` via `ids.new_id("sin")` (usecases.rs:9739). The store's `resolve` is the only
  single-use guard: `WHERE id=$1 AND space_id=$4 AND status='pending'` → second resolve = `Conflict`
  (infra/stores/signin.rs:156,169; memory.rs:8913). No expiry anywhere.
- `record` is an upsert: `ON CONFLICT (id) DO UPDATE` overwrites space/bot/origin/display_name/
  thread_id while preserving status/granted_by/created_at (signin.rs store :77-84).
- `bots.signIns` lists **all** space requests, newest first, and exposes the raw id in
  `SignInRequestView` (rpc.rs:4104-4127; wire.rs:2114-2127). `resolve_signin` checks only context
  user presence, not `requested_by` (usecases.rs:9765-9775).
- Origin handling: app trims only trailing `/` and `display_name.trim()` (:9749-9750);
  `req_signin_url_must_parse` checks `Url::parse` + non-empty host only (:9791-9808) — despite its
  doc comment promising "absolute https URL", `http://host` passes. Store migration CHECKs only
  status + non-empty origin (0055:16-17); RLS is `space_id = current_space_id() OR is_system()` (:26-29).

## 2. Failure modes

### 2.1 Token replay (CONFIRMED shape, GUESS likelihood)
`id` is the bearer "answer token" for `bots.resolveSignIn`. Single-use is enforced only in the
store; the domain type admits invalid states — Pending + `Some(granted_by)`, Granted + `None`, or
a Granted record whose `origin` was later overwritten by a replayed `record(upsert)` — so a retry of
`signin.site` can mutate a resolved request's target while keeping `granted_by`. The audit trail no
longer matches what was approved. Ids are unguessable (opaque), so external replay is unlikely, but
idempotent retries are realistic (GUESS). No domain state machine to reject illegal transitions.

### 2.2 Expiry (CONFIRMED)
No `expires_at` or TTL on the request or its id. A Pending request and its answer capability stay
live forever; `bots.signIns` has no age filter (default limit 50, :127). Stale pending rows
accumulate (storage/UI growth) and an old id can be answered long after consent context is gone.
Domain is clock-free, so expiry cannot even be expressed today.

### 2.3 Enumeration → unauthorised grant (CONFIRMED gap, GUESS impact)
Any member of the space can list every request (origin, display name, bot, status, timestamps, id),
then call `bots.resolveSignIn` with any pending id. `resolve` gates on `id + space_id + pending`
only — not on `resolved_by == requested_by` nor on a role/bot ownership. This breaks REQ-060's
"through that person's explicit grant": one member can grant or refuse another member's request,
for a bot they may not own. Listing turns opaque ids into usable capabilities.

### 2.4 Normalisation / vocabulary drift (CONFIRMED)
Only `trim_end_matches('/')` is applied; host case, default ports, IDN/Unicode, and scheme are not
canonicalised, so equivalent origins produce distinct requests/grants and break REQ-060's
"same bot needs it later → reuse governed by a grant". Doc promises https; implementation accepts
http. `display_name` trims to a possible empty string (accepted). Status is duplicated as strings
in four layers (domain enum, infra match, RPC match, SQL CHECK); an unknown stored status maps to
`AppError::Unavailable`, masking a data-integrity fault as transient. `created_at`/`updated_at` are
unvalidated strings that can drift from the DB timestamptz used for ORDER BY.

## 3. Concrete hardening for v0.4.0 (ordered by value/risk)

- **A. Domain state machine + fallible constructor (low risk, highest local value).** Add a
  `RequestId` newtype (reuse `opaque_id!`) and `NewSignInRequest::try_new(...)` /
  `SignInRequest::validate(&self)` enforcing: non-empty origin + host, non-empty trimmed
  display_name, Pending ⇒ `granted_by == None`, terminal ⇒ `Some`, and a `resolve(granted, by)
  -> Result<Self, DomainError>` that refuses transitions out of terminal states.
  *Risk:* new `DomainError` variants are outside this write scope; keep additive and call from
  infra `into_domain` + testkit first, then make construction strict.
- **B. Replay-safe recording (medium).** Change `record` to insert-only (`ON CONFLICT DO NOTHING`
  then fetch, or require expected status Pending) so a replayed id cannot overwrite a resolved
  record's target. *Risk:* changes retry/idempotency behavior and the store/app contract — ADR +
  `docs/PARITY.md` DEVIATION, owner sign-off.
- **C. Expiry (medium-high).** Add `expires_at` to the domain + migration + pure `is_expired(now)`
  (callers pass `now`; domain stays pure); gate `resolve` with a typed `Expired` error and sweep stale
  pending rows. *Risk:* migration + wire-visible change (a stale id now fails) — ADR + owner approval.
- **D. Enumeration/authorisation gate (medium-high, product decision).** Require
  `resolved_by == requested_by` (or an explicit approver/space role) in `resolve_signin` **and** in
  the store `WHERE` clause; stop returning the raw id as a capability in `bots.signIns` (scope list
  to the caller's own requests, or return a non-actionable reference). *Risk:* the approval UI must
  still reach the right approver; who may grant is a REQ-060/STATE question — do not guess silently.
- **E. Origin canonicalisation (medium).** Add a domain `Origin` newtype with `normalize_origin`:
  https-only outside loopback (align doc/impl), lowercase host, drop default port, strip trailing
  `/`; use the canonical form as the grant/reuse key. IDN support needs `idna` (cargo add + docs.rs +
  `DEPENDENCIES.md` + ADR). *Risk:* backfill existing rows; stored origin shape changes = DEVIATION.
- **F. Single status vocabulary (low).** Give `SignInRequestStatus` `as_str`/`parse` + serde rename
  and use them in infra/RPC instead of hand-written matches; surface `UnknownStatus` instead of
  `Unavailable`. *Risk:* error-enum + serde wire-shape tests; additive, safe.
- **G. Timestamps (low-medium).** Pure `validate_created_at`/`validate_updated_at` (RFC3339/UTC) or
  validated newtypes; prefer a justified dependency (ADR) over a hand-rolled parser.

## 4. Risks / non-goals / gates

- Domain crate is pure and IO/clock-free (lib.rs:1-8): validators must be shape-only, `now` passed in;
  URL parsing may need to stay at the app boundary if `reqwest` cannot enter the domain.
- Never put a secret *value*, id-as-secret, or personal data in errors/logs; REQ-060's "never
  plaintext to any client" holds. Domain stays `#![deny(missing_docs)]`; clippy `-D warnings`, no
  `unwrap`/`expect`/`panic` outside tests.
- B/C/D/E change externally visible behavior — record in `docs/STATE.md` + `docs/PARITY.md`, wait
  for owner approval. A/F/G are local and reversible. Newtypes ripple into api/infra/testkit, so
  sequence A+F→G→B+E→C+D. `tools/trace_check.py` counts declarations, not comments — REQ-060 in test names.
## 5. Suggested tests (domain unit + proptest)

- `req_060_pending_has_no_grant_and_terminal_has_one` (invariant, proptest over constructors)
- `req_060_resolve_replay_from_terminal_state_is_rejected`
- `req_060_expired_pending_request_cannot_be_resolved`
- `req_060_equivalent_origins_normalise_to_one_grant_key`
- `req_060_non_https_origin_is_refused`
- `req_060_status_parse_as_str_round_trip` (proptest) and `req_060_unknown_status_is_typed_error`

## Bottom line

A data-only struct with no domain invariants: the request id is a bearer answer capability with no
expiry, enumerable and resolvable by any space member, with single-use replay protection living only
in the store and a one-`trim` "normalisation" whose doc contradicts its implementation. A/F/G are
local, reversible wins; B/C/D/E genuinely harden replay, expiry, enumeration and canonicalisation
but change visible behavior and need ADRs plus owner sign-off. All runtime impact above is GUESS
until the proposed tests are run.
