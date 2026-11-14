# STATE

## Current milestone

**Before-M5 — complete.** All three phases landed and pushed (`d9762a4` [P1],
`7ebd427`+`1a19a24` [P2], `38e886a` [P3, BYOK]). Migration 0024 fixed the shared-memory FK
defect end to end (`9dca0cc`). 258 tests green at every push.

**M-completed plan (in order, nothing skipped, nothing started early).**
- M5 — Integrations: REQ-032 (connector catalogue + per-space installs), REQ-033 (MCP remote +
  opt-in stdio), REQ-034 (plugins, no-local-code), REQ-035 (skills, SHOULD), REQ-046 (write-only
  secrets), plus deferred defects D-05 (MCP private endpoints, per-space allow-list + audit event),
  D-06 (plugin submissions require an account + shared counter).
- M6 — Artifacts, pages, board: REQ-036/037/038/039 + D-03 (magic-byte sniff).
- M7 — Channels: REQ-047 (inbound mail), REQ-048 (messaging), REQ-050 (bot webhooks).
- M8 — Money and models: REQ-018 (caps + purchase card), REQ-051 (billing credits), REQ-052 (model catalogue).
- M9 — Operate: REQ-044 (export/import/delete), REQ-045 (audit JSONL, security events, SCIM),
  REQ-054 (structured logs + metrics + CSP reports), REQ-058 (admin surfaces).
