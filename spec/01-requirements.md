# 01 — Requirements

Priorities: **MUST** = parity gate, **SHOULD** = expected, **COULD** = optional for a first release.
Every requirement is testable and observable from outside. Implementation notes live in
`../ARCHITECTURE.md`; evidence citations for each id are in `TRACE.md`.

## Conversation

### REQ-001 — Send a message to a bot and start a run (MUST)
A member can send text, attachments and mentions to a bot thread; the system persists the message and
starts exactly one run for it.
- Given a signed-in member of the space and an existing bot, when they send "plan my week", then one
  message row exists in that bot's thread and one run is created with a terminal-or-active status.
- Given the bot is archived, when a member sends to it, then the call fails with a not-found class
  error and nothing is persisted.
- Rationale: this is the only entry point into the agent.

### REQ-002 — Sending is idempotent per client nonce (MUST)
A retried send returns the same message and run instead of creating a second turn.
- Given a send with `clientNonce: N` succeeded, when the same request is repeated with `N`, then the
  response is byte-identical in its message and run ids and no new run exists.
- Rationale: mobile and desktop clients retry on flaky networks.

### REQ-003 — A message during an active run steers it (MUST)
New user text arriving while the bot is working joins the current run instead of queueing a second
one, except when the active run is non-conversational (routine, webhook, bot-to-bot, ticket work).
- Given a running run on a bot thread, when the member sends a second message, then the run's attempt
  receives it as a steering message and the thread shows one working run.
- Given an active routine run, when the member sends a message, then it waits and is answered by the
  continuation after that turn.
- Rationale: chatting must never feel locked.

### REQ-004 — One continuous thread per bot (MUST)
Each bot has exactly one visible conversation; helper and group threads are separate; archived threads
stay readable on request.
- Given a bot with history, when a member opens it, then the same thread id is returned as before,
  including archived messages when `includeArchived` is requested.
- Rationale: continuity is the product's identity.

### REQ-005 — Ordered history and live updates (MUST)
Messages carry a per-thread monotonic sequence; clients can page backwards and subscribe to a stream
of typed events with a heartbeat.
- Given a thread with 200 messages, when a client asks for a page of 50, then it receives the newest
  50 ordered by sequence with a cursor for older pages.
- Given a subscriber on an idle thread, when nothing happens for the heartbeat interval, then a
  heartbeat event arrives and the connection stays open.
- Rationale: the UI needs resumable, ordered state.

### REQ-006 — Stop work (MUST)
A member can stop the bot's current work, and stop a helper run independently.
- Given an active run, when the member stops it, then the run reaches `cancelled` and no further tool
  call starts.
- Given a helper run started by a parent run, when the member stops only the helper, then the parent
  stays active.
- Rationale: control and cost safety.

### REQ-007 — Pending cards never block the chat (MUST)
Questions and approvals are messages with a pending state that any member can answer until someone
does; concurrent chat keeps working.
- Given a run waiting on an approval, when a member sends an unrelated message, then a new run starts
  and the approval card remains answerable.
- Given a card answered by one member, when another member answers, then the second answer is rejected
  and the card shows the first result.
- Rationale: an agent that freezes a conversation is unusable.

### REQ-008 — Replies, quotes, reactions and mentions (MUST)
A message can reply to an earlier message, carry a quote, receive emoji reactions, and mention other
members or bots.
- Given a reply to message M, when the thread renders, then the reply references M and the quote text.
- Given a mention of another bot in a group, when the run executes, then that bot is addressed as a
  participant, not as a user.
- Rationale: threaded, multi-actor conversations.

### REQ-009 — Read state, unread divider and snooze (SHOULD)
Per-member read position drives an unread divider; a chat can be snoozed or archived without losing
history.
- Given a member has read up to sequence S, when new messages arrive, then their unread count is
  computed from S and the divider sits after S.
