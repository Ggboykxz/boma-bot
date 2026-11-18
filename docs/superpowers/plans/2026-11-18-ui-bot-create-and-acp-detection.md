# Bot creation in the rail and ACP executor detection — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** give the shell a BoardUI "new bot" action and a detected-executor row that can be attached in one click, with no silent attachment.

**Architecture:** two independent slices. Slice 1 is web-only: a small BoardUI form component toggled from the Bots rail heading, calling the existing `bots.create` RPC. Slice 2 adds a server-side `bots.detectedExecutors` RPC that probes the compiled ACP executor registry on PATH, plus an Attach button calling the existing `bots.connectAcp`. The web UI stays shared between desktop and browser; no Node injection, no new service.

**Tech Stack:** Next.js App Router + BoardUI (free tier only) + `@remixicon/react`, `cx()` from `@/utils/cx`; Rust/Axum RPC with `boma_app::acp_provider::executors()`; Playwright + `@axe-core/playwright`.

**Spec:** `docs/superpowers/specs/2026-11-18-ui-bot-create-and-acp-detection-design.md`, spec/01-requirements.md REQ-004/REQ-061, spec/08-ui-ux.md §8.3.

## Global Constraints

- BoardUI is mandatory: install missing components via the BoardUI MCP server, style only with semantic tokens and composite type utilities, merge classes with `cx()`, never raw palette classes or hex; dark mode rides the tokens.
- Tests must carry the REQ id in their name; trace budget stays `--max-missing 7`.
- All user-visible copy through `useTranslation()`; add keys to `en`, `fr`, `he` in `apps/web/lib/i18n.ts` (English fallback already implemented).
- No auto-attach: an executor is attached only after an explicit click.
- No upstream code, wording, brand or assets.
- Both release boot gates (Windows + Linux) must stay green before publication.

## Review Focus

- Empty rail / no roster answer: the `+` action must still be visible and usable — covered by a test in Task 2 with the roster route failing.
- Duplicate or whitespace-only bot name: must be rejected before the RPC call with a catalogue error — covered in Task 2.
- A slow or missing executor on PATH: detection must time out and the shell must still render — covered in Task 4 with a stubbed probe.
- A remote deployment (no executor on the server): detected row must be absent and no attach attempted — covered in Task 4.
- The seeded REQ-009 golden and axe gates: adding the button must not break the rail assertions — covered in Task 5.

---

### Task 1: Install the BoardUI form components

**Files:**
- Create via MCP: `apps/web/components/base/input/*`, `apps/web/components/base/buttons/icon-button.tsx`
- Modify: `apps/web/package.json`, `apps/web/package-lock.json` (dependency install)

**Interfaces:**
- Produces: `Input` from `@/components/base/input/input` (props `label`, `value`, `onChange(string)`, `placeholder`, `hint`, `errorMessage`, `isRequired`, `validationBehavior="aria"`, `isInvalid`, `size="small"`); `IconButton` from `@/components/base/buttons/icon-button` (props `icon` = component reference, `aria-label`, `size="small"`, `disabled`). `Button` from `@/components/base/buttons/button` already exists (props `variant`, `type`, `disabled`).

- [ ] **Step 1: install via the BoardUI MCP server**

Call `install_components` with `names: ["input", "icon-button"]`, `projectDir: /home/user/myapp/rebuild-workspace/.worktrees/before-m5/apps/web`, context describing the bot-creation form. Do not pass `overwrite`.

- [ ] **Step 2: verify the install**

Run: `cd apps/web && npm run typecheck`
Expected: PASS, the new components compile; `git status` shows the new source files and a lockfile change only.

- [ ] **Step 3: commit**

`git add apps/web && git commit -m "feat(ui): install BoardUI Input and IconButton for the bot form"`

---

### Task 2: Create a bot from the rail (REQ-004)

**Files:**
- Create: `apps/web/components/shell/bot-create-form.tsx`
- Modify: `apps/web/components/shell/bot-rail.tsx:162-224` (Bots nav), `apps/web/lib/i18n.ts` (three catalogues)

