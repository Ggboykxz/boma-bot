# 0255 — `crates/domain/src/plugin.rs` — architecture (v0.4.0)

- Worker 255 · Lens: architecture · Target: v0.4.0 · Read-only review; no code was changed.
- Module: `crates/domain/src/plugin.rs` (308 lines) at `before-m5` (v0.3.0-rc5).
- Contract read for context: `spec/01-requirements.md` REQ-034, `spec/golden/plugin-submission.json`,
  `spec/golden/plugin-submission-invalid.json`, `spec/05-known-defects.md` (D-06),
  `spec/TRACE.md` :51, and the consumers `crates/domain/src/lib.rs` :95,
  `crates/api/src/public.rs` :36–83, `crates/app/src/usecases.rs` :427–455, :1281–1341,
  `crates/app/src/ports.rs` :2685–2763, `crates/testkit/src/scripts.rs` :950–966,
  `crates/testkit/src/build.rs` :7896, `crates/api/tests/rpc_surface.rs` :4138,
  `crates/domain/src/ids.rs` :11–56, `AGENTS.md` (Types / Errors / Tests standards).

## What the plugin domain model is today

One value struct (`PluginSourceRef` :18) with three public `String`/`Option<String>` fields, one
seven-variant error enum with a hand-written `Display` (:46–78), one free parser
`parse_plugin_source(&str)` (:115) built on `url::Url`, two length constants (:81–84), one private
`safe_relative_path` helper (:87), and nine REQ-034 unit tests. The module is pure — no IO, clock
or async (correct for `crates/domain`).

