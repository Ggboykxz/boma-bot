# Worker #256 — `crates/domain/src/plugin.rs` — reliability lens — v0.4.0 proposal

## Scope and status

Read-only review of `crates/domain/src/plugin.rs` (308 lines) plus its direct consumers
`crates/api/src/public.rs:80`, `crates/app/src/usecases.rs:1308,1333`, `crates/domain/src/lib.rs:95`,
`crates/infra/src/stores/admin.rs:274-304`, `spec/05-known-defects.md` (D-01, D-06) and the two
golden fixtures `plugin-submission{,-invalid}.json`. No code was changed. All claims are
CONFIRMED from those files unless marked GUESS.

This module is the single, IO-free grammar gate for plugin source references (REQ-034). It owns
parsing, canonicalisation (`PluginSourceRef::canonical()`) and the `PluginSourceError` taxonomy;
it does not own submission status transitions or the network fetch. Those two failure modes are
therefore reviewed as *consumer-facing reliability contracts* this module participates in, and
the boundary is stated explicitly rather than pretending the parser owns them.

## What the module does today

`parse_plugin_source` validates `https://<host>/<path>[.git][@ref][#subdir]`: HTTPS-only, no
credentials/query/port, a safe ASCII path (`[A-Za-z0-9-_.]`), a length-capped ref that rejects
`..`, `%#?\`, whitespace and controls, and a `safe_relative_path` subdir. The result is rendered
by `canonical()` as `url@ref#subdir` and used as the install key. 10 unit tests cover the happy
and rejection paths; no property/fuzz test exists despite this being an external-format parser.

## Failure modes

1. **Reason is destroyed at every call site (invalid-submission diagnosis).** The rich 7-variant
   `PluginSourceError` is thrown away: `public.rs:80` uses `let Ok(..) else` → fixed
   `"Invalid source."`, and both `usecases.rs:1308,1333` use `map_err(|_| InvalidName(".."))`.
   REQ-034 requires validation to fail *with the reason*; the golden only checks the generic body.
   Operators and clients cannot tell `NotHttps` from `BadSubdir`, so a bad ref looks like a bad URL.
2. **Ref grammar is incomplete and breaks canonical round-trips (malicious submissions).**
   Ref validation rejects `%#?\`, `..` and whitespace/controls but *allows* `@`, `:`, `*`, `[`, `]`,
   `~`, `^`, leading `.` and trailing `.lock`. A source `repo@a@b` parses with `ref_ = "a@b"`;
   `canonical()` then emits `https://host/repo@a@b`, which does **not** re-parse to the same value
   (`find('@')` splits at the first marker). An unstable install key breaks dedupe and lets the
   reviewed submission differ from what `marketplace.install` later keys on — a submit→review→
   install TOCTOU across the boundary (admin.rs:288 updates only `WHERE status='pending'`). These
   are also characters `git check-ref-format` rejects, so the fetcher may resolve or fail on them
   inconsistently. CONFIRMED for `@` acceptance/round-trip; GUESS that a real git backend misresolves
   the other characters.
3. **Unbounded host / silent normalisation (malicious submissions + install failures).** By design
   (golden fixture) any host is accepted; there is no loopback/private-DNS guard and no port, so a
   submission can point a server-side clone at an internal HTTPS service (SSRF/rebinding surface).
   `input.trim()` and `cleaned.to_string()` normalise the raw input, so two different strings can
   map to one canonical key with no warning. Any `url`-crate normalisation change across versions
   can silently rewrite stored install keys. GUESS: low exploitability today because the fetcher
   keeps a fixed file allow-list, but the parser is where a host policy must be expressible.
4. **No parser-level stress coverage (install failures).** No proptest/fuzz asserts
   `parse(canonical(p)) == p` or that all inputs terminate without panic; the length caps make
   panic unlikely (CONFIRMED by inspection) but nothing enforces the invariant that makes the
   install key trustworthy.

## Concrete hardening for v0.4.0

- **H1 — preserve the reason through consumers.** Keep `PluginSourceError` `Display` strings
  byte-identical (goldens depend on `"Invalid source."` at the HTTP edge) but add a stable
  `code()` / `class()` accessor and carry it in the app error instead of `InvalidName`. Record the
  specific reason in structured logs at the boundary; leave the client body as-is unless parity is
  approved. Add a test asserting each variant maps to a distinct logged code.
- **H2 — close the ref grammar and make `canonical()` total/idempotent.** Reject `@`, `:`, `*`,
  `[`, `]`, `~`, `^`, leading `.`, trailing `.lock`, and leading/trailing `-` in refs, or restrict
  refs to a full 40-hex SHA / validated branch-tag form. Add a proptest asserting
  `parse_plugin_source(&p.canonical()) == Ok(p)` for all accepted inputs — this is the single
  invariant that protects the install key and the reviewed-source equality.
- **H3 — make host policy explicit without breaking the golden.** Add a pure
  `fn is_allowed_host(&self) -> bool` / configurable `HostPolicy` on `PluginSourceRef` (default
  allow-all to preserve `example.test` parity) so infra can deny loopback/private hosts at fetch
  time. Reject or record inputs where `trim()`/URL normalisation changed the source, and pin
  `canonical()` output with golden tests to catch `url`-crate drift.
- **H4 — validate at the boundary for malicious submissions.** Add size/charset caps for `name`
  and `contact` if not already enforced downstream, and keep the parser as the choke point so
  submit/install/review cannot diverge. Cross-layer note (outside this module): pair this with
  D-06 (auth/challenge) and D-01 (shared rate counter); the parser alone cannot stop anonymous
  floods.
- **H5 — regression/property tests.** Keep the 10 `req_034_*` tests; add the H2 round-trip
  property, a fuzz corpus of malformed URLs (no panics, bounded time), and a test that the exact
  source reviewed is the canonical key installed.

## Risks and mitigations

- **Externally visible change (DEVIATION):** H2 rejects refs the golden grammar currently accepts
  and H1 may alter error bodies. Under AGENTS.md, record any wire/format change in `docs/PARITY.md`
  and wait for approval. Mitigation: land H2/H4/H5 (internal validation) first; keep Display and
  client bodies byte-identical; treat H1's client-visible reason text as a separate, approved item.
- **Canonical key migration:** tightening the grammar can orphan previously stored install keys.
  Mitigation: audit stored `source` values (GUESS — storage exists in `plugin_installs.source`) and
  add a dual-read migration window before enforcing H2.
- **Host policy false negatives:** a default-deny policy would break legitimate self-hosted
  plugins. Mitigation: keep H3 default allow-all and opt in per deployment, with an audit event.
- **Over-permissive default:** H3's allow-all leaves SSRF possible if infra forgets to call it.
  Mitigation: make the check part of the fetcher's mandatory path and test that a loopback source
  is refused when the policy is enabled.
- **Test cost:** property/fuzz adds CI time. Mitigation: cap cases and run the fuzz corpus in a
  scheduled job rather than on every commit.