**Interfaces:**
- Consumes: `botsCreate({ name, description? }, signal?)` → `BotReply` (has `id`) from `@/lib/api/rpc`; `refreshRoster()` from `@/lib/roster`; `useTranslation()`.
- Produces: `BotCreateForm` React component with no props; owns `open` state and renders its own `IconButton` trigger. `BotRail` imports and renders it inside the Bots nav.

- [ ] **Step 1: write the failing test**

Create `apps/web/tests/bot-create.spec.ts` with three `test(...)` blocks named with `REQ-004`: (a) clicking the Bots `+` reveals a name field, submitting creates a bot and routes to `/app/<id>/`; (b) a blank/whitespace name keeps the form open and does not call `bots.create`; (c) with `bots/list` failing, the `+` button is still present. Follow the existing `page.route("**/rpc/bots/*")` fixture pattern in `tests/home.spec.ts`; assert the i18n label from `shell.rail.new-bot`, not hardcoded English.

- [ ] **Step 2: verify the test fails**

Run: `cd apps/web && npx playwright test tests/bot-create.spec.ts`
Expected: FAIL — no `+` button in the Bots nav.

- [ ] **Step 3: add catalogue keys**

Add `shell.rail.new-bot`, `bots.create.name`, `bots.create.name.placeholder`, `bots.create.description`, `bots.create.submit`, `bots.create.error` to `en`, `fr` and `he` in `apps/web/lib/i18n.ts`.

- [ ] **Step 4: implement `BotCreateForm`**

In the new file: `IconButton` with `RiAddLine` and `aria-label={t("shell.rail.new-bot")}` toggles a `<form>`; two BoardUI `Input` fields (name required with `validationBehavior="aria"` + `isInvalid`, optional description), a BoardUI `Button type="submit"` disabled while submitting; on submit trim the name, call `botsCreate`, `await refreshRoster()`, `router.push('/app/' + reply.id + '/')`; on error show `bots.create.error` from the catalogue. Close and reset on success.

- [ ] **Step 5: wire the rail**

Render `<BotCreateForm />` in the Bots nav next to the `<h2>` in `bot-rail.tsx`, leaving the Groups `+` untouched.

- [ ] **Step 6: verify**

Run: `cd apps/web && npx playwright test tests/bot-create.spec.ts && npm run typecheck && npm run lint`
Expected: PASS (3/3 tests, clean typecheck/lint).

- [ ] **Step 7: commit**

`git add apps/web && git commit -m "feat(ui): create a bot from the shell rail (REQ-004)"`

---

### Task 3: Server-side executor detection RPC (REQ-061)

**Files:**
- Modify: `crates/api/src/rpc.rs` (procedure list near `(namespaces::BOTS, "connectAcp")`, dispatch match near `bots_connect_acp`, request/reply structs), `crates/api/src/wire.rs` (reply type), `spec/03-api-contract.md` (procedure list)

**Interfaces:**
- Consumes: `boma_app::acp_provider::executors() -> &'static [ExecutorSpec]` where each spec has public `id: &'static str` and `command: &'static str`.
- Produces: `bots.detectedExecutors` — a POST RPC with `{}` body (use the existing strict `parse` with an empty struct) returning `DetectedExecutorsReply { executors: Vec<String> }` serialized camelCase; a pure helper `detected_executor_ids(probe: impl Fn(&str) -> bool) -> Vec<String>` filters `executors()` by the probe; ids come only from the compiled registry.

- [ ] **Step 1: write the failing wire test**

In `crates/api/tests/rpc_surface.rs`, add `req_061_detected_executors_reports_only_known_ids`: call `detected_executor_ids(|_| true)` and assert the result equals the ids from `boma_app::acp_provider::executors()`; call it with `|_| false` and assert empty; an unknown id can never appear. Then assert the POST `bots.detectedExecutors` reply shape with the failing probe injected.

- [ ] **Step 2: verify it fails**