The parser is the intended single gate ("every caller validates the same grammar before any
bytes are fetched", :4–6), but the architecture leaves that gate as a *convention*, not a type:

- `PluginSourceRef`'s fields are public and callers build literals directly
  (`testkit/build.rs:7896`, `api/tests/rpc_surface.rs:4138`), so a ref can reach the fetcher
  without ever passing `parse_plugin_source`. This breaks the "make invalid states unrepresentable"
  rule and is the central architecture debt.
- `canonical()` (:30) returns a bare `String` that is a *second type in disguise*: the install key
  and the value stored in `plugin_submissions.source`, `plugin_installs.source` and
  `skills.plugin_source` (`ports.rs:2716/2735/2752`, SQL `plugin_source = $1`), and the argument to
  `remove_plugin_skills(context, &canonical)` (`usecases.rs:1346`). Nothing types the canonical key.
- `PluginSourceError`'s taxonomy and Display messages are read **only inside this file's tests**
  (grep across `crates/` confirms no external use). Every boundary discards the reason:
  `public.rs:80` and `usecases.rs:1308/1333` map any parse failure to a fixed string. The rich
  reason model is dead weight at runtime.
- The host is never exposed. The infra fetcher is told to enforce acquisition policy but gets
  only a canonical string, so it must re-parse to learn the host; explicit default ports
  (`https://host:443/repo`) also slip past `parsed.port().is_some()` (:127).

Pinned invariants for v0.4.0 (do not break): the golden `https://example.test/plugin.git#main`
parses with `subdir == "main"` and `ref_ == None` (:200–205, :202's fragment is the subdir marker);
`@ref` rides inside the *path* (:135–140); `.git` is stripped; canonical form is
`url[@ref][#subdir]`; `MAX_SOURCE_LENGTH = 2048`, `MAX_REF_LENGTH = 200`; the fixed API text
`Invalid source.` (`plugin-submission-invalid.json`). Any text change is a DEVIATION requiring
`docs/PARITY.md` and approval.

## Target structure for v0.4.0 (GUESS)

Keep one pure module and one parse gate, but make the *validated ref* and its *canonical key*
distinct types, shrink or actually surface the error taxonomy, and add parser invariants as
properties. No runtime dependency without an ADR.

## Concrete upgrades, each with a risk

1. **Make `PluginSourceRef` self-validating (parse, don't validate).** Make the fields private and
   add `git_url()/ref()/subdir()` accessors; only `parse_plugin_source` (and a `TryFrom<&str>` alias)
   construct a validated ref. Risk: two public struct literals are CONFIRMED in testkit/api tests;
   they need a staged `#[cfg(test)]` constructor or migration via `parse(...).expect(...)`, and any
   missed site fails to compile (loud, not silent). Keep all canonical strings byte-identical.

2. **Introduce a `CanonicalSource` newtype for the install key.** Have `canonical()` return it and
   carry it through `NewPluginSubmission.source`, `NewPluginInstall.source` and
   `PluginInstallSummary.source`, with `From<&PluginSourceRef>` and a `TryFrom<&str>` so stored
   text can be re-validated. Follow `opaque_id!`-style `serde(transparent)` + `as_str()`
   (`ids.rs:11–56`). Risk: it changes several public signatures and store adapters (medium-high
   blast radius); the text must stay identical or install replacement and `plugin_source = $1`
   lookups break. Round-trip `parse(canonical(p)) == p` must be proven by Upgrade 5 before landing
   (a `/`-bearing ref or `@`-bearing subdir is the likely counterexample).

3. **Stop paying for an error nobody reads.** Either (a) derive `thiserror::Error` with the exact
   existing messages (repo default; deletes ~15 lines of boilerplate, no behavior change), then
   keep one opaque public variant with a private reason used by tests; or (b) surface a stable
   reason code to callers. Risk: (a) is low but tests that assert `PartialEq` on variants must keep
   all seven; (b) changes the pinned 400 body `Invalid source.` — a DEVIATION and a spec/golden
   conflict (REQ-034 says "with the reason", the golden says fixed wording) that must be recorded
   in `docs/STATE.md` and decided by the owner, not guessed.

4. **Expose `host()` and close the default-port gap.** Add `host() -> &str`; reject an explicit port
   equal to the scheme default that currently evades `WithQueryOrPort` (:127), while leaving the
   host allow-list policy to the fetcher (grammar stays minimal). Risk: tightening port acceptance
   changes a syntactic form — if any caller relies on `:443`, record as DEVIATION; the accessor is
   additive and low risk. GUESS: no test covers `:443`; confirm with a differential check against
   the original before changing acceptance. Do not add an SSRF allow-list here without an ADR.

5. **Property/fuzz the parser; keep the module pure.** Add proptest invariants (accepted sources
   have no credentials/query/port/control chars/`..`, stay under the length caps, and round-trip
   through `canonical`), plus a small fuzz corpus; wire into CI. Keep fetching, manifest parsing
   and skip decisions out of `crates/domain` — those live in `usecases.rs::build_plugin_plan`
   (:1116–1241) and stay there in v0.4.0. Risk: proptest is an existing dev-dependency (no new
   runtime dep); a failing round-trip property blocks Upgrade 2 until fixed; a fuzz target adds CI
   time. Example-only tests are not enough for an external-format parser under repo standards.

## Explicit non-goals

No change to golden text, wire strings, error Display wording or canonical bytes; no new runtime
dependency without an ADR; no moving fetch/manifest/skip logic into the domain; no host allow-list;
no change to CORS/rate limiting (D-06) or `marketplace.submit`/`marketplace.install`.

## Evidence

CONFIRMED — read this session: `plugin.rs` :1–308; `lib.rs` :95; `public.rs` :36–83;
`usecases.rs` :427–455/:1281–1341; `ports.rs` :2685–2763; `scripts.rs` :950–966;
`ids.rs` :11–56; both `spec/golden/plugin-submission*.json`; `spec/01-requirements.md` REQ-034;
`spec/05-known-defects.md` D-06; grep showing `PluginSourceError` used only inside `plugin.rs`.
GUESS — that no caller depends on `:443` acceptance and that canonical round-trip holds for all
accepted refs/subdirs; verify both (Upgrade 5 property) before Upgrades 2 and 4 land.