- M10 — UI complete + desktop: REQ-055 (all screens from `spec/08-ui-ux.md`, 7 UI goldens, axe,
  RTL, Tauri shell, plus REQ-011's remaining pieces of the strict list).
- M11 — Harden and release: regression across the workspace, all defects closed, budgets measured,
  ADRs complete, upgrade guide, signed release notes.

REQ-010 (group chats) and REQ-011 (activity view) arrive with M11 alongside their regression set,
as written in `docs/PLAN.md`. Deferred REQ-049 (COULD) and REQ-056 (mobile) stay out, recorded.
Every milestone ships test-first at the API/use-case level, then PG/functional proof, then docs +
STATE; gates re-run at each push (fmt, clippy -D warnings, cargo test workspace).

**M5 — in flight. Five slices committed: REQ-046 (secrets, `835d8e7`), REQ-034 slice 1
(plugin submissions, `bc03d1e`), REQ-035 (skills, `d56f248`), REQ-033 (MCP servers, `494d194` +
the `d427ee7` fmt fix), REQ-032 (connectors, in this revision).**

5. **REQ-032 (connector catalogue and per-space installs, MUST)** — the catalogue is code
   (`builtin_connectors`: `rss` — a public feed reader, no credential, best-effort parse of RSS
   2.0 `item` and Atom `entry`, title/link/summary, 20 entries default, cap 50 — and `http-json`
   — a guarded GET with the installation's bearer, 16 KiB cap, content sniffed JSON or text).
   `connections.list|install|remove` RPC; credentials sealed under the space's DEK (ADR-020
   machinery), never listed, `connected_credential` the only read path; installed tools join the
   run catalogue as `connector.<id>.<tool>` (Read effect, SSRF-guarded); the bot's tool grant
   applies exactly like installed connectors; **a revoked install contributes no tools everywhere,
   and a direct dispatch attempt is refused before any network call.** 321 tests green (docker-
   gated suites excluded — kernel keyring exhausted on this box; CI is the evidence path). The
   connector executor's happy path against a loopback fixture is untestable because connectors
   intentionally have no private-allow-list — recorded in docs/STATE.md as an accepted limit; the
   parse/guard/seal paths are covered by unit and PG tests.
**M5 — in flight. Four slices committed: REQ-046 (secrets, `835d8e7`), REQ-034 slice 1
(plugin submissions, `bc03d1e`), REQ-035 (skills, `d56f248`), REQ-033 (MCP servers, in this
revision).**

4. **REQ-033 (MCP servers, MUST) — honestly PARTIAL.** Done: `mcp.servers.list|add|remove|probe`
   RPC (namespace `mcp`, procedure `servers.*` per the spec's dotted naming); the registration of
   a **remote HTTP MCP server with a bearer token** (sealed under the space's DEK, ADR-020
   machinery, never listed, `reveal_bearer` the only read path); discovery via
   `initialize` → `tools/list` over plain `application/json` JSON-RPC (HTTP POST per call, 30 s,
   1 MiB cap, no redirects); probed tools persisted (`mcp_server_tools`) and merged into the run
   catalogue as `mcp.<server>.<tool>`, **Write-class**, so every call rides the approval gate
   exactly once (REQ-017) and lands as a `role=tool` message; the private-endpoint allow-list is
   per server per space (D-05), every probe and every approved call writes `audit_events`
   (`mcp.private_call`); the SSRF guard is one function with five refusal reasons
   (`crates/infra/src/net_safety.rs`), and the metadata endpoints are never reachable. stdio
   servers are **refused by default** with the explanation (HTTP 400, static message), which is
   the spec's acceptance criterion for default-disabled stdio. ADR-021 records the posture; the
   three gaps named here at this revision (OAuth leg, SSE framing, the allow-listed spawn) are
   closed in the entry below — the spec's "with OAuth where needed" criterion is met.
   **308 tests green** (docker-gated suites excluded — this box's rootless daemon exhausted its
   kernel session-keyring quota; CI is the evidence path for those).

**Environment — cargo target relocated.** `/var` is a 16 GiB tmpfs that hit 100 % on
2026-11-13T14:10 (a linker bus error took the test suite down); the cache was
`/var/opencode-target` (16 GiB, regenerable). Work continues with
`CARGO_TARGET_DIR=/tmp/opencode-target` (updated in `/tmp/opencode/enter-env.sh`); the owner
should review deleting `/var/opencode-target` to reclaim the space — it rebuilds itself.
**CI incident on `bc03d1e` (owner: "the workflows must work on every push") — both failures were
mine, and both are fixed in the skills revision.** Run 43436566757 was red on: (1) the `rust` job's
clippy step — the plugin-submission tests carried an unused import and a manual `String::new` at
`crates/testkit/src/build.rs:3399/3415` (fixed when the skills slice landed: `#[cfg(test)]` on the
module, `String::new()`); (2) the `compose` job's probe step — the app container never answered:
`compose.yaml` passed `BOMA_MASTER_KEY` with an empty value, and `SecretBox::from_env` read
"present but empty" as an invalid key, so `build_app` refused to boot and the container died before
`/health` came up. `from_env` now treats empty and whitespace-only values as absent (consistent
with every other `.env` parser in the binary and with `.env.example`), with a unit test
(`req_046_an_empty_env_key_is_treated_as_absent`). The compose probe step now dumps
`docker compose logs app` when it fails, so the next failure is legible. Rule recorded by the
owner: **CI must be green on every push, verified, not assumed** — every push from here watches the
run to conclusion and reports it.

**M5 — complete (pending CI). Seven slices committed: REQ-046 (`835d8e7`), REQ-034 slice 1
(`bc03d1e`), REQ-035 (`d56f248`), REQ-033 (`494d194` + `d427ee7`), REQ-032 (`fc8de75`),
REQ-034 slice 2 (`e9cb7ff`), REQ-033 remaining gaps — SSE, opt-in stdio, OAuth (this revision).**

7. **REQ-033 — remaining gaps closed (SSE, opt-in stdio, OAuth, this revision).**
   - **SSE**: the HTTP transport asks for `application/json, text/event-stream` and parses the
     first `data:` JSON-RPC message out of an event-stream answer (`first_sse_message`, skipped
     comments/blank events/`[DONE]`); unit tests pin it.
   - **Opt-in stdio**: `MCP_STDIO_ALLOWED_COMMANDS` (comma-separated bare programs, empty by
     default — the spec's default). A stdio server may only be registered when its program is
     listed (refused with `StdioDisabled` naming why); the transport doubles the check at spawn
     time. `StdioMcpTransport` spawns per call (no persistent process), speaks newline-delimited
     JSON-RPC (`initialize` → `notifications/initialized` → call), refuses shell syntax by shape
     (`split_command`/`program_allowed` in the domain), and is proven against a real spawned
     Python fixture (`crates/infra/tests/mcp_stdio.rs`). The binary routes by transport through
     `McpTransportDispatch` (HTTP everywhere; stdio only for allow-listed programs). `tokio`
     gained the `process` feature — no new crate.
   - **OAuth (PKCE, RFC 7636)**: `mcp.oauth.start` returns the authorization URL
     (`response_type=code`, S256 challenge from the PKCE pair, `state` for CSRF, ten-minute
     session, redirect `http://127.0.0.1/boma-mcp/callback` — the member pastes the callback
     URL's `code` and `state` into `mcp.oauth.complete`; this build runs no local HTTP listener
     for the callback). The verifier is sealed under the space's DEK in `mcp_oauth_sessions`;
     the row is single-shot — deleted at exchange time whether the exchange succeeds or fails,
     so a replayed `state` never reaches the authorization server (proven against Postgres).
     The exchange is form-encoded at the token endpoint (`client_secret` included when the
     member registered one); the bearer is sealed under the space's DEK; the server is marked
     `connected`; the tools are discovered — the spec's acceptance. An exchange refusal marks
     the server `error`. The token URL answers to the same outbound guard and the same
     per-space allow-list exception as the server URL itself (defect D-05); the authorization
     time is validated at registration.
   - Ports are scriptable end to end (`OAuthClient`); production wires `OAuthTokenClient`
     (`getrandom`+`sha2` for S256, HTTPS only, no redirects, 30 s timeout, 64 KiB answer cap).
     Docker-gated suites remain unverified locally (rootless keyring quota exhausted); CI is
     the evidence path — also for `req_027_a_docker_selection_provisions_against_the_daemon`,
     which joined the local skip list for the same reason.

6. **REQ-034 (plugin marketplace, MUST) — slice 2.** `marketplace.list|install|submit` RPC.
   The install is a real one: `GitHubPluginFetcher` (api.github.com → SHA pin verified against a
   full-SHA ref, recursive tree, `raw.githubusercontent.com` for component files; HTTPS only,
   no redirects, 100 KiB per file, 40 files, 4 MiB tree) — the loopback wiremock path reuses the
   same code via a doc-hidden `with_loopback_endpoints` constructor. The no-local-code rule:
   **nothing that is not a skill or a remote HTTPS MCP server installs**; stdio parts are named
   and skipped (`mcp_stdio`), unsafe companions are named and skipped (`skill_non_text`),
   built-in-name collisions skipped (`name_taken`), empty/invalid instructions skipped
   (`skill_invalid`), non-component parts named (`editor_only` or `unsafe_path`), registration
   failures named (`mcp_invalid`). Skills land in the space (origin plugin, `plugin_source`
   canonical), plugin MCP servers register without credentials and without private access,
   `plugin_installs` records the outcome (`parts`, `skipped`); a reinstall revokes the plugin's
   own skills wholesale first. Scripted fetcher drives the tests offline; live GitHub fetch
   against the public API is untested at this revision (no live calls in the suite — network
   is loopback-only in tests). **328 tests green** (docker-gated suites excluded, same cause).
**CI on `d56f248` — green, verified, not assumed.** Run 43439074293: `rust` (fmt, clippy, nextest,
coverage, deny, trace), `web` (build, typecheck, lint, Playwright + axe) and `compose` (image
build + migrate + serve + health probe) all succeed. The `bc03d1e` failure and its two root causes
are described above; this entry will keep the same discipline on every push from here.

3. **REQ-035 (skills, SHOULD)** — built-in (`interrogate`, our words), taught and plugin skills:
   migration `0028_skills.sql` (RLS), `SkillStore` port + PG + in-memory adapters, wire RPC
   `skills.list|get|teach`, agent tools `skill.load` (Read, executes) and `skill.teach`
   (Write, parks on the approval gate), and the run's provider request now carries the skill
   names and one-line descriptions (`ProviderRequest.skills`). The worker's tool dispatch acts
   for the run's member context, like the memory read (the system context cannot see the
   member's skills — a defect the memory adapter cannot express was avoided the same way).
   283 tests green; the docker-gated tests (REQ-026/028/029, `computer_conformance`) could not
   run at this revision: **this box's rootless docker exhausted its kernel session-keyring quota**
   (`unable to create session key: disk quota exceeded`), all boots fail identically and
   unrelated to the code; earlier in this session REQ-026/028/029 were all green at least once,
   and CI runs the full suite on every push. Restarting the owner's daemon frees the quota; it
   was not restarted without asking.


1. **REQ-046 (write-only bot secrets, MUST)** — see the previous block; committed `835d8e7`,
   `boma rekey` operator command wired and documented, 272 tests green at that revision.
2. **REQ-034 (plugin marketplace, MUST), slice 1 — anonymous submissions.** `POST
   /api/v1/plugin-submissions` (cookieless, CORS `*`, OPTIONS preflight, 32 KiB cap checked by
   the handler with a fixed wording, outer 33 KiB gate), per-IP shared counter from D-01 (10/hour,
   `429` + `retry-after: 3600` past the cap), source grammar in the domain
   (`crates/domain/src/plugin.rs`, HTTPS only, no credentials/query/port, safe path segments,
   `.git` dropped, `@ref` and `#subdir`), row recorded in `plugin_submissions` (migration 0027,
   tenant-less by design, like `rate_limits`). Goldens `plugin-submission` and
   `plugin-submission-invalid` replay byte for byte; the invalid fixture's wording
   (`Invalid source.`) is asserted exactly. **289 tests green**, trace 39/58, both goldens marked
   `implemented`, ratchet tightened 20 → 19.
   **D-06 ruling:** the "shared counter from D-01" half is done (it is the counter the endpoint
   uses); the "require an account or a challenge" half is NOT done, because the spec's own golden
   pins this path as cookieless and public — enforcing an account or a challenge would change the
   documented public contract, which is a DEVIATION needing your approval. Recorded under
   Decisions needed (DEC-16).
   **REQ-034 slice 2 — `marketplace.list|install|submit` RPC, the no-local-code installer (skills
   without scripts, omissions named, remote-only MCP parts, nothing installed on a malformed
   source)** — not done yet; the install pipeline and the skill store it persists into are the
   next slice.

**M5 — in flight, secrets done (REQ-046).** Bot secrets are write-only and encrypted at rest
(envelope AES-256-GCM per DEC-10 option (a), ADR-020): every space owns a random DEK wrapped under
`BOMA_MASTER_KEY` (migration 0026; 0025 keeps its original recorded checksum — see the note below);
every secret is sealed under its space DEK; the rekey path re-wraps DEKs only, proven by
`req_046_a_secret_round_trips_through_postgres_and_survives_a_rekey` against
`rekey_space_keys`. RPC surface added (parity with the reference's namespace): `botSecrets.set`
`/list` `/forget` under `/rpc/botSecrets/{procedure}` — write-only, names-only list. The operator
rotation is `boma rekey` (reads `BOMA_MASTER_KEY` current + `BOMA_NEW_MASTER_KEY` new, re-wraps DEKs
only, refuses to run under an ephemeral key; wired in the CLI, documented in `.env.example` and
`compose.yaml`). The shell
executor injects `BOMA_SECRET_<NAME>` into `computer_shell` only; values never enter a message,
an event, a reply or the log. GUESS (owner to veto): the environment injection surface (computer
shell is today's only tool runner) and the namespace name `botSecrets`. 272 tests green at this
revision, trace ratchet tightened 21 → 20.

**Incident — migration 0025 edited after it was recorded (my error, no data lost).** The secrets
slice recorded `0025_bot_secrets.sql` in the dev DB, then the file was edited to add `space_keys`,
producing `VersionMismatch(25)` across the shared-DB suite (39 tests failed at once). The file was
restored byte-exact (`req_053_the_migrations_apply_and_are_recorded` passes again) and the envelope
columns land as a separate **0026_space_keys.sql** — no data left the machine. Lesson re-applied:
new schema, new migration. The dev DB carries migrations through 0026.

Before-M5 correctness fix — migration 0024 (memory_documents shared rows).** The first full
run of the worker against Postgres caught a live defect: 0013 stored shared-memory rows with
`bot_id = ''` and `FOREIGN KEY REFERENCES bots (id)`, so every "shared" document violated the FK
and NO shared memory could ever be written on a real deployment — the in-memory adapter, which
models `Option<BotId>`, had masked it for months. Migration 0024 moves the shared marker to
`NULL` (bot_id nullable, dropped default, NULL-shaped CHECK replacing `memory_documents_check`,
partial unique index `memory_documents_shared_key` for the upsert arbiter). The PG adapter now
binds NULL, matches with `IS NOT DISTINCT FROM`, parses `Option<String>` into `Option<BotId>`,
and the `req_022_a_memory_entry_is_available...` invariants hold against real Postgres now
(`req_022_the_worker_injects_only_memory_of_the_runs_space_in_postgres`). The worker also reads
scoped memory as the run's *member* context instead of the system context, so the space is a
real barrier on the PG path (a defect the memory adapter cannot express). Gates at this revision:
258 tests green, clippy, fmt, deny, machete. Dev DB `boma` carries migrations through 0024.

**Before-M5 Phase 3 (BYOK provider) — done.** `boma-infra::OpenAiCompatibleAdapter` implements the
`ModelProvider` port against an OpenAI-compatible HTTP surface (REQ-015): `finish_reason`/`tool_calls`
map to `ProviderTurn` exactly like the scripted provider, memory rides as a scoped system message,
the wire carries only the built-in tool schemas the dispatcher enforces. No live calls in tests —
fixture replay through a loopback wire server (wiremock); the operator's key rides `secrecy`.
`BOMA_PROVIDER_ENDPOINT`/`BOMA_PROVIDER_MODEL`/`BOMA_PROVIDER_API_KEY` wire the deployment all-or-none
(half-set is a startup error) — the variable names are a GUESS flagged for the owner. The deployment
wires nothing unless all three are set; absent, the scripted provider keeps end-to-end exercise green.
Counts CONFIRMED at this revision: 257 tests (cargo test --workspace --all-features --locked), clippy,
fmt, deny (reqwest/secrecy/wiremock licences: MIT/Apache-2.0), machete, secret scan, trace ratchet 21.
Phase 2 of this plan (defects D-01..D-09 honestly in scope) landed as 7ebd427; before-M5 Phase 1 as d864865.

**Before-M5 Phase 2 (defects) — done and pushed.** D-01 (shared auth rate limiting across
replicas via migration 0023's bucket table, 10 sign-in / 5 sign-up per IP per window, 429 with
the stable envelope), D-04 re-verified (unlisted proxies cannot steer the resolved address; a
trusted proxy steers by left-most valid XFF entry; empty trusted list by default), D-07
re-verified (global cap plus per-procedure caps enforced before the handler), D-08 (session
lifetime now bounded, 5 min..=60 days; secure cookies pinned by test; the linking policy does not
apply yet because the rebuilt auth has no second provider), D-09 (typed enum enforced by a
workspace scanner test), D-02 (already closed). Deferred to their owning milestones: D-03
(attachment type sniffing — needs the artifact upload pipeline), D-05 (MCP private endpoints —
needs the MCP integration surface), D-06 (plugin submissions — needs the plugin endpoint).
Counts CONFIRMED on this machine at `7ebd427`: 250 tests (cargo test --workspace --all-features
--locked --no-fail-fast), clippy, fmt, deny, machete, secret scan, trace ratchet 21.

**Before-M5 Phase 1 — M0–M4 closure for the agent loop is done and pushed (`d9762a4`).** REQ-016/017/019/022
improved materially: a write/spend tool turn parks the run on one approval card per run (`cards_thread_run_key`),
the person answers, the parked call applies exactly once behind the effect key `tool:{run}:{call}`, and the run
resumes to `queued`. `delegate.helper` spawns a `HelperResult` child run; the parent waits for it, the helper's
outcome is written as a structured `role=tool` message on the parent thread, and the parent re-enters. The tool
loop is bounded by `MAX_TOOL_CALLS = 8` before any tool executes. Scoped memory (bot first, space-shared, REQ-022)
rides into the provider turn. Counts are CONFIRMED by commands run on the pushed tree: `cargo test --workspace
--all-features --locked --no-fail-fast` → **237 passed, 0 failed** (this rebuild environment has no `cargo-nextest`
binary after the server restart; `cargo test` is the observed gate); `clippy --workspace --all-targets
--all-features --locked -- -D warnings` green; `fmt --check` green; `cargo deny check` exit 0 (advisories/bans/
licenses/sources ok, duplicates are warnings only); `cargo machete` clean; `tools/secret_scan.py` clean; trace
ratchet tightened 40 → 21 missing (`--max-missing 21`, 37/58 ids named by tests). Pushed to `origin/dev`
(`c30cb37..d9762a4`). The earlier `VersionMismatch(21)` incident is recorded: 0021 froze in the dev DB, so 0022
carries the status CHECK; migrations are never edited once recorded. Phase 2 (D-01..D-09) and Phase 3 (BYOK model
provider over the same port) remain.

- **M0 — complete after this audit.** Build/lint, `/healthz` + `/readyz`, and CI red/green were
  already observed. Two gaps closed here: the Compose stack set `BOMA_SANDBOX`, a variable nothing
  reads (the binary reads `SANDBOX_PROVIDER`), so it silently ran with no computers while claiming
  docker — compose and `.env.example` now name the real variable and default to `none`
  (fail-closed, REQ-027); and a `compose` CI job now builds the image and probes all four health
  paths, which is the only environment where the criterion can be observed (this box has ~380 MB
  free against a ~1.3 GB builder image). D-02's obligation is now enforced twice: a test that
  probes the development mail paths and a CI grep that fails on any `route("/api/dev…` registration.
- **M1 — complete.** The walking skeleton test signs up, lands in a space, creates a bot, sends,
  replays the nonce, and the worker folds steering into one run; `send-message` and
  `send-message-replay` replay byte for byte; the RLS suite passes; the thread screen is a UI
  golden. REQ-057 was recorded as "not built yet" in `TRACEABILITY.md`: it is built
  (`apps/web/lib/i18n.ts`) and covered by eight Playwright tests, including RTL. The row is
  corrected.
- **M2 — one real gap.** Steering, stop, cards, replies/reactions, lease recovery, model fallback,
  tool catalogue, approval exactly-once, and legible failures are all proved. **REQ-019
  (subagent delegation, SHOULD) is only half done**: a parent can create a helper run, but nothing
  resumes the parent with the helper's result — `crates/app/src/worker.rs` has no helper handling.
  M2 recorded that branch as "lands with M3"; M3 shipped without it. It is the one unmet acceptance
  criterion in M0–M4 and needs a decision (see Decisions needed).
- **M3 — complete.** Append-only revisions, shared memory scope, cron `@once`, dispatch coalescing
  and try-before-enable are all proved. REQ-022's "the entry is available in a later run's context"
  is explicitly deferred to the agent engine (the scripted provider builds no prompt context); it
  is not one of M3's exit criteria.
- **M4 — complete.** All five exit criteria observe green, freshly re-run here: the provider
  conformance suite 6/6 against the rootless daemon and `req_028_…through_a_real_browser` 1/1
  against `zenika/alpine-chrome`. The last stale marker is closed: the `computer-takeover-conflict`
  API golden, pending since M4 with a reason that no longer held, now replays. Closing it needed a
  real wire fix: the takeover conflict answered "That already exists." where the capture says
  "Someone else has control.", so `AppError::ScreenTaken` now maps to that message.

Still true of M4: the agent-driven tool loop is not wired (the scripted provider calls no tool), so
computer tools are proved at the use-case boundary, not by an agent run; and idle release has no UI
surface (M10).

## Previous milestone

**M4 — computers — done.** Exit criteria from the plan all observe green on `802344c` (rust and
web jobs): the Docker supervisor boots a computer (`computer.start`), the screen streams behind a
sealed capability (`computer.openScreen` + `/api/internal/screen-target`), takeover is exclusive
(`computer.takeover`/`release`, other members can neither take nor shed the hold), idle computers
are released (`BOMA_COMPUTER_IDLE_MINUTES`, default 10), and the provider conformance suite runs
(`boma-infra::computer_conformance`, six tests covering boot/reuse/shell/confinement/takeover/
capability sealing/private isolation; the suite is the contract any provider answers to). Full
gate suite green at push (`fmt`, `clippy -D warnings`, `nextest` 221/221, `deny`, `machete`, trace
budget 21/40, secret scan). The REQ-028 push takes the suite to 224/224 (two configuration tests
for the browser computer and the real-Chromium end-to-end test). CONFIRMED on CI run
43338876264 (`a35c82e`, rust and web jobs green): 224/224 tests, including the Chromium test,
and `cargo llvm-cov` at 84.42 % of lines (gate: 80 %).

**REQ-028 observe-and-act is done against a real browser.** The supervisor now talks CDP over the
websocket of the computer's headless Chromium: observe (PNG through `Page.captureScreenshot`),
navigate, evaluate, click, type, scroll and DOM snapshot, all behind the same takeover and
running-only gates as the shell tools. The proof is one end-to-end test against
`zenika/alpine-chrome` (`crates/infra/tests/chromium.rs`: the frame carries the PNG magic bytes,
the DOM contains the page's button, a typed value reads back, a click increments the page's own
counter, a scroll moves `window.scrollY`). A deployment opts in with `BOMA_COMPUTER_IMAGE`,
`BOMA_COMPUTER_CMD` (whitespace-separated argv, e.g. `--headless --no-sandbox
--remote-debugging-port=9222 --remote-debugging-address=0.0.0.0 about:blank`) and
`BOMA_COMPUTER_CDP_PORT=9222`; the container port is published on a random host port and read
back by inspecting the container. The supervisor sends both `ExposedPorts` and the binding, and
asks for the host port with an empty string: a binding alone publishes nothing on a daemon that
follows the older contract (found on CI, where the port came back unbound), and `"0"` is not the
documented way to ask for a free one. The screen stream (REQ-030) falls back to CDP when the image
carries no frame command. The shell and file tools keep working on the alpine image unchanged.

- **REQ-027 (provider-agnostic computers, fail-closed, MUST)** is done: `SANDBOX_PROVIDER`
  selects the backend at startup — absent means `none` (valid), `docker` selects the local
  supervisor and the process pings the daemon before binding; `novita` is refused naming
  `NOVITA_API_KEY` when absent and refused as not supported by this build when present; anything
  else is refused naming `SANDBOX_PROVIDER` and the supported values. `/ready` carries the
  configured provider in `sandbox`. Wire answers `COMPUTERS_UNAVAILABLE` (503) under `none` and
  `computeComputer.status` answers `stopped` (never an error) for a bot with no computer yet.
- **REQ-026 (a computer per bot, lazy, released when idle, MUST)** is done: migration
  `0017_computers.sql` (computers/flags, computer_runs with at most one open per computer,
  computer_execution_leases, agent_homes, browser_profiles, all RLS); the Docker supervisor boots
  `boma-computer-<id>` from `BOMA_COMPUTER_IMAGE` (default `alpine:3.21`) sleep-infinity with a
  RAM/CPU cap and a home volume at `/home/bot`, reuses a running/stopped container and bumps a
  screen generation on every real boot; `computer.start`/`computer.stop`/`computer.status`/
  `computer.settings`/`computer.provider` procedures; idle sweeper in the worker loop releases a
  computer after `BOMA_COMPUTER_IDLE_MINUTES` (default 10) of no use and preserves the home.
  Golden `computer-status` replays byte for byte.
- **REQ-030 (screen access without a session, MUST)** is done: migration `0018_screen_capabilities.sql`
  (hash-only bearer storage, RLS); `computer.openScreen` mints a secret bound to (computer,
  generation); `POST /api/internal/screen-target` (16 KiB cap) opens the frame through the sealed
  bearer and *only* through it — no session, no cookie, nothing else grants pixels. Bytes come
  from the frame command run inside the container; absent one, the answer is 403, never fake
  pixels. `meta` + one `frame` event then the stream closes; bumping screen generation kills every
  outstanding capability.
- **REQ-029 (user takeover, MUST)** is done: a member holds exclusive control of the screen
  (`takeover_by_user`); while the hold is in effect every computer-driven input path refuses; only
  the holder may release; a different member cannot take or shed the hold; `computer.takeover` /
  `computer.release` RPC procedures, `ComputerTakeoverReply` wire shape
  (`status`, `provider`, `takeoverByUser?`, `takeoverAt?`).
- **REQ-028 (observe and act on the computer, MUST)** is done. The shell half runs against the
  alpine image — `computer_shell` (arbitrary `sh -c`, output capped at 64 KiB),
  `computer_read_file`, `computer_write_file` (base64-pipe write), path confinement to `/home/`
  (no `..`, no shell meta-characters) — and the browser half runs against a Chromium image:
  `computer_observe` (PNG), `computer_navigate`, `computer_evaluate`, `computer_click`,
  `computer_type`, `computer_scroll`, `computer_dom_snapshot`, each opening a fresh CDP session
  whose whole setup is bounded (5 s readiness budget, 10 s per call) so a half-open published
  port cannot hang a caller. All of them go through `computer_caller` (takeover refusal,
  running-only). The CDP client is `crates/infra/src/cdp.rs`; the supervisor publishes the CDP
  port and reads the host port back from `inspect_container`. Chromium 124 exposes no
  `Input.dispatchWheelEvent`, so scrolling uses `Input.synthesizeScrollGesture`. Proved by
  `crates/infra/tests/chromium.rs` against a real rootless daemon and a real Chromium.

Honest gaps carried in M4:

- **REQ-031 (team vs private computers, SHOULD)** is done: `computers.private` backfilled to
  `true`; a space has at most one team computer (partial unique index on non-private rows);
  `computer.start {botId, mode}` with `mode = private` (default) boots a per-bot computer, and
  `mode = team` boots or reuses the one shared by every bot of the space; `computer.settings`
  reports `mode`; other bots in the space see only their own computer (private) or the shared one
  (team). Proved at the use-case boundary (testkit) and the provider boundary (conformance).
- The **agent-driven tool loop** (the model calling computer tools by itself) is not part of this
  milestone yet: the scripted provider does not call tools at all. Computer tools are proved at
  the use-case boundary (RPC + shell integration), not by an agent run.
- **Idle release exists but has no UI surface** — the RPC is in place; the web panel is M10.

## M3 — done, hardened

**M3 — done, hardened.** M3 (Memory and schedules) closed at `9d812a4` and was hardened at
`ca0af8e` after a self-review caught two missed acceptance criteria and one silent bug:
REQ-022 (memory documents with append-only revisions and `memory.write`/`read`/`list`/`remove`/
`provider`), REQ-023 (cron-style routines with `@once`, next-run computation in the routine's IANA
timezone, `routines.create`/`list`/`remove`), REQ-024 (the worker sweeps due routines into runs;
`@once` fires once and deactivates; overlapping wakes coalesce; a retryable failure re-tries with
backoff — 3 attempts, 60s doubling to a 1800s cap, `runs.not_before` gates the claim; past a
threshold of five counted failures the routine pauses itself and **the owner is told** in the
thread; a clean completion resets the budget) and REQ-025 (`routines.runNow` fires the routine
once as a normal turn, attributed but moving no counters). The Postgres `INSERT INTO runs`
never wrote `routine_id` (the in-memory adapter masked it): fixed, with a fresh-queue Postgres
regression test. CI green for both jobs at `ca0af8e`; nextest 192/192; clippy, fmt, deny,
machete, trace (27/40), secret scan clean; web suite re-verified locally at `ca0af8e` (build,
typecheck, lint, 20 passed / 3 skipped).

Row order matches implementation order within M3: REQ-022 → REQ-023 → REQ-024 → REQ-024 (hardening) → REQ-025.

Slices delivered:

- **REQ-022 (memory documents, MUST)** is done: per-bot and space-shared memory documents with
  append-only revisions. Migration `0013_memory.sql` creates `memory_documents` (projection: the
  current row) and `memory_revisions` (the append-only chain, `document_id + revision` unique);
  both under RLS. The wire gains `memory.write` (`{botId?, scope, path, content}` → `{path,
  revision, updatedAt}`), `memory.read`, `memory.list`, `memory.remove`, `memory.provider` (only
  `builtin` exists). The `memory-write` golden replays byte-for-byte. Write and read are proved at
  three levels (domain path rules, RPC surface, real Postgres); the scope grammar refuses
  `scope=bot` without a `botId`, `scope=shared` with one, and any other scope string. A slice of
  the acceptance — memory available in a run's context — lands with the agent engine, which builds
  the model prompt; the scripted provider ignores context until then, which is honestly currently
  everywhere (same story as REQ-016's tool-call loop).
- **REQ-023 (routines as cron definitions, MUST)** is done for its create path: a bot wakes on
  cron expressions in its own IANA timezone (`croner` + `chrono` + `chrono-tz`, MIT), and `@once`
  fires exactly once five minutes after the request drops. Migration `0014_routines.sql` creates
  `routines` and `routine_runs` (the audit trail) with RLS; `routines.create` computes
  `nextRunAt` in the timezone and returns it as UTC; `routines.list` reads it back; triggered
  alarm fires are not dispatched yet (REQ-024). The two goldens (`routine-create`, `routine-once`)
  replay byte-for-byte: `routine-create` pinned against a clock after Thursday's 8am slot, so the
  next weekday wake lands Friday 07:00 UTC; `routine-once` from the default instant lands
  00:05. Reply shape omits `timezone` for `@once` (a one-shot has no timezone semantics worth
  serialising).

## Environment notes

- Docker, for M4 (Computers), runs **rootless** on the developer box: `dockerd-rootless` 29.9.3
  (docker, rootlesskit, slirp4netns, runc, fuse-overlayfs via nixpkgs). XDG_RUNTIME_DIR=/tmp/opencode/xdg,
  socket `unix:///tmp/opencode/docker.sock`, data-root `/tmp/opencode/docker-data`.
  Verified: images pull through slirp4netns and containers run (hello-world, alpine).
  The daemon is not under systemd; it does not survive a reboot and must be restarted by hand.
  The sandbox `/tmp` and `~/.nix-profile` were wiped twice this milestone; the environment is
  documented here so a wipe can be recovered: rustup toolchain into /tmp/opencode/{rustup,cargo},
  CARGO_TARGET_DIR=/tmp/opencode/target, Postgres recreated from /nix/store pg binaries on 5433.
- The sandbox disk (`/home`) is nearly full; keep builds in `/tmp/opencode` and clean
  regenerable caches there. Deleting `node_modules` is safe: `npm ci` restores it.


- **REQ-006 (stop work, MUST)** is done (`e453df1`): `runs.cancel` stops a run wherever it is, the
  worker stops producing into it (pre-check, per-chunk check, per-write terminal guard), a
  terminal run absorbs no further write in either store, the stop is idempotent, and the live
  stream announces `cancelled`. The helper-run half (stop the helper, not the parent) lands with
  REQ-019 delegation.
- **REQ-003 (steering, MUST)** is done: a message written during a conversational run
  (`user`/`helper_result`/`retry` trigger, leased or running) joins the active run instead of
  queueing a second one; the steering text rides the run (`steering_from_seq`), the worker runs
  one provider pass per pending steering state (bounded at four), and the reply lands as one bot
  message for both texts. A routine or webhook turn is not conversational, so a message written
  during one waits for the next turn, as the spec requires. Important catch: `prompt_of` read the
  history *oldest-first* — the prompt is now the newest user text, and the steering test caught it.
  Migrations now carry `0003_steering.sql` (two columns).
- **REQ-007 (pending cards, MUST)** is done (`29bc3cb`): the card block renders as a region with an
  action button and an answer textbox, so a pending question stays answerable while the composer
  keeps working. The pending-question-card UI golden replays.
- **REQ-015 (model selection with fallbacks, MUST)** is done for its M2 slice: a bot may carry a
  visibly at `send_message` with the model named — never silently executes elsewhere; an unpinned
  choice falls back to the deployment default (`DeploymentModels` on `Services`, seeded from
  `Config` by the binary). Domain `resolve` has 5 unit tests; the testkit worker test asserts the
  resolved provider/model on the run. Migration `0006_model.sql` adds the three model columns on
  `bots` and `runs`; migration `0005_tools.sql` (REQ-016) is already in; `model.rs` is the new
  pure-rules module. BYOK / OAuth provider credentials are M8 (money and models), not wired in M2.
- **REQ-016 (per-run tool catalogue, MUST)** is done in its M2 slice: a run receives only the tools its bot may use. The catalogue anchors on
  the bot (a `TenantContext::System` worker may ask), reads the space's enabled installs and the
  bot's grant, and a bot with an empty grant sees every enabled install. The catalogue is built
  per provider pass, never cached, and the model receives only what it may use. `BUILTIN_TOOLS`
  is explicitly empty until each built-in lands with the milestone that executes it (memory with
  REQ-022, messaging with REQ-048): an honest empty list beats a silent non-empty one. Not yet in
  this slice: the model actually calling a tool (the tool-call loop), and the audit event for a
  refused call; those land with the agent engine that makes the call.
- **REQ-017 (approval gate, MUST)** is done: `threads.answer` answers a card once, records the
  authorized effect under an idempotency key (`external_effects`, unique per run), and resumes the
  run that was waiting. The second answer to the same card is refused. Proved at three levels (in
  memory, RPC surface, real Postgres) plus the `thread-answer` golden replay. The domain gains
  `Card`/`CardKind`/`CardStatus`/`ExternalEffect`; migration `0004_cards.sql` creates `cards` and
  `external_effects` with RLS.
- **REQ-019 (subagent delegation, SHOULD)** is wired in its M2 skeleton slice: a run can delegate to
  a helper run on the same bot and thread. The helper run carries `trigger: helper_result` and
  `parent_run_id` (migration `0008_helper_runs.sql`), and `write_reply` writes the result as a
  `role: tool` message instead of a bot message, so the helper's internal steps do not leak into
  the chat. The domain gains `Run::is_helper`. Honest boundary: the model never yet decides to
  delegate by itself — the agent engine does that, and it is not built yet; a scripted test drives
  `delegate_helper_run` directly and asserts the child run exists with the parent reference and
  the helper trigger. The "helper answers, parent continues" branch of REQ-019 lands with M3.
- **REQ-008 (reactions, MUST)** is done: a member toggles one emoji on a message with one RPC
  (`threads.react` `{ messageId, emoji, action: add|remove }`), the store keeps one row per
  (message, person, emoji), the reply carries the full truth for the message so the client never
  counts, and the transcript renders the chips (toggle on click, quick-picker for a new emoji).
  Reactions survive a reload (hydrated into `page_messages` with one attributed query — one
  `SELECT ... WHERE message_id = ANY($1)` in Postgres, one pass in memory). Proved at three levels
  (memory → RPC surface → real Postgres) plus the web spec (`reactions.spec.ts`). Migration
  `0009_reactions.sql` creates `message_reactions` with RLS. Mentions (REQ-008) wait on REQ-010
  group chats; that stays documented, not faked.
- **REQ-008 (replies and quotes, MUST)** is done for its send path: `SendMessage` carries
  `reply_to_message_id` and `reply_quote`; a reply to a message references it and carries the
  quote of its text. The wire `MessageView` exposes both fields; the composer shows a reply
  banner with the quote; the transcript renders replies as a quoted block; the composer keeps a
  reply context with a cancel button; a Playwright test covers the click-to-reply flow.

- **REQ-009 (read state, SHOULD)** is done — read state and snooze, owner chose option A:
  per-member `thread_read_states` and `thread_snoozes` (migrations 0010 and 0011, RLS plus
  effective space predicates in the roster and reaction queries, which a superuser ignores — the
  defense in fact lives in the predicate, the policy is defense in depth), `ReadStateStore` and
  `RosterStore` with Postgres and memory adapters, and the wire gains `threads.head`,
  `threads.markRead`, `threads.markUnread`, `threads.snooze`, `threads.unsnooze` (`{threadId,
  until?}` with an RFC3339 check) and **`bots.list`** — an additive procedure the rail needs to
  render snoozed and unread state per member; it is not in the contract's `threads.*` list, it
  conflicts with nothing and it changes no existing procedure. The transcript renders one
  `role="separator"` divider after the last read message; the rail renders a regular list
  (snoozed bots hidden) plus an expandable "Snoozed (n)" shelf with unsnooze and an unread badge;
  a snooze expires on its own when `until` passes. The thread header carries the Snooze toggle.
  Proved at three levels (memory, RPC, Postgres) plus the RLS probe and a Playwright spec; the
  `REQ-009-unread-divider` UI golden replay is green. Note: this slice widened the `time`
  workspace dependency to `["formatting", "parsing"]` (same version, no new dependency).

Remaining in M2, in dependency order: mentions (REQ-008) wait on REQ-010 group chats (M11, not
M2); the tool-call loop and the refusal audit event from REQ-016 land with the agent engine that
makes the call (milestone M3/M5); the BYOK and provider OAuth login legs of REQ-015 land with M8
(models and credentials); the "helper answers, parent continues" branch of REQ-019 lands with M3
(the agent engine).

The current commit carries REQ-003, REQ-006, REQ-007, REQ-008 (replies/quotes and reactions),
REQ-009 (read state and snooze), REQ-015, REQ-016, REQ-017 and REQ-019 (skeleton).

## Done

- 2026-11-12 — **CI diagnosis: both workflows green.** The repo has `.github/workflows/ci.yml`
  (two jobs: `rust` — fmt, clippy, nextest, llvm-cov `--fail-under-lines 80`, deny, machete,
  trace, secret scan; `web` — build, typecheck, lint, playwright/golden/axe). It runs on every
  push. Three pushes had failed without anyone looking: `405e276` (clippy `too_many_lines` on
  the pinned 1.99.1 toolchain), `3716c35` (a line over rustfmt's limit), and `03616ab`/`a891f54`
  (`react-hooks/set-state-in-effect` in `bot-rail.tsx` — fixed by turning the roster into a tiny
  external store consumed through `useSyncExternalStore` and calling `refreshRoster()` from the
  effect; `apps/web/lib/roster.ts`). Also tightened the CI trace budget from 50 to 40 to match the
  documented ratchet. Both jobs verified green at `bce104c` via the Actions API (the repo is
  public; anonymous API reads statuses but not logs — logs were read with the owner's PAT, never
  persisted). Local gates did not cover `npm run lint` — the gap that let the lint fail slip.
- 2026-11-12 — **M2 sweep: `runs.retry` closed (REQ-012), the last stale pending golden.** The
  manifest carried `run-retry` as pending with the reason « retry is implemented once leases and
  fence tokens exist » — both had existed since M1, so the reason was stale and the owner's
  "finish everything first" rule applied. Implemented: migration `0012_attempt.sql` (`runs.attempt`,
  starts at 1), the `failed → queued` transition only, atomically, with the fence and lease dropped,
  `completed` and `cancelled` refused with `CONFLICT`, one attempt added, `/rpc/runs/retry`
  (`{runId}` → `{runId, status, attempt}`), the `run-retry` golden replays byte-for-byte and is
  marked implemented. Proved at three levels (memory, RPC surface, real Postgres) plus the golden
  replay. `group-create` (REQ-010) is now explicitly labelled M11, matching PLAN.md.
- 2026-11-12 — Toolchain rebuilt from scratch after the sandbox wiped `/tmp/opencode` (rustup
  installed Rust 1.91.1 (minimal profile + rustfmt/clippy/rust-src), `cargo-nextest` 0.9.151 from
  source, gcc 16.2 and PostgreSQL 17.7 through nix, `/tmp/opencode/env.sh` recreated (port 5433,
  db `boma`, trust auth). Headless Chromium's system libraries (glib, nss, nspr, atk, at-spi,
  dbus, X11/xcb/composite/damage/ext/fixes/randr, xkbcommon, alsa, expat, gbm, udev) restored
  through nix and referenced by `LD_LIBRARY_PATH` in `/tmp/opencode/ldpath.sh`; the Playwright
  suite passes with `FONTCONFIG_FILE=/tmp/opencode/fonts.conf`.
- 2026-11-12 — **REQ-009 (read state) committed locally (`f3a9bc6`); push recovered.** The
  `git push` hung at the credential step (`git credential fill` empty, VS Code's askpass answered
  "Missing or invalid credentials."; `ls-remote` anonymous worked). The owner then pasted a PAT
  which was used for one-off pushes via an inline `credential.helper` — never written to any
  file, config or log, and nothing was left behind on disk. The owner should rotate that token:
  it transited the chat in clear.
- 2026-11-12 — **M1 walking skeleton implemented and committed** (`feat(app,api,infra,bin,conformance): M1 walking skeleton`).
  - `boma-domain`: runs gained lease fields (`lease_owner`, `lease_fence`, `lease_expires_at`) and
    `holds_lease`; tenancy, ids, password validation, cursor helpers, event and message models,
    run state wire values.
  - `boma-app`: ports for every store/service (accounts, users, sessions, spaces, bots, threads,
    messages, runs with atomic `claim_next_queued`, events, invitations, ids/tokens/clock/hasher)
    and the use cases (sign-up/sign-in, create space/bot, send message with nonce idempotency,
    messages page, subscribe, activity list, claim, fenced status writes with the legible failure
    recorded, worker loop streaming message-update deltas). Sign-in failures are uniform
    (`AppError::InvalidCredentials`, dummy-hash branch, same shape for unknown address and wrong
    password).
  - `boma-api`: `/rpc/{namespace}/{procedure}` dispatcher with `deny_unknown_fields` DTOs, the
    `/api/auth/*` surface (capabilities, sign-up, sign-in, sign-out), session cookie `HttpOnly;
    SameSite=Lax; Secure` (config-gated), `x-space-id` header for tenants, stable
    `{code, error}` envelope, `x-request-id`/`no-store`/`nosniff`/`referrer-policy` headers; 12 RPC
    integration tests.
  - `boma-infra`: `0002_m1.sql` (12 tables, RLS on the eight tenant tables, `boma.current_space_id()`,
    `boma.is_system()`, `boma.rfc3339()`), `with_tenant`/`as_system` with transaction-scoped
    `set_config`, and the SQL adapters. The **lease reclaim** fix: a worker that disappeared
    mid-turn is handed out again once its lease expires (REQ-013, REQ-014), with the fence grown.
    `EventRow` decode matches the `type` column. **Integration tests run against a real Postgres
    with a fresh database per claim-dependent test** (`common::fresh_database`): the cross-tenant
    queue is global by design, so no two tests may claim from the same queue. The RLS probe
    connects as a non-superuser role because Postgres never applies policies to a superuser.
    The M1 adapters have 8 integration tests (`crates/infra/tests/m1.rs`); `m1.rs` cleans up after
    itself (the rows fall with the organization).
  - `boma-conformance`: the golden replay world (`golden_world.rs` — seeded space/bot/thread with
    fixed ids and a fixed clock); SSE fixtures compare their `data:` frames up to the fixture's
    frame count; body fixtures replay the nonce double-send. **13 API goldens replay byte for
    byte**; `golden-status.json` marks them implemented.
  - `boma-testkit`: in-memory stores (same rules: sequences, nonce idempotency, atomic claim,
    stale-fence refusal) and scripted ids/clock/tokens.
  - Failures are legible (REQ-021): when a run fails, the run carries a short, safe reason the chat
    can show — never a driver's internals, never the value that broke the rule.
  - **Web**: `/app/<botId>` thread screen (transcript as a log, live SSE with reconnect, composer)
    and `/app/activity` (one row per run, retry only on failed rows, states written in words), built
    on BoardUI semantic tokens and the i18n catalogue (en/fr/he). `lib/api/rpc.ts` speaks the real
    RPC surface with `credentials: "include"`. Two contrast faults found by axe are fixed by tokens.
  - **UI goldens implemented**: `REQ-001-send-message` and `REQ-012-run-statuses` pass in Playwright
    (route interception, `golden-status.json` says implemented). Web suite: **14 passed, 5 skipped**
    (the remaining five goldens wait on M2+).
  - **Environment note**: the local dev Postgres is a superuser and RLS does not apply to
    superusers, so the RLS proof runs as a dedicated unprivileged role (`boma_rls_probe`) — the
    same condition CI uses.
  - The traceability ratchet moved **down again** as requirements landed: 39 missing of 58 named
    (budget unchanged at 40). The M1 web goldens are counted by the Playwright manifest, and the
    two new ones (`REQ-001-send-message`, `REQ-012-run-statuses`) replay implemented.
  - **DEC-14 is closed**: the owner accepted the MIT licence on 2026-11-12.
  - The repository was pushed to a **public** GitHub remote (`boma-bot`) on 2026-11-12, with the
    board's approval to publish everything, including `spec/` and the vendored BoardUI docs.
- 2026-11-12 — Product name fixed by the owner: **Boma Bot** (`@boma/*`, `BOMA_*`, `com.boma.app`).
  Rebuild of the target analysed in `../re-workspace`. The analysed tree keeps its own identifiers so
  `file:line` citations stay valid.
- 2026-11-12 — Spec exported from `../re-workspace` with `tools/export-spec.sh`: 48 files, 58
  requirements, 30 API goldens, 7 UI goldens; golden sanity and secret scan clean (0 warnings).
  Manifest in `spec/EXPORT.md`. **CONFIRMED** (command output).
- 2026-11-12 — `/plan` written: `docs/PLAN.md` (scope, stack table, architecture, budgets, M0–M11,
  defect map, test strategy, UI plan) and 19 ADRs in `docs/adr/`. **CONFIRMED** (files exist).
- 2026-11-12 — `/scaffold` (M0) written: six-crate Cargo workspace with workspace lints and pinned
  toolchain; `domain` (run state machine), `app` (readiness + capabilities), `infra` (validated
  Postgres settings, lazy pool, embedded migrations), `api` (router, error envelope, `/health`,
  `/ready`, `/internal/health`), `bin` (`boma serve|worker|migrate|admin|probe`), `conformance`
  (golden manifest gate, API replay, OpenAPI contract); `migrations/0001_init.sql`; `compose.yaml`,
  `Dockerfile`, `Makefile`, `.env.example`; CI with fmt/clippy/nextest/coverage/deny/machete/trace/
  secret gates plus the web job; `apps/web` (Next `output: "export"`, Tailwind v4 placeholder tokens,
  Playwright shell/a11y/golden suites); `apps/desktop/README.md` deferral. **CONFIRMED** (files
  exist; see "Verified here").
- 2026-11-12 — Toolchain installed and **M0 compiled and run**:
  - `rustup` 1.29.3 with the pinned toolchain **1.99.1** (rustfmt, clippy, llvm-tools-preview),
    installed under `/tmp/opencode/rustup` with `CARGO_HOME=/tmp/opencode/cargo` (this box has no
    system compiler and no `sudo`; the C toolchain, `make`, `pkg-config` and PostgreSQL 17.11 come
    from the nix store).
  - `Cargo.lock` (230 packages) and `apps/web/package-lock.json` generated and **committed**: the
    CI and the Docker build now use `--locked` and `npm ci` for real.
  - First compile found and fixed seven real defects (see "Defects found by compiling").
  - `cargo build --release --locked --bin boma` → a 5.9 MB binary; `boma --version`, `boma --help`
    and `boma worker` (exits 1 with a clear message) verified.
  - **Against a real PostgreSQL 17.11**: `boma migrate` applies `migrations/0001_init.sql`
    (`app_meta` and `_sqlx_migrations` exist afterwards), `boma serve` answers `/health` 200
    `{"ok":true}`, `/ready` 200 with the capability report, `/internal/health` 200, `/healthz` 200
    (alias), an unknown route 404 with the stable envelope, all of them carrying `x-request-id`,
    `cache-control: no-store`, `x-content-type-options` and `referrer-policy`. `boma probe` succeeds
    against the running server and fails on a closed port. **CONFIRMED** (command output).
  - **In the browser**: `next build` (static export of `/`, `/app`, `/_not-found`), `tsc --noEmit`
    clean, `eslint --max-warnings=0` clean, Playwright **9 passed / 7 skipped** (the skipped ones
    are the UI goldens still marked pending, each with its milestone and reason) and axe with zero
    critical or serious violation in light, dark and RTL. **CONFIRMED** (command output).
  - `cargo deny check` → advisories, bans, licences and sources **all ok**; `cargo machete` → no
    unused dependency. **CONFIRMED** (command output).
- 2026-11-12 — `/ui-kit` done: **BoardUI (free tier) installed** through its MCP server and the
  shell rebuilt on it.
  - Vendored into `apps/web/`: `styles/theme.css`, `styles/typography.css`, `styles/globals.css`,
    `utils/cx.ts`, `utils/directional-icon.ts`, `utils/use-dismiss-on-outside-press.ts`,
    `components/foundations/direction/direction.tsx`, `components/base/buttons/button.tsx`. The
    placeholder `app/globals.css` is gone; the layout imports BoardUI's stylesheet.
  - The always-on rules are in the **fenced BoardUI block of the root `AGENTS.md`** (the diff
    only adds design conventions, reviewed line by line) and in `.cursor/rules/boardui.mdc`; the
    agent skill is in `.claude/skills/boardui/` — **restart opencode** to load it.
  - A minimal root `package.json` marks the repository as a project root: `install_rules` writes
    into the root `AGENTS.md` only when the directory holds a `package.json`. The web client keeps
    its own manifest and lockfile; nothing is hoisted.
  - The shell now speaks the catalogue: every label goes through `t(locale, key)`, the direction
    provider reads the document's `lang`/`dir` through `useSyncExternalStore`, and the boot script
    applies the stored theme and locale before the first paint.
  - The axe gate found two real contrast faults and both are fixed by picking tokens, not by
    literals: BoardUI leaves `--color-text-secondary` at 3.95:1 on the dark ground (re-mapped to
    `neutral-400` in `styles/boma.css`), and the navigation labels used the secondary text token
    on the secondary surface (4.42:1, now the primary text token). Both are listed in
    `docs/UI.md`.
  - Inter is loaded through `next/font/google` and self-hosted: no font request leaves the
    machine at runtime. **CONFIRMED** (command output, see "Verified here").
- 2026-11-12 — All third-party versions resolved from the live registries (`crates.io` API
  `max_stable_version`, `registry.npmjs.org`), never typed from memory. Table:
  `docs/DEPENDENCIES.md`. **CONFIRMED** (API responses in session).
- 2026-11-12 — `golden-status.json`: 30 API + 7 UI entries, 2 implemented (`health`, `ready`) and 35
  pending, each with its milestone and a reason. It is the single manifest for the Rust and Playwright
  runners. **CONFIRMED** (bijection checked by script and by `tests/golden_manifest.rs`).
- 2026-11-12 — `tools/trace_check.py` gained `--max-missing N`, a ratchet that fails above a budget
  instead of demanding 58/58 from day one. Default behaviour is still strict. Budget now **50**
  (`Makefile` `TRACE_MAX_MISSING`, CI). **CONFIRMED** (`python3 tools/trace_check.py spec .
  --max-missing 50` → 8/58, exit 0).
- 2026-11-12 — `tools/secret_scan.py` gained a per-line `secret-scan: allow` marker (with a reason)
  instead of a file-wide exemption; used by two test fixtures that must contain a Postgres URL.
  **CONFIRMED** (scan → "no credential pattern found").

## Verified here

Commands actually run in this session, and their result. Nothing else is claimed.

| Check | Command | Result |
|---|---|---|
| Formatting | `cargo fmt --all -- --check` | clean |
| Lint | `cargo clippy --workspace --all-targets --all-features -- -D warnings` | clean, 0 warnings |
| Types | `cargo check --workspace --all-targets --all-features` | clean |
| Tests | `cargo nextest run --workspace --all-features` | **140 tests, all passed** (run three times, stable) |
| Web tests | `npm run test` (Playwright + axe) with `FONTCONFIG_FILE=/tmp/opencode/fonts.conf` | **14 passed, 5 skipped** (pending goldens) |
| Coverage | `cargo llvm-cov --workspace --all-features --summary-only --fail-under-lines 80` | **84.47 % lines** (floor 80); the floor gate passes |
| Licences and advisories | `cargo deny check` | advisories ok, bans ok, licences ok, sources ok |
| Unused dependencies | `cargo machete --with-metadata` | none |
| Traceability | `python3 tools/trace_check.py spec . --max-missing 40` | 19/58 named by a test declaration, budget 40, exit 0 |
| Secrets | `python3 tools/secret_scan.py .` | 0 findings |
| Whole gate | `make check` | **exit 0** (M0); the M1 gates above were re-run individually on this machine |
| Web build | `npm run build` | static export of `/`, `/app`, `/_not-found` |
| Web types | `npm run typecheck` | clean |
| Web lint | `npm run lint` (`--max-warnings=0`) | clean |
| Browser | `npm run test` (Playwright + axe) | **12 passed, 7 skipped** (pending goldens) |
| Design system | `boardui init_boardui`, `install_rules`, `install_components`, `install_skill` | vendored into `apps/web/`, rules in the root `AGENTS.md`, skill in `.claude/skills/boardui/` |
| API goldens | `cargo test -p boma-conformance --test golden_api` | **13/13 implemented replay byte for byte** |
| UI goldens | `npx playwright test tests/golden.spec.ts` with `FONTCONFIG_FILE=...` | **REQ-001-send-message and REQ-012-run-statuses replay; the five later goldens still skip with reasons** |
| Release build | `cargo build --release --locked --bin boma` | 5.9 MB binary |
| Database | `boma migrate`, `boma serve`, `boma probe` against PostgreSQL 17.11 | migrations applied, all routes answer |
| Compose and CI YAML | `yq` parse of `compose.yaml` and `.github/workflows/ci.yml` | valid |

**Not run:** `docker build`, `docker compose up` (no Docker daemon and no `sudo` on this machine),
and therefore the container image and the Compose stack are **unverified**. The CI workflow itself has
never run on GitHub either: there is no remote and nothing was pushed. The Dockerfile was reviewed
and its build steps mirror the commands that were run by hand (`rustup component add …`,
`cargo build --release --locked --bin boma`), and the three image tags were checked against Docker
Hub (`rust:1.99.1-bookworm`, `postgres:17.11-alpine`, `debian:bookworm-slim` all exist).

Environment notes, so the next session does not rediscover them:

- The toolchain lives outside the repository (`RUSTUP_HOME=/tmp/opencode/rustup`,
  `CARGO_HOME=/tmp/opencode/cargo`, `CARGO_TARGET_DIR=/tmp/opencode/target`); `source
  /tmp/opencode/env.sh` restores the PATH.
- Playwright's Chromium needs the nix libraries: the binaries under `~/.cache/ms-playwright` were
  patched with `patchelf` (RPATH + nix loader) because this box has no system Chromium libraries.
- This box has **no system fonts at all**; Chromium measured every glyph as zero-height until
  `FONTCONFIG_FILE` pointed at the nix DejaVu fonts (`/tmp/opencode/fonts.conf`). Without it the
  accessibility suite fails on "heading is hidden" — an environment quirk, not a product defect. CI
  (ubuntu-24.04) has fonts.

## Defects found by compiling

Each of these was a real bug in the M0 scaffold, found by running the toolchain:

1. **`/ready` answered `500` instead of `503`** when a dependency was down: the error envelope
   derived the status from the code, and `INTERNAL` maps to 500. `ApiError` now carries its own
   status; `unavailable()` is `503` + `INTERNAL`, a genuine internal failure stays `500`
   (`crates/api/src/error.rs`, test `req_053_an_unavailable_dependency_is_a_503_that_stays_internal`).
2. **`ErrorBody` was not deserialisable**, so the envelope test could not read its own output.
   `Serialize`/`Deserialize` are now both derived (`crates/api/src/error.rs`).
3. **The run-state test contradicted the reference.** `req_012_only_failed_may_return_to_the_queue`
   forbade `leased → queued`, but the reference table (`packages/core/src/run-state.ts`, `allowed`)
   allows a lease to be released, a waiting run to be re-queued and a failed run to be retried. The
   table was right, the test was wrong; it now asserts the reference's three cases.
4. **`tower_http::timeout::TimeoutLayer::new` is deprecated** since 0.6.7 → `with_status_code`.
5. **`anyhow::Context` does not apply to `Box<dyn Error + Send + Sync>`** (the type `try_init`
   returns) → the error is built with `anyhow!` instead.
6. **`serde_yaml_ng` is unmaintained** (RUSTSEC-2024-0449 through `unsafe-libyaml`) → replaced by
   `serde-saphyr` 2.1.1, `deserialize` feature only.
7. **The workspace crates had no SPDX expression**, so `cargo deny` reported them `unlicensed`; the
   licence is now `MIT` in `[workspace.package]` (see "Decisions taken during M0").
8. **React logged hydration mismatches on every page**: the accessibility suite toggled the `dark`
   class and the RTL suite rewrote `lang`/`dir` after hydration, so React compared attributes it
   had not rendered. The theme is now applied by an inline script in `<head>` (Next.js guide
   "Preventing flash before hydration") and `<html>` carries `suppressHydrationWarning`; the dark
   scheme follows `prefers-color-scheme` and the suite no longer patches the DOM. Zero hydration
   warnings remain.

## Blocked

- **The Playwright suites need `FONTCONFIG_FILE=/tmp/opencode/fonts.conf`** on this machine: it has
  no system fonts, and Chromium dies with `SkFontMgr_FontConfigInterface Not implemented` the first
  time it renders a Hebrew (or any non-Latin) glyph — which is why every Hebrew test crashed before
  it could fail an assertion. CI (ubuntu-24.04) has system fonts and does not need it.
- **The later UI goldens stay pending** (`spec/golden/ui/`): `REQ-026-computer-panel` (M10, the
  panel; M4 shipped the RPC, not the screen), `REQ-037-shared-page`,
  `REQ-037-shared-page-invalid` (M6). `REQ-007-pending-card` and `REQ-009-unread-divider` are
  implemented and replayed. Each pending fixture carries its milestone; the runner skips it
  explicitly.

- **The Compose stack is built and probed in CI** (job `compose`, added with the M0–M4 audit):
  `docker compose up -d --build` then `/health`, `/ready`, `/healthz`, `/readyz`. This box cannot
  run it (the overlay has ~380 MB free; the Rust builder image alone is ~1.3 GB), so CI is the only
  place the criterion is observed.
- **Tauri shell deferred to M10** (`apps/desktop/README.md`): REQ-055 is a SHOULD and a shell that
  was never compiled would be a false claim. Mobile (REQ-056) stays deferred per ADR-016.
- **opencode must be restarted** for the BoardUI skill and the always-on rules to load: MCP tools
  and skills are bound at session start.

## Deviations

| Id | What | Why | Where enforced |
|---|---|---|---|
| DEV-01 | `/healthz` and `/readyz` exist as aliases of `/health` and `/ready` | AGENTS.md mandates those names; the spec's contract is `/health` and `/ready` | `crates/api/src/router.rs`; aliases are absent from the OpenAPI document and `tests/contract.rs` asserts that |

Any other externally visible difference would be a DEVIATION needing owner approval; none is known.

## Decisions taken during M0

- **MSRV 1.94** (not an older one): `sqlx` 0.9.0 declares `rust_version = 1.94.0`. Toolchain pinned to
  `1.99.1` (current stable at scaffold time), matching the `rust:1.99.1-bookworm` builder image.
- **TypeScript pinned to 6.0.3**, not `latest` (7.0.2): 7.x is the native/Go compiler port (~2.5 MB
  unpacked) that does not expose the JavaScript API Next's type plugin drives. Rationale recorded in
  `docs/DEPENDENCIES.md`; revisit when Next supports 7.x.
- **ESLint pinned to 9.39.5**, not `latest` (10.14.0): `eslint-config-next` pulls
  `eslint-plugin-react@7.37.5`, whose peer range ends at ESLint 9.7, so npm 10 refuses the tree with
  `ERESOLVE`. `@types/node` follows the Node 22 runtime (22.20.5), not the newest 26.x types.
- **Fixture manifest is JSON** (`golden-status.json`), not TOML: one parser for the Rust runner and
  the Playwright runner, no extra crate.
- **Lazy pool for `serve`**, eager for `migrate`: a database that starts after the process must make
  `/ready` answer `503`, not crash the process (REQ-053).
- **`worker` exits non-zero today** with "arrives in M1": a worker that exits `0` without draining
  would tell a supervisor that jobs are processed.
- **The project licence is MIT** (`Cargo.toml` + `LICENSE`), chosen by the agent so `cargo deny`
  could pass. The owner may replace it with any other SPDX expression and re-run `cargo deny check`;
  the dependency allow-list in `deny.toml` already covers MIT, Apache-2.0, BSD, ISC, MPL-2.0 and the
  usual permissive set.
- **Integration tests need a real database**: `crates/infra/tests/database.rs` reads
  `BOMA_DATABASE_URL` and fails loudly when it is missing instead of skipping itself. The `Makefile`
  exports a default URL; CI provides the PostgreSQL service it already declares.
- **Integration test files allow `expect`/`unwrap`/`panic` explicitly** (`#![allow(...)]` at the top
  of the file). Clippy's `allow-*-in-tests` only covers `#[test]` functions and `#[cfg(test)]`
  modules, not the helpers of a `tests/` file; AGENTS.md allows these calls in test code, so the
  permission is taken in writing, once per file.

## Next

1. **M5 — integrations** per `docs/PLAN.md`: the connector catalogue and per-space installs
   (REQ-032), remote MCP plus opt-in stdio (REQ-033), plugin submissions with review (REQ-034),
   skills (REQ-035) and write-only secrets (REQ-046).
2. The agent loop still makes no tool calls (the scripted provider answers text only; BYOK is
   deferred to M8), so the computer tools are proved at the use-case and provider boundaries, not
   by an agent run, and the observe/act use cases are not yet exposed on the RPC surface.
3. Coverage (80 % lines) is a CI job and runs on every push: 84.42 % of lines on `a35c82e`. This
   machine's `/tmp` cannot hold a fresh instrumented rebuild across sessions, so the number is
   read from CI output rather than measured here.
4. Owner reviews `docs/PLAN.md` §2 (stack table) and vetoes any row; answers DEC-01…DEC-03 if they
   have runtime knowledge. DEC-14 (the licence) is resolved: MIT.
5. Restart opencode after a BoardUI kit change, so the skill and the always-on rules reload.

## Decisions needed

Resolved since the first draft: **DEC-14** (MIT accepted by the owner, 2026-11-12).


From `spec/06-decisions-needed.md`. Each has a recommendation recorded in an ADR; silence means the
recommendation stands.

| Id | Question | Recommendation | ADR |
|---|---|---|---|
| DEC-01 | Does a reconnect replay events from `Last-Event-ID` in the reference? | **GUESS** yes; we implement resume-by-seq regardless | 005 |
| DEC-02 | Stale fence write: ignored or rejected? | **GUESS** ignored, logged at warn | 004 |
| DEC-03 | Is `waiting_takeover` queue-holding? | Code says yes (CONFIRMED in code); intent **GUESS** | 006 |
| DEC-04 | Surface scope for v1 | Web + desktop; mobile deferred | 013 / 016 |
| DEC-05 | Tenancy model | RLS + typed tenant context | 002 |
| DEC-06 | Agent runtime | Native Rust loop | 004 |
| DEC-07 | Realtime transport | SSE + WS for screen | 005 |
| DEC-08 | Computer providers | Docker supervisor + desktop host | 009 |
| DEC-09 | Billing scope | Limits native, provider behind a port | 012 |
| DEC-10 | Secrets encryption | Envelope, rotatable master key | 008 |
| DEC-11 | Locales at v1 | en + fr + he | 014 |
| DEC-12 | Asset reuse | Nothing visual reused | 015 |
| DEC-13 | Keep RPC procedure names | Yes, for the documented ones | 003 |
| DEC-14 | **Project licence** | MIT (owner accepted 2026-11-12) | — (`docs/DEPENDENCIES.md`) |

Raised by the M0–M4 audit, no recommendation yet:

| Id | Question | Recommendation | ADR |
|---|---|---|---|
| DEC-15 | REQ-019's "the parent receives the helper's result and continues" is unimplemented. Should it land now or with the agent engine? | **Resolved** — landed now as the structured helper outcome (`role=tool`, parent resumed), `e8c3bc9`, before M5 | — |
| DEC-16 | REQ-034 / D-06: the spec's own golden pins `POST /api/v1/plugin-submissions` as cookieless and public (`plugin-submission.json`, 200 `{ok, id}`), but D-06 asks to "require an account or a challenge". Should the endpoint require one (a DEVIATION from the golden), or stay cookieless with the D-01 shared counter as the only control? | Keep cookieless per the golden (it is the spec's documented public contract); 10/hour per IP via the shared Postgres counter (D-01); challenge leg deferred to you | — |


## Deferred requirements

| REQ | Why | Where recorded |
|---|---|---|
| REQ-049 (voice, COULD) | Not needed for a functional core; interfaces stay additive | PLAN §5 |
| REQ-055 (desktop, SHOULD) | Tauri shell lands at M10, after screens and signed releases exist | ADR-016, `apps/desktop/README.md` |
| REQ-056 (mobile, SHOULD) | PWA ships in M1; the Expo shell waits until device pairing is exercised | ADR-016, PLAN §5 |

## Proposals outside the spec

None. Anything proposed during the build goes here first, with the REQ it would touch.

## Known GUESS (to confirm)

- No runtime observation of the reference exists (the original was never installed or run). Every
  `spec/` claim is static or documentation-backed; `spec/TRACE.md` marks the two doc-only rows.
- The reference's run-state table was read from its source (`packages/core/src/run-state.ts`), which
  is stronger than documentation but still not a runtime observation.
- BoardUI Pro is not activated on this machine (`license_status`: none). The UI uses free-tier
  components; a Pro template would be an owner decision plus a licence check before any public repo.
- The JSON Schema validation of `opencode.json` never ran; the file was edited by hand and reviewed,
  not validated by a tool.
- The Compose stack and the Docker image are built and probed **in CI** (job `compose`); this box
  cannot run them, so the local observation is still absent.
- The plugin-submission golden (`spec/golden/plugin-submission*.json`) disagrees with the reference
  source for `plugin-submissions.ts` (the fixture accepts a synthetic single-segment host source and
  pins cookieless access; the source code requires a GitHub hostname and emails a confirmation
  address). The rebuild follows the fixture: HTTPS grammar, no GitHub-only restriction, no account,
  shared counter as the only control. The divergence is recorded under DEC-16.

## What was consulted in the original

| Path | Why |
|---|---|
| `../re-workspace/notes/ARCHITECTURE.md` | entry points, flows, trust boundaries |
| `../re-workspace/notes/validation-audit.md` | V-01…V-09 → defects D-01…D-09 |
| `../re-workspace/notes/open-questions.md` | injection findings, rebuild questions |
| `../re-workspace/notes/spec/**` | the contract itself |
| `../re-workspace/target/rakazo/packages/core/src/run-state.ts` | the authoritative run-state table, to settle the `leased → queued` transition (2026-11-12) |
| `../re-workspace/target/rakazo/apps/api/src/plugin-submissions.ts` | the endpoint the plugin-submission golden was captured against (2026-11-13) |
| `../re-workspace/target/rakazo/packages/adapters/src/plugin-connectors.ts` | the source grammar, the no-local-code installer and the skipped-parts vocabulary (2026-11-13) |
| `../re-workspace/target/rakazo/packages/adapters/src/builtin-skills.ts` | what a built-in skill looks like (2026-11-13) |
| `../re-workspace/target/rakazo/apps/api/src/routes/connections.ts` | the connector catalogue and per-space install shape (2026-11-13) |
| `../re-workspace/target/rakazo` (other paths) | only through the citations already recorded in the spec; not re-read in this session |

**Credentials seen in chat.** A GitHub PAT was pasted in chat on 2026-11-13 for a push. It was used
for that session's pushes only, never written to any file, log or commit; the owner should rotate it.


## Implementation notes carried over from the M3 session (verbatim, 2026-11-13)

- **REQ-024 (dispatch, retry, coalescing, MUST)** is done, including the two acceptance criteria
  the first cut missed. The worker's poll loop sweeps due routines before claiming runs
  (`boma_app::usecases::dispatch_routines`), creates a `trigger: routine` run per fire,
  reschedules to the next cron occurrence (`next_run_at` on the routine row), deactivates a spent
  `@once`, and resets the failure counter on completion. A routine whose previous run is still
  open is **not** fired a second time: the wake is recorded as `coalesced` and the schedule
  advances (no stampede). A failed routine-woken run whose failure is **retryable** retries with
  backoff (migration `0016_run_not_before.sql`; `runs.not_before` gates the claim): up to
  [`MAX_ATTEMPTS`] = 3 attempts, 60 s × 2^(n-1) capped at 1800 s
  (`boma_domain::run::retry_delay_seconds`); only then does the failure count toward
  `account_failures`. Past a threshold of 5 consecutive failures the routine pauses itself and
  **the owner is told** — a `System` message lands in the thread with the failure story, always
  on pause, and per run for routines created with `notify: true`. Failures reset on a clean
  completion via `record_completed`. A run ending `failed` whose failure is **not** retryable
  counts immediately. Routine-woken runs carry `routine_id` on `runs` (migration
  `0015_run_routine_id.sql`) — and the Postgres `INSERT INTO runs` forgot to write it (fixed in
  the same slice; the in-memory adapter masked the omission, so dispatch/coalescing/attribution
  only ever worked in tests). The single-flight check is `find_open_routine`: a co-worker may
  skip a wake whose run is already open elsewhere.
- **REQ-025 (run now, SHOULD)** is done: `routines.runNow` fires a routine once as a normal
  turn. The trial run is attributed to the routine (`routine_id`, trigger `User` so the failure
  counters do not move) but it does not depend on or advance the cron schedule — the routine
  keeps its `active` state and `nextRunAt` untouched. The provider pass uses the routine's
  prompt when a run carries `routine_id`, because a routine-woken history carries no person
  message. The wire body is `{routineId}`; the reply is `{routineId, runId, status}`.

Row order matches implementation order.