- Given a snoozed chat with new activity, then it leaves the regular list until the snooze ends.
- Rationale: chat-list hygiene at scale.

### REQ-010 — Group chats with several bots (MUST)
A group holds several bots and members; a message can address specific bots by mention; bots can hand
work to each other and the group decides who answers.
- Given a group with three bots, when a member mentions two of them, then only those two run.
- Given a bot-to-bot request, when the receiving bot answers, then its reply is attributed to that bot
  in the group thread and the requesting run continues with the result.
- Rationale: the collaboration story.

### REQ-011 — Activity view replaces the list (SHOULD)
The chat list has a regular view and an activity view; only one shows at a time and Activity replaces
the whole regular list, including pinned, archived and snoozed shelves, which return on exit.
- Given the member switches to Activity, when the view renders, then no pinned or archived shelf is
  visible; when they switch back, then the previous shelves are intact.
- Rationale: two lists showing the same chats is noise.

## Runs and the agent loop

### REQ-012 — Explicit run state machine with one executor (MUST)
A run moves only through the allowed transitions, and concurrent processes cannot execute it twice.
- Given statuses `queued → leased → running → completed`, when a second process tries to start a
  leased run whose fence is stale, then it is refused and the first owner continues.
- Given a run in `completed`, `failed` or `cancelled`, when any transition is attempted, then only
  `failed → queued` (retry) is allowed.
- Rationale: correctness under concurrency and deploys.

### REQ-013 — In-flight work survives a deploy (MUST)
A process that is shutting down or out of headroom hands its runs to another process instead of
abandoning them.
- Given a running run on a process that starts shutting down, when shutdown begins, then the run is
  published for another process and resumes there with its attempt history intact.
- Rationale: deploys must not lose or duplicate work.

### REQ-014 — Orphaned runs are recovered (MUST)
Any process periodically finds runnable rows whose owner died or whose lease expired and starts them.
- Given a run whose lease expired five minutes ago, when a worker scans, then it claims and executes
  it exactly once.
- Rationale: crashes are normal in a self-hosted box.

### REQ-015 — Model selection with fallbacks and bring-your-own-key (MUST)
A run picks a model from bot, space or user preferences, can be pinned, and falls back when the first
choice is unavailable; credentials can be the deployment's, a user's or a provider OAuth login.
- Given a bot pinned to a model the deployment cannot reach, when the run starts, then the run
  reports a user-visible model error rather than executing with a different model silently.
- Given a user with their own credential, when they run a bot, then requests go through their
  credential and usage is attributed to them.
- Rationale: "any model, with your own keys".

### REQ-016 — Tools are a per-run catalogue (MUST)
Each run receives the tools its bot may use: built-in tools, installed connectors, MCP servers, skills
and memory, filtered by what is enabled for the space and the bot.
- Given a bot with no connectors, when its run starts, then the tool list contains only built-ins and
  memory/skill tools.
- Given a tool the bot may not use, when the model calls it, then the call is refused and recorded.
- Rationale: authority must follow configuration, not the prompt.

### REQ-017 — Approvals gate side effects, and effects are idempotent (MUST)
A policy per space and per user decides which tool calls need approval; an approval executes its
effect once even if retried.
- Given a tool call classified as requiring approval, when the model requests it, then a pending card
  is created and no effect happens until a member approves.
- Given an approved effect that already ran, when the run resumes and replays it, then the recorded
  effect key prevents a second execution.
- Rationale: agents take real actions; they must not double-charge or double-send.

### REQ-018 — Spending limits and purchase requests (MUST)
A space can cap model spending and the bot can ask to buy something, which becomes an approval and a
recorded payment.
- Given a spending limit close to its cap, when the bot requests a purchase, then the request is a
  card and the payment only exists after approval.
- Given the cap reached, when the bot tries another paid request, then it is refused with a visible
  reason.
- Rationale: unattended agents must not spend freely.

