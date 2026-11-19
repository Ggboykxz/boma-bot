# Worker #375 — `crates/infra/src/stores/plugin_submissions.rs` — architecture (v0.4.0)

- Module: `crates/infra/src/stores/plugin_submissions.rs` (54 lines, REQ-034); lens: architecture / store structure.
- Mode: READ-ONLY proposal; no code changed, no tests run. Shape/findings CONFIRMED by reading this session
  (module, port, use case, public endpoint, admin store, migration, memory twin, sibling stores).
  Upgrades and risks are GUESS (design inference). No wire/API/data-format change proposed by default;
  anything externally visible is flagged as a DEVIATION needing `docs/PARITY.md` + owner approval.

## 1. Current shape (CONFIRMED)

- One `PostgresPluginSubmissionStore { pool: PgPool }` implements the single-method
  `PluginSubmissionStore::record` (`ports.rs:2227-2237`); `#[must_use] const fn new`, no state, no tests in file.
- Deliberately outside tenant/RLS: bare `.execute(&self.pool)` (L39-51), unlike `plugin_installs.rs:74`
  which uses `tenant::with_tenant`. Justified in the module doc (L3-5): the cookieless path has no space.
  The scope boundary is a comment, not a structure.
- `record` is one INSERT, no transaction, no `ON CONFLICT`. A duplicate `id` (unique violation) and a
  bad `::timestamptz` cast are both flattened to `AppError::Unavailable` via `tenant::unavailable`
  (L51; `tenant.rs:153-156`). No idempotency despite the retrying 429 path (`public.rs:50-57`).
- The write owns 6 of 7 columns; `status` defaults to `'pending'` in migration `0027` (CHECK over six
  values). The same table is read/updated by `PostgresAdminStore` (`admin.rs:232-302`, REQ-058):
  one table, two adapters, two ports, and no shared row type (`stores/rows.rs` has no submission struct).
- All values are strings at the store boundary. `submitter_user_id: Option<UserId>` is hand-stringified
  via `as_ref().map(|user| user.as_str().to_owned())` (L35-38); `created_at` is an app-clock string cast
  `::timestamptz`, while reads use `boma.rfc3339()` for a stable format (`admin.rs:245`).
- Canonicalization and length caps live in `submit_plugin` (`usecases.rs:436-456`), not the store. The
  port doc says "exactly as submitted (parse or not)" (`ports.rs:2203`) but the caller parses and
  canonicalizes (`public.rs:80-83`): the store cannot enforce its own invariant.
- The memory twin mirrors the write and hardcodes `status: "pending"` (`memory.rs:5082-5098`);
  its review queue is a separate port (`memory.rs:8680+`). Error logging is generic
  ("the database refused a query") vs the store-specific warn in `rate_limits.rs:78-82`.

## 2. Architecture findings (CONFIRMED unless marked)

1. **Scope is a comment, not a structure.** A future method could pick `with_tenant` and silently read
   empty under RLS; the wrong choice does not fail loudly. GUESS: make deployment-wide scope the
   adapter's default rather than each method's choice.
2. **Split ownership of one table.** Writer (REQ-034) and reviewer (REQ-058) duplicate the column list
   and the six-value status vocabulary in port docs, use-case validation and the SQL CHECK
   (`ports.rs:4241`, `usecases.rs:9166-9175`, `0027:15`). A rename must move two adapters + memory twin.
3. **No idempotency on a retrying endpoint.** A lost `200` makes a client retry and create a second row;
   the server-generated `sub` id cannot dedupe it. GUESS: any real fix needs a client key (wire change).
4. **Weak types push validation outward.** The store accepts any string, so it cannot guarantee a
   canonical source or a parseable timestamp; bad input degrades to `Unavailable` with no context.
5. **Thin observability.** No store-specific log or counter; `contact`/`submitter` must stay out of logs
   (personal data, per AGENTS.md), which a naive debug log would violate.

