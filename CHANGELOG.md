# Changelog

Notable changes to Boma Bot. This repository is pre-1.0; the format follows Keep a Changelog
loosely and the versions follow semver from the public contract's point of view (wire and HTTP
interfaces, migrations, data formats), not from internal refactors.

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
