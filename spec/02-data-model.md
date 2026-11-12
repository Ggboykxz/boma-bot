# 02 — Data model

126 entities in the reference implementation. This document specifies the ones a rebuild must model,
their invariants, and the state machines that carry behavior. Field types are logical (string, uuid,
timestamp, json, enum); physical column names are free.

Global conventions:
- Every top-level aggregate carries `spaceId` (tenancy) except user-global rows (`User`, `Session`,
  `Account`, `Passkey`, `UserDevice`, credentials).
- Timestamps are UTC; all lists are ordered by an explicit sequence or timestamp, never by insertion
  order of the storage engine.
- Deletion of a space cascades to its aggregates; deletion of a user keeps audit rows.
- Ids are opaque strings (cuid/uuid) and must not be treated as ordered or guessable.

## Identity and tenancy

| Entity | Key fields | Constraints |
|---|---|---|
| User | name, email, emailVerified, locale, timezone, avatarStyle, deactivatedAt, modelCreditCapMicros | email unique; deactivated users cannot act |
| Session / Account / Verification / Passkey | user, identifiers, expiry | sessions expire and revoke on deletion |
| Organization | name | one per billing entity |
| Member | organizationId, userId, role | unique (organization, user); role ∈ owner/admin/member |
| Space | organizationId, name, isDefault, deletingAt | exactly one default per user context |
| SpaceMember | spaceId, userId, role | unique (space, user); removal ends access |
| Invitation / SpaceInvite | email, token hash, role, expiresAt, status | token single-use; status ∈ pending/approved/declined/revoked |
| AccessGrant | user, reason, grantedBy | server-level access, never granted by sign-up |
| SecurityEvent / AuditEvent | kind, actor, space, target, metadata, at | append-only; audit exportable as JSONL |
| DeploymentSettings | singleton | operator-editable, not per tenant |
| RateLimit | key, window, count | shared across processes for auth endpoints |

Invariants: a user may belong to several spaces; every bot, thread, run and artifact belongs to
exactly one space; a member's role never exceeds the organization role that granted it.

## Bots and conversations

| Entity | Key fields | Constraints |
|---|---|---|
| Bot | spaceId, userId (owner), name, description, instructions, instructionHistory, color, avatarSet, sectionId, archivedAt, parentBotId, spawnKey, memoryScope, computerId, voiceId, modelProvider, modelId, pinned, position | one thread per bot; archive hides it from lists but keeps history |
| BotSection | name, position | grouping in the sidebar |
| ChatGroup / ChatGroupMember | name, members | a group thread has one owner bot per member |
| Thread | spaceId, botId ⊕ groupId ⊕ externalConversationId, userId, nextMessageSeq, nextEventSeq, historyCompactedUpToSeq | exactly one of bot/group/external; sequence numbers monotonic per thread |
| Message | threadId, seq, role, blocks json, botId?, authorUserId?, replyToMessageId?, replyQuote?, runId?, clientNonce?, searchTokens | unique (thread, seq); unique (thread, clientNonce); blocks is an array of typed blocks (text, image, file, tool, card) |
| ThreadRead | threadId, userId, lastSeq | drives unread counts |
| Event | threadId, seq, type, payload | append-only stream backing subscriptions |
| SteeringMessage | runId, messageId, at | consumed once by the active attempt |
| Task | threadId, kind, state | container for a unit of work (run, ticket work) |

Thread types: `bot` (one bot, one member context), `group` (several bots and members),
`external` (messaging platform conversation). Message roles: user, bot, system, tool.

Unread rule: unread = messages with `seq > lastSeq` not authored by the reader, excluding hidden
background steps.

## Runs

| Entity | Key fields | Constraints |
|---|---|---|
| Run | spaceId, botId, threadId, taskId, userId, status, trigger, modelProvider, modelId, thinkingLevel, modelPinned, leaseOwner, leaseFence, leaseExpiresAt, checkpoint, clientNonce, sourceMessageId, routineId, retryOfRunId, wakeAt, firstResponseAt, startedAt, completedAt, lateSince, error | at most one active run per bot thread; one active computer holder per bot |
| Attempt | runId, fence, status, error, startedAt, finishedAt | one row per execution attempt; fence matches the run's lease |
| ExternalEffect | runId, effectKey, kind, payload, appliedAt | unique effectKey per run — the idempotency record for approvals |
| Run projectors | run activity rows derived from runs, not stored separately | activity list is a projection over runs and messages |

**Run state machine** (transitions are exhaustive; anything else is an error):

```
queued ──► leased ──► running ──► completed
  │           │  ▲        │  └──► failed ──► queued (retry)
  │           │  └────────┘        │
  │           └──► queued          └──► cancelled
  └──► cancelled     running ──► waiting_input ──► queued | leased | cancelled
                     running ──► waiting_takeover ──► queued | leased | cancelled
```

- `queued, leased, running` = working (visible as activity).
- `waiting_input, waiting_takeover` = paused on a human; they hold no compute but keep the queue.
- Triggers: user message, routine, webhook, bot_message, tickets, created, helper result, retry.
  Non-conversational triggers (routine, webhook, tickets, bot_message, created) never accept steering.

## Routines

| Entity | Key fields |
|---|---|
| Routine | spaceId, botId, userId, threadId, name, prompt, crons string[], timezone, active, notify, webhookEnabled, githubEnabled, messageProvider, emailSenders, model fields, watch json, watchState json, lastRunAt, nextRunAt, accountFailures |