### REQ-019 — Subagent delegation (SHOULD)
A run can delegate a scoped task to a helper run that reports its result back to the parent.
- Given a parent run delegating research, when the helper finishes, then the parent receives a
  structured result and continues; the helper's internal steps do not leak into the chat.
- Rationale: parallel work without a second conversation.

### REQ-020 — Streaming replies to clients (MUST)
Text, tool activity and state changes reach clients incrementally over the subscription stream, with
the run's terminal state announced last.
- Given a long answer, when the model streams, then clients receive progressive message updates
  before the run completes.
- Given a client that reconnects mid-stream, when it resumes, then it can rebuild the current state
  from the snapshot plus events without losing the message.
- Rationale: perceived latency and reliability.

### REQ-021 — Failures are user-legible (MUST)
Provider errors, tool denials and cancellations surface as a run status plus a reason, never as a
silent stop.
- Given a model provider returns 429, when the run fails, then the run records the failure with a
  retryable classification and the chat shows why.
- Rationale: self-hosted operators must be able to debug from the UI.

## Memory

### REQ-022 — Memory documents and shared memory (MUST)
A bot keeps memory documents it can read and write, and can write to a space-shared memory visible to
other bots; a configurable provider can back this.
- Given a bot writes a memory entry, when a later run starts, then that entry is available in its
  context within the configured scope.
- Given memory saved as shared space memory, when another bot in the space runs, then it can read it.
- Rationale: persistence is what makes a teammate.

## Routines

### REQ-023 — Routines as cron definitions (MUST)
A routine is a prompt plus one or more cron expressions and a timezone, active or not, owned by a bot.
- Given two cron expressions and a timezone, when the routine is saved, then the next due time is
  computed in that timezone and exposed.
- Given `@once`, when the routine fires, then it deactivates instead of rescheduling.
- Rationale: scheduled work is the second half of "teammate".

### REQ-024 — Reliable dispatch, retry and coalescing (MUST)
Due routines wake their bot, failures retry with backoff, account-level failures pause the routine, and
overlapping wakes coalesce into one run.
- Given a run for routine R is still active when the next schedule arrives, then no second run is
  created and the schedule is recorded as coalesced.
- Given repeated failures, when the account-failure threshold is reached, then the routine pauses and
  the owner is told.
- Rationale: unattended schedules must not stampede or spam.

### REQ-025 — Try a routine before enabling it (SHOULD)
A member can run a routine once, see the result as a normal turn, and keep or discard it.
- Given a draft routine, when the member runs it now, then a run is started with the routine's prompt
  and no schedule change.
- Rationale: crons are easy to get wrong.

## Computer

### REQ-026 — A computer per bot, started lazily and released when idle (MUST)
A bot's computer (browser, shell, files, desktop) starts when work needs it and is released when
inactive, respecting configured capacity.
- Given a bot with no computer, when a run needs a browser, then a computer is provisioned and the run
  waits for it with a bounded timeout.
- Given an idle computer past the idle window, when the sweeper runs, then the computer is stopped and
  its home is preserved per retention policy.
- Rationale: capacity is finite; "always on" is unaffordable.

### REQ-027 — Provider-agnostic computers, fail-closed (MUST)
The computer backend is configurable (local Docker supervisor, cloud providers, a user's own device,
or none) and an invalid configuration refuses to start rather than silently degrading.
- Given `SANDBOX_PROVIDER=novita` without its key, when the API starts, then startup fails with a
  message naming the missing variable.
- Given provider `none`, when a run needs a computer, then it gets a clear "computers unavailable"
  answer.
- Rationale: self-hosting means many backends.

### REQ-028 — Observe and act on the computer (MUST)
Bots interact with the computer through a small tool surface: screenshot/observe, click/type/scroll,
navigate, snapshot the DOM, read/write files, run shell, open apps.
- Given a page loaded in the bot browser, when the bot acts, then the action targets the observed
  screen or element reference and the next observation reflects it.
