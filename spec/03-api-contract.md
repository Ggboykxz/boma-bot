# 03 — API contract

Two interfaces. Both are JSON over HTTPS, same origin as the web app.

## 3.1 RPC interface (`POST /rpc/<namespace>/<procedure>`)

- **Transport**: HTTP POST, one JSON body, one JSON response. The reference implementation uses oRPC
  with a fetch link pointed at `/rpc`; a rebuild may choose its own framing but must keep the
  procedure names, the request/response shapes and the error model below.
- **Auth**: session cookie (`credentials: include`). No bearer tokens for people. Requests without a
  valid session get `401`.
- **Space selection**: header `x-<product>-space-id: <spaceId>`. When absent, the caller's default
  space is used. A space the caller does not belong to behaves as not found.
- **Caching**: every `/rpc/*` and `/api/auth/*` response carries `cache-control: no-store`.
- **Idempotency**: mutating procedures that create a turn accept `clientNonce`; replaying the same
  nonce returns the original result.
- **Validation**: every procedure validates input and output against a schema before the handler runs
  and before the reply is sent. Unknown fields are rejected where the schema is strict.
- **Errors**: a stable envelope with a machine code (`UNAUTHORIZED`, `FORBIDDEN`, `NOT_FOUND`,
  `BAD_REQUEST`, `CONFLICT`, `TOO_MANY_REQUESTS`, `INTERNAL`) plus a message safe for users. Internal
  details never cross the boundary. Tenant violations are reported as not found, not forbidden.

### Namespaces (52)

`analytics, aiConsent, billing, accessGrants, preferences, spaces, organizations, deployment, computerProvider,
security, audit, updater, models, bots, groups, botSections, threads, computer, memory, routines,
scratchpad, boards, tickets, skills, agentSkills, availability, marketplace, capabilities, mcp,
onboarding, integrationSetup, connections, messaging, approvalRules, trustedLinkSites, autoReview,
spaceSettings, artifacts, pages, usage, export, notifications, search, links, runs, spendingLimit,
paymentWallet, voice, externalConversations, agentSecrets, vault, botSecrets`

### Core procedure contracts (representative; 283 procedures in total)

**threads**
- `threads.head` → `{threadId, lastSeq, lastMessageAt}` for the caller's current bot or group.
- `threads.messages` — input `{threadId?, cursor?, limit?}` → `{messages: Message[], nextCursor?}`,
  newest-first page, ordered by `seq`.
- `threads.subscribe` — input `{threadId}` → an event stream (server-sent events when the client uses
  the fetch link). Event types: `message`, `message-update`, `run`, `typing`, `card`, `roster`,
  `heartbeat`; each carries `threadId, seq, type, payload, createdAt`.
- `threads.send` — input `{text?, artifactIds?, mentions?, replyToMessageId?, replyQuote?, clientNonce?,
  botId?, groupId?}` → `{messageId, runId?, status}`. Fails when the bot is archived or the space is
  read-only. Idempotent on `clientNonce`.
- `threads.sendWithRoutines` — same plus a routine draft to save, returning `{send, routineRunIds}`.
- `threads.stop`, `threads.stopHelper` → `{ok: true}`; stop is a no-op on a terminal run.
- `threads.answer` — input `{cardId, answer}` → resolves a pending question or approval exactly once.
- `threads.react`, `threads.markRead`, `threads.markUnread`, `threads.snooze`, `threads.clear`,
  `threads.updatePolicy`.

**runs**
- `runs.list` — input `{botId?, cursor?, limit?}` → activity rows
  `{id, status, trigger, promptSnippet, agentMessage?, startedAt, completedAt}`.
- `runs.get`, `runs.stop`, `runs.retry` (only from `failed`).

**bots**
- `bots.create` — input `{name, description?, instructions?, avatarSet?, color?, sectionId?}` →
  `Bot`. Creating a bot also creates its thread and, when configured, its intro run.
- `bots.update`, `bots.archive`, `bots.remove`, `bots.import` (agent import), `bots.duplicate`.
- `bots.secrets.*` (see `botSecrets`).

**groups**
- `groups.create` — input `{name, botIds[]}` → group with one thread.
- `groups.addMember`, `groups.removeMember`, `groups.rename`, `groups.remove`.

