# v0.4.0 reliability proposal — `crates/infra/src/plugin.rs` (worker #310, lens: reliability)

## Scope and status (CONFIRMED)
Read-only review of the 250-line module plus its consumers: `crates/bin/src/commands.rs:149-150`,
`crates/app/src/usecases.rs:1114-1413`, `crates/domain/src/plugin.rs:115-179`,
`crates/infra/src/stores/admin.rs:274-304`, `crates/domain/src/mcp.rs:174-204`,
`crates/domain/src/skill.rs:103-111`. No code changed; the only write is this file. Every claim is
CONFIRMED from source read in this session unless marked GUESS.

## What the module owns
`GitHubPluginFetcher` is the sole network ingress for plugins (REQ-034): resolve ref -> SHA on
api.github.com, read the recursive tree, filter to `plugin.json`, `.mcp.json`, `skills/*/SKILL.md`
and text companions, then fetch each from raw.githubusercontent.com. Limits are fail-closed:
5000 tree entries, 4 MiB tree JSON, 40 files, 100 KiB/file, no redirects, no symlinks. Downstream
(`build_plugin_plan`) treats the bytes as inert data: skills become text, MCP servers are HTTPS-only
(stdio refused). The fetcher has no unit tests of its own; integration tests use a scripted fetcher.

## Failure modes

### 1. Unpinned review -> install: the reviewed artifact is not the installed one (critical; untrusted code + sandbox escape)
`fetch()` enforces SHA equality ONLY when `source.ref_` is already a full 40-hex SHA (lines 143-150).
A branch/tag ref or the default `HEAD` resolves afresh on every install; `admin.rs:287-295` stores
only a status, never a resolved SHA, while the install key is `canonical()` (source + ref). The
module doc claims "a plugin can never be swapped out from under the member who reviewed it" —
that holds only for SHA pins. Between approval and install the author can move `main` to different
bytes, bypassing the human review gate. `resolved_sha` is returned (line 247) but `marketplace_install`
ignores it (`usecases.rs:1338-1341`).

### 2. `with_loopback_endpoints` disables every network guard and is public (high; sandbox escape)
`loopback_only = !api_base.starts_with("https://")` (line 73). When true, BOTH the HTTPS check in
`get()` (line 79) and the `github.com` host check (line 121) are skipped. The constructor is `pub`
with no `cfg(test)`, documented "for tests only" — nothing enforces that. Any production caller
passing `http://...` silently gets an SSRF-capable fetcher; the allow-list is a convention, not a
type invariant. GUESS: no current caller does this; the risk is future misuse.

### 3. Untrusted tree paths are interpolated into fetch URLs and local names (high; sandbox escape)
The `path` field comes from untrusted tree JSON and is used verbatim: `relative = &path[prefix.len()..]`
(lines 185-192) and `format!("{raw}/{repo}/{sha}/{path}")` (line 238). Filtering is a literal
`relative.contains("..")` plus a symlink-mode check (`120000`, line 195), but percent-encoded
traversal (`%2e%2e%2f`), `?`/`#` URL delimiters and `//` are never normalized or encoded. A crafted
entry can make the raw host serve a different object than the reviewed path (URL/path confusion) or
break out of the `subdir` confinement. GUESS: depends on GitHub raw normalization; verify with a
local wire server before claiming exploitability.

### 4. No aggregate budget, no body streaming cap, no overall deadline (high; install failures/DoS)
Each `get()` has a 30 s timeout but checks size AFTER `response.bytes().await` buffers the whole body
(lines 84-105). A 40-file plugin means up to 42 sequential requests x 30 s ~ 21 minutes with no
deadline on `fetch()`, and up to 40 x 100 KiB retained plus an unbounded in-flight body. A slow or
hostile source stalls a request/worker and can exhaust memory before the cap applies.

### 5. Failure reason and context are lost (medium; install failures, diagnosis)
Transport errors are the only ones logged (warn, line 91); non-2xx status (94-98), JSON parse
(131-132, 157-158), truncated tree (159-167) and size violations return bare `Unavailable` or
`Rule(InvalidName)`, with no source/phase/SHA/status fields. "Unavailable" conflates network,
malformed JSON and policy refusal, so an operator cannot tell a flaky GitHub from a bad plugin.
Worker #256 flags the same reason destruction at the domain parser.

### 6. Untrusted content crosses as model instructions (medium; untrusted code)
The module fetches bytes without checking they are text or marking provenance; downstream `SKILL.md`
bytes become skill instructions (32 KiB cap) fed to the assistant. There is no execution here
(CONFIRMED), so this is a prompt-injection surface, not RCE, but the fetcher is the only ingress and
offers no content-level signal. Companion count is capped by MAX_FILES, not by total bytes.

## Concrete hardening for v0.4.0

- H1 (closes #1): make pinning the default — resolve the reviewed SHA at approval time, store it with
  `PluginSourceRef`, verify `fetched.resolved_sha` equals it, and attach the SHA to the install record
  and logs. If branch refs must stay, add an explicit `allow_unpinned` flag (default off) with an
  audit event.
- H2 (closes #2): move the test constructor behind `#[cfg(test)]` (or a crate-private type) and make
  endpoints a typed newtype validated at construction so a production fetcher cannot exist without
  HTTPS + `github.com`/`raw.githubusercontent.com`. Test that an `http://` base is refused.
- H3 (closes #3): parse tree `path` as a relative path, URL-encode each segment for `raw_url`, reject
  encoded separators/traversal (`%2e`, `%2f`, `%5c`), NUL and absolute paths, and share the normalizer
  with the domain parser.
- H4 (closes #4): wrap `fetch()` in one `tokio::time::timeout` (~60-90 s), stream bodies with
  `response.chunk()` and abort past `MAX_FILE_BYTES`, and add a total-bytes budget (e.g. 1 MiB)
  checked before each fetch.
- H5 (closes #5): introduce a typed `PluginFetchError { phase, source, resolved_sha, status, bytes }`,
  log it with `tracing` fields at every refusal, keep the client body byte-identical (goldens), and
  distinguish policy refusals from transport failures.
- H6 (closes #6): validate UTF-8 at the boundary (reject binary `SKILL.md` instead of
  `from_utf8_lossy`), tag bytes as untrusted in `FetchedPlugin`, and cap total installed bytes.
- H7: add direct wiremock tests for this fetcher (current tests cover only the scripted fetcher):
  pin mismatch, loopback refusal, encoded traversal, body overshoot, aggregate deadline.

## Risks

- H1 is externally visible (DEVIATION): branch-ref installs become impossible by default. Record in
  `docs/PARITY.md` and wait for approval; consider a dual-read window for stored install keys.
- H2 can break self-hosted/dev endpoints; keep the test path under `cfg(test)` and gate any opt-in
  host policy with an audit event.
- H3 encoding may change how GitHub addresses unusual filenames: verify with wiremock and a real
  public repo before enforcing, and surface any newly skipped skill in `skipped` rather than failing
  silently.
- H4's deadline can cause false refusals on poor networks; pick generous, configurable bounds and
  hold no lock across the awaits.
- H5/H6 must not change golden error bodies (reason text belongs in logs only); stricter UTF-8 may
  reject plugins that currently install with replacement characters (DEVIATION — approve first).

## Verification to add
`cargo test -p boma-infra plugin::` with wiremock for all four phases; a test that a non-SHA ref is
refused by default; a proptest that no crafted tree path escapes `subdir` after normalization; a
hanging-server timeout test; and a log assertion that every refusal carries phase/source/SHA.
