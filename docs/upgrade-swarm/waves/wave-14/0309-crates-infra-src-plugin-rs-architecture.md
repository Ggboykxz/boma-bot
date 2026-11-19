# 0309 — `crates/infra/src/plugin.rs` — architecture (v0.4.0)

- Worker #309 · Lens: architecture · Target: v0.4.0 · Read-only review; no code was changed.
- Module: `crates/infra/src/plugin.rs` (250 lines) at `before-m5` (v0.3.0-rc5): the GitHub
  acquisition adapter for REQ-034, exported as `GitHubPluginFetcher` from `crates/infra/src/lib.rs:56`.
- Read for context (CONFIRMED by direct reads): `spec/01-requirements.md` REQ-032/033/034,
  `crates/app/src/ports.rs` :2675–2791 (`FetchedPlugin`, `PluginFetcher`, `PluginInstallStore`),
  `crates/app/src/usecases.rs` :1071–1145 (`PluginPlan`, `build_plugin_plan`) and :1325–1443
  (`marketplace_install`, `record_install`), `crates/domain/src/plugin.rs` via wave-12 proposal 0255,
  `crates/infra/src/connectors.rs` (`HttpConnectorRunner`, REQ-032), `AGENTS.md` (Errors / Async /
  Types / Traceability standards).

## Current structure (CONFIRMED)

`GitHubPluginFetcher` holds a `reqwest::Client` (30 s timeout, redirects disabled), two endpoint base
strings, and a `loopback_only: bool` (:38–43). Two constructors build it: `new()` pins the public
GitHub endpoints (:51–56) and the public `with_loopback_endpoints()` re-points the *same logic* at a
wiremock server (:63–75). `get()` (:78–106) is a bounded GET (HTTPS check, UA header, success check,
byte cap). The single `PluginFetcher::fetch()` impl (:109–250) is one 138-line method — it carries
`#[allow(clippy::too_many_lines)]` and runs four ordered phases: (1) resolve ref → SHA and enforce a
full-SHA pin, (2) parse the recursive tree and reject `truncated`/oversized trees, (3) filter entries
to `plugin.json` / `.mcp.json` / `skills/*/SKILL.md` / text companions (skipping blobs, symlinks
`120000`, `..`, and large files), (4) fetch exactly those files into `FetchedPlugin { files,
resolved_sha }`.

Position in the install/runner pipeline (CONFIRMED): this adapter is stage 1 only. `marketplace_install`
(`usecases.rs:1332`) calls `fetch` → `build_plugin_plan` → `remove_plugin_skills` → `teach_skill` per
skill → `mcp_server_add` per HTTP MCP server → `record_install`. There is no plugin *code* runner by
design (REQ-034 no-local-code rule): execution capability is the separate `ConnectorRunner` port
(REQ-032). `FetchedPlugin` is therefore the ABI between acquisition and install and must stay
structure-compatible.

## Guiding principle for v0.4.0

Make acquisition a typed pipeline with one security policy, not one long method guarded by a bool.
Preserve every public type signature, error text, limit value and selection rule byte-for-byte —
`plugin.rs` is where the REQ-034 safety contract is enforced, so any behavioral delta is a DEVIATION
to record in `docs/PARITY.md` before shipping (AGENTS.md). The wiremock constructor is test-only in
intent but public in fact; treat narrowing it as an API change to announce, not a silent edit.

## Concrete upgrades (5), each with a risk

1. **Split `fetch()` into typed pipeline stages.** Introduce private intermediate types —
   `ResolvedCommit { sha }`, `TreeSnapshot { entries }`, `FileSelection`, then `FetchedPlugin` — and
   four small methods (`resolve_sha`, `load_tree`, `select_files`, `download_files`) composed by a
   short `fetch()`. Drop the `too_many_lines` allow; each phase becomes unit-testable in isolation.
   *Risk:* the phase order and the early-return semantics (oversized selection, pin mismatch) are
   load-bearing for "nothing is installed on malformed input" (REQ-034). *Mitigation:* replay the
   existing REQ-034 fetch tests plus golden malformed-source cases before and after, keep error
   strings identical, and add a test that a pin mismatch aborts before any file request is issued.