**computer**
- `computer.status` → `{status, provider, startedAt?, screenGeneration?}`.
- `computer.start`, `computer.stop`, `computer.takeover`, `computer.release`.
- `computer.provider` → available providers and their capability flags.
- `computer.settings` → memory/cpu/idle configuration within deployment limits.

**memory**
- `memory.list`, `memory.read`, `memory.write`, `memory.remove`, `memory.provider`.

**routines**
- `routines.create` — input `{botId, name, prompt, crons[], timezone, active, notify}` → routine with
  `nextRunAt`.
- `routines.update`, `routines.remove`, `routines.runNow`, `routines.runs`.

**mcp / connections / marketplace**
- `mcp.servers.list|add|remove|probe`, `mcp.oauth.start|complete`.
- `connections.list`, `connections.install`, `connections.remove`, `connections.reconnect`.
- `marketplace.list`, `marketplace.install`, `marketplace.submit`.

**artifacts / pages**
- `artifacts.create` — input `{botId?, groupId?, name, mimeType, contentBase64, clientNonce?}` →
  artifact with `version`.
- `artifacts.get` / `getById` → `{…, contentBase64}`; `artifacts.listVersions`, `artifacts.listSpace`,
  `artifacts.remove`.
- `pages.save`, `pages.get`, `pages.share`, `pages.revokeShare`.

**account / deployment**
- `spaces.list`, `spaces.create`, `spaces.invite`, `spaces.members`, `spaces.removeMember`.
- `export.start`, `export.status`, `export.download`; account deletion procedures.
- `deployment.settings.get|update` (operator only), `security.events.list`, `audit.export`.
- `models.catalog`, `models.connect`, `models.disconnect`, `models.preferences.*`.

## 3.2 HTTP endpoints

| Method | Path | Auth | Purpose / limit |
|---|---|---|---|
| GET | `/health` | none | liveness `{ok: true}` |
| GET | `/ready` | none | readiness: bounded DB probe + capability report |
| GET | `/internal/health` | internal | worker health used by the supervisor |
| GET | `/api/auth/capabilities` | none | which sign-in methods exist (no secrets) |
| * | `/api/auth/*` | library | better-auth surface: sign-in, sign-up, session, passkeys, reset, verification (64 KB body cap) |
| * | `/api/auth/scim/v2/*` | bearer | SCIM provisioning |
| GET | `/api/monitoring/config`, POST `/api/monitoring/tunnel` | session / envelope cap | browser error tunnel |
| POST | `/api/csp-report` | none | CSP violation reports |
| POST | `/api/v1/plugin-submissions` | none, CORS `*` | submit a plugin (32 KB, 10/hour/IP) |
| POST | `/api/v1/integration-claims`, `/api/v1/integration-claims/confirm` | token | claim an integration |
| GET | `/api/v1/pages/shared/:token` | token | public page read (120/min/IP, `noindex`) |
| GET | `/api/v1/assets/mermaid.js`, `/api/v1/assets/page-runtime.js` | none | static scripts with ETag |
| POST | `/api/v1/bots/:botId/webhook` | bot bearer | wake a bot with a payload (64 KB) |
| POST | `/api/v1/email/inbound` | relay bearer | inbound mail for a bot address (25 MB) |
| GET | `/api/v1/email/unsubscribe` | HMAC token | marketing opt-out |
| POST | `/api/v1/messaging/*`, `/api/v1/phone/*` | platform signature/bearer | messaging and telephony webhooks (256 KB) |
| POST | `/api/billing/webhook` | provider signature | payment events (512 KB) |
| GET | `/api/v1/messaging/install/slack/callback` | state | install redirect |
| POST | `/api/internal/screen-target` | sealed bearer | screen proxy capability (16 KB) |
| * | `/api/devices/*` | session / device key | device pairing, connect (WebSocket upgrade), session |
| GET | `/api/dev/emails` | none, dev only | local mail box (must not exist in production) |
| POST | `/rpc/*` | session | the RPC interface above (16 MB) |

`openapi.yaml` in this directory specifies the HTTP endpoints above; the RPC interface is specified by
the procedure contracts in 3.1 and replayed by the golden examples.
