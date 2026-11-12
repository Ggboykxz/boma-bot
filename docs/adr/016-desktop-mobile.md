# ADR-016 — Desktop and mobile shells: Tauri for desktop, a PWA plus Expo later for mobile

Status: proposed (awaiting owner go)

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

- **Desktop**: Tauri v2. The web build is the same static export from ADR-013; the Tauri core adds a
  partitioned webview profile, native passkey support via the platform authenticator, signed
  auto-update, and an optional "run a local stack" mode that shells out to the Compose file with
  explicit consent. Signed artifacts in CI; no silent downloads.
- **Mobile**: not in the first release. The web app is a installable PWA (manifest, offline shell,
  safe-area insets, no hover-only affordances) from M1; an Expo shell is a later milestone once the
  device-pairing contract is exercised by real users. REQ-056 is therefore deferred, not dropped, and
  the deferral is recorded in `docs/STATE.md` (it is a SHOULD).

## Consequences

- One language (Rust) for the shell and the backend; the desktop bundle is a fraction of an Electron
  build, which matters on the low-end hardware the spec targets.
- Tauri's system webview differs per OS; the UI must not depend on a bundled Chromium version, so the
  browser support matrix in `04-nonfunctional.md` (ES2022, SSE, no exotic APIs) is the contract.
- Deferring mobile means REQ-056's acceptance criteria are unproven for now; `docs/STATE.md` lists it
  as deferred with the reason, and `tools/trace_check.py` records the ORPHAN with that explanation
  rather than silently ignoring it.