2. **Replace `loopback_only: bool` with a typed endpoint policy.** Model `EndpointPolicy::PublicGitHub`
   vs `EndpointPolicy::LoopbackTest { api, raw }` (or inject a `UrlPolicy` validator) so HTTPS-only,
   host allow-listing and redirect behaviour are decided in one place instead of being toggled by a
   flag read in two methods (:73, :79, :121). *Risk:* `with_loopback_endpoints` is `pub` and used by
   integration tests; changing its signature breaks callers, and a wrong policy split could let a
   non-HTTPS URL through in production mode (a security regression). *Mitigation:* keep a compatibility
   constructor with the old signature delegating to `LoopbackTest`, add a proptest over URL inputs
   asserting the public policy rejects every non-HTTPS / non-`github.com` URL, and keep no-redirects
   on both policies.

3. **Deserialize GitHub responses into typed DTOs instead of `Value` navigation.** Replace the
   `.get(...).and_then(...)` chains (:131–142, :157–177, :185–218) with `#[derive(Deserialize)]`
   structs for the commit and tree responses (with explicit fields, `Option`/`Vec` where absence is
   meaningful). *Risk:* today missing fields silently default (`entries` → empty, `kind`/`mode` →
   `""`, `size` → `usize::MAX`); typed parsing makes some malformed payloads newly fail. That is the
   intended fail-closed direction but is still a wire-visible behavior change. *Mitigation:* pin each
   default with test fixtures (missing `tree`, missing `sha`, wrong types) so the chosen fail-closed
   outcome is explicit, gate with `tools/trace_check.py` REQ-034 naming, and record any changed
   install outcome in `docs/PARITY.md`.

4. **Centralise resource limits as an injected `FetchLimits` value.** Group `MAX_TREE_BYTES`,
   `MAX_TREE_ENTRIES`, `MAX_FILES`, `MAX_FILE_BYTES` and `COMPANION_EXTENSIONS` (:21–30) into one
   struct carried by the adapter, with `new()` supplying today's exact values. *Risk:* making limits
   configurable can tempt callers to loosen them, weakening the fail-closed acquisition boundary; a
   field-name mix-up could also swap the tree cap with the per-file cap. *Mitigation:* keep the fields
   private with no public setter in v0.4.0, assert boundary values ("limit accepted, limit+1 refused")
   in unit tests, and keep the constants' numeric values verbatim.

5. **Separate error mapping and enrich observability at the boundary.** Today every failure is either
   `AppError::Rule(DomainError::InvalidName("..."))` or `AppError::Unavailable` (:80, :95, :101, :122,
   :138, :147, :164, :174, :184, :222), and only the send failure is logged, without source/SHA context
   (:91). Introduce a private typed fetch error carrying a stable reason, map once into `AppError`,
   and add structured `tracing` fields (`source`, resolved `sha`, phase) on failures. *Risk:* changing
   error variants or message text leaks to clients through the API error format and breaks golden
   fixtures; logging a full source URL risks leaking credential-bearing refs. *Mitigation:* keep the
   exact existing messages, log only the canonical source and SHA (never raw URL credentials), and
   verify the API error contract test plus the plugin goldens before shipping.

## Cross-cutting risks / sequencing

- Sequence 3 → 1 → 4 → 2 → 5: typed DTOs and per-stage tests first (the safety net), then the
  pipeline split, then limits, then the policy type, then error/logging polish.
- Freeze `FetchedPlugin` / `PluginFetcher` signatures and the four selection rules; the install
  pipeline in `usecases.rs` and the domain canonical key (wave-12 proposal 0255) are other workers'
  scope — coordinate rather than duplicate.
- Gate each step with `cargo fmt --check`, `clippy -D warnings`, unit + `tests/` integration, golden
  and OpenAPI contract suites. No new dependency is required for items 1–5.

## Out of scope for this module

The source grammar and canonical key (`domain/src/plugin.rs`, wave-12 0255), the plan builder and
`marketplace_install` orchestration, the `SkillStore`/`McpServerStore` writes, and the connector
runner (`connectors.rs`) are other modules. This proposal only restructures acquisition inside `plugin.rs`.