- Given a file write, when the path escapes the bot's home, then it is refused.
- Rationale: computer use is the differentiator.

### REQ-029 — User takeover (MUST)
A member can take exclusive control of one bot's screen, do a manual step, and hand it back; the bot
pauses while the human holds control.
- Given a bot asking for takeover, when a member takes control, then the bot's input actions are
  refused until release and the UI shows "you have control".
- Given a successful sign-in done manually, when control is released, then the browser state is kept
  for later runs according to the checkpoint policy.
- Rationale: CAPTCHAs and passwords.

### REQ-030 — Screen access without a session (MUST)
Screen bytes are fetched through a signed capability that names bot, computer and generation, valid
only while that computer is current.
- Given a valid capability, when the frame is requested, then the proxy streams it; given a stale
  generation, then it is refused.
- Rationale: streaming through the API must not widen session authority.

### REQ-031 — Team and private computers (SHOULD)
A bot computer is either shared within the space's trust boundary (team) or fully isolated (private).
- Given a private computer, when another bot in the space runs, then it cannot see that home or
  browser identity.
- Rationale: some work must not be shared.

## Connectors, MCP, plugins, skills

### REQ-032 — Connector catalogue and per-space installs (MUST)
A catalogue of integrations can be browsed and installed into a space; installation records accounts
and exposes tools to bots.
- Given a catalogue entry, when a member installs it, then its tools appear for bots in that space and
  its credentials are stored per installation.
- Given a connector removed, when a bot tries its tool, then the call is refused.
- Rationale: "thousands of integrations" without per-bot plumbing.

### REQ-033 — MCP servers, remote and opt-in local (MUST)
A member can add an MCP server by URL (with OAuth where needed) and, only when the deployment allows
named commands, a local stdio server.
- Given a remote MCP server with OAuth, when the member connects, then tools are discovered and usable.
- Given stdio disabled (default), when a local command server is added, then it is refused explaining
  why.
- Rationale: MCP is the ecosystem path; local execution is a boundary.

### REQ-034 — Plugin marketplace with a no-local-code rule (MUST)
Plugins are packaged bundles of connectors and skills installed from a source reference; parts that
would execute third-party code on the server are not installed.
- Given a plugin containing a skill with scripts, when it is installed, then the skill is installed
  without scripts and the omission is named.
- Given a malformed source, when it is submitted, then validation fails with the reason and nothing is
  installed.
- Rationale: catalogue safety.

### REQ-035 — Skills (SHOULD)
Skills are named instruction bundles a bot can load, including built-in ones and ones taught in
conversation.
- Given a taught skill, when a later run needs it, then the skill's instructions are available and its
  use is visible.
- Rationale: reusable procedures.

## Artifacts, pages, boards

### REQ-036 — Artifacts with versions (MUST)
A bot can create files (images, documents, data) attached to a bot or a group; updating creates a new
version; members can list, read and delete them.
- Given an artifact updated twice, when listed, then versions 1..3 exist and the latest is returned by
  default.
- Given an artifact deleted, when it is requested, then it is gone for every member of the space.
- Rationale: output of real work.

### REQ-037 — Share a page by link (MUST)
A bot-built page can be made readable by anyone with the link, signed out, and revoked.
- Given a shared page token, when an anonymous browser fetches it, then the current version renders
  with `noindex` and `no-referrer`; given a revoked or unknown token, then a uniform 404.
- Rationale: sharing results with people who have no account.

### REQ-038 — Ticket board (COULD, feature-flagged)
When enabled, spaces get a board of tickets with comments that bots can work on in the background.
- Given the flag off, when a member opens the board, then it is not found even when signed out.
- Rationale: task tracking for agent work.

### REQ-039 — Bot-built pages and dashboards (SHOULD)
A bot can produce an interactive page (charts, tables, links) rendered in a sandboxed viewer with a
strict content policy.
- Given a generated page, when it renders, then scripts run only inside the sandbox document and the
  app's own policy is not weakened.
