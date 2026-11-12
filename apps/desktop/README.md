# `apps/desktop` — deferred to milestone M10

This directory is intentionally empty.

REQ-055 (desktop client, SHOULD) is still in scope, and ADR-016 chooses **Tauri v2** for it. It is
not scaffolded at M0 because a Tauri project that has never been compiled is worse than an honest
placeholder: it would claim a buildable shell while containing no verified configuration, and M0's
exit criteria do not include it (`docs/PLAN.md` §milestones).

## What lands here, and when

Milestone **M10** ("UI complete + desktop") adds:

- `src-tauri/` — the Tauri v2 Rust core, with the same `com.boma.app` identifier and the static web
  export from `apps/web/out` as its front end.
- A partitioned webview profile: the session cookie is owned by the app, never shared with another
  window or with a system browser.
- Native passkey support through the platform authenticator (WebAuthn relying party `com.boma.app`).
- Signed auto-update: artifacts published with checksums, verification before install, and install
  on restart — never a silent download.
- An optional "run a local stack" mode that invokes the Compose file only after explicit consent.

## What must be true before it starts

1. The web export is stable enough to embed (M1–M9 screens, not the M0 shell).
2. Auth and sessions exist (M1), because the partition requirement is about session isolation.
3. The release pipeline signs artifacts (`docs/adr/018-deployment-ci.md`), because an auto-updater
   without a signature is a remote code execution path.

## What you can do today

Nothing in this directory. The web client at `apps/web` runs in a browser, and the API at
`crates/api` is the same contract the shell will use. Mobile is deferred for the same reason,
recorded in `docs/STATE.md` (REQ-056, SHOULD).