Run: `cargo test -p boma-api req_061_detected_executors -- --nocapture`
Expected: FAIL — procedure not found.

- [ ] **Step 3: implement the probe and the handler**

Add a `detect_executor(command: &str) -> bool` helper that resolves the command on PATH and probes it with a 2-second `tokio::time::timeout`; a spawn error or timeout means "not detected". Register `bots.detectedExecutors` in the procedure list and dispatch, returning `DetectedExecutorsReply` with the ids of detected registry entries. Never start an ACP session for detection, never attach anything.

- [ ] **Step 4: document the procedure**

Add `bots.detectedExecutors` next to `bots.connectAcp` in `spec/03-api-contract.md` with its empty request and reply shape.

- [ ] **Step 5: verify**

Run: `cargo test -p boma-api req_061_detected_executors && cargo fmt --check`
Expected: PASS, format clean.

- [ ] **Step 6: commit**

`git add crates spec && git commit -m "feat(api): detect known ACP executors on PATH (REQ-061)"`

---

### Task 4: Detected-executor row and attach action (REQ-061)

**Files:**
- Create: `apps/web/components/shell/acp-detected-row.tsx`
- Modify: `apps/web/components/shell/bot-rail.tsx`, `apps/web/lib/api/rpc.ts`, `apps/web/lib/i18n.ts` (three catalogues)

**Interfaces:**
- Consumes: `botsDetectedExecutors()` → `{ executors: string[] }` and `botsConnectAcp({ botId, executor })` → `AcpAgentConnectedReply` (fields `botId`, `executor`, `attachedAt`) — both added to `@/lib/api/rpc.ts` as `call("bots", <procedure>, body)` wrappers; `botsList` to pick the target bot; `refreshRoster()`, `useTranslation()`.
- Produces: `AcpDetectedRow` React component with no props; fetched once on mount via the existing abort-controller pattern in `BotRail`; renders nothing when the list is empty.

- [ ] **Step 1: write the failing test**

Add `REQ-061 a detected executor is offered and attached only on click` to `tests/bot-create.spec.ts`: stub `bots/detectedExecutors` to return `["opencode"]`, assert a catalogue-labelled row appears with an Attach button; assert no `bots/connectAcp` call before the click; click it and assert one POST with `{ botId, executor: "opencode" }`, then a roster refresh.

- [ ] **Step 2: verify it fails**

Run: `cd apps/web && npx playwright test tests/bot-create.spec.ts`
Expected: FAIL — no detected row.

- [ ] **Step 3: add RPC client wrappers and catalogue keys**

Add `botsDetectedExecutors` and `botsConnectAcp` to `apps/web/lib/api/rpc.ts` with the exact signatures above; add `shell.rail.acp.detected`, `shell.rail.acp.attach`, `shell.rail.acp.attached`, `shell.rail.acp.no-bot` to all three catalogues.

- [ ] **Step 4: implement the row**

`AcpDetectedRow` maps ids to the catalogue label, renders a compact row with a BoardUI Attach button; on click calls `botsConnectAcp({ botId: firstActiveBot.id, executor: id })`, then `refreshRoster()` and shows the `attached` note. When there is no active bot, the Attach button is disabled with the `no-bot` hint. If the list is empty, render `null`. Handle a failed detection as empty (no error surface).

- [ ] **Step 5: wire the rail and verify**

Render `<AcpDetectedRow />` just below the bot list in `bot-rail.tsx`. Run: `cd apps/web && npx playwright test tests/bot-create.spec.ts && npm run typecheck && npm run lint`
Expected: PASS (4/4 tests).

- [ ] **Step 6: commit**

`git add apps/web && git commit -m "feat(ui): offer detected ACP executors with explicit attach (REQ-061)"`

---

### Task 5: Golden, accessibility and full gate

**Files:**
- Modify: `spec/golden/ui/REQ-009-unread-divider.json` only if the golden replay fails because of the new always-visible button; `docs/STATE.md`

