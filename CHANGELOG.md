# Changelog

Notable changes to Boma Bot. This repository is pre-1.0; the format follows Keep a Changelog
loosely and the versions follow semver from the public contract's point of view (wire and HTTP
interfaces, migrations, data formats), not from internal refactors.

## [Unreleased]

Everything pushed to `dev` after v0.3.0-rc5 (tag `40e4959`), verified by dev run `44559012804`
(7/7 jobs green, including the packaged Windows boot gate). Entries are grouped by kind; **all
of this ships in the next release**, and `CHANGELOG.md` is the authoritative record — nothing
lands in a release without an entry here.

### Fixed

- **No implicit approval on an empty question answer (REQ-017, ADR-033)** — `CardBlock` no longer
  submits `draft || approve` for question cards: Submit stays disabled until a non-empty answer
  exists, the action reads "Send answer", and a `REQ-017` Playwright test proves no `threads.answer`
  call fires on an empty field. The REQ-007 pending-card golden now pins the explicit affordance;
  the three core locales carry the new copy. Approval and purchase cards are unchanged.
- **Tenant settings are applied in one transaction-scoped statement (REQ-043, ADR-002)** — the space
  id and the system reason were set by two sequential queries, leaving a window in which one was
  present without the other. One `set_config` pair closes that window and halves the round trips;
  a source-scan test pins the invariant. No wire change, no policy change.

### Docs

- **OpenBot parity swarm, waves 1-5** (`d5636ce`, `d80f1b5`, `a7745ae`, `41d4814`, `53fe72b`) —
  50+ specialised workers restate upstream capabilities in our own words as parity proposals
  (inspiration only; no upstream code, brand, wording, URLs or Pro components). Proposals live under
  `docs/openbot-swarm/`.
- **Internal v0.4.0 upgrade swarm, all 530 briefs delivered** (`9789428` … `0f51bce`) — one
  read-only proposal per module per lens (architecture / reliability) read from the real files,
  waves 1-22, `docs/upgrade-swarm/`.
- **v0.4.0 upgrade plan** (`bea5176`) — `docs/upgrade-swarm/PLAN-v0.4.0.md`: 12 cross-cutting
  themes with cited brief ids, a P0/P1/P2 roadmap, an owner-decision register and the coverage
  appendix. First P0 item (tenant statements) already landed; the rest await approval.
