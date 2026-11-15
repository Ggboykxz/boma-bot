# IMPROVEMENTS — the same level as OpenBot, capability by capability

Date: 2026-11-15 (v2, complete)
Subject: `github.com/CopilotKit/OpenBot` at `f1905cd` (v1.0.3 area; MIT template for frontends and
contracts, CopilotKit-licensed server images). All 21 OpenAPI area files read; all docs/ read;
exercise of the weak points verified against the recorded source.
Decision record: `ADR-026` (AG-UI), `docs/STATE.md` (M12–M21 sequencing); nothing of OpenBot enters
this repository — ideas are reimplemented against our own spec, with provenance acknowledged per
concept below.

## What OpenBot is, briefly

MIT-licensed frontends and API contracts; the server runs from published images; a separately
licensed CopilotKit "Intelligence" carries durable threads, memory, Automatic learning and governance
on the same premises (local or cluster). Every tool decision passes a gateway (resolve → evaluate
policy → audit → forward/refuse). A managed multi-organizational front (Slack/Teams, iOS/Android,
web, desktop) sits over the same API. k3d one-command bring-up.

## Capability matrix — every OpenBot area vs Boma Bot v0.1.0 — nothing excluded

| Area (OpenBot source of truth) | OpenBot offers | Boma Bot v0.1.0 | Verdict / milestone |
|---|---|---|---|
| `bots` | CRUD for bots, import/export, sharing | M1/M2: yes — catalog + export; duplication `bots.duplicate` | comparable; keep |
| `accounts` | people; onboarding; standing instructions; API keys; sessions; linked accounts; IdPs | M5/M9: profile; sign-in; SSO capability config (unverified runtime), admin SCIM | M17 standing instructions; M15/SSO proof; OIDC runtime covered below |
| `admin-settings` | deployment/org knobs, capabilities, network policies, model usage, computer actions, setup checklist | M9 admin surfaces; M8 model usage report; M5 MCP allow-list | comparable; keep (extend where natural) |
| `approvals` | approval settings, reviewer guidance, boundaries, what-if, shared accounts | REQ-017: cards everywhere, exactly-once effects, grants table | M16: scoped auto-approvals + what-if previews |
| `apps` | app directory; install apps per bot/admin; shared accounts; per-app approval rules | M5 connectors catalogue + per-space install; admin review | comparable; keep; M16 covers approval-rules side |
| `audit-events` | audit vocabulary, export, security events | M9 JSONL export + security events | comparable; keep |
| `bot-grants` | share grants (which bot may use which app); requests to administrators | M11.3b delegate.bot (room-local); no cross-workspace grants | M16 scope: grants across spaces (documented decision, ADR later) |
| `browsers` | Bots have browsers; **people get their own shared OpenBot browser with tabs, recording and narration** | M4: bot-only computer panel; REQ-029 takeover | M17: a person's browser pane (reuse takeover machinery) |
| `components` | charts/cards bots draw; drafts + publishing; which bots may draw each; data functions readable | REQ-007/017: question/approval/answer block kinds only | M15 decide: adopt bot-drawn component drafting if it earns its keep |
| `computers` | screen, files, control, admin list | M4: complete (doctor, takeover, idle release, conformance suite) | comparable; keep |
| `connections` | per-person app connections + OAuth consent; shared app accounts; per-action approval | M5 connectors, M11.3b handoffs | comparable; keep; M16 covers approval rules |
| `conversations` | members/messages/reactions/dictation; **voice** calls; tasks; conversation join tokens; **product events** incl. what people saw/copied | M1 reactions/mentions; M7/M9; tasks via board; no dictation/voice/either analytics | M15: dictation + events; M14: voice (gated); M12: join tokens if needed for extension |
| `learning` | Automatic learning (spaces contribute; skills delivered on subscription model) | REQ-035 skills + REQ-022 memory — manual teach only | M18: Automatic learning (approval-gated, local, no license text) |
| `memory` | person-scoped facts + read in apps | M3 memory (append-only, shared+Bot scopes); apps can read memory via MCP tools | comparable; keep |
| `model-connections` | vault, catalogue, per-conversation model choice | REQ-052 catalogue + BYOK (M8); per-bot model prefs | comparable; keep |
| `notifications` | phones with push; Slack/Teams destinations; bot→outside limits per bot; caps | Slack/Telegram ingress (M7); no push; REQ-048 fronts partly shipped | M15: push devices + destinations + caps |
| `passwords-and-recorder` | sign-in requests; saved logins; browser-connected recording | none | M13: sign-in requests + Recorder (our name: credentials suite) |
| `schedules` | routines, check-ins, goals, follow-ups | REQ-023/024 routines + REQ-050 GitHub wakes | comparable; keep |
| `skills` | named instruction packs | REQ-035 skills (teach + pickup) | comparable; keep |
| **Desktop** | Tauri: signed installers (cosign/notary), k3d bring-up, inert updater | Electron 45, unsigned, ADR-016 | M15: desktop release criteria (auth ↔ owner Apple/Windows identity) |
| **Browser extension** | MV3 (background/content/offscreen) control + observe | none | M20: extension (sign-in, thread read, approvals, takeover) |
| **Mobile** | iOS/Android | REQ-056 deferred (your call) | stays deferred, noted |
| **Skin/branding** | white-label-able skins & brand config | no theming of this kind | M21 (optional): skins/brand, not prioritized |
| **Learning governance** | Admin → Learning settings, sharing choices | none | covered by M18 |
| **Kubernetes** | helm chart, k3d bootstrap | plain compose | M15: k3d-style single-node chart |

