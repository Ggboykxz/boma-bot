# STATE

## Current milestone

**M2 — in progress.** Slices land one commit at a time; tests written before the code.

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
- **REQ-008 (replies and quotes, MUST)** is done for its send path: `SendMessage` carries
  `reply_to_message_id` and `reply_quote`; a reply to a message references it and carries the
  quote of its text. The wire `MessageView` exposes both fields; the composer shows a reply
  banner with the quote; the transcript renders replies as a quoted block; the composer keeps a
  reply context with a cancel button; a Playwright test covers the click-to-reply flow. Mentions
  (REQ-008) wait on REQ-010 group chats, reactions wait on a `message_reactions` table — both
  are deliberately left for the next slice.

Remaining in M2, in dependency order: REQ-009 read state (SHOULD), reactions (REQ-008), mentions
(REQ-008); the tool-call loop and the refusal audit event from REQ-016 are the last piece of
REQ-016 and land with the agent engine that makes the call (milestone M3/M5); the BYOK and
provider OAuth login legs of REQ-015 land with M8 (models and credentials); the "helper answers,
parent continues" branch of REQ-019 lands with M3 (the agent engine).

The current commit carries REQ-003, REQ-006, REQ-007, REQ-008 (replies/quotes only), REQ-015,
REQ-016, REQ-017 and REQ-019 (skeleton); M2 is not done yet.

## Done

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
- **The M2+ UI goldens stay pending** (`spec/golden/ui/`): `REQ-007-pending-card`,
  `REQ-009-unread-divider`, `REQ-026-computer-panel`, `REQ-037-shared-page`,
  `REQ-037-shared-page-invalid`. Each carries its milestone; the runner skips them explicitly.

- **No Docker daemon and no `sudo`** on this machine: the container image and the Compose stack are
  written and reviewed but never built. Everything Compose does was verified by hand against a real
  PostgreSQL 17.11 instead (same image major, same environment variables, same migration step).
- **Tauri shell deferred to M10** (`apps/desktop/README.md`): REQ-055 is a SHOULD and a shell that
  was never compiled would be a false claim. Mobile (REQ-056) stays deferred per ADR-016.
- **No remote**: nothing has been pushed, so CI has never run on GitHub. The workflow is therefore
  unproven as a workflow, even though every command inside it was run by hand here.
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

1. Restart opencode, so the BoardUI skill and the always-on rules load.
2. `/implement M2`: runs and tools milestone — steering (REQ-003), cards, orphaned-run recovery
   hardening, approvals gate; then the five remaining UI goldens as their screens land.
3. Owner reviews `docs/PLAN.md` §2 (stack table) and vetoes any row; answers DEC-01…DEC-03 if they
   have runtime knowledge. DEC-14 (the licence) is still open.
4. Configure a remote and let CI run for real: the workflow has never executed on GitHub. The local
   dev Postgres lives on port 5433 with a superuser `boma`; CI/Makefile default to 5432.
5. Re-run the coverage gate with the full debug build when the disk allows the from-scratch
   instrumented rebuild (it was run with `CARGO_PROFILE_*_DEBUG=0`; the machine's `/tmp` is 91 %
   full across sessions and repeatedly forced cache eviction).

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
- The Compose stack and the Docker image were never built or run (no daemon).

## What was consulted in the original

| Path | Why |
|---|---|
| `../re-workspace/notes/ARCHITECTURE.md` | entry points, flows, trust boundaries |
| `../re-workspace/notes/validation-audit.md` | V-01…V-09 → defects D-01…D-09 |
| `../re-workspace/notes/open-questions.md` | injection findings, rebuild questions |
| `../re-workspace/notes/spec/**` | the contract itself |
| `../re-workspace/target/rakazo/packages/core/src/run-state.ts` | the authoritative run-state table, to settle the `leased → queued` transition (2026-11-12) |
| `../re-workspace/target/rakazo` (other paths) | only through the citations already recorded in the spec; not re-read in this session |
