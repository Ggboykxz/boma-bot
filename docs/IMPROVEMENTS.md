# Improvements — catching up to OpenBot, without rebuilding

Date: 2026-11-15
Subject: `github.com/CopilotKit/OpenBot` at `f1905cd` (v1.0.3 area, MIT template)
Decision record: `ADR-026` (external agents), `docs/STATE.md` (M12–M16 sequencing)
Method: full read of the target (README, docs/, api/, app/, desktop/, extension/, agent-* , tests/),
license check, and a capability diff against the shipped Boma Bot v0.1.0. Every "borrow" here is
**reused in spirit only**: no OpenBot source or assets enter this repository; we reimplement
against our own spec (`AGENTS.md` — prefer reimplementation; license compatibility).

## What OpenBot actually is

Per-area REST contract (`api/openapi/`, ~24 domains), generated orval clients, signed desktop
images, Helm chart, k3d local story. Split into MIT-licensed **frontend templates + API contracts**
here plus **closed-source server images + managed "Intelligence"** (durable threads, memory,
automatic learning, governance). If you self-host only the template, you get the shells; the runtime
is pull-based and CopilotKit-licensed. Built on CopilotKit + AG-UI; 13+ agent harness adapters in
-repo (`agent-ag2`, `agent-mastra`, …).

## Capability matrix vs Boma Bot v0.1.0

| Area | OpenBot | Boma Bot v0.1.0 | Verdict |
|---|---|---|---|
| Desktop shell | Tauri v2, signed installers, inert updater, k3d | Electron, unsigned, manual updates (ADR-016) | **Close the gap**: M15 (signed desktop + updater statements) |
| External agents | AG-UI endpoints; any framework; managed or self-hosted | none | **Borrow**: M12 |
| Agent harness adapters | 13 in-tree adapters | none | **Borrow concept**: adapters via M12 pipeline |
| Contract-first REST (OpenAPI) | sources per area, bundle, redocly lint, orval clients | single-file `spec/openapi.yaml` (frozen export spec), hand-written TS client | Keep ours (frozen contract for parity; we rebuild TS client generation in M15 if it pays off); record as ADR |
| Passwords & Recorder | saved logins, sign-in requests, session recording into the vault | none | **Borrow**: M13 |
| Automatic learning | Intelligence "Learning" delivers skills to bots | none (manual teach + approval) | **Borrow (limited)**: skill auto-delivery is M13+, not in M12 |
| Voice / dictation / calls | dictation + voice calls surface APIs | none recorded (REQ-049 deferred by your call) | **Propose**: revisit REQ-049 decision in M14 (needs your un-defer) |
| Mobile client | Expo/Flutter fronts | none (REQ-056 deferred by you) | Stays deferred per your call |
| Browser extension | Chrome/Firefox-class extension (background/content/offscreen) | none | **Borrow**: M16 (small, high-signal) |
| Slack / Teams fronts | connectors built-in | Slack/Telegram ingress (M7) | comparable; keep ours |
| Helm / Kubernetes | chart + operator docs | plain `compose.yaml` + `docs/UPGRADING.md` | **Borrow shape**: M15 (single-node first) |
| Enterprise RBAC/SSO/SCIM | full authn/authz stack | equivalent features (M1, M5, M9) | comparable; keep ours |
| Billing | handled/by Enterprise tier | full self-host purchase flow (M8) | ours wins for self-hosters; keep |
| Component drafts/publishing | marketplace-ish of bot UI components | rich block library (thread blocks) | ours sufficient; keep |
| Connections / app directory | apps → bot-grants + shared accounts | bot connectors (M5) | comparable; keep |
| Managed Intelligence | CopilotKit-only paid service | not applicable (self-host, one binary) | reject (out of scope; we are the alternative) |

## The improvement program — M12 through M16

Ordered by *value at the lowest risk* to the v0.1.0 base you just shipped. Every slice lands
behind its own ADR + CI green + `trace` (which now goes *up*, to full coverage of the new req ids).

1. **M12 — External agents (AG-UI)** — a bot may now be backed by *any* AG-UI endpoint (the
   operator's Bedrock, Claude Agent SDK, ADK, Mastra — anything with an AG-UI server). Our run
   state machine feeds the AG-UI transport; Approvals flow through existing cards (REQ-017);
   the remote agent owns its thread context; our memory/skills/routines stay ours.
   Spec addition REQ-059 (the only new REQ id we take on).
2. **M13 — Sign-in requests and the Recorder** — bots ask people for sign-ins where no tool
   exists; the person approves on the desktop; a credential is sealed into the vault exactly like
   a bot secret; replay happens inside the computer through the same takeover-exclusive control.
   Spec addition REQ-060.
3. **M14 — Voice and dictation** — dictation into the composer (already in the browser stack),
   and voice call signalling over WebRTC behind the feature gate; only after you un-defer REQ-049.
   Blocked on your word.
4. **M15 — Release maturity** — signed desktop when you hand over an Apple/Windows identity,
   otherwise a portable AppImage + updater manifest that keeps the same UX honest; helm chart for
   the single-node case; trace to 58/58 if you un-defer voice/mobile, else remain 56/58 by record.
5. **M16 — Browser extension** — control/observe the Bots you're talking to from any tab.
   Scoped small: sign-in, thread read, approval answering, takeover.

Mobile stays off this program per your call (M14+); REQ-049 stays deferred until you say otherwise.

## Governance

Improvements extend the spec, they do not fork it. Each milestone ships:
a new migration *never edited*, REQs with tests carrying their id in the name, an ADR per decision,
and a STATE.md record. The frozen M0–M11 contract remains the floor; everything else extends it.

## OpenBot licensing red lines

- No OpenBot code, assets (images, icons, SVG artwork), or wording copy enters this repository.
- The MIT template's *ideas* are re-implemented against our own contracts; every borrowed concept gets
  its ADR with the concept named (AG-UI/AIC). If a future milestone needs a dependency, it goes into
  `docs/DEPENDENCIES.md` with license, version, and reason.
- "coworkers", brand-teal palette, and artwork are out of scope.