- Rationale: "builds dashboards you can share".

## Identity and tenancy

### REQ-040 — Sign-up and authentication methods (MUST)
Deployment chooses available methods: password, e-mail code, OIDC SSO, passkeys; sign-up can be
closed or allow-listed; the capabilities endpoint reports what is available.
- Given sign-ups disabled, when an unknown person tries to register, then it is refused.
- Given OIDC configured, when a user signs in, then the account is linked by verified e-mail only, and
  implicit linking of other accounts is refused.
- Rationale: self-hosted deployments have very different identity needs.

### REQ-041 — Sessions and trusted origins (MUST)
Sessions are cookie-based; the API accepts only configured origins; session and account responses are
never cached.
- Given a cross-origin request from an unlisted origin, when it calls the API, then CORS refuses it.
- Given any `/api/auth/*` or `/rpc/*` response, then `cache-control: no-store` is present.
- Rationale: credential safety in a self-hosted deployment behind arbitrary proxies.

### REQ-042 — Spaces, members and invitations (MUST)
A user belongs to an organization and to one or more spaces; roles govern what they can do; invitations
carry a token and expire; leaving and removal are explicit.
- Given an invitation token, when the invitee accepts while signed in as the invited address, then they
  become a space member; given a different address, then it is refused.
- Given a member removed from a space, when they call any space RPC, then it fails as unauthorized.
- Rationale: tenancy.

### REQ-043 — Tenant isolation is enforced in the data layer (MUST)
Every user-initiated query runs under a tenant context; cross-tenant access is possible only through
explicitly justified system paths.
- Given a request for another space's bot id, when it is executed, then it returns not-found rather
  than data.
- Given a system path (auth, webhooks, boot), then it runs as system with a recorded reason.
- Rationale: one missed check must not leak another customer's data.

### REQ-044 — Export, import and delete an account (SHOULD)
A user can export their data as an archive, import a previous export into a new account, and delete
the account with its consequences.
- Given an export, when downloaded, then it contains a manifest plus bots, threads, messages and
  artifacts in a documented layout.
- Given a deletion, when confirmed, then sessions are revoked, billing is settled and data is removed
  or retained per the stated policy.
- Rationale: portability and GDPR-style obligations.

### REQ-045 — Audit log, security events and directory sync (SHOULD)
Security-relevant actions are recorded as audit events, exportable, with optional forwarding; SCIM can
provision and deprovision accounts.
- Given a member leaves through SCIM, when the change is applied, then their sessions and devices are
  settled like a manual departure.
- Given an audit export, when generated, then it is JSONL with a stable shape.
- Rationale: enterprise operation.

### REQ-046 — Secrets for bots (MUST)
Bots can request, store, list and forget secrets; values are encrypted at rest and never shown again;
a member can inject one during a run.
- Given a stored secret, when a later run uses it, then the value is available to the tool but not
  returned in any message or log.
- Given a secret forgotten, when a run asks for it, then the bot must request it again.
- Rationale: agents need credentials without leaking them.

## Channels

### REQ-047 — Inbound e-mail per bot (SHOULD)
Each bot can have an address; authenticated inbound mail starts or continues work, with sender
verification rules.
- Given mail to a bot address from the owner, when it arrives with a valid relay token, then a run
  starts in that bot's thread.
- Given mail from an unverified stranger and no routine rule, then it is dropped or held per policy.
- Rationale: e-mail is a universal interface.

### REQ-048 — Messaging platforms (SHOULD)
A member can install the product into Slack/Teams/Telegram/Discord-class surfaces; messages map to
threads and replies come back to the same channel.
- Given an installation, when a user messages the bot in the platform, then a run starts and its reply
  is delivered to that conversation.
- Given an installation removed, when a message arrives, then it is rejected.
- Rationale: teammates live where the team talks.

