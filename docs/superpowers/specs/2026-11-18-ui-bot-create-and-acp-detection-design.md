# Design — bot creation in the rail and ACP executor detection (2026-11-18)

## Intent and why

The owner's v0.3.0-rc2 smoke found two first-session defects in the UI/experience: once onboarding
is over there is no way to create another bot, and a machine with `opencode` installed is not
detected or offered as a teammate. UI and experience take priority over the 16 open parity issues.

## Goals

1. A member can create a second (or third…) bot from the app shell without redoing onboarding.
2. A deployment whose machine has a known ACP executor installed detects it and offers a one-click
   attach; the operator still makes the choice.
3. Use the free BoardUI components the owner named to improve the chat experience:
   `composer-loader`, `agent-thinking`, `file-upload`, `sidebar`, `date-calendar` — all installed
   via the MCP server. Pro components are excluded (no licence on this machine).

## Non-goals

- Editing an existing bot's instructions/avatar (proposal issue #17 — needs a spec amendment).
- Auto-attaching a detected executor; expanding the executor matrix (#18); hosted services.
- Changing the approval gateway or the permissive-default posture.

## Constraints

- **BoardUI is mandatory for the UI.** Use the installed source-owned BoardUI components, install
  a BoardUI form/input component via the MCP server for the new fields instead of hand-building a
  lookalike, style only with semantic tokens and composite type utilities, merge classes with
  `cx()` from `@/utils/cx`, and let dark mode ride the tokens. No raw palette classes or hex.
- Spec REQ-004 (one continuous thread per bot) and REQ-061 (local CLI agents as durable teammates).
  Tests carry the REQ id in their name; the trace budget stays at 7.
- All user-visible copy goes through the catalogue (`en`, `fr`, `he`; English fallback), including
  the new rail labels and the detection message.
- The seeded REQ-009 UI golden must keep passing; if the new always-visible button changes the
  assertion, refresh the golden and record the change explicitly.
- No upstream code, wording, brand or assets are used.

## Approaches considered

1. **Inline form in the bot rail + server-side detection (chosen).** The rail already has an inline
   create form for groups; mirroring it for bots is a small, consistent change. Detection has to be
   server-side: a browser cannot inspect the machine's PATH, and the Electron shell deliberately
   injects no Node into the renderer. The server checks the compiled executor registry with a short
   timeout and the UI calls the existing `bots.connectAcp` on attach.
2. Modal create flow + client/desktop IPC detection (rejected). A modal belongs with the agent-edit
   work (#17) and would wait on a spec amendment; desktop IPC would break the web/desktop parity
   rule (the same UI runs in both) and the shell's no-Node-in-renderer boundary.
3. Automatic attach on startup (rejected). Silent attachment of a local CLI agent to a bot changes
   permissions and attribution without an operator decision, against our charter.

## Design

**Slice 1 — create a bot from the rail.** Add a BoardUI icon button next to the Bots heading in
`bot-rail.tsx` (matching the Groups affordance), toggling a small inline BoardUI form (name,
optional purpose). Submit calls `bots.create`, refreshes the roster and navigates to the new
bot's room. Disabled while submitting; legible error from the catalogue on failure. Playwright
test named `REQ-004 …` covers create → rail → room.

**Slice 2 — detect and attach an ACP executor.** Add a `bots.detectedExecutors` RPC: the server
checks PATH for the executors in the compiled registry (`opencode` today) with a short timeout,
returns the found ids and never blocks startup. The rail shows a compact detected-executor row with
an **Attach** BoardUI button (label makes clear it is detected on the deployment machine); attach
calls the existing `bots.connectAcp`, refreshes the roster and navigates to the bot. No executor is
attached without the click. Playwright/wire test named `REQ-061 …` pins the detection list and the
attach path with a stubbed probe.

**Slice 3 — free BoardUI chat components.** Installed via MCP with their transitive components
(`composer-loader`, `agent-thinking`, `file-upload`, `sidebar`, `date-calendar`); licences and local
edits recorded in `docs/DEPENDENCIES.md` and `docs/UI.md`. Integration calls, decided here:
- `agent-thinking` replaces the bespoke shimmer label in the thread while a run is active, using a
  catalogue label and `prefers-reduced-motion` still honoured by the component.
- `composer-loader` wraps `ThreadComposer` while a reply is pending; `ThreadComposer` gains a
  `transparent` prop so the loader can paint the pill surface without a double background.
- `file-upload` is **not** wired into the composer yet: `threads.send` has no attachment ids, so
  the dropzone would be a dead end (forbidden). Tracked as issue #20 (REQ-001 attachments).
- `sidebar` is **not** swapped in: it carries its own navigation and would overwrite our rail,
  the new bot-create action and the REQ-009 goldens. Availability recorded for the next shell pass.
- `date-calendar` waits for the schedule/routine UI proposal (#14); no dead route.

The RPC addition is recorded in `spec/03-api-contract.md`; `docs/STATE.md` records both slices and
the evidence. No new REQ id and no ADR are needed: both slices implement existing REQs.

## What is left to the builder

Follow the plan produced by `writing-plans`: BoardUI component discovery/install, the two slices,
catalogue keys in three locales, golden check, Playwright tests, and a green CI run (fmt, clippy,
tests, web lint/typecheck/playwright/golden/axe, trace) before publication.