## 3. Concrete upgrades for v0.4.0 (with risks)

**U1 — Make deployment-wide scope explicit.** Add a private `insert` helper that owns the bare-pool
write; `record` supplies values only, never picks a scope. Risks: (a) never route through `with_tenant`
— a system context there sets `boma.space_id=''` and RLS hides the row (`tenant.rs:59-62`);
(b) keep REQ-034's "not tenant data" invariant and the `0047`/`0002` policies untouched;
(c) internal refactor, no parity impact.

**U2 — Share a row type and status vocabulary with the reviewer.** Add a `PluginSubmissionRow` and a
typed status (`TryFrom<&str>` over the six CHECK values) plus a pure mapper to `AdminPluginSubmission`
in `stores/rows.rs`; use it here and in `admin.rs`. Risks: (a) mapping must stay byte-identical,
including `boma.rfc3339` output; (b) admin.rs and the memory twin move in the same change;
(c) the enum must accept all six values or goldens break; (d) GUESS: widening the port is a bigger
change — defer beyond v0.4.0.

**U3 — Type the store boundary.** Accept a canonical `PluginSourceRef` string and a parsed
`time::OffsetDateTime` (Rfc3339) for `created_at`, binding formatted values; keep `UserId` conversion
in one helper. Risks: (a) the port struct is shared with the memory twin and every caller — coordinate;
(b) a malformed timestamp must still yield `AppError::Unavailable`, never a clippy-denied panic;
(c) stored strings must stay byte-identical for the REQ-034 golden; (d) GUESS about port-change appetite.

**U4 — Idempotent record for the retrying path.** Start with `ON CONFLICT (id) DO NOTHING` keeping the
same `PluginSubmissionSummary`; only if retry volume justifies it, add a versioned migration with a
natural-key/idempotency hash. Risks: (a) a client-supplied key is a wire change → DEVIATION + owner
approval; (b) any new uniqueness constraint needs migration + backfill for existing rows;
(c) `DO NOTHING` changes error semantics only — keep the id reply identical; (d) GUESS about volume,
measure before adding an index.

**U5 — Store-local errors and honest telemetry.** Replace `map_err(tenant::unavailable)` with a local
helper that logs a submission-specific warn (as `rate_limits.rs:78-82` does) and can emit a counter.
Risks: (a) still return `AppError::Unavailable` to keep the API error format stable;
(b) never log `contact` or `submitter_user_id`; (c) prefer existing `tracing` over a new dependency;
(d) no new metrics endpoint or gate change.

## 4. Ordering, invariants, non-goals

Order: **U1 → U2 → U3 → U5**, with **U4** after an owner decision. Invariants: never tenant-scoped,
status vocabulary and error strings stable, canonical source byte-identical, memory twin in lockstep,
REQ-034 in test names, rustdoc on public items. Non-goals: no RLS/ADR/spec/gate edit, no port split,
no new dependency, no change to the review workflow.

## 5. Evidence & tests to add

- CONFIRMED — read this session: `plugin_submissions.rs:1-54`; `ports.rs:2198-2237,4231-4339`;
  `usecases.rs:427-456,9166-9212`; `public.rs:36-104`; `admin.rs:232-302`; `tenant.rs:41-62,150-156`;
  `rate_limits.rs:27-82`; `plugin_installs.rs:65-102`; migration `0027`; `memory.rs:5018-5098`;
  `rows.rs` (no submission struct); `m1.rs:1853-1888`; defect D-06.
- GUESS — goldens pinning exact strings/reason; client-retry volume for U4; port-change appetite for U2/U3.
- Suggested names: `req_034_record_stays_outside_rls`, `req_034_duplicate_submission_id_is_idempotent`,
  `req_034_review_row_maps_through_rows_rs`, `req_034_record_keeps_canonical_source_and_pending_status`.

Line budget: <120. Status: proposal only; READ-ONLY worker, no code changed and no tests run.
