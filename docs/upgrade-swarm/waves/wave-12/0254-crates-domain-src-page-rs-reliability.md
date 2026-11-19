# Worker #254 — `crates/domain/src/page.rs` — reliability (v0.4.0 proposal)

Scope: read-only review of `page.rs` (337 lines) plus its callers (`app/usecases.rs`,
`api/public.rs`, `api/pages.rs`) and the sibling pagination rules (`message.rs`, `thread.rs`).
All findings marked CONFIRMED were verified by reading those files this session; impact claims
marked GUESS were not executed. This module validates bot-built page *documents*; it owns no
cursor or page-size logic, so the three named failure modes are mapped onto its actual contract.

## Failure modes

1. **HIGH — validate-on-write / serve-on-read gap (CONFIRMED).** `validate_page_source` is called
   only in `pages_save` (`app/usecases.rs:1480`). `pages_shared` (`usecases.rs:1590-1602`) returns
   the stored `version.source` verbatim, and `api/public.rs:186-197` /
   `api/pages.rs:40-47` render it to anonymous clients. The module doc (lines 5-6) asserts that
   "what the public route serves was already validated once" — an invariant, not an enforced
   check. Any write outside `pages_save` (migration, import, direct SQL, older version) serves
   unvalidated block JSON through a cached, cookieless route. This is the reliability analogue of
   an "unbounded page": the boundary check happens at a different point than the trust boundary.
2. **MED — trimmed validation, untrimmed persistence (off-by-one, CONFIRMED).** Line 167 takes
   `let title = source.title.trim()` for the empty/length checks, but line 272 returns the
   original `source`. A title of exactly 200 chars plus surrounding spaces passes and is stored
   with >`PAGE_TITLE_MAX` chars; the render path reads the raw stored title
   (`public.rs:189-193`). No control-character check exists for the title (only line 118 for the
   name). Downstream consumers that assume "title <= 200 chars" are violated (GUESS: no consumer
   currently relies on it, but the invariant is documented at line 15).
3. **MED — unbounded chart points (CONFIRMED).** The chart arm (lines 222-245) requires >=1 point
   and bounds each label/value, but never bounds `points.len()`. Tables have `PAGE_MAX_ROWS=500`
   and `PAGE_MAX_COLUMNS=12`, blocks `PAGE_MAX_BLOCKS=50`; charts have no equivalent. The only
   ceiling is `PAGE_SOURCE_MAX_BYTES=256 KiB`, so ~1.5k+ tiny points are accepted and inflated
   into an HTML island plus one DOM row per point.
4. **MED — size measured on re-serialized bytes (CONFIRMED).** Lines 157-163 check
   `serde_json::to_string(value).len()`, i.e. normalized bytes, not the accepted wire bytes.
   serde_json collapses whitespace and `\uXXXX` escapes (CONFIRMED by the APIConfig bound of
   1 MiB at `api/config.rs:83`, so a ~1 MiB body can normalize under 256 KiB — GUESS: exact
   amplification depends on the payload). The path also allocates the serialized string, a full
   `value.clone()`, and a `PageSource` before rejecting.
5. **LOW — error taxonomy (CONFIRMED).** Every source failure returns
   `DomainError::InvalidName` (lines 106-124, 155), so callers cannot distinguish a bad page name
   from a malformed block; error mapping and logs lose signal.
6. **LOW — boundary tests incomplete (CONFIRMED).** `req_037_the_block_cap_is_enforced`
   (331-336) and `req_037_the_refusals_are_exact` (311-329) only test overflow by one / mixed
   invalid cases. No test asserts "exactly at the limit is accepted" for any cap, so a future
   `>` -> `>=` edit is unguarded — the classic off-by-one regression.
7. **RELATED, outside this file — negative cursors (CONFIRMED by code read).** `page.rs` has no
   cursor or pagination: a whole page is one bounded document, so a negative index cannot occur
   here. The real cursor parser is `message.rs:198-204`: `raw.parse::<i64>()` accepts `seq:-5`,
   and `thread.rs:54-62` rejects page size 0/>100 correctly but page reads take an unguarded
   `Option<i32>` version (`usecases.rs:1538`). GUESS: a negative cursor fed to a `seq < ?`
   query may replay the newest page and spin a client. Needs a shared domain guard, coordinated
   with the owner of `message.rs` / the messages route.

## Concrete hardening (in priority order)

1. Re-validate on read: in `pages_shared`, run `validate_page_source(&source)` and map `Err` to
   `Ok(None)` so the uniform 404 of REQ-037 is preserved. Cost: one serde pass over <=256 KiB.
   Add a route-level test that a tampered stored row answers 404 with the public headers.
2. Validate the exact persisted string: either assign `source.title = source.title.trim().to_owned()`
   before `Ok(source)`, or reject when `trim() != raw`; add a control-character check for the
   title mirroring line 118. Add `req_037_title_boundary_is_exact` (200 accepted, 201 and padded
   rejected).
3. Add `PAGE_MAX_POINTS` (suggest 500, matching tables) and enforce it in the chart arm with an
   error naming the limit; add `req_039_chart_points_are_capped`.
4. Count accepted bytes at the boundary, not normalized bytes: have the API pass the raw body
   length into the domain check, or switch the domain entry point to `from_slice` on the original
   bytes and check `.len()` before cloning. Keep `DefaultBodyLimit` as the hard outer cap.
5. Table-driven boundary suite for every cap (name, title, blocks, text, columns, rows, cells,
   points, label, image bytes, value) asserting limit accepted / limit+1 refused, plus a
   `proptest` that arbitrary `serde_json::Value` never panics and that every accepted source
   round-trips through `serde_json` unchanged. Names carry REQ-037/REQ-039.
6. Introduce `DomainError::InvalidPageSource(String)` and use it for block failures; keep
   `InvalidName` for names only.
7. Coordinate a `seq >= 0` guard in `parse_cursor` and `version >= 1` validation for page reads
   with the sibling module owners; do not edit those files from this wave.

## Risks / deviations

- Re-validation on read turns a previously-200 corrupt row into a 404: intended, but record in
  `docs/PARITY.md` as a behavior change and check golden fixtures for stored titles with padding
  before merging (steps 1-2 together may break a golden that currently passes an over-long title).
- Rejecting padded titles and >500-point dashboards narrows a SHOULD feature; pick the point cap
  generously and log the reason. Both are externally visible and need parity sign-off.
- A new error variant may alter the public API error envelope; keep the wire mapping stable.
- Byte-count change touches the domain signature and its callers (`usecases.rs:1480`) — internal
  only, but run the full contract suite.
- No code was changed by this worker; all impact statements about runtime behavior beyond the
  cited code are GUESS until the tests above are run by the owning build agent.
