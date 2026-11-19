# Boma Bot v0.4.0 upgrade synthesis — decision-grade plan

Status: PROPOSAL only. This document is the single artefact written by the lead architect of the
upgrade swarm. All 530 source briefs were read-only reviews of the `before-m5` worktree at
`53fe72b` (v0.3.0-rc5). No product code, spec, golden, dependency, CI file or migration was edited.

## 1. Executive summary

The swarm reviewed 265 modules through two lenses (architecture, reliability) and converged on a
clear picture: the v0.3.0-rc5 codebase is structurally sound and parity-conscious — typed errors,
`thiserror`/`anyhow` boundaries, fail-closed RLS, BoardUI tokens, goldens, CI gates — but its
correctness and security rest on *conventions that nothing enforces at read time, in the database,
or across surfaces*. The 530 briefs repeatedly describe the same small set of defects wearing
different module names: a system-context RLS bypass keyed on a plain GUC string with no tenant
bound (`0266`, `0412`, `0432`); validity checked only on write, never on read (`0236`, `0388`,
`0338`); a wire/serde contract that can drift with no compiler or contract-test signal (`0158`,
`0142`); distinct failures collapsed into one opaque `Unavailable`/`400` (`0136`, `0388`, `0280`);
TOCTOU/lost-update races on "approve/move/dispatch" paths (`0270`, `0244`, `0148`); unbounded
bodies, queries and memory (`0149`, `0160`, `0148`); secrets echoed into logs and errors (`0136`,
`0132`, `0151`); an O(N²) heartbeat storm with no SSE replay (`0148`, `0147`, `0290`); fail-open
offline fallbacks and swallowed errors (`0122`, `0070`, `0076`); and constants duplicated across
CI, Helm, router and config (`0001`, `0413`, `0150`). The recommended v0.4.0 response is therefore
not a rewrite: it is a short P0 of cheap, reversible, no-wire-change hardening, followed by a P1 of
reliability work whose wire-visible parts sit behind an explicit owner decision register (§5), and
a P2 of structural deduplication. Migrations stay additive throughout (§6).

## 2. Methodology & coverage

- **Inventory:** `docs/upgrade-swarm/briefs-upgrade.jsonl` — 530 JSONL records, ids `1..530`, fields
  `id`, `module`, `lens`, `slug`. 265 unique modules × 2 lenses (265 `architecture`, 265
  `reliability`). Verified: `jq -r .lens … | sort | uniq -c` and `jq -r .module … | sort -u | wc -l`.
- **Provenance per brief:** each file under `docs/upgrade-swarm/waves/wave-1` … `wave-22` is named
  `<id>-<slug>.md`, cites its inventory id and module in its header, records which real files it read
  this session, and labels findings CONFIRMED (read) vs GUESS (inferred). Workers were forbidden to
  touch product files; this document is the only write.
- **Sampling:** broad reading across every area — CI/release (`.github/workflows/*` `0001`–`0004`,
  desktop `0005`–`0008`), web screens + libs (`0043`–`0116`), api/app/domain crates and stores
  (`0121`–`0410`), stores (`0357`–`0410`), infra/tenant (`0411`–`0412`), Helm/compose
  (`0119`/`0120`, `0413`/`0414`), migrations (`0420`–`0530`) — with targeted deep reads of the
  highest-signal briefs (`0136`, `0148`, `0158`, `0236`, `0266`, `0388`, `0412`, `0413`, `0485`).
- **Charter compliance:** no upstream content, no new dependency, no weakened gate, no spec/ADR edit;
  BoardUI semantic-token and composite-type rules preserved (several briefs explicitly audit them,
  e.g. `0073`, `0075`, `0101`, `0080`). The fenced BoardUI block in `AGENTS.md` stays authoritative.

## 3. Cross-cutting themes