### REQ-049 — Voice and calls (COULD, feature-flagged)
Voice replies and calls (inbound/outbound) can be enabled with provider credentials and push
notifications.
- Given calls disabled, when a client asks for call capabilities, then it is told they are not found.
- Given an incoming call, when it is answered, then a voice session starts on the conversation.
- Rationale: hands-free use.

### REQ-050 — Bot webhooks and repository events (SHOULD)
Each bot can expose a webhook authenticated by its own bearer secret, and repository events can wake
routines that opt in.
- Given a valid bearer, when a payload arrives, then a run starts with the payload as context; given a
  wrong or missing secret, then a uniform 401.
- Rationale: event-driven agents.

## Billing and models

### REQ-051 — Billing, credits and plan limits (SHOULD)
Paid plans go through a payment provider with checkout, subscription state, credits and plan limits that
gate features.
- Given a checkout completed, when the webhook is verified, then the subscription becomes active and
  limits apply.
- Given an unpaid account on a paid feature, when it is used, then it is refused with an upgrade path.
- Rationale: hosted deployments need revenue; self-hosted can leave it off.

### REQ-052 — Model catalogue and credentials (MUST)
The product lists available models per provider, validates a credential when connected, supports OAuth
logins for providers that offer them, and can cap credits per account.
- Given an invalid API key, when the member connects it, then the connection is rejected with the
  provider's reason.
- Given provider OAuth, when the login completes, then the credential is stored and refreshable.
- Rationale: "any model".

## Operations and clients

### REQ-053 — Health and readiness (MUST)
Liveness and readiness endpoints exist; readiness checks the database with a statement timeout and
reports configured capabilities without leaking secrets.
- Given a healthy stack, when `/health` is called, then it returns ok; when `/ready` is called, then it
  returns ok after a bounded database probe.
- Given a degraded database, when `/ready` is called, then it fails within the timeout.
- Rationale: orchestrators and operators.

### REQ-054 — Observability (MUST)
Structured logs carry request and run correlation; run timing is recorded with named fields;
OpenTelemetry export and CSP violation reporting are available.
- Given a completed run, when logs are inspected, then `run.execution_started`, `run.first_response`
  and `run.finished` exist with duration fields.
- Given a blocked resource, when the browser reports it, then the API accepts the report.
- Rationale: you cannot operate what you cannot see.

### REQ-055 — Desktop client (SHOULD)
An Electron shell loads the web app, keeps sessions in a partition it controls, supports native
passkeys, auto-update, and can manage a local stack.
- Given the desktop app started, when the user signs in, then the session persists across restarts
  without exposing cookies to other windows.
- Given an update available, when downloaded and verified, then it installs on restart.
- Rationale: a computer-bound product benefits from a desktop shell.

### REQ-056 — Mobile client (SHOULD)
An Expo app covers sign-in, chat, computer viewing, artifacts, groups, spaces and push notifications,
with secure credential storage.
- Given a signed-in mobile user, when a run finishes and notifications are on, then a push arrives.
- Given the app resumed after a long background, when it opens a thread, then it resynchronises from
  its snapshot.
- Rationale: chat products are used on phones.

### REQ-057 — Internationalization and RTL (SHOULD)
User-visible text goes through message catalogues; 14 locales ship; right-to-left layouts work for
Hebrew and Arabic-class locales, including in e-mail.
- Given a Hebrew locale, when the app renders, then direction is RTL and layout mirrors without
  mirrored charts or code.
- Given a missing translation, when a string renders, then it falls back to English.
- Rationale: declared support must be real.

### REQ-058 — Administration surfaces (SHOULD)
Server operators get diagnostics (CLI and internal health), plugin submission review, access grants
and deployment settings; none of it is reachable by ordinary members.
- Given a member without the admin role, when they call an admin-only procedure, then it is refused.
- Given a plugin submission, when an admin approves or rejects it, then the state changes and the
  submitter is notified.
- Rationale: operating without SSH into the database.
