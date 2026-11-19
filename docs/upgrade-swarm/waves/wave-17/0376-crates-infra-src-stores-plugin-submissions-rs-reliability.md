# Reliability proposal — `crates/infra/src/stores/plugin_submissions.rs` (v0.4.0)

- Worker: 376 · Lens: reliability · Module: `PostgresPluginSubmissionStore`
- Status: proposals only (CONFIRMED = read in this session; GUESS = inferred). No code changed.

## Current behaviour (CONFIRMED)

`record()` is a single `INSERT ... VALUES ($1..$6::timestamptz)` on the pool, no transaction,
no `as_system` wrapper (correct — table is deliberately outside RLS), mapped through
`tenant::unavailable`, returning `PluginSubmissionSummary { id }`. Inputs are prepared upstream:
`submit_plugin()` mints `id = new_id("sub")` per call, canonicalises `source`, caps `name`/`contact`,
and takes `created_at` from the app clock as a `String`. The table (migration 0027) has
`id text PRIMARY KEY`, nullable `name`/`contact`/`submitter_user_id`, a `status` CHECK with
`'pending'` default, and `created_at timestamptz`. Review is separate: `admin::decide_submission`
guards on `status='pending'` and returns `Conflict` on `rows_affected()==0` — single-row review
races are already handled there and are out of scope here.

## Failure modes

- **F1 — Duplicate submissions on retry (MEDIUM, CONFIRMED).** The id is generated in the app
  layer, not supplied by the caller, and there is no idempotency key. A client retry after a
  post-commit timeout creates a *new* id and a second row for one logical submission, inflating
  the review queue. Two requests with the same id (test/mocked generators) hit the PK.
- **F2 — Deterministic faults misreported as outage (MEDIUM, CONFIRMED).** `tenant::unavailable`
  flattens *every* `sqlx::Error` — unique violation on `id`, `created_at` cast failure, CHECK
  violation, pool timeout, connection loss — into `AppError::Unavailable` (503). Bad input then
  looks transient, so clients retry, worsening F1, and real data bugs stay invisible to operators.
  A ready-made `tenant::is_unique_violation` helper exists but is unused by this store.
- **F3 — Invalid `submitter_user_id` ref (LOW, CONFIRMED).** No FK to `users`; a deleted or forged
  member id is stored verbatim. Anonymous rows carry `NULL`, so parity is unaffected for the
  cookieless path; only the authed RPC path can leave an orphaned ref.
- **F4 — Untrusted `created_at` string (LOW, CONFIRMED).** A malformed app-clock string raises a
  cast error at the boundary → 503 via F2. Rows inserted in the same clock tick also tie on
  `created_at`; the admin queue orders by `created_at DESC` with no unique tie-break, so boundary
  rows can flap between refreshes (already noted in wave-14/0324 for `admin.rs`).
- **F5 — No write-path observability or bound (LOW, CONFIRMED).** No store-level tracing of
  insert vs. duplicate vs. failure; relies on the generic warn in `tenant::unavailable`. No
  statement timeout of its own (inherits pool acquire timeout).

## Concrete hardening (v0.4.0)

- **H1 — Idempotent insert (addresses F1).** Switch to `INSERT ... ON CONFLICT (id) DO NOTHING
  RETURNING id`; treat `0` returned rows as "already recorded" and return the existing
  `PluginSubmissionSummary`, i.e. a retry is a safe replay. Fully solving at-least-once delivery
  still requires threading a caller idempotency key from `public.rs`/usecases into
  `NewPluginSubmission.id`. *Risk:* changes externally visible retry semantics → DEVIATION in
  `docs/PARITY.md`, needs approval; touches the port and the testkit memory store.
- **H2 — Classify errors instead of flattening (addresses F2).** Match the `sqlx::Error` in this
  store (or add a focused `tenant::map_write_error`): unique/CHECK/cast → `AppError::Conflict` or
  `AppError::Rule`, connection/timeout → `AppError::Unavailable`. Keep driver detail at `debug`
  (REQ-054) and expose only the existing stable envelope. *Risk:* adds error branches to a now
  trivial method; a wrong classification could turn a real outage into a client fault. Verify
  against `AppError::is_client_fault()`.
- **H3 — Referential integrity (addresses F3).** Versioned migration adding
  `submitter_user_id text REFERENCES users(id) ON DELETE SET NULL`, with a matching FK check in
  the memory store. *Risk:* FK on a non-tenant table that references tenant user rows; `ON DELETE`
  behaviour and write cost must be validated; GUESS that no deployment relies on orphan refs.
- **H4 — Trusted timestamp / deterministic order (addresses F4).** Prefer DB `DEFAULT now()` over
  the string cast (drop `created_at` from the INSERT, or type it as `DateTime<Utc>` in the port),
  and add a unique tie-break in the queue query. *Risk:* golden/integration tests that assert a
  clock-supplied `created_at` value may need updating; changing stored timestamps is a DEVIATION.
- **H5 — Observability + bound (addresses F5).** Add structured `tracing` at `debug` with outcome
  (`inserted`/`duplicate`/`failed`) and submission id only — never `contact` or
  `submitter_user_id` (PII, no personal data in logs); document reliance on the configured pool/
  statement timeout. *Risk:* id cardinality in logs — keep at `debug`, do not export as a metric
  label.

## Constraints / non-goals

- Do **not** add a unique constraint on `source`: duplicate submissions of the same plugin are
  intentional and the reply must not reveal whether someone else submitted it (CONFIRMED,
  `public.rs` module doc). Review-race guard stays in `admin.rs`.
- Store remains outside RLS and issues no tenant context — preserve that.

## Tests to add (traceability: REQ-034 in the name)

- `req_034_retry_with_same_id_is_idempotent` (integration) — insert twice, assert one row and the
  same returned id.
- `req_034_duplicate_id_is_conflict_not_unavailable` (unit/integration) — H2 classification.
- `req_034_invalid_created_at_is_client_fault` and `req_034_deleted_submitter_is_nulled`.
- Keep the existing round-trip test in `crates/infra/tests/m1.rs` green.

**Recommendation:** ship H2 + H5 first (low risk, no externally visible change); treat H1/H3/H4 as
DEVIATION-gated for v0.4.0 approval.