**T1 — System-context RLS bypass is a string, not a capability.** Areas/REQ: tenancy, all stores,
migrations; REQ-043, REQ-037, REQ-044/045, D-09. Evidence: `0266` (System context carries no space;
`is_system()` lifts isolation on every table), `0412` (bypass keyed on non-empty `boma.system_reason`
GUC, no per-caller allow-list or audit), `0432` (any connection can `SET` the two-part GUC; FORCE RLS
still doesn't bind BYPASSRLS), `0444` (FK/RI checks bypass RLS → cross-tenant `routine_id` nulling).
Fix: cheap reversible first — combine the two `set_config` calls into one statement, assert the
space⊕system XOR invariant, make the ADR-002 source-scan real, assert the pool role at startup
(`0412` H1/H2/H3/H5); then bind scoped reasons to a space behind private factories and a per-reason
capability in RLS (`0266` H1–H3), owner-gated as a DEVIATION.

**T2 — Validity enforced at write, not read.** Areas/REQ: domain ids, row mappers, CHECK constraints;
REQ-009/010, REQ-048, REQ-040. Evidence: `0236` (`#[serde(transparent)]` deserializes straight into
the inner `String`, bypassing `new()`), `0206` (same hole for `Email`, so a malformed identity passes
REQ-040), `0388` (NULL→empty id, `unwrap_or(MAX)` signed→unsigned saturation, `null`→`bot thread`
coercion), `0338` (CHECK guards writes only, not reads). Fix: manual `Deserialize`/`try_new` at trust
boundaries; fail-loud typed errors at read mappers after an audit query; keep permissive paths for
storage-shaped values so wire parity is preserved.

**T3 — Serde/wire drift and a hollow OpenAPI contract.** Areas/REQ: api wire/openapi/client, web
`rpc.ts`; REQ-001/005/010/011, REQ-045, ADR-003/018. Evidence: `0158` (three inconsistent
null-vs-absent styles, no `#[serde(default)]` round-trip, ~100 views with no `ToSchema`, four
`serde_json::Value` blobs), `0142` (generated document not compared to `spec/openapi.yaml` in the
required direction), `0141` (metadata/tags drift, duplicate `ops` tag), `0124` (hand-written TS
capability interface with unchecked cast). Fix: generate the client from the served document; add a
round-trip proptest for every reply, register views with `ToSchema`, strict golden-only deserializer,
and a bidirectional contract test.

**T4 — Error taxonomy collapse (everything → `Unavailable`/`400`).** Areas/REQ: api/app/domain
errors, all adapters; REQ-027/040/052/053/058. Evidence: `0136` (every `AppError::Rule`→400, same
error→409 at `rpc.rs:1636`; `unavailable()` is `INTERNAL`+503), `0388` (a corrupt row mapped to 503
looks transient), `0280`/`0272` (connector and artifact I/O all collapse to one opaque `Unavailable`),
`0228` (domain classification lives outside the module and collapses). Fix: typed non-retryable
`Corrupt`/`DataIntegrity` and distinct permanent-vs-transport errors; central status table; reserve
`Unavailable` for genuine pool/IO faults; keep the two-field envelope and existing external mapping
until a DEVIATION is approved.

**T5 — TOCTOU, double-approval and lost-update races.** Areas/REQ: tickets, memory, plugins, routines,
router, web composer; REQ-012/017/038, D-02. Evidence: `0270` (`move_to` last-writer-wins + read-back
TOCTOU), `0244` (memory `revision + 1` with no expected revision → lost updates), `0256` (plugin
install TOCTOU across the app/store boundary), `0148` (SSE producer detached, no cancellation),
`0068` (client nonce minted per call, so server dedup cannot collapse two sends). Fix: single
`UPDATE … WHERE expected=… RETURNING` statements, idempotency keys held once per logical action,
atomic claim/lease for dispatch (`0442`), and tracked tasks drained at shutdown.

**T6 — Unbounded bodies, queries and memory.** Areas/REQ: router, screen, export, realtime, metrics;
REQ-053, D-07, spec §4.3. Evidence: `0148` G4 (history replay loads whole range, `after_seq: 0`
unbounded), `0160` (`ArtifactContentStore::read` returns unbounded `Vec<u8>` before caps apply),
`0154` FM5 (only the request body is capped, not response memory), `0149` (eight inline body-cap
literals), `0134` (raw path fallback → unbounded metric cardinality). Fix: one typed body-cap policy
table, bounded/paginated replay, streaming with a byte budget, and a shared containment/deny table.

**T7 — Secret and PII leakage via Debug, logs and echoed errors.** Areas/REQ: config, error, rpc,
correlation, auth; REQ-054/045, D-02. Evidence: `0136` F1 (`ApiError::explained(String)` emits text
verbatim with no cap; derived `Debug` could print an echoed key), `0132` (redaction test covers only
part of `HttpConfig`), `0151` (logging any body would leak `botSecrets`/MCP secrets), `0133` (a path
can reach a log line; prefix redaction must be proven). Fix: one bounded `explained_reason(&'static
str)`, log domain detail server-side only, extend `secrecy::SecretString` coverage and the redaction
test, keep request ids/identifiers only at debug.

**T8 — Timer/heartbeat O(N²) storms and SSE replay gaps.** Areas/REQ: api realtime, infra events,
web run-status; REQ-005/020. Evidence: `0148` S1/S2 (per-subscriber ticker → N appends × N broadcasts
per 15 s), `0147` (heartbeat persisted per subscriber), `0290` (swallowed `Lagged(_)` with no resync),
`0148` G1/G2 (no `id:`, no `retry:`, no gap frame). Fix: one shared per-thread heartbeat task with
teardown on last subscriber (or SSE comment keepalives, an owner decision), `id:`/`retry:` frames,
`Lagged`→replay `events.since(last_seq)`, `after_seq` validation and a replay watermark; subscriber
and queue-depth caps with metrics.

**T9 — Stale/offline fallbacks that fail open and swallowed errors.** Areas/REQ: auth, authcap,
endpoint safety, connectors, web screens; REQ-040/053, D-01. Evidence: `0122`/`0124` (`local_first`
is an unauthenticated implicit-owner/fail-open flag), `0287`/`0288` (optional SSRF checker port =
fail-open; redirect chain can fail open), `0070` (`allSettled` swallows total failure), `0076`/`0080`
(429/5xx/offline rendered as "link does not exist"; no error boundary), `0232`/`0294` (credential
reveal and offline fallback swallowed with no stability signal). Fix: remove fail-open optionals,
classify errors by code, surface a visible retry, add timeouts and `navigator.onLine`, and emit a
metric on every swallow.

**T10 — Migration checksum freeze: all changes additive.** Areas/REQ: every migrated table;
REQ-043/008. Evidence: `0485` (SQLx `migrate!` verifies checksums; editing 0037 bytes breaks every
deploy; ship new `0059_*`/`0060_*` files only), `0434` (0010 immutable), `0425` (new constraints
`NOT VALID` then validated after audit), `0512` (constraint drop/re-add "not strictly additive" →
defer to ADR), `0490` (fast-default PG-version assumption must be confirmed). Fix: new additive files
only — create objects, add nullable/defaulted columns, `NOT VALID`→`VALIDATE`; never DROP/RENAME/
ALTER TYPE/backfill in place; keep RLS expressions and FK cascades byte-compatible.

**T11 — Config and constants duplicated across surfaces.** Areas/REQ: CI, Helm, router, config,
correlation; ADR-002/016/018, REQ-053. Evidence: `0001` (two toolchain sources, coverage/trace
budgets hardcoded in CI but parameterised in Make, ADR-018 contract job missing), `0413` (three
copies of the chart version, hidden Postgres fork), `0150` (layer stack duplicated across root and
automation routers), `0132`/`0131` (config defaults duplicated, dangling constant). Fix: one home
per constant — `tools/ci.env` or Make targets for budgets, `.Chart.AppVersion` default for the image
tag, one `apply_common_layers` helper and one reserved-prefix table, typed validated config.

**T12 — Missing error boundaries and timeouts (mostly web).** Areas/REQ: all client flows; REQ-001/
012/037/040/055, WCAG 2.2 AA. Evidence: `0080` (no `error.tsx`, uncaught `decodeURIComponent`
blanks the whole app, bogus deep links become dead retry loops), `0076` (no timeout, no retry,
infinite skeleton on `token === null`), `0044` (no timeout on `authSignUp`/capabilities, double-submit
race), `0074` (storage failure swallowed). Fix: add `error.tsx`/`global-error.tsx` boundaries with a
visible retry, total route classifiers, `AbortSignal.timeout` on every fetch, in-flight refs for
double-submit, and BoardUI error tokens/composite types (unchanged).

## 4. Phased v0.4.0 roadmap

### P0 — correctness/security, must-ship (small, high-confidence, no wire change)

1. **Cheap RLS/tenant hardening + open the binding decision.** Motivated by `0412` H1/H3/H5/H7,
   `0266` H4/H5, `0432`. Impact: closes the one-setting window, enforces ADR-002, fails fast on a
   BYPASSRLS pool role, stops warn-level driver `Display`; REQ-043. Risk: low/reversible; the deep
   space-binding (`0266` H1–H3) is owner-gated (§5), not in this slice. First slice: combine the two
   `set_config(..., true)` calls and add the ADR-002 source-scan test.
2. **Kill the heartbeat storm and add cancellation.** `0148` S1/S2/D1/D2, `0147`. Impact: removes
   O(N²) append/broadcast amplification and leaked tasks; REQ-005/020. Risk: low if frame shape is
   unchanged; comment keepalives are a decision (§5). First slice: one shared per-thread heartbeat
   task + `sender.closed()`/shutdown-token branch with teardown at last subscriber.
3. **Stop secret/PII leakage.** `0136` P1/P2, `0132`, `0151`, `0133`. Impact: bounded static reason
   text, no echoed provider keys, redaction test extended; REQ-054/045. Risk: low; wording changes
   are DEVIATION-gated but the default path is unchanged. First slice: make `explained` a bounded
   `&'static str` constructor and log the dynamic detail instead of echoing it.
4. **Fail loud on corrupt reads; keep HTTP mapping stable.** `0388` H2/H4, `0236`, `0206`, `0136`.
   Impact: corrupt rows become typed non-retryable errors instead of 503; REQ-009/010. Risk:
   medium — audit queries first so previously "working" bad rows don't break reads. First slice: add
   internal `DataIntegrity` error for JSON decode failures and replace `unwrap_or(MAX)` with fallible
   conversions, leaving the external status map untouched.
5. **Error taxonomy truth at the source.** `0412` H7, `0136` P4/P5, `0228`. Impact: permanent tenant
   faults no longer tagged retryable; exhaustive code→status test; REQ-053. Risk: low if no new
   public code is added. First slice: split "refused to set tenant context" from transport
   `Unavailable` internally and drive the status test from an exhaustive table.
6. **Web error boundaries + timeouts on critical flows.** `0080` H1/H4, `0076`, `0044` items 1–2,
   `0074`. Impact: no whole-app blanks, no infinite skeletons, no double-submit; REQ-001/040/055.
   Risk: low/medium — not-found-vs-home changes are DEVIATION-gated (§5). First slice: add
   `app/app/error.tsx` and a total, wrapped `routeFromPath` classifier.

### P1 — reliability (wire-visible items behind owner approval)

1. **SSE replay, gaps and backpressure.** `0148` G1–G4/B1–B3, `0147`, `0290`. Impact: slow clients
   get a visible gap+replay instead of silent loss; REQ-005. Risk: medium, wire-visible (`id:`/`retry:`) — DEVIATION. First slice: `Lagged(n)`→`events.since(last_seq)` replay with a watermark.
2. **Bound every body, query and allocation.** `0149`, `0154`, `0160`, `0134`, `0244`, `0368`.
   Impact: no unbounded replay/blob/cardinality; REQ-053/D-07. Risk: low/medium. First slice: replace
   the eight inline body-cap literals with one typed policy table and add oneshot 413 tests.
3. **Close TOCTOU/duplicate races.** `0270`, `0244`, `0256`, `0148`, `0442`, `0068`. Impact: no
   double-approve/double-fire/lost update; REQ-012/038. Risk: medium. First slice: convert `move_to`
   to one conditional `UPDATE … RETURNING` that drops the read-back.
4. **Remove fail-open fallbacks.** `0122`, `0124`, `0287`, `0288`, `0070`, `0232`, `0294`. Impact:
   absent SSRF checker or credential failure can't silently widen access; REQ-040. Risk: medium —
   `local_first` semantics are owner-gated. First slice: make `check_outbound_url` mandatory and add
   a private-redirect contract test.
5. **Wire/serde contract pins.** `0158`, `0142`, `0141`, `0157`, `0105`. Impact: generated client and
   OpenAPI cannot drift from views; REQ-001/045. Risk: medium (typed payloads may reject loose rows).
   First slice: add a round-trip test over all reply structs and the bidirectional contract test.
6. **CI supply-chain hygiene.** `0001` P1/P3, `0002`, `0004`. Impact: pinned toolchain/actions,
   read-only token, checksummed Helm; ADR-016/018. Risk: low/medium (bump friction). First slice:
   drop `dtolnay/rust-toolchain@stable` on desktop legs and add top-level `permissions: contents: read`.
7. **Helm version truth + validation.** `0413`, `0414`, `0119`/`0120`. Impact: one version source,
   additive `values.schema.json`, explicit DB fork; REQ-055. Risk: medium (renaming resources orphans
   the PVC). First slice: default `image.tag` from `.Chart.AppVersion` and add a CI agreement assert.

### P2 — structure/debt

1. **Single-source the middleware, route table and RPC registry.** `0149`, `0150`, `0151`, `0133`.
   Impact: header/cap/timeout policy applied once; one-line first slice: extract `apply_common_layers`.
2. **Consolidate domain id and error vocabulary.** `0235`, `0228`, `0227`, `0259`, `0269`. Impact:
   one id implementation and status vocabulary; first slice: one `opaque_id!` macro + `try_new`.
3. **Single-source config across surfaces.** `0001` P5, `0131`, `0132`, `0413`. Impact: no CI/ADR
   drift; first slice: a shared `tools/ci.env` sourced by Makefile and ci.yml.
4. **Additive migration hardening suite.** `0485`, `0425`, `0431`, `0512`, `0490`. Impact: upgrade
   safety; first slice: new `NOT VALID`→`VALIDATE` constraint file with an idempotent re-apply test.
5. **Desktop boot/menu and platform posture.** `0005`, `0006`, `0007`, `0008`, `0003`. Impact: named
   bring-up failures, green Windows boot gate; macOS/arm64 packing stays owner-gated (§5). First
   slice: dedupe the two health probes and boot the window on macOS `activate`.
6. **Public-share invariants in a new additive migration.** `0485`, `0367`/`0368`, `0145`/`0146`.
   Impact: visibility↔token invariant DB-checked; semantics decision in §5. First slice: a nullable
   `token_hash` CHECK/partial index in a new file, no edit to 0037.

## 5. ADR & owner-decision register

| Owner-gated item | Brief ids | Why a decision is needed |
|---|---|---|
| **System-context RLS / tenant binding** | `0266`, `0412`, `0432`, `0442`, `0444`, `0448` | Binding scoped reasons to a space and restricting the bypass changes which rows a principal can read — externally visible data access, needs ADR + DEVIATION sign-off; pool-role assertion and XOR check are the cheap ungated part. |
| **Wire changes** | `0157`, `0158`, `0148`, `0125`, `0101` | `id:`/`retry:`, gap/eof frames, typed payloads, route/report-shape edits and the `threads.send` attachmentIds field all change the wire contract; must be recorded in `docs/PARITY.md` and approved. |
| **Status-code changes** | `0136`, `0138`, `0154`, `0395` | Mapping `Rule`→409, health `ok:true`→503, screen failures 403→503, and 503→409 on duplicate session hash are observable; spec pins the code list, so adding `UNAVAILABLE` is forbidden without approval. |
| **Public-share semantics** | `0145`, `0146`, `0076`, `0485`, `0368` | Whether 429/5xx/offline may be distinguished from "link never existed" trades anti-enumeration (REQ-037 uniform 404) against honesty; the anonymous fetch also rests on one app-side `as_system` switch invisible to the schema. |
| **signup.closed / capability** | `0044`, `0043`, `0064`, `0063` | `signup.closed` is translated but dead; exposing `signUp: open|closed|allowlisted` in `AuthCapabilities` is a new wire field and REQ-040 server gate (first-owner arbitration needs a lock) — product intent, not just a bug fix. |
| **macOS / arm64 desktop packing** | `0005`, `0006`, `0008`, `0003` | macOS `activate` blank window and PE x64-vs-arm64 preflight are real, but M35 explicitly excludes macOS packs, signing and new artifacts; adding them needs an owner decision to lift that non-goal. |
| **Attachments wire issue #20** | `0101`, `0084` | The installed `file-upload` component is deliberately unwired because `threads.send` has no `attachmentIds`; adding it is a contract change + generated-client update and must reconcile the 8 MiB client default with the 16 MiB server cap. |
| **Migration constraint rewrites** | `0512`, `0490` | Dropping/re-adding `audit_events_kind_check` and relying on fast-default PG behavior are not strictly additive or version-independent; defer to ADR/operator sign-off rather than editing an immutable file. |

## 6. What we did NOT change

- **No product code edited.** No Rust, TypeScript, workflow, chart, Dockerfile or compose file was
  modified by the swarm or by this synthesis; `docs/upgrade-swarm/PLAN-v0.4.0.md` is the sole write.
- **No spec, golden or dependency edited.** `spec/**`, `spec/golden/**`, `docs/PARITY.md`, gates,
  `Cargo.toml`/`Cargo.lock` and npm manifests are untouched; every externally visible change stays a
  DEVIATION awaiting approval.
- **Migrations stay additive.** No checksum-bearing file was edited; every schema idea ships as a new
  file that only creates objects or adds nullable/defaulted columns and `NOT VALID`→`VALIDATE`
  constraints (§3 T10).
- **BoardUI tokens/charter preserved.** Semantic tokens, composite type utilities, `cx()`, RTL and
  one-design-system rules are kept; no component library or second theme was introduced.
- **No upstream content.** No brand, domain, package scope or identifier from the analysed original
  enters the plan; `before-m5` names are used throughout.

## 7. Appendix

### Coverage stats

| Area (module prefix) | Briefs | Modules | Representative ids |
|---|---:|---:|---|
| web (`apps/web/**`, `apps/desktop/**`) | 114 | 57 | `0005`, `0044`, `0080`, `0101`, `0116` |
| api (`crates/api/**`) | 38 | 19 | `0136`, `0142`, `0148`, `0158` |
| app + bin (`crates/app/**`, `crates/bin/**`) | 46 | 23 | `0160`, `0186`, `0190` |
| domain (`crates/domain/**`) | 66 | 33 | `0236`, `0244`, `0266`, `0270` |
| infra (`crates/infra/**` excl. stores) | 48 | 24 | `0280`, `0288`, `0411`, `0412` |
| infra-stores (`crates/infra/src/stores/**`) | 94 | 47 | `0368`, `0380`, `0388`, `0410` |
| migrations (`migrations/**`) | 116 | 58 | `0432`, `0444`, `0485`, `0512` |
| ci/release/helm (`.github/**`, chart, compose) | 8 | 4 | `0001`–`0004`, `0119`, `0120`, `0413`, `0414` |
| **Total** | **530** | **265** | ids `0001`–`0530` |

Counts use the area mapping applied to `briefs-upgrade.jsonl`: 530 = 114+38+46+66+48+94+116+8.
The `ci/release/helm` row aggregates 4 modules × 2 lenses (`.github/workflows/ci.yml`,
`.github/workflows/release.yml`, `compose.yaml`, `deploy/helm/boma-bot/Chart.yaml`); the "web" row
includes the 2 desktop modules. Brief ids in this document are the zero-padded inventory ids
(e.g. brief 412 = file `0412-*.md`).

### Regenerate / verify

```bash
cd rebuild-workspace/.worktrees/before-m5
# Inventory integrity
wc -l docs/upgrade-swarm/briefs-upgrade.jsonl                  # => 530
jq -r '.lens'    docs/upgrade-swarm/briefs-upgrade.jsonl | sort | uniq -c   # 265 + 265
jq -r '.module'  docs/upgrade-swarm/briefs-upgrade.jsonl | sort -u | wc -l   # => 265
find docs/upgrade-swarm/waves -name '*.md' | wc -l             # => 530
# Area counts (same mapping as the table above)
python3 - <<'PY'
import json, collections
rows=[json.loads(l) for l in open('docs/upgrade-swarm/briefs-upgrade.jsonl')]
def area(m):
    if m.startswith('apps/'): return 'web'
    if m.startswith('crates/api/'): return 'api'
    if m.startswith(('crates/app/','crates/bin/')): return 'app+bin'
    if m.startswith('crates/domain/'): return 'domain'
    if m.startswith('crates/infra/src/stores/'): return 'infra-stores'
    if m.startswith('crates/infra/'): return 'infra'
    if m.startswith('migrations/'): return 'migrations'
    return 'ci/release/helm'
print(collections.Counter(area(r['module']) for r in rows))
PY
# Prove this was the only write / gates unchanged
git status --short
git diff --stat
```

Cross-check a cited id: `grep -rl "^# 0412" docs/upgrade-swarm/waves` or open the matching
`docs/upgrade-swarm/waves/wave-*/<id>-*.md`. Every brief id in §3–§5 maps to exactly one file there.