- **Release discipline recorded** — every release now carries a complete delta of everything that
  changed, docs included (see AGENTS.md "Working method" and the `publish` job's preflight check).

### CI

- The release `publish` job now fails fast when `CHANGELOG.md` has no `## [<version>]` section for
  the tag being published: the release notes and the artifacts must enumerate every change, or the
  publication is refused.

## [0.3.0-rc5] - 2026-11-19

Release candidate 5: the UI/UX pass requested after rc4, validated by full dev run `44429722001`.

### Added

- **Contextual approval cards (REQ-007/017)** — BoardUI `Button`/`Input`, a translated region name,
  effect/reversal copy and an explicit status line; chosen by a seven-worker Queen swarm by both
  majority (5/7) and weighted vote (6.6/8.6). No wire change.
- **Board assignee filter (REQ-038)** — BoardUI `Select` with column counts following the filter.
- **Motion pass (REQ-057)** — pointer-only press feedback, strong-ease-out card entrance, 40 ms
  stagger; reduced motion keeps the fade and drops movement. BoardUI form controls replace raw
  inputs; 14 locales ship with core strings translated and English fallback for the long tail.

### Fixed

- Swarm dissent from the Rust worker was accepted as a scope guard: no transport or golden data
  change shipped with the approval-card slice.

## [0.3.0-rc4] - 2026-11-18

Release candidate 4: adds the group-room execution controls the owner asked for after rc3.

### Added

- **Pause/Resume (REQ-067)** — the activity list offers Pause on queued runs and Resume on paused
  ones through `runs.pause`/`runs.resume`; the web run vocabulary gained the `paused` status.
- **Reassign a delegation (REQ-068, ADR-031)** — `reassign_delegation` validates group membership
  and room scope, stops the old run at its tool boundary, creates the new run with the same context
  and `parent_run_id`, and records the hand-off as a `System` message plus its event. New
  `groups.reassign` RPC, `groups.get.activeRun`, and BoardUI "Reassign to …" buttons in the room.

CI evidence: dev run `44407160912` — 7/7 jobs green including the packaged Windows boot; rc4 cut
from the same tree after the version bump.

## [0.3.0-rc3] - 2026-11-18

Release candidate 3: the owner's rc2 smoke found two first-session defects — no way to create a
second bot after onboarding, and no sign that `opencode` was installed. Both are fixed, using the
free BoardUI components installed at the owner's request.

### Fixed

- The Bots rail now has a BoardUI `+` action with an inline create form (`IconButton`, `Input`,
  `Button`) calling `bots.create` and opening the new room; blank names are rejected before the
  call and failures keep the form usable. Three `REQ-004` Playwright tests.
- New `bots.detectedExecutors` RPC (two-second PATH probe of the compiled registry, strictly
  parsed empty body) and a rail row offering **Attach** via `bots.connectAcp` — never automatic.
  `req_061_detected_executors_reports_only_known_ids` and a `REQ-061` Playwright test.
- The thread uses BoardUI `agent-thinking` and `composer-loader` instead of the bespoke run
  shimmer, with a `REQ-012` Playwright test; UI goldens and axe stay green.
- The desktop pack legs retry transient Maven 404s while keeping the pinned PostgreSQL SHA-256
  checksum; a CDN hiccup had failed an otherwise healthy Linux leg.

### Known limitations

- `file-upload` is installed but not wired: `threads.send` has no attachment ids yet (issue #20).
  `sidebar` and `date-calendar` are installed but unused (shell/goldens and proposal #14).
- No Pro BoardUI component ships (no licence on this machine).

CI evidence: full dev run `44372621714` — 7/7 jobs green including the packaged Windows boot;
rc3 cut from the same tree after the version bump.

## [0.3.0-rc2] - 2026-11-18

Release candidate 2 for v0.3.0 — the first Windows first launch that actually works end to end.
CI now boots the packaged portable on a clean `windows-latest` runner and blocks publication
unless both desktop boot gates are green.

### Fixed (the Windows desktop first-launch chain, all reproduced on a clean runner)

- `SANDBOX_PROVIDER=none` no longer validates `DOCKER_HOST` (REQ-027): the Linux default socket
  is never absolute on Windows, so the whole configuration was rejected and every `boma`
  subcommand refused to boot — the owner's "the migration did not apply". The socket is
  validated only when the docker provider is selected.
  (`req_027_a_no_computer_deployment_ignores_an_unusable_docker_host`; red then green.)
- The desktop's readiness probe appended `/health` to a URL already ending in `/health`, so every
  health poll got a 404 and after 90 s the app destroyed the stack itself. The probe now takes
  the base origin; `isDeploymentReachable` owns the suffix.
- Electron derived `userData` from the scoped package name (`%APPDATA%\@boma\desktop`); the app
  now has a root `productName`, so its data lives at `%APPDATA%\Boma Bot\boma`, as documented.
- The vendor staging copies the OpenSSL DLLs beside `boma.exe` (Windows resolves DLLs next to the
  executable); the portable pack boots without a missing-library dialog.
- Bring-up failures persist to `boma.log` and `boot.log` (step + raw output) instead of appearing
  only in the failure dialog; the Windows boot job relays them as artifacts.
- The Windows boot harness reaps the app tree (`taskkill /T /F`) and exits explicitly — the boot
  check had been silently hanging until CI's job timeout, masking a green run as cancelled.
- The boot harness speaks the RPC surface correctly (`POST /rpc/<ns>/<procedure>`, JSON bodies).
- The desktop shell drops the BrowserWindow icon that pointed at a file the packer never shipped.

### Changed

- The release pipeline gates publication on both desktop boot jobs; the Windows portable is
  boot-tested before any attestation, checksums or upload runs.
- Version metadata everywhere moved to `0.3.0-rc2`.

CI evidence: run `44199971295` — 7/7 jobs green. The Windows portable unpacks, starts, applies the
migrations, serves, answers the RPC round-trip, renders both screens with zero axe findings, and
sends a message typed from the composer (screenshots uploaded as CI artifacts).

Release candidate 2: the owner smokes the Windows installer and/or portable before v0.3.0 final.

## [0.3.0-rc] - 2026-11-16

Release candidate for v0.3.0. The Windows/Linux desktop is now a fully self-contained app, like
OpenBot: the installer carries the web export, the `boma` server binary and an embedded
PostgreSQL 17.12.0 (zonkyio, pinned verified download). First launch runs `initdb` in the app's
user-data folder, starts Postgres on a fresh loopback port, applies the migrations
(compiled into the binary), starts `boma serve` and `boma worker` as supervised children, and
opens the app — no Docker, no system Postgres, no account, no setup. `BOMA_DESKTOP_URL` still
selects an externally-hosted origin (server tarball path). The HTTP port persists between
launches; crashes quit with a named-step dialog, never an anonymous dead end. Linux stays
`dir` + tar.gz; macOS stays out of scope for this rc.

CI pinned evidence: both desktop legs pack with the embedded stack, Linux additionally
rehearses the full boot (initdb → migrate → serve/worker → RPC round-trip) headlessly; the Windows
leg compiles through vcpkg-provided OpenSSL (`x64-windows-static-md`, static, no DLLs).
React version metadata everywhere moved to `0.3.0-rc`.

Release candidate: the Windows owner smokes the installer before v0.3.0 final is cut.

## [0.2.3] - 2026-11-16

Patch release of v0.2.2: the desktop shell is no longer a dead-end when the server is not running.
On first launch it probes `http://127.0.0.1:3000/health`; if nothing answers it offers a one-click
local stack (docker compose, `BOMA_LOCAL_FIRST=true`, loopback-only, no account, no password) and
waits for readiness; if docker or the compose file is missing, it says exactly which two supported
paths exist (the one-click stack, or the server tarball + `BOMA_DESKTOP_URL`). The compose file
gains an optional `BOMA_LOCAL_FIRST` passthrough and a parameterized `BOMA_BIND_ADDR` for
single-machine local-first deployments.

## [0.2.2] - 2026-11-16

Patch release of v0.2.1: ships the new brand. The enclosure/ticks mark authored for v0.2.0 was
rejected by the owner; the shipping brand is the owner-supplied robot-cat head (dark gunmetal,
cyan glowing eyes — `assets/brand/boma-bot-head.svg`). New app icons everywhere (desktop NSIS
installer art, web favicon/apple-touch/icon, README lockup, landing and auth shells). No product
changes.

## [0.2.1] - 2026-11-16

Patch release of v0.2.0: the v0.2.0 release published the Linux artifacts, but its Windows desktop
job failed its own verification step before uploading — `gh attestation verify` accepts exactly one
file, and the workflow passed three in one call. The Windows assets were never published. Nothing
in the product changed; the Windows installer and portable zip ship here with the documented
attestation flow intact. Verify with `gh attestation verify` as shown in docs/UPGRADING.md.

## [0.2.0] - 2026-11-16

The first release after the v0.1.0 baseline: the improvement program (M12, M13.1, M15) and the
first Windows desktop build. 61 of 64 REQ ids are covered by tests carrying their id in the
name; REQ-049 (voice, COULD) and REQ-056 (mobile, SHOULD) stay deferred by record, and REQ-061
is authored but not built.

### Added

- **External agents over AG-UI (M12, REQ-059, ADR-026)**: a bot can be driven by an external
  AG-UI agent (TCP executor in the loopback tests, real connector in deployments); the run's
  answer appears in the room attributed to the bot, and the approval gate relays post-approval
  effects exactly once. `bots.connectExternal` / `bots.externalAgentStatus` /
  `bots.disconnectExternal` plus an inline panel in the thread header.
- **Sign-in requests (M13.1, REQ-060)**: a bot can ask a person to sign into a site; the ask is
  a write-class effect that passes the ordinary approval gate; answers land in `signin_requests`
  (`0055_signin_requests.sql`, PG + RLS); RPC `bots.requestSignIn` / `bots.signIns`; grant is
  one-shot, a second resolve is refused.
- **Web Push (M15.1, REQ-064)**: content-free notifications (waiting / completed / failed),
  per-(space, member, bot) UTC-day cap, `/sw.js`, VAPID (`BOMA_VAPID_PRIVATE_KEY`),
  member-supplied endpoints screened by an SSRF guard. Migration `0056_notifications.sql`.
- **Notification destinations (M15.2, REQ-065)**: Slack/Teams incoming webhooks per space,
  sealed at rest under the space key, delivered as a single title line only, per-(member, bot)
  opt-in, registration Owner/Admin-only. Migrations `0057_notification_destinations.sql`,
  `0058_notification_destination_flags.sql`.
- **k3d single-node chart (M15.3)**: `deploy/helm/boma-bot` — app + worker + migrate Job +
  in-cluster throwaway Postgres (or an external DSN), optional ingress; CI job lints and
  templates it.
- **Local-first bring-up (M15.4, REQ-066, ADR-029)**: `BOMA_LOCAL_FIRST=true` on a loopback
  bind — no account, no password, no sign-in round trip; the operator (or a code agent over
  loopback) is the local owner, seeded on first use. Refused at boot on any non-loopback bind.
- **Windows desktop (REQ-055)**: NSIS installer (`boma-desktop-<version>-win-x64-setup.exe`,
  per-user by default) and portable zip (`boma-desktop-<version>-win-x64-portable.zip`), built
  on a real `windows-latest` runner; unsigned by decision (ADR-016); keyless Sigstore
  attestations for both assets and for `SHA256SUMS-windows.txt`. No auto-update; updates manual.
- **Brand**: the original Boma Bot mark (enclosure-and-core) is wired into the desktop
  installer, the app icons, the web favicon/apple-touch-icon and the README lockups
  (`assets/brand/`).

### Fixed

- **The web sign-in screens read the wrong capability report**: the web client had been written
  against the OpenBot-shaped `/api/auth/capabilities` document; the binary serves the spec's
  shape, so the sign-in form never rendered and `sso.map` dereferenced `null`. The web type now
  matches the spec exactly; fixtures replay the real document.
- Release notes pointed at CI jobs that did not exist under those names; they now name all five
  (rust, web, chart, compose, desktop on ubuntu + windows), and the desktop install instructions
  document the Sigstore verification flow for the release artifacts.

### Security

- No new secrets are introduced anywhere in the release path; the Windows artifacts are attested
  with the same keyless Sigstore/OIDC chain as the Linux ones, and
  `SHA256SUMS-windows.txt` is attested itself. Verify, never trust the bytes.

## [0.1.0] - 2026-11-15

First release — the whole product, rebuilt in Rust from the spec, milestone by milestone.
55 of 58 REQ ids are covered by tests carrying their id in the name; REQ-049 (voice, COULD) and
REQ-056 (mobile, SHOULD) stay deferred by record (`docs/PLAN.md`, `docs/STATE.md`).

### Added

- **Platform (M1–M3)**: persistent bots on threads; one message ↔ at most one run; idempotent
  sends (`clientNonce`); crash-safe run state machine with leases, fences and retry; steering
  messages; helper runs (REQ-019); memory (append-only revisions, scoped); routines (cron,
  quiet hours, retry ladder, self-pause on repeated failure); the full RPC surface and the
  SSE stream.
- **Computers (M4, REQ-026..031)**: one lazy computer per bot, released when idle; Docker
  supervisor with takeover-exclusive sessions, screen streaming behind a capability
  (`computer.openScreen` + `/api/internal/screen-target`), private isolation and a team mode;
  fail-closed provider selection (`SANDBOX_PROVIDER`).
- **Integrations (M5, REQ-032..035, 046)**: connector catalogue and per-space installs; MCP
  remote + opt-in stdio with host allow-listing and SSRF guards; OAuth PKCE; skills; plugin
  submissions with a marketplace install path and admin review; write-only bot secrets with a
  `BOMA_MASTER_KEY` vault and a `rekey` command.
- **Artifacts, pages, board (M6, REQ-036..039)**: content-addressed artifact storage with
  sniffed types (D-03) and revisioned families; shareable pages (public, uniform for
  missing/revoked, sandboxed renderer); ticket board flag-gated (`BOMA_BOARD`).
- **Channels (M7, REQ-047, 048, 050)**: inbound mail with held/allowlisted/policy routing and a
  drop-audit; Slack / Telegram / generic HMAC messaging ingress with installation management;
  bot webhooks with wake-routines for repository events.
- **Money (M8, REQ-018, 051, 052)**: spend caps and the purchase card; Stripe checkout; model
  catalogue with live key validation; plan-gated purchase.
- **Operate (M9, REQ-044, 045, 054, 058)**: export/import/delete account as a streamed zip;
  audit export as JSONL with secret redaction; SCIM provisioning (flag-gated); structured logs
  with request ids; metrics (`BOMA_METRICS`); CSP reporting; admin surfaces gated by space
  role (members, invitations, grants, plugin review).
- **UI (M10, REQ-055 + all UI goldens)**: every screen from `spec/08-ui-ux.md`; 7/7 UI goldens
  implemented and replay-verified; axe with zero critical/serious violations including RTL;
  Electron desktop pack in CI (release signing is the owner's identity, see release notes);
  the static export is served by the Rust binary (`apps/web`, ADR-013).
- **Groups (M11.1–M11.3b, REQ-010)**: create a group with member bots and one shared thread;
  mention-routed fan-out (`@name`, case-insensitive, creation order, one run per mentioned
  bot, per-bot nonce scoping); the `/app/g/:groupId` room with attributed transcript and a rail
  section with an inline "New group" form; `groups.get|list|messages`; bot-to-bot handoff
  (`delegate.bot` — write-class, approval card, one hop, receiver attributed in the room,
  requester resumed with the relayed result).
- **Docs**: `docs/adr/` 001–025, `docs/PLAN.md`, `docs/UPGRADING.md`, `docs/TRACEABILITY.md`,
  `docs/DEPENDENCIES.md`, README screenshots (`docs/screenshots/`).

### Security

- Argon2id password hashing; sealed secrets vault; constant-time comparisons everywhere a
  bearer is checked; per-IP/per-key shared rate limiting (D-01); default-deny forwarded
  headers unless `BOMA_TRUSTED_PROXIES` is set (D-04); magic-byte sniffing on upload (D-03);
  SSRF allow-listing per space with an audit event (D-05); no `/api/dev/` mail-capture route
  exists (D-02); per-procedure body caps (D-07); auth configuration is asserted, not inherited
  silently (D-08); `runAsSystem` has an allow-listed reason set and a CI scanner (D-09).

### Removed

- The `GET /api/dev/emails` development mail reader is absent by construction, pinned by a test.

### Known limitations / deferred

- REQ-049 (voice, COULD) and REQ-056 (mobile, SHOULD) are deferred and recorded; trace covers
  56 of 58 ids.
- Browser automation computers need `SANDBOX_PROVIDER=docker` with
  `BOMA_COMPUTER_IMAGE=zenika/alpine-chrome:latest` — the default image is a plain shell.
- Bot-to-bot handoffs are one hop and always approval-gated (ADR-025).
- The Electron app carries an inert, static update feed; there is no silent auto-update (ADR-016).
- The public page renderer (`/p/<token>`) serves the current build's blocks only.

### Quality evidence

- `cargo test --workspace` green; `boma-infra` PG leg covers every migration 0001–0053;
  the API golden world replays byte-exact (`group-create`, and every other implemented
  fixture); UI goldens replayed via Playwright; axe clean incl. RTL; CI runs
  fmt/clippy/nextest/coverage ≥ 80%/deny/trace/secrets, web build+typecheck+lint+playwright/
  golden/axe, Electron pack and the compose build on every push.
- `trace` reports 56/58 REQ ids in test names (REQ-049/056 deferred by record).