**Interfaces:**
- Consumes: all four Playwright tests above; existing `tests/golden.spec.ts` and `tests/a11y.spec.ts`.

- [ ] **Step 1: run the web suite including goldens and axe**

Run: `cd apps/web && npm run build && npx playwright test`
Expected: PASS, including REQ-009 golden replay and zero critical/serious axe findings. If the golden fails only because of the new button, update the seeded assertion minimally and record the deviation in `docs/STATE.md`.

- [ ] **Step 2: run the Rust gate**

Run: `cargo test --workspace --locked` and `cargo clippy --workspace --all-targets -D warnings`
Expected: PASS (the 12 Docker-dependent computer tests may fail locally without Docker; CI runs them — record that, do not weaken the gate).

- [ ] **Step 3: record the work and commit**

Append both slices to `docs/STATE.md` with the test names and CI evidence. Commit: `git add docs && git commit -m "docs: record the bot-create and ACP-detection UI slices"`. Push to `dev`; wait for all seven CI jobs including `desktop-boot-windows` before any release.

---

### Task 7: Free BoardUI chat components (owner's UI list)

**Files:**
- Installed already (Task 1 scope, expanded): `composer-loader`, `agent-thinking`, `file-upload`, `date-calendar`, `sidebar` + transitive components via MCP; licences/local edits in `docs/DEPENDENCIES.md` and `docs/UI.md`.
- Modify: `apps/web/components/thread/thread-screen.tsx:512-557`, `apps/web/components/thread/thread-composer.tsx:41-54`, `apps/web/tests/bot-create.spec.ts` (or a new `thread-ui.spec.ts`)

**Interfaces:**
- Consumes: `AgentThinking` from `@/components/application/agent-thinking/agent-thinking` (props `variant`, `label`, `showTimer`); `ComposerLoader` from `@/components/application/composer-loader/composer-loader` (props `active`, `children`); `runIsActive` and `sending` from `ThreadScreen`; `cx()` from `@/utils/cx`.
- Produces: `ThreadComposer` gains `transparent?: boolean` (default false) removing only its surface classes; `ThreadScreen` wraps the composer in `ComposerLoader active={runIsActive || sending}` and renders `AgentThinking` instead of the bespoke shimmer span.

- [ ] **Step 1: write the failing test**

Add `REQ-012 the run status uses the BoardUI thinking indicator and the composer lights up` to a new `apps/web/tests/thread-ui.spec.ts`: stub a running run via the existing fixtures, assert the BoardUI thinking component is visible with the translated run label, and assert the composer wrapper carries the loader's active treatment. Keep the existing REQ-012 statuses test untouched.

- [ ] **Step 2: verify it fails**

Run: `cd apps/web && npx playwright test tests/thread-ui.spec.ts`
Expected: FAIL — bespoke shimmer, no loader.

- [ ] **Step 3: add the `transparent` prop and swap the components**

In `ThreadComposer`: add the prop, merge form classes with `cx()` so `bg-background-primary-default`/`border` are omitted when transparent (the loader paints the surface). In `ThreadScreen`: import both components, replace the shimmer `<span>` with `<AgentThinking variant="wave" label={runLabel} showTimer={false} />`, remove the now-unused `CSSProperties` import if lint reports it, and wrap `<ThreadComposer transparent …>` in `<ComposerLoader active={runIsActive || sending}>`.

- [ ] **Step 4: verify**

Run: `cd apps/web && npx playwright test tests/thread-ui.spec.ts && npm run typecheck && npm run lint && npx playwright test tests/golden.spec.ts tests/a11y.spec.ts`
Expected: PASS, goldens and axe still green.

- [ ] **Step 5: record the deliberate deferrals and commit**

In `docs/STATE.md`, record that `file-upload` waits for issue #20 (no `threads.send` attachment ids), `sidebar` waits for a shell/golden pass, and `date-calendar` waits for proposal #14 — installed but not wired, so no dead UI. Commit: `git add apps/web docs && git commit -m "feat(ui): BoardUI thinking indicator and composer loader (REQ-012)"`
