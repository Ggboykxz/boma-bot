# v0.4.0 reliability proposal — `crates/infra/src/stores/pages.rs` (worker #368, lens: reliability)

Scope: the Postgres adapter for pages and their immutable versions (REQ-037) and its immediate
callers (`app/usecases.rs::pages_*`, `api/public.rs::shared_page`, migration 0037). Read-only
inspection this session: claims about what the code *does* are CONFIRMED; claims about runtime
impact are GUESS unless a test is named. All hardening below is proposal-only.

## What already protects us (CONFIRMED)

- Every method runs inside `tenant::with_tenant` / `tenant::as_system`, so a failed step rolls back
  the whole `put_version` sequence (existence check, counter, page INSERT/UPDATE, version INSERT,
  current-version UPDATE). Do not add a second transaction.
- RLS policies on both tables scope every row to `boma.current_space_id()` or a closed
  `SystemReason`; the public token path uses `SystemReason::SharedPageRead` only.
- Replay is backed by unique indexes: `pages_token_hash` and
  `pages_space_creator_nonce (space_id, created_by, client_nonce) WHERE client_nonce IS NOT NULL`,
  plus `UNIQUE (page_id, version)` on `page_versions`. There is no `list` method on `PageStore`.

## Failure mode 1 — cursor drift

- **C1 — two competing "current version" cursors (CONFIRMED).** `get(None)` resolves the version
  with `SELECT max(version) FROM page_versions` (lines 269–276), but `shared_by_token_hash` joins
  `p.current_version_id = v.id` (line 348). Migration 0037 declares `current_version_id` as plain
  `text` with no FK. Nothing enforces agreement: a member reading a page can see version N while
  the public link renders N-1, or a dangling pointer makes the link a uniform 404 for an existing
  page. GUESS: drift appears after any backfill or alternate writer that appends a version without
  the final UPDATE; no test asserts the two paths agree.
- **C2 — unlocked version-counter read-modify-write (CONFIRMED).** `put_version` computes
  `coalesce(max(version),0)+1` (152–158) and later INSERTs it (188–199) without `FOR UPDATE` on the
  parent page. Two concurrent saves of one page compute the same `next`; `UNIQUE (page_id, version)`
  rejects one and `tenant::unavailable` maps it to `AppError::Unavailable` (503), not a retryable
  conflict. GUESS: intermittent 503s under bot bursts; no corruption because the loser rolls back.
- **C3 — replay check and write live in different transactions (CONFIRMED, usecases.rs:1493–1525).**
  `find_by_nonce` commits before `put_version` starts. Two saves with the same nonce both miss, then
  race on the partial unique index; the loser gets 503 instead of the page its nonce already
  produced. The index is the only backstop and its error is not mapped to a replay answer.
- **C4 — caller clock vs DB clock (CONFIRMED, low).** `created_at` is a caller string bound to
  `timestamptz`, while the final UPDATE sets `updated_at = now()`. `boma.rfc3339` truncates to whole
  seconds, so stored timestamps round-trip at second precision. GUESS: display-order noise only.

## Failure mode 2 — unbounded queries

- **U1 — no pagination in this adapter (CONFIRMED).** There is no `list`/cursor method; reads are
  point lookups on PK / unique indexes. The classic unbounded-scan risk lives in *other* stores
  (see `runs.rs::list`), not here — do not add a page list without a keyset cursor and LIMIT.
- **U2 — `find_by_nonce` has neither `LIMIT 1` nor `ORDER BY` (lines 108–117) (CONFIRMED).** Correctness
  rests entirely on the partial unique index. If that index is dropped, disabled, or bypassed by a
  system-context write, `fetch_optional` takes an arbitrary row. GUESS: nondeterministic replay — a
  client can be handed another writer's stale page.
- **U3 — no size bound on `source` jsonb at the store boundary (CONFIRMED).** Depth/size validation is
  in `validate_page_source` in the usecase, not in the port. Any future direct caller (bot tool,
  backfill, admin path) can persist an unbounded document, and `shared_by_token_hash` returns the
  whole `source` to the public HTML renderer (`api/public.rs:193`) with no read-side cap. GUESS: one
  oversized shared page becomes a repeatable DoS; the public rate limit is per-IP, not per-page.
- **U4 — uncapped public read cost (CONFIRMED).** Every shared-page lookup opens a system
  transaction (`set_config` ×2 + JOIN + commit) with no `statement_timeout` and no route-level
  concurrency cap. GUESS: sustained anonymous traffic starves member writes on the shared pool.

