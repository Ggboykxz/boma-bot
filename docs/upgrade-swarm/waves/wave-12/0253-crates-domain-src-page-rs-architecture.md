# 0253 — `crates/domain/src/page.rs` — architecture (v0.4.0)

- Worker: 253 · Lens: architecture · Target: v0.4.0 · Read-only review; no code edited.
- Module: 337-line pure domain module: 14 limit constants, `ChartPoint`, the five-variant
  `PageBlock` enum (`text`/`table`/`chart`/`link`/`image`), `PageSource`,
  `validate_page_name`, private `href_ok`/`image_src_ok`, the 118-line
  `validate_page_source` write-time contract, and 4 unit tests.
- Spec anchors: REQ-037 / REQ-039 (bot-built pages, share token), AGENTS.md "Types"
  (newtypes; make invalid states unrepresentable), "Errors" (typed errors; never leak
  internals), "Tests" (property/fuzz tests for parsers of external formats), "HTTP"
  (strict deserialization, validation at the boundary).
- "Page" is overloaded in this tree: `page.rs` is a *document*; pagination lives in
  `message.rs` (`MessagePage`, `cursor_from_seq`/`parse_cursor`), `thread.rs`
  (`validate_page_size`, `MAX_PAGE_SIZE`/`DEFAULT_PAGE_SIZE`), and `app/ports.rs`
  (`ActivityPage`). CONFIRMED from greps this session.
- Read this session: `page.rs`, `lib.rs:55-85`, `error.rs` grep, `message.rs:176-204`,
  `thread.rs:40-62`, `app/ports.rs:278-285`, two sibling wave-12 proposals (format).
- CONFIRMED = read this session; GUESS = inferred.

## Structure today (CONFIRMED)

All policy lives in free functions: `validate_page_source(&serde_json::Value) ->
Result<PageSource, DomainError>` serialises the value to measure `PAGE_SOURCE_MAX_BYTES`,
deserialises with `deny_unknown_fields`, then walks every block checking lengths,
allow-lists and finiteness. `PageSource` and `PageBlock` both derive `Deserialize`, so
nothing prevents a caller from building or loading a *non-validated* `PageSource`.
Every refusal is reported as `DomainError::InvalidName(String)` with a formatted reason —
a semantically wrong error type for schema/cap/allow-list failures, and one whose `Display`
prints its payload verbatim (cf. wave-12 doc 0227). The module is correctly pure: no IO,
no async, one crate dep (`crate::DomainError`); that must stay.

Architectural findings:

1. **Validation and data are separable.** The `PageSource` type carries no proof of
   validation; `pages_save` (`usecases.rs:1480`) calls the validator explicitly, but any
   other `serde_json::from_value::<PageSource>` bypasses all limits.
2. **Unsafe values are representable in the type.** `Link { href: String }` can hold
   `javascript:` and `Image { src: String }` can hold `data:text/html`; safety is a
   runtime check that must be remembered at each construction site.
3. **`f64` is unbounded at the type level.** `ChartPoint.value` needs a finiteness and
   `1e15` cap check in the walker; serde accepts non-finite tokens poorly, so the
   contract is split between serde and manual code.
4. **The two "page" concepts share a name but no abstraction.** Pagination limits,
   cursor codec and page containers are scattered across three modules and two crates;
   `next_cursor: Option<String>` is a bare string in both `MessagePage` and
   `ActivityPage` (CONFIRMED), so "is a cursor valid?" has no single home.
5. **Thin test surface.** 4 happy/refusal tests; no property test over arbitrary JSON,
   no byte-pinning of the accepted/rejected boundary, no test tying the block caps to
   the public-wire snapshot.

## Concrete upgrades for v0.4.0 (each with a risk)

1. **Self-validating `PageSource` (parse once, then trusted).** Make the fields private and
   expose `PageSource::try_from(&serde_json::Value)` that performs today's checks; keep
   `validate_page_source` as a thin compatibility shim returning the same type. A parsed
   `PageSource` is then unrepresentable-invalid, matching AGENTS.md "Types".
   Risk: breaks any direct `Deserialize`/struct-literal construction. Mitigate by keeping
   `Serialize` (and a `#[serde(try_from)]` adapter only if a grep proves no caller
   deserialises directly) and pinning the JSON shape with a golden test first.

2. **Typed block payloads: `Href`, `ImageSrc`, `ChartValue`.** Transparent newtypes whose
   `TryFrom`-style constructors enforce `https?://`/`mailto:`, the image allow-list and
   `finite && abs <= 1e15`; convert to `DomainError::InvalidName` with today's exact
   messages. Validation moves from a 118-line walker into the values themselves.
   Risk: wire/serde representation must stay byte-identical and error text must not drift;
   land as one atomic change with the golden test from #1 and a `docs/PARITY.md` note.

3. **Dedicated page-validation error, then map at the boundary.** Replace the catch-all
   `InvalidName(String)` refusals with a `DomainError::InvalidPageSource { reason: &'static
   str, index: Option<usize> }` (static, non-echoing reasons), keeping the rendered message
   unchanged by a single mapping in `AppError`/`ApiError`.
   Risk: new variant ripples into the workspace-exhaustive error mapping and any client
   snapshot; verify no caller matches `InvalidName` for pages before removing it, and
   avoid echoing attacker input (REQ-040 privacy pattern from doc 0227).

4. **Unify pagination behind one domain abstraction.** Add `Cursor` newtype in a small
   `cursor`/`pagination` module with `from_seq`/`parse` methods, a generic `Page<T> {
 items, next_cursor: Option<Cursor> }`, and move `validate_page_size` beside it;
 alias `MessagePage`/`ActivityPage` to `Page<...>` to keep public names stable.
   Risk: touches three crates and the API DTO/OpenAPI snapshot; `next_cursor` text must
   stay `seq:<n>` (REQ-005 test) and REQ ids must remain in test names for trace_check.
   Highest blast radius — GUESS the API effort until `api/` DTOs are re-read.

5. **Property + boundary tests for the external-format parser.** With `proptest`, assert
   the invariants for any generated document: if `validate_page_source` accepts, then
   every href/src is on the allow-list, every value finite and bounded, table rows match
   columns, and all caps hold; add a byte-pinned set of near-boundary accept/reject cases.
   Risk: new dev-dependency (add via `cargo add --dev`, no hand-typed version) and CI time;
   generators must include control characters, non-Latin text and huge counts or the test
   gives false confidence. Tests are additive (low functional risk).

## Sequencing / non-goals

Order: 1 → 2 → 3 (page-document hardening), then 5 (safety net), then 4 (cross-crate
pagination unification). Each step: tests first with REQ-037 in the name, then
`cargo fmt --check`, clippy `-D warnings`, tests. Non-goals here: IO/async/clock in the
domain, redesign of `AppError`/`ApiError` (doc 0169) or the API envelope, new block kinds,
new limits, and any externally visible wire change without an ADR / `docs/PARITY.md`
approval. Do not add the pagination abstraction unless the owner confirms the spec wants
that consolidation (currently a proposal only — log in `docs/STATE.md`).

## Open questions (GUESS where noted)

- Does any caller deserialise `PageSource` directly, bypassing the validator? GUESS no;
  grep `crates/app`, `crates/api`, `crates/infra` before choosing #1's shim.
- Are pages/blocks part of the v0.4.0 wire contract frozen by a golden file? Unverified —
  check `spec/golden/` before changing error text (#2/#3).
- Is pagination consolidation in v0.4.0 scope or a later milestone? Ask the owner (#4).