Invariants: `crons` non-empty; `@once` is the single one-shot value and deactivates after firing;
`nextRunAt` is computed in `timezone`; an active routine has a scheduled wakeup; `accountFailures`
resets on success and pauses the routine at threshold.

## Memory

| Entity | Key fields |
|---|---|
| MemoryDocument | spaceId, botId?, scope (bot/shared), path, content, updatedAt |
| MemoryRevision | documentId, revision, content, at |
| SpaceMemoryConfig | spaceId, provider, settings |

Rule: memory writes are append-only revisions; the current document is a projection of the latest
revision. Shared memory is readable by every bot in the space and writable only by allowed scopes.

## Computer

| Entity | Key fields |
|---|---|
| Computer | spaceId, provider, externalId, status, screenGeneration, memoryLimit, cpuLimit, lastUsedAt, stoppedAt |
| ComputerRun | computerId, runId, startedAt, endedAt | one active holder per computer |
| ComputerExecutionLease | runId, owner, fence, expiresAt | mirrors the run lease for computer ownership |
| ComputerUpdate | computerId, updateId, status | tracks provider-side image updates |
| BrowserProfile | spaceId, botId?, storageKey, checkpointAt | canonical browser identity per trust boundary |
| AgentHome | spaceId, botId, storageKey, archivedAt | files, per provider |
| HomeDeletionNotice | homeId, requestedAt, completedAt | retention bookkeeping |

Computer status: `queued, running, interrupted, completed, failed` for provisioning work;
screen generation increments on every restart and invalidates older screen capabilities.

## Artifacts, pages, boards

| Entity | Key fields | Constraints |
|---|---|---|
| Artifact | spaceId, botId?, groupId?, userId, runId?, name, description, mimeType, size, hash, storageKey, rootArtifactId, version, standalone, clientNonce | unique (space, user, clientNonce); versions form a chain rooted at the first artifact |
| Page | spaceId, botId, slug/token hash, visibility (private/link), shareSecretId, currentVersionId | link visibility requires a token hash |
| PageVersion | pageId, version, source, compiledAt | immutable |
| Board / Ticket / TicketComment | spaceId, key, title, state, assignee, comments | board is feature-flagged; ticket work runs as background runs |

Artifact content is stored out of band (object store or filesystem) and served through the API as
base64 in RPC replies. Page source is the bot-authored document compiled by the page runtime; shared
pages are served by token lookup, never by id.

## Connectors, MCP, plugins, skills

| Entity | Key fields |
|---|---|
| Connection / AgentConnection | spaceId, connectorId, account ref, status (pending/connected/revoked/error), secretRef |
| IntegrationProviderConfig | provider, settings, enabled |
| CapabilityInstall | spaceId, capabilityId, parts installed, version |
| McpServer | spaceId, name, transport (http/stdio), url or command, auth state, allowPrivate |
| McpOAuthSession | serverId, state, tokens (sealed) |
| BotMcpServer / AgentSkillSpace / CapabilityInstallSpace | join rows for per-bot or per-space enablement |
| PluginSubmission | source ref, status (pending/applied/revoked), review, submitter contact |
| AgentSkill / TaughtSkill | name, instructions, files, scope |
| ModelOAuthSession / UserModelCredential | provider, sealed tokens, expiry |

Rules: a connector install owns its credentials; a bot only sees tools from installs enabled for it;
plugin installs record which parts were skipped and why.

## Billing, models, secrets

| Entity | Key fields |
|---|---|
| BillingAccount / BillingAuditEvent | organizationId, provider customer/subscription ids, plan, status |
| ModelCreditAccount / ModelCreditEntry / ModelCreditLot | userId or spaceId, balance in micros, entries append-only |
| Payment / PaymentWallet / Purchase | spaceId, amount minor units, status, runId, approvalId |
| SpendingLimit | userId or spaceId, cap, window, spent |
| Secret / BotSecret / AgentSecret / VaultCredential | owner scope, name, sealed ciphertext, kind, lastUsedAt, expiresAt |
| UserModelPreference / SpaceModelPreference / SpaceModelRoleChoice / UserModelRoleChoice | resolved model per role (chat, helper, review, voice) |
| UserVoiceCredential / SpaceVoicePreference / UserVoicePreference | voice provider and voice id |

Money is stored in integer minor units or micros — never floating point. Secrets are stored sealed
(AES-GCM with a key per space plus a master-key wrapper) and are never readable back through any API.

## Users' devices and notifications

| Entity | Key fields |
|---|---|
| UserDevice / UserDeviceConnection | userId, name, public key, lastSeenAt, pairing state |
| PushToken / CallPushDevice | userId, platform, token, environment |
| NotificationPreference | userId, per-event toggles |
| EmailSend | userId, kind, sentAt | deduplicates welcome and campaign mail |
| WorkActivity(Start)Token | short-lived tokens for activity reporting |

## Import / export formats

**Account export** — a zip containing `manifest.json`
(`{version, exportedAt, userId, counts}`), then `bots/`, `threads/`, `messages/`, `artifacts/`,
`routines/`, `memory/` entries referenced by the manifest. Entry names are validated on import
(rejected when they escape the archive root), ids are remapped, and import is best-effort per section
with a report of what was skipped.

**Golden examples** — see `golden/*.json`: `{request, response, notes}` where request/response are
public-interface payloads with synthetic ids and timestamps.

**Audit export** — JSONL, one event per line, stable field names, no secrets.