## Failure mode 3 — decode errors

- **D1 — timestamp decode into `String` is brittle (CONFIRMED).** `boma.rfc3339(...)` is an
  `IMMUTABLE` SQL function; `to_char(NULL, ...)` yields SQL NULL, which sqlx cannot decode into
  `String` → `ColumnDecode` → `tenant::unavailable` → 503. The columns are NOT NULL today, but one
  legacy/imported row with a NULL timestamp would make `pages.get` and the public link fail
  permanently instead of returning the required 404/200. GUESS: no such rows exist now.
- **D2 — jsonb validity is not page-shape validity (CONFIRMED).** `version_from` takes a raw
  `serde_json::Value`; jsonb guarantees parseable JSON, not the validated block document. A
  backfill can store arbitrary JSON; the public renderer reads `.get("title")/.get("blocks")`
  defensively but serves whatever type is present. GUESS: confusing output rather than a crash.
- **D3 — uniform-failure contract is not quite uniform (CONFIRMED).** `pages_shared` (usecases.rs:
  1594–1596) short-circuits malformed tokens before hashing and before the DB, so they answer
  measurably faster than well-formed misses; the golden requires unknown/revoked/malformed to be
  indistinguishable in body, status *and timing*. Separately, a decode failure in
  `shared_by_token_hash` becomes a 500 — allowed for a DB outage by spec, not for corrupt data.
- **D4 — counter overflow (CONFIRMED, negligible).** `max(version)+1` is an unchecked `i32` add;
  at `i32::MAX` it wraps in release and panics in debug. GUESS: unreachable in practice.

## Concrete hardening (with risks)

1. **H-C1:** add a deferred FK `pages.current_version_id → page_versions(id)` plus a backfill that
   sets it to `max(version)`; make `get(None)` read `current_version_id` so both paths agree.
   Risk: migration + behaviour change for any currently stale pointer (public view changes) — record
   as DEVIATION if so; FK needs an explicit `ON DELETE` policy.
2. **H-C2:** in `put_version`, `SELECT id FROM pages WHERE id=$1 FOR UPDATE` before computing `next`,
   and map unique violations on `(page_id, version)` to a typed retryable conflict, not `Unavailable`.
   Risk: serialises saves per page and lengthens transactions; verify `FOR UPDATE` under RLS for both
   member and system contexts.
3. **H-C3:** fold replay into the write — `INSERT ... ON CONFLICT (space_id, created_by, client_nonce)
   WHERE client_nonce IS NOT NULL DO NOTHING RETURNING *`, then re-read on conflict. Risk: port
   change and conflict-target coupling to the partial index; concurrent replays start returning the
   stored page (an improvement, but test it).
4. **H-U2:** add `ORDER BY created_at DESC, id DESC LIMIT 1` to `find_by_nonce` and treat >1 row as a
   conflict. Risk: negligible cost; keeps the unique index as the real guarantee.
5. **H-U3:** enforce a documented byte/depth limit on `source` in the domain *and* at the adapter,
   and cap the block count rendered by the public route. Risk: rejects previously accepted large
   pages (DEVIATION check) and needs a stated limit plus a test.
6. **H-U4:** `SET LOCAL statement_timeout` for the shared read and a route-level concurrency cap for
   the public path. Risk: timeouts convert slow reads to 500s; must preserve the uniform 404 for
   misses and never masquerade an outage as "not found".
7. **H-D1/D2:** decode timestamps as `Option<String>` and revalidate the jsonb into the page newtype
   on read, mapping corruption to a typed internal error that the public route turns into its
   uniform 404. Risk: extra deserialisation per read; a typed error variant is an API change — log
   detail at `error` without leaking internals (REQ-054).
8. **H-D3:** always hash and query (dummy hash for over-long tokens) so malformed tokens take the
   same path as misses. Risk: more DB load from garbage tokens; measure before adopting.

## Suggested REQ-tagged tests

`req_037_member_and_shared_read_agree_on_current_version`,
`req_037_concurrent_put_version_never_returns_unavailable`,
`req_037_concurrent_nonce_replay_returns_stored_page`,
`req_037_find_by_nonce_is_bounded_and_deterministic`,
`req_037_null_timestamp_yields_typed_error_not_500`,
`req_037_corrupt_source_jsonb_answers_uniform_404`,
`req_037_malformed_token_timing_matches_unknown_token`,
`req_037_oversized_source_rejected_at_store_boundary`.
