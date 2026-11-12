# TRACE — requirement to evidence

Every requirement in `01-requirements.md` traced to code in `target/rakazo` @
`1115613b8d007328c5a625a1cc2b096161a5788c`, plus the golden fixture that replays it.

Status values:
- **C** — CONFIRMED in code (the anchor was read directly).
- **Cd** — CONFIRMED (doc): asserted by in-repo documentation and cross-checked against code where
  cited.
- **G** — GUESS; if a requirement rests on a guess, the guess is also recorded in
  `06-decisions-needed.md` and the requirement says so.

No row is marked runtime-confirmed: this pass had no runtime observation (`notes/validation-audit.md`,
"runtime: no"). Paths are relative to `target/rakazo/`.

| REQ | Requirement (short) | Evidence | St | Golden |
|---|---|---|---|---|
| REQ-001 | Send a message and start a run | `apps/api/src/routes/threads.ts:215`; `apps/api/src/thread-target.ts:496`; dispatch `thread-target.ts:170` | C | `send-message.json`, `ui/REQ-001-send-message.json` |
| REQ-002 | Idempotent per client nonce | replay branch `apps/api/src/thread-target.ts:525`; unique `(thread, clientNonce)` in `packages/db/prisma/schema.prisma` (Message) | C | `send-message-replay.json` |
| REQ-003 | Steering during an active run | `packages/core/src/run-state.ts:134` (`STEERING_CONTINUATION_PROMPT`); non-conversational trigger exclusion `run-state.ts:26-45` | C | — |
| REQ-004 | One continuous thread per bot | `Thread` unique bot/group/external in `packages/db/prisma/schema.prisma`; creation path `apps/api/src/routes/bots.ts:312` | C | `bot-create.json` |
| REQ-005 | Ordered history and live updates | `apps/api/src/routes/threads.ts:185` (`subscribe`), heartbeat `:195-203`; snapshot semantics `packages/core/src/thread-snapshot-events.ts` | C | `thread-messages.json`, `thread-subscribe.json` |
| REQ-006 | Stop work | `apps/api/src/routes/threads.ts:307` (`stop`), `:312` (`stopHelper`) | C | — |
| REQ-007 | Pending cards never block the chat | queue-holding set excludes `waiting_input` `packages/core/src/run-state.ts:19-23` | C | `ui/REQ-007-pending-card.json` |
| REQ-008 | Replies, quotes, reactions, mentions | `Message.replyToMessageId` / `replyQuote` in schema; `threads.react` in `packages/contracts/src/rpc.ts` | C | — |
| REQ-009 | Read state, unread divider, snooze | `ThreadRead.lastSeq` in schema; `threads.markRead|markUnread|snooze` in `packages/contracts/src/rpc.ts` | C | `ui/REQ-009-unread-divider.json` |
| REQ-010 | Group chats with several bots | `apps/api/src/routes/bots.ts:737` (`groups.create`) | C | `group-create.json` |
| REQ-011 | Activity view | `apps/api/src/runs.ts:31` (`activityPreview`), `:61` (`activityNotificationsEnabled`) | C | `run-list.json` |
| REQ-012 | Explicit run state machine | `packages/core/src/run-state.ts:44-54` (transition table), status groups `:4-23` | C | `run-retry.json`, `ui/REQ-012-run-statuses.json` |
| REQ-013 | In-flight work survives a deploy | lease expiry + fence in `packages/core/src/run-state.ts`; handover `packages/adapters/src/run-wakeup.ts:25-70` | C | — |
| REQ-014 | Orphaned runs are recovered | `packages/adapters/src/run-recovery.ts:20` (`findRunRecoveryBlockers`), lease expiry via `expireComputerExecutionLeases` (`apps/api/src/routes/computer.ts:24,155`) | C | — |
| REQ-015 | Model selection, fallbacks, BYOK | `packages/adapters/src/model-selection.ts`; `packages/adapters/src/run-model.ts` | C | — |
| REQ-016 | Per-run tool catalogue | `packages/adapters/src/run-tool-selection.ts`; `packages/adapters/src/run-tools.ts`; `builtin-tools.ts` | C | — |
| REQ-017 | Approvals gate effects; effects idempotent | `packages/adapters/src/approval-effect.ts`; `packages/adapters/src/run-effects.ts`; effect key `packages/core/src/approval-effect-key.ts` | C | `thread-answer.json` |
| REQ-018 | Spending limits and purchases | `packages/adapters/src/spending-gate.ts`; `builtin-tools.ts:923` (`request_purchase`); `purchase-gate.ts` | C | — |
| REQ-019 | Subagent delegation | helper trigger constant `packages/adapters/src/run-recovery.ts:1` (`BACKGROUND_HELPER_TRIGGER`); dispatch in `packages/adapters/src/background-job-handlers.ts` | C | — |
| REQ-020 | Streaming replies | `packages/adapters/src/pi-runtime.ts`; fan-out `packages/adapters/src/realtime.ts:30`; SSE `apps/api/src/routes/threads.ts:185` | C | — |
| REQ-021 | Failures are user-legible | `packages/i18n/src/run-error.ts`; run error field on `Run` in schema | C | — |
| REQ-022 | Memory documents and shared memory | `packages/memory/src/index.ts`; `apps/api/src/routes/memory.ts:24` | C | `memory-write.json` |
| REQ-023 | Routines as cron definitions | `packages/core/src/cron.ts:1-21`; `apps/api/src/routes/routines.ts:64` | C | `routine-create.json`, `routine-once.json` |
| REQ-024 | Dispatch, retry, coalescing | `packages/adapters/src/routine-coalescing.ts`, `routine-backoff.ts`, `routine-lateness.ts`; enqueue `apps/api/src/routines.ts:155` | C | — |
| REQ-025 | Try a routine before enabling | `apps/api/src/routes/routines.ts` (`runNow`) | C | — |
| REQ-026 | One computer per bot, lazy, released when idle | `packages/adapters/src/computer-lifecycle.ts`, `computer-idle.ts`, `computer-reuse.ts` | C | `ui/REQ-026-computer-panel.json` |
| REQ-027 | Provider-agnostic computers, fail-closed | `packages/adapters/src/sandbox-factory.ts:99` (14 providers) | C | `computer-status.json` |
| REQ-028 | Observe and act on the computer | `builtin-tools.ts:275-360` (`computer_observe`, `computer_act`, browser tools) | C | — |
| REQ-029 | User takeover | `apps/api/src/routes/computer.ts:363` (`takeover`), `:88` (waiting_takeover lookup) | C | `computer-takeover-conflict.json` |
| REQ-030 | Screen access without a session | `apps/api/src/screen-proxy.ts:9-40` (sealed capability, constant-time compare, 16 KB cap) | C | — |
| REQ-031 | Team and private computers | `Computer`/`ComputerRun` sharing fields in schema; `computer.settings` in `packages/contracts/src/rpc.ts` | Cd | — |
| REQ-032 | Connector catalogue and per-space installs | `apps/api/src/routes/connections.ts:139` (`connections.list`); install path in same file | C | — |
| REQ-033 | MCP servers, remote and opt-in local | `apps/api/src/routes/mcp.ts:59-106`; stdio allow-list `apps/api/src/env.ts:360`; `packages/adapters/src/mcp-transport.ts:299` | C | — |
| REQ-034 | Plugin marketplace, no local code | `apps/api/src/plugin-submissions.ts:37,139-154`; policy `PRODUCT.md` ("Connections, skills, and plugins") | Cd | `plugin-submission.json`, `plugin-submission-invalid.json` |
| REQ-035 | Skills | `apps/api/src/agent-skills.ts`; `apps/api/src/taught-skills.ts`; `apps/api/src/routes/memory.ts:210,247` | C | — |
| REQ-036 | Artifacts with versions | `apps/api/src/artifacts.ts:24,469,498` (create, versions, family delete) | C | `artifact-create.json`, `artifact-versions.json` |
| REQ-037 | Share a page by link | `apps/api/src/pages.ts:299,365` (token mint, hashed store); `apps/api/src/shared-pages.ts:8-38` (uniform 404, rate limit, headers) | C | `shared-page.json`, `shared-page-invalid.json`, `ui/REQ-037-shared-page.json` |
| REQ-038 | Ticket board (flagged) | `apps/api/src/routes/boards.ts`; flag in `apps/web/src/App.tsx:295` | C | — |
| REQ-039 | Bot-built pages and dashboards | `packages/page-kit`; script assets `apps/api/src/script-assets.ts:47-70` | C | — |
| REQ-040 | Sign-up and authentication methods | `packages/auth/src/index.ts:166-280`; capabilities `apps/api/src/app.ts:821` | C | `auth-capabilities.json` |
| REQ-041 | Sessions and trusted origins | `packages/auth/src/index.ts:253-258,927`; CORS mirror `apps/api/src/app.ts:801-812` | C | `unauthorized.json` |
| REQ-042 | Spaces, members, invitations | `apps/api/src/routes/spaces.ts` | C | `space-create.json`, `space-invite.json` |
| REQ-043 | Tenant isolation in the data layer | `packages/db/src/row-security.ts` (`runAsTenant`/`runAsSystem`); 26 RLS migrations under `packages/db/prisma/migrations`; tenant wrap `apps/api/src/routes/context.ts:415-419` | C | `cross-tenant-not-found.json` |
| REQ-044 | Export, import, delete | `apps/api/src/account-export.ts:132,221`; import + deletion paths in `apps/api/src/routes/account.ts` | C | `export-start.json` |
| REQ-045 | Audit, security events, directory sync | `apps/api/src/security-audit.ts`; `apps/api/src/audit-export.ts`; SCIM `apps/api/src/app.ts:885` | C | — |
| REQ-046 | Secrets for bots | `apps/api/src/routes/` (`botSecrets`, `agentSecrets`, `vault` in `packages/contracts/src/rpc.ts`); sealing in `packages/core` encryption module | C | — |
| REQ-047 | Inbound e-mail per bot | `apps/api/src/email-inbound.ts:53-57` (relay bearer), 25 MiB cap | C | — |
| REQ-048 | Messaging platforms | `apps/api/src/messaging-webhook.ts`, `messaging-inbound.ts`; Slack callback `apps/api/src/app.ts:972` | C | — |
| REQ-049 | Voice and calls (flagged) | `apps/api/src/voice.ts:70-296`; `threads.answerCall` `apps/api/src/routes/threads.ts:490` | C | — |
| REQ-050 | Bot webhooks and repository events | `apps/api/src/webhook.ts:50-58` (uniform 401), `:110` (GitHub HMAC) | C | `bot-webhook-denied.json` |
| REQ-051 | Billing, credits, plan limits | `apps/api/src/billing/webhook.ts`; `packages/adapters/src/plan-limits.ts`; credits models in schema | C | — |
| REQ-052 | Model catalogue and credentials | `apps/api/src/routes/models.ts:670`; model OAuth sessions in schema | C | `models-catalog.json` |
| REQ-053 | Health and readiness | `apps/api/src/health.ts`; worker health `apps/worker/src/index.ts:353` | C | `health.json`, `ready.json` |
| REQ-054 | Observability | `packages/logging/src/correlation.ts`, `ids.ts`, `redaction.ts`; CSP reports `apps/api/src/csp-report.ts` | C | — |
| REQ-055 | Desktop client | `apps/desktop/src/main.ts:488`; local stack `apps/desktop/src/local-stack.ts` | C | — |
| REQ-056 | Mobile client | `apps/mobile/index.ts:7`; device pairing `apps/api/src/user-devices.ts` | C | — |
| REQ-057 | i18n and RTL | `packages/i18n/locales/` (15 locales, two RTL); `packages/i18n/src/run-error.ts` | C | — |
| REQ-058 | Administration surfaces | `apps/web/src/App.tsx:252,272` (`/admin/plugins`, `/admin/access`); access grants in `packages/contracts/src/rpc.ts` | C | — |

## Coverage summary

- Requirements with at least one golden: 25 of 58 (30 API fixtures, 7 UI fixtures). The rest are
  covered by the procedure contracts in 03 and by their anchors here; goldens were chosen for the
  paths a rebuild is most likely to get wrong (send/stream/approve/status/public links), not for
  breadth.
- Requirements resting on documentation only: REQ-031, REQ-034 (both marked **Cd**); each also has a
  code anchor, so neither is a guess.
- Requirements with an open decision attached: REQ-005 (DEC-01), REQ-012/013 (DEC-02), REQ-043
  (DEC-05), REQ-016–021 (DEC-06), REQ-005/020 (DEC-07), REQ-026/027/031 (DEC-08), REQ-015/018/049/051/052
  (DEC-09), REQ-046 (DEC-10).
- No requirement depends on a `06-decisions-needed.md` item for its *existence*; the decisions change
  *how* it is built, not whether it is needed.

## Golden index

API fixtures live in `golden/*.json` with keys `request`, `response`, `notes`. UI fixtures live in
`golden/ui/` with keys `state`, `expect`, `notes` (validated by `rebuild-workspace/tools/check_ui_golden.py`).
