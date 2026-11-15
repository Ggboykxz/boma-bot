# ADR-016 — Desktop and mobile shells: Tauri for desktop, a PWA plus Expo later for mobile

Status: accepted with the owner's veto applied (2026-11-15, milestone M10 gate)

> **Veto applied (owner, 2026-11-15):** "Electron est mieux que tauri" — the proposed Tauri
> decision is withdrawn and replaced by Electron. The spec (`spec/01-requirements.md`
> REQ-055, "An Electron shell loads the web app") already names Electron, so the owner's
> choice is the spec's word again, recorded here rather than silently. The scoring below is
> kept as the record of the consideration, not as the decision.

## Context

REQ-055 (desktop, SHOULD) requires a shell that keeps a session it controls, supports native passkeys,
auto-updates and can run a local stack. REQ-056 (mobile, SHOULD) requires the same product on a phone
with device pairing and push. The reference ships Electron and Expo. Criterion (5) rewards few moving
parts; criterion (4) rewards working on low-end devices.

## Alternatives

- **Desktop**: (a) Tauri v2 (system webview, Rust core, small binary); (b) Electron (Chromium bundled,
  large, Node runtime); (c) no desktop app, PWA only.
- **Mobile**: (a) PWA first, Expo/React Native later; (b) Expo now; (c) no mobile.

## Scoring

| Criterion | Tauri | Electron | PWA only |
|---|---|---|---|
| 1 fits the spec (passkeys, auto-update, local stack) | 1 | 1 | 3 (no native passkey RP, weak update story) |
| 2 maturity | 2 | 1 | 1 |
| 3 security | 1 (small surface, no Node) | 3 (Node in the shell) | 1 |
| 4 performance on low-end | 1 | 3 | 1 |
| 5 simplicity (one language with the backend) | 1 (Rust) | 3 (Node + npm toolchain) | 1 |
| 7 licence | 1 | 1 | 1 |

## Decision

- **Desktop**: Electron. The web build is the same static export from ADR-013. The Electron
  main process owns a dedicated `persist:boma` session partition (cookies and storage live in
  Electron's partition, not the OS default, and are cleared on sign-out); native passkeys go
  through the platform authenticator via the webview's WebAuthn support; auto-update is
  `electron-updater` with a signed feed — the signing identity and the publish feed are the
  owner's to decide (DEC proposal in STATE.md), so locally dev builds do not auto-update and
  the signed feed stays off until the owner signs. An optional "run a local stack" mode shells
  out to the Compose file with explicit consent. No Node in the renderer; context isolation and
  `sandbox: true` by default.
- **Mobile**: not in the first release. The web app is a installable PWA (manifest, offline shell,
  safe-area insets, no hover-only affordances) from M1; an Expo shell is a later milestone once the
  device-pairing contract is exercised by real users. REQ-056 is therefore deferred, not dropped, and
  the deferral is recorded in `docs/STATE.md` (it is a SHOULD).

## Consequences

- The desktop bundle is Electron's: larger than Tauri's, but it carries Chromium everywhere
  (no per-OS webview drift) and the same engine as the browser support matrix in
  `04-nonfunctional.md`, so the screens tested in Playwright are the screens Electron shows.
- The renderer has no Node (`contextIsolation`, `sandbox`); IPC, if any is ever added, goes
  through a small typed surface, documented when it exists — today the shell is a thin partition
  + window manager and does not proxy the API.
- Signing keys and the update feed are the owner's decision (DEC proposal); until then the
  auto-update path is present but inert, and `dev` builds do not phone home.
- Deferring mobile means REQ-056's acceptance criteria are unproven for now; `docs/STATE.md` lists it
  as deferred with the reason, and `tools/trace_check.py` records the ORPHAN with that explanation
  rather than silently ignoring it.