## Decision record (nothing neglected, nothing silently absorbed)

**Borrow (as new milestones):** AG-UI external agents (M12), sign-in requests + Recorder (M13),
dictation, push devices + caps + Slack/Teams as destinations (M15), scoped auto-approval rules and
what-if previews (M16), standing instructions (M17), automatic learning limited (M18), enterprise
packaging + desktop release maturity (M19), extension (M20), optional skins (M21), bot-drawn
component drafts (+ publishing) — evaluate at M15: with current blocks it is plausible the answer is
"not yet / never"; if it enters, it is M22 and ADR'd then.

**Keep ours as better-in-this-house (no change):** the API contract (our frozen spec vs their
OpenAPI-blessed contract — parity rules make our contract the pact for Rakazo's migration; touching
it is DEV-worthy), our test discipline (golden-replay, axe, RTL), our single-binary deployment,
self-hosted purchase flow.

**Reject (explicit, for cause):** Managed Intelligence anything (CopilotKit-license text — we
cannot and will not repackage licensing we don't own); their client bundle, their artwork, their
brand names ("coworkers", "OpenBot"), their OpenAPI-contract of record (ours wins for this program);
the licensing model of AG-UI is fine, but we reuse the *protocol*, nothing else.

## The program

| # | Milestone | REQ ids | Size | Notes |
|---|---|---|---|---|
| M12 | External agents (AG-UI) | REQ-059 (new, MUST) | M | ADR-026; parity for external agents first |
| M13 | Sign-in requests & Recorder | REQ-060 (new, MUST) | M | credentials into our vault; replay through takeover-exclusive computers |
| M14 | Dictation + voice calls | REQ-049 (un-defer, your call) | M | dictation ships first; voice/call needs your un-defer |
| M15 | Push notifications + desktop scope | — | M | Web Push, notification policy per bot (REQ-048 shape), fewer than M20 saturation concerns |
| M16 | Scoped auto-approvals + what-if | — (ADR later) | M | rules engine against our existing card/effect machinery |
| M17 | Standing instructions + personal browser | — | M | per-user, per-bot persistent guidance; person-browser pane later in M; defer browser pane if window closes |
| M18 | Automatic learning (limited) | REQ-061 (new if you bless) | L | approval-gated skills, local corpus, no license text |
| M19 | Release maturity & enterprise packaging | — | M | k3d helm, desktop signing, trace path to 58 if you un-defer |
| M20 | Browser extension | — | M | control/observe |
| M21 | Skins / white-label (optional) | — | S | only if you want multi-tenant white-label |
| M22 | Bot-drawn component drafts (optional) | — | L | decide at M15 checkpoint |

Mobile remains deferred per your word; REQ-049 stays deferred until you un-defer it.

## Governance

Every milestone ships: migration-never-edited migrations, REQ-id tests, ADR per decision,
IMPROVEMENTS/PLAN/STATE record, trace back up, release replay. The frozen M0–M11 floor never
weakens; improvements extend it.

## OpenBot licensing red lines

No code/assets/wording of OpenBot in this repo; MIT ideas reimplemented; provenance per concept
recorded; any new dependency in `docs/DEPENDENCIES.md`.
