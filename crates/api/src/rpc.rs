//! The RPC surface: `POST /rpc/{namespace}/{procedure}` (ADR-003).
//!
//! One route, one dispatcher. The namespace and the procedure come from the path, the body is
//! parsed by the procedure's own request type, and the reply is the procedure's own response
//! type. A procedure that does not exist is *not found*, a body that does not match the schema is
//! *bad request* — both in the stable error envelope (spec/03-api-contract.md §3.1).
//!
//! The procedure names are the reference's, so the golden fixtures replay unchanged.

use axum::{
    Json, Router,
    body::Bytes,
    extract::{Path, State},
    response::{IntoResponse, Response},
    routing::post,
};
use boma_app::usecases;
use boma_domain::SpaceRole;
use serde::Deserialize;

use crate::{
    auth::AuthContext,
    error::ApiError,
    state::AppState,
    wire::{
        AnswerReply, BotReply, BotsListReply, CancelReply, ComputerOpenScreenReply,
        ComputerProviderInfo, ComputerProviderReply, ComputerSettingsReply, ComputerStatusReply,
        ComputerTakeoverReply, ConnectionsInstallReply, ConnectionsListReply,
        ConnectionsRemoveReply, ConnectorView, InvitationReply, MarketplaceInstallReply,
        MarketplaceListReply, MarketplaceSubmitReply, McpOAuthStartReply, McpServerReply,
        McpServerView, McpServersListReply, McpServersRemoveReply, MemoryDocumentReply,
        MemoryDocumentsReply, MemoryProviderReply, MemoryRemoveReply, MemoryWriteReply,
        MessagesReply, PluginInstallView, ReactReply, RosterBotView, RoutineRemoveReply,
        RoutineRunNowReply, RoutineView, RoutineWriteReply, RoutinesReply, RunRetryReply, RunView,
        RunsReply, SecretForgetReply, SecretSetReply, SecretView, SecretsReply, SendReply,
        SkillReply, SkillView, SkillsReply, SnoozeReply, SpaceReply, TeachSkillReply,
        ThreadHeadReply,
    },
};

/// The namespaces this milestone serves.
pub mod namespaces {
    /// Conversations: sending, reading, subscribing.
    pub const THREADS: &str = "threads";
    /// Bots and their conversations.
    pub const BOTS: &str = "bots";
    /// Spaces and their members.
    pub const SPACES: &str = "spaces";
    /// Runs and their activity.
    pub const RUNS: &str = "runs";
    /// Memory documents (REQ-022).
    pub const MEMORY: &str = "memory";
    /// Routines: cron definitions that wake a bot (REQ-023).
    pub const ROUTINES: &str = "routines";
    /// The computer: a bot's sandbox (REQ-026..031).
    pub const COMPUTER: &str = "computer";
    /// Secrets for bots, write-only (REQ-046).
    pub const BOT_SECRETS: &str = "botSecrets";
    /// Skills: named instruction bundles a bot loads (REQ-035).
    pub const SKILLS: &str = "skills";
    /// MCP servers: the integration registry of the space (REQ-033).
    pub const MCP: &str = "mcp";
    /// Connectors: the builtin catalogue of integrations (REQ-032).
    pub const CONNECTIONS: &str = "connections";
    /// Marketplace: submissions and installs of plugins (REQ-034).
    pub const MARKETPLACE: &str = "marketplace";
}

/// Every procedure this deployment serves.
///
/// The list is the contract: a procedure that is not here is not reachable.
#[must_use]
pub fn procedures() -> Vec<(&'static str, &'static str)> {
    vec![
        (namespaces::THREADS, "send"),
        (namespaces::THREADS, "messages"),
        (namespaces::THREADS, "subscribe"),
        (namespaces::THREADS, "answer"),
        (namespaces::THREADS, "react"),
        (namespaces::THREADS, "head"),
        (namespaces::THREADS, "markRead"),
        (namespaces::THREADS, "markUnread"),
        (namespaces::THREADS, "snooze"),
        (namespaces::THREADS, "unsnooze"),
        (namespaces::BOTS, "create"),
        (namespaces::BOTS, "get"),
        (namespaces::BOTS, "list"),
        (namespaces::SPACES, "create"),
        (namespaces::SPACES, "invite"),
        (namespaces::RUNS, "list"),
        (namespaces::RUNS, "cancel"),
        (namespaces::RUNS, "retry"),
        (namespaces::MEMORY, "write"),
        (namespaces::MEMORY, "read"),
        (namespaces::MEMORY, "list"),
        (namespaces::MEMORY, "remove"),
        (namespaces::MEMORY, "provider"),
        (namespaces::ROUTINES, "create"),
        (namespaces::ROUTINES, "list"),
        (namespaces::ROUTINES, "remove"),
        (namespaces::ROUTINES, "runNow"),
        (namespaces::COMPUTER, "status"),
        (namespaces::COMPUTER, "start"),
        (namespaces::COMPUTER, "stop"),
        (namespaces::COMPUTER, "openScreen"),
        (namespaces::COMPUTER, "takeover"),
        (namespaces::COMPUTER, "release"),
        (namespaces::COMPUTER, "settings"),
        (namespaces::COMPUTER, "provider"),
        (namespaces::BOT_SECRETS, "set"),
        (namespaces::BOT_SECRETS, "list"),
        (namespaces::BOT_SECRETS, "forget"),
        (namespaces::SKILLS, "list"),
        (namespaces::SKILLS, "get"),
        (namespaces::SKILLS, "teach"),
        (namespaces::MCP, "servers.list"),
        (namespaces::MCP, "servers.add"),
        (namespaces::MCP, "servers.remove"),
        (namespaces::MCP, "servers.probe"),
        (namespaces::MCP, "oauth.start"),
        (namespaces::MCP, "oauth.complete"),
        (namespaces::CONNECTIONS, "list"),
        (namespaces::CONNECTIONS, "install"),
        (namespaces::CONNECTIONS, "remove"),
        (namespaces::MARKETPLACE, "list"),
        (namespaces::MARKETPLACE, "submit"),
        (namespaces::MARKETPLACE, "install"),
    ]
}

/// Builds the RPC routes.
pub fn router() -> Router<AppState> {
    Router::new().route("/rpc/{namespace}/{procedure}", post(dispatch))
}

/// Dispatches one procedure call.
///
/// The caller must be signed in: every procedure in the spec acts for a member of a space
/// (REQ-041, REQ-043).
#[allow(clippy::too_many_lines)] // the match is the procedure table; splitting hides the contract
async fn dispatch(
    State(state): State<AppState>,
    Path((namespace, procedure)): Path<(String, String)>,
    headers: axum::http::HeaderMap,
    body: Bytes,
) -> Response {
    let auth = match state.authenticate(&headers).await {
        Ok(auth) => auth,
        Err(error) => return error.into_response(),
    };

    let outcome: Result<Response, ApiError> = match (namespace.as_str(), procedure.as_str()) {
        (namespaces::THREADS, "send") => send(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::THREADS, "messages") => messages(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::THREADS, "subscribe") => crate::realtime::subscribe(state, auth, body).await,
        (namespaces::THREADS, "answer") => threads_answer(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::THREADS, "react") => threads_react(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::THREADS, "head") => threads_head(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::THREADS, "markRead") => threads_mark_read(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::THREADS, "markUnread") => threads_mark_unread(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::THREADS, "snooze") => threads_snooze(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::THREADS, "unsnooze") => threads_unsnooze(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::BOTS, "create") => bots_create(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::BOTS, "get") => bots_get(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::BOTS, "list") => bots_list(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::SPACES, "create") => spaces_create(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::SPACES, "invite") => spaces_invite(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::RUNS, "list") => runs_list(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::RUNS, "cancel") => runs_cancel(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::RUNS, "retry") => runs_retry(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::MEMORY, "write") => memory_write(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::MEMORY, "read") => memory_read(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::MEMORY, "list") => memory_list(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::MEMORY, "remove") => memory_remove(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::MEMORY, "provider") => Ok(Json(MemoryProviderReply {
            provider: "builtin".to_owned(),
        })
        .into_response()),
        (namespaces::ROUTINES, "create") => routines_create(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::ROUTINES, "list") => routines_list(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::ROUTINES, "remove") => routines_remove(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::ROUTINES, "runNow") => routines_run_now(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::COMPUTER, "status") => computer_status(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::COMPUTER, "start") => computer_start(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::COMPUTER, "stop") => computer_stop(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::COMPUTER, "openScreen") => computer_open_screen(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::COMPUTER, "takeover") => computer_takeover(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::COMPUTER, "release") => computer_release(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::COMPUTER, "settings") => computer_settings(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::COMPUTER, "provider") => {
            Ok::<_, ApiError>(Json(computer_provider(state.app())).into_response())
        }
        (namespaces::BOT_SECRETS, "set") => secrets_set(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::BOT_SECRETS, "list") => secrets_list(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::BOT_SECRETS, "forget") => secrets_forget(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::SKILLS, "list") => skills_list(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::SKILLS, "get") => skills_get(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::SKILLS, "teach") => skills_teach(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::MCP, "servers.list") => mcp_servers_list(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::MCP, "servers.add") => mcp_servers_add(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::MCP, "servers.remove") => mcp_servers_remove(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::MCP, "servers.probe") => mcp_servers_probe(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::MCP, "oauth.start") => mcp_oauth_start(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::MCP, "oauth.complete") => mcp_oauth_complete(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::CONNECTIONS, "list") => connections_list(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::CONNECTIONS, "install") => connections_install(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::CONNECTIONS, "remove") => connections_remove(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::MARKETPLACE, "list") => marketplace_list(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::MARKETPLACE, "submit") => marketplace_submit(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        (namespaces::MARKETPLACE, "install") => marketplace_install(state, auth, body)
            .await
            .map(|reply| Json(reply).into_response()),
        _ => Err(ApiError::not_found()),
    };

    match outcome {
        Ok(response) => response,
        Err(error) => error.into_response(),
    }
}

/// `threads.send` — write a message and start the run that answers it.
async fn send(state: AppState, auth: AuthContext, body: Bytes) -> Result<SendReply, ApiError> {
    let request: SendRequest = parse(&body)?;
    let outcome = usecases::send_message(
        state.app(),
        &auth.actor.context,
        &usecases::SendMessage {
            bot_id: request.bot_id,
            text: request.text,
            client_nonce: request.client_nonce,
            reply_to_message_id: request.reply_to_message_id,
            reply_quote: request.reply_quote,
        },
    )
    .await?;

    Ok(SendReply {
        message_id: outcome.message.id.as_str().to_owned(),
        run_id: outcome.run.id.as_str().to_owned(),
        status: outcome.run.status.as_str().to_owned(),
    })
}

/// The body of `threads.send`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SendRequest {
    /// The bot that receives the message.
    bot_id: String,
    /// What the person wrote.
    text: String,
    /// The nonce that makes the send idempotent.
    #[serde(default)]
    client_nonce: Option<String>,
    /// The message this one answers, when it is a reply.
    #[serde(default)]
    reply_to_message_id: Option<String>,
    /// A short quote of the message being answered.
    #[serde(default)]
    reply_quote: Option<String>,
}

/// `threads.messages` — a page of the thread's history, newest first.
async fn messages(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<MessagesReply, ApiError> {
    let request: MessagesRequest = parse(&body)?;
    let page = usecases::page_messages(
        state.app(),
        &auth.actor.context,
        &request.thread_id,
        request.cursor.as_deref(),
        request.limit,
    )
    .await?;

    Ok(MessagesReply {
        messages: page
            .messages
            .iter()
            .map(crate::wire::MessageView::from)
            .collect(),
        next_cursor: page.next_cursor,
    })
}

/// The body of `threads.messages`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MessagesRequest {
    /// The thread to read.
    thread_id: String,
    /// The cursor of the older page.
    #[serde(default)]
    cursor: Option<String>,
    /// How many messages the caller wants.
    #[serde(default)]
    limit: Option<usize>,
}

/// `bots.create` — create a bot and its thread in one transaction.
async fn bots_create(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<BotReply, ApiError> {
    let request: BotCreateRequest = parse(&body)?;
    let created = usecases::create_bot(
        state.app(),
        &auth.actor.context,
        &request.name,
        request.description,
        request.color,
    )
    .await?;

    Ok(BotReply {
        id: created.bot.id.as_str().to_owned(),
        name: created.bot.name,
        thread_id: created.thread.id.as_str().to_owned(),
        archived_at: created.bot.archived_at,
        created_at: created.bot.created_at,
    })
}

/// The body of `bots.create`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BotCreateRequest {
    /// The name a person sees.
    name: String,
    /// What the bot is for.
    #[serde(default)]
    description: Option<String>,
    /// The colour a person sees.
    #[serde(default)]
    color: Option<String>,
}

/// `bots.get` — read one bot.
async fn bots_get(state: AppState, auth: AuthContext, body: Bytes) -> Result<BotReply, ApiError> {
    let request: BotGetRequest = parse(&body)?;
    let bot = usecases::find_bot(state.app(), &auth.actor.context, &request.bot_id).await?;
    Ok(BotReply {
        id: bot.id.as_str().to_owned(),
        name: bot.name,
        thread_id: bot.thread_id.as_str().to_owned(),
        archived_at: bot.archived_at,
        created_at: bot.created_at,
    })
}

/// The body of `bots.get`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BotGetRequest {
    /// The bot to read.
    bot_id: String,
}

/// `spaces.create` — create a space and join it as its owner.
async fn spaces_create(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<SpaceReply, ApiError> {
    let request: SpaceCreateRequest = parse(&body)?;
    let created = usecases::create_space(state.app(), &auth.actor.context, &request.name).await?;
    Ok(SpaceReply {
        id: created.space.id.as_str().to_owned(),
        name: created.space.name,
        is_default: created.space.is_default,
        role: created.role.as_str().to_owned(),
    })
}

/// The body of `spaces.create`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SpaceCreateRequest {
    /// The name a person sees.
    name: String,
}

/// `spaces.invite` — invite somebody into the caller's space.
async fn spaces_invite(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<InvitationReply, ApiError> {
    let request: SpaceInviteRequest = parse(&body)?;
    let invitation = usecases::invite(
        state.app(),
        &auth.actor.context,
        &request.email,
        request.role,
        state.invitation_ttl_seconds(),
    )
    .await?;
    Ok(InvitationReply {
        invitation_id: invitation.id.as_str().to_owned(),
        expires_at: invitation.expires_at,
        status: "pending".to_owned(),
    })
}

/// The body of `spaces.invite`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SpaceInviteRequest {
    /// Who is invited.
    email: String,
    /// The role the invitee will have.
    #[serde(default = "default_role")]
    role: SpaceRole,
}

/// The role an invitation grants when the caller does not say.
fn default_role() -> SpaceRole {
    SpaceRole::Member
}

/// `threads.answer` — answer a card and resume the run it blocked (REQ-017).
async fn threads_answer(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<AnswerReply, ApiError> {
    let request: ThreadAnswerRequest = parse(&body)?;
    let outcome = usecases::answer_card(
        state.app(),
        &auth.actor.context,
        &request.card_id,
        &request.answer,
    )
    .await?;
    Ok(AnswerReply {
        ok: true,
        run_id: outcome.card.run_id.map(|run| run.as_str().to_owned()),
    })
}

/// The body of `threads.answer`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ThreadAnswerRequest {
    /// The card to answer.
    card_id: String,
    /// The answer a member gives.
    answer: String,
}

/// The body of `threads.head`, `threads.markRead` and `threads.markUnread` (REQ-009).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ThreadHeadRequest {
    /// The thread whose head the caller wants.
    thread_id: String,
}

/// `threads.head` — the caller's view of the thread's tail and own read position (REQ-009).
async fn threads_head(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<ThreadHeadReply, ApiError> {
    let request: ThreadHeadRequest = parse(&body)?;
    let head = usecases::thread_head(state.app(), &auth.actor.context, &request.thread_id).await?;
    Ok(ThreadHeadReply::from(&head))
}

/// `threads.markRead` — move the caller's read position to the newest message (REQ-009).
async fn threads_mark_read(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<ThreadHeadReply, ApiError> {
    let request: ThreadHeadRequest = parse(&body)?;
    let head = usecases::mark_read(state.app(), &auth.actor.context, &request.thread_id).await?;
    Ok(ThreadHeadReply::from(&head))
}

/// `threads.markUnread` — move the caller's read position back to zero (REQ-009).
async fn threads_mark_unread(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<ThreadHeadReply, ApiError> {
    let request: ThreadHeadRequest = parse(&body)?;
    let head = usecases::mark_unread(state.app(), &auth.actor.context, &request.thread_id).await?;
    Ok(ThreadHeadReply::from(&head))
}

/// The body of `bots.list` (REQ-009).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BotsListRequest {}

/// `bots.list` — the roster the rail renders from, with per-member unread and snooze (REQ-009).
async fn bots_list(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<BotsListReply, ApiError> {
    let _: BotsListRequest = parse(&body)?;
    let bots = usecases::list_bots(state.app(), &auth.actor.context).await?;
    Ok(BotsListReply {
        bots: bots.iter().map(RosterBotView::from).collect(),
    })
}

/// The body of `threads.snooze` (REQ-009).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ThreadSnoozeRequest {
    /// The thread to snooze.
    thread_id: String,
    /// Until when; absent leaves the chat snoozed until it is explicitly unsnoozed.
    #[serde(default)]
    until: Option<String>,
}

/// The body of `threads.unsnooze` (REQ-009).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ThreadUnsnoozeRequest {
    /// The thread to unsnooze.
    thread_id: String,
}

/// `threads.snooze` — hide the chat from the regular list until `until` (REQ-009).
async fn threads_snooze(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<SnoozeReply, ApiError> {
    let request: ThreadSnoozeRequest = parse(&body)?;
    let outcome = usecases::snooze_thread(
        state.app(),
        &auth.actor.context,
        &request.thread_id,
        request.until.as_deref(),
    )
    .await?;
    Ok(SnoozeReply::from(&outcome))
}

/// `threads.unsnooze` — bring the snoozed chat back to the regular list (REQ-009).
async fn threads_unsnooze(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<SnoozeReply, ApiError> {
    let request: ThreadUnsnoozeRequest = parse(&body)?;
    let outcome =
        usecases::snooze_thread(state.app(), &auth.actor.context, &request.thread_id, None).await?;
    Ok(SnoozeReply::from(&outcome))
}

/// The body of `memory.write` (REQ-022).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MemoryWriteRequest {
    /// The owner bot, for scope `bot`.
    #[serde(default)]
    bot_id: Option<String>,
    /// `bot` or `shared`.
    scope: String,
    /// The path of the document.
    path: String,
    /// The content to write.
    content: String,
}

/// The body of `memory.read` and `memory.remove` (REQ-022).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MemoryReadRequest {
    /// The owner bot, for scope `bot`.
    #[serde(default)]
    bot_id: Option<String>,
    /// `bot` or `shared`.
    scope: String,
    /// The path of the document.
    path: String,
}

/// The body of `memory.list` (REQ-022).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MemoryListRequest {
    /// The owner bot, for scope `bot`.
    #[serde(default)]
    bot_id: Option<String>,
    /// `bot` or `shared`.
    scope: String,
}

/// `memory.write` — append a revision and return the projection (REQ-022).
async fn memory_write(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<MemoryWriteReply, ApiError> {
    let request: MemoryWriteRequest = parse(&body)?;
    let outcome = usecases::memory_write(
        state.app(),
        &auth.actor.context,
        &request.scope,
        request.bot_id.as_deref(),
        &request.path,
        &request.content,
    )
    .await?;
    Ok(MemoryWriteReply::from(&outcome.document))
}

/// `memory.read` — read one document (REQ-022).
async fn memory_read(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<MemoryDocumentReply, ApiError> {
    let request: MemoryReadRequest = parse(&body)?;
    let document = usecases::memory_read(
        state.app(),
        &auth.actor.context,
        &request.scope,
        request.bot_id.as_deref(),
        &request.path,
    )
    .await?;
    Ok(MemoryDocumentReply::from(&document))
}

/// `memory.list` — list the documents of a scope (REQ-022).
async fn memory_list(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<MemoryDocumentsReply, ApiError> {
    let request: MemoryListRequest = parse(&body)?;
    let documents = usecases::memory_list(
        state.app(),
        &auth.actor.context,
        &request.scope,
        request.bot_id.as_deref(),
    )
    .await?;
    Ok(MemoryDocumentsReply {
        documents: documents.iter().map(MemoryDocumentReply::from).collect(),
    })
}

/// `memory.remove` — remove a document and its revision chain (REQ-022).
async fn memory_remove(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<MemoryRemoveReply, ApiError> {
    let request: MemoryReadRequest = parse(&body)?;
    let outcome = usecases::memory_remove(
        state.app(),
        &auth.actor.context,
        &request.scope,
        request.bot_id.as_deref(),
        &request.path,
    )
    .await?;
    Ok(MemoryRemoveReply {
        path: outcome.path,
        removed: outcome.removed,
    })
}

/// The default every routine gets: awake.
fn default_active() -> bool {
    true
}

/// The body of `routines.create` (REQ-023).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RoutinesCreateRequest {
    /// The bot the routine wakes.
    bot_id: String,
    /// The name a person sees.
    name: String,
    /// The prompt the agent receives when the routine fires.
    prompt: String,
    /// The cron expressions, or `@once`.
    crons: Vec<String>,
    /// The IANA timezone the cron computes in.
    timezone: String,
    /// Whether the routine wakes; a routine a member created is awake by default.
    #[serde(default = "default_active")]
    active: bool,
    /// Whether the owner is told about each run.
    #[serde(default)]
    notify: bool,
}

/// The body of `routines.list` (REQ-023).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RoutinesListRequest {
    /// The bot whose routines the caller lists.
    bot_id: String,
}

/// The body of `routines.remove` (REQ-023).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RoutinesRemoveRequest {
    /// The routine to remove.
    routine_id: String,
}

/// `routines.create` — create a routine and compute its next wake (REQ-023).
async fn routines_create(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<RoutineWriteReply, ApiError> {
    let request: RoutinesCreateRequest = parse(&body)?;
    let outcome = usecases::routines_create(
        state.app(),
        &auth.actor.context,
        &request.bot_id,
        &request.name,
        &request.prompt,
        &request.crons,
        &request.timezone,
        request.active,
        request.notify,
    )
    .await?;
    Ok(RoutineWriteReply::from(&outcome.routine))
}

/// `routines.list` — list a bot's routines (REQ-023).
async fn routines_list(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<RoutinesReply, ApiError> {
    let request: RoutinesListRequest = parse(&body)?;
    let routines =
        usecases::routines_list(state.app(), &auth.actor.context, &request.bot_id).await?;
    Ok(RoutinesReply {
        routines: routines.iter().map(RoutineView::from).collect(),
    })
}

/// `routines.remove` — remove a routine (REQ-023).
async fn routines_remove(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<RoutineRemoveReply, ApiError> {
    let request: RoutinesRemoveRequest = parse(&body)?;
    let outcome =
        usecases::routines_remove(state.app(), &auth.actor.context, &request.routine_id).await?;
    Ok(RoutineRemoveReply {
        routine_id: outcome.routine_id.as_str().to_owned(),
        removed: outcome.removed,
    })
}

/// The body shared by the bot-scoped computer procedures.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ComputerRequest {
    /// The bot whose computer is asked about.
    bot_id: String,
}

/// `computer.takeover` — take exclusive control of the bot's screen (REQ-029).
///
/// While a member holds the screen, computer-driven input refuses to act. The hold is
/// exclusive: only the holder may release it. Idempotent for the holder.
async fn computer_takeover(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<ComputerTakeoverReply, ApiError> {
    let request: ComputerRequest = parse(&body)?;
    let app = state.app();
    if app
        .services
        .computer
        .as_deref()
        .is_none_or(|runtime| runtime.provider() == "none")
    {
        return Err(ApiError::computers_unavailable());
    }
    let computer = usecases::computer_takeover(app, &auth.actor.context, &request.bot_id).await?;
    Ok(ComputerTakeoverReply::for_computer(&computer))
}

/// `computer.release` — hand the screen back (REQ-029).
///
/// Only the holder may release.
async fn computer_release(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<ComputerTakeoverReply, ApiError> {
    let request: ComputerRequest = parse(&body)?;
    let app = state.app();
    if app
        .services
        .computer
        .as_deref()
        .is_none_or(|runtime| runtime.provider() == "none")
    {
        return Err(ApiError::computers_unavailable());
    }
    let computer = usecases::computer_release(app, &auth.actor.context, &request.bot_id).await?;
    Ok(ComputerTakeoverReply::for_computer(&computer))
}

/// The body of `computer.settings` (REQ-026).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ComputerSettingsRequest {
    /// The bot whose computer to configure.
    bot_id: String,
    /// The RAM cap, in megabytes, when the computer may use more.
    #[serde(default)]
    memory_limit_mb: Option<u64>,
    /// The CPU cap, in CPUs.
    #[serde(default)]
    cpu_limit: Option<f64>,
    /// Minutes of idleness before release.
    #[serde(default)]
    idle_minutes: Option<u64>,
}

/// `computer.settings` — read or adjust the computer's configuration (REQ-026).
///
/// With no fields beyond `botId` the call is a read. Fields present are applied, within the
/// deployment limits; anything outside is refused.
async fn computer_settings(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<ComputerSettingsReply, ApiError> {
    let request: ComputerSettingsRequest = parse(&body)?;
    let app = state.app();
    let context = &auth.actor.context;
    let bot_id =
        boma_domain::BotId::new(request.bot_id.clone()).map_err(|_| ApiError::not_found())?;
    app.stores
        .bots
        .find(context, &bot_id)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(ApiError::not_found)?;
    let is_update = request.memory_limit_mb.is_some()
        || request.cpu_limit.is_some()
        || request.idle_minutes.is_some();
    if !is_update {
        let computer = usecases::computer_status(app, context, &request.bot_id).await?;
        return Ok(match computer {
            Some(computer) => ComputerSettingsReply {
                memory_limit_mb: computer.memory_limit_mb,
                cpu_limit: computer.cpu_limit,
                idle_minutes: computer.idle_minutes,
                mode: if computer.private {
                    "private".to_owned()
                } else {
                    "team".to_owned()
                },
            },
            None => ComputerSettingsReply {
                memory_limit_mb: 512,
                cpu_limit: 1.0,
                idle_minutes: 10,
                mode: "private".to_owned(),
            },
        });
    }
    let runtime = app
        .services
        .computer
        .as_deref()
        .filter(|runtime| runtime.provider() != "none")
        .ok_or_else(ApiError::computers_unavailable)?;
    let _ = runtime; // settings validate against the deployment limits regardless of provider
    let mut computer = usecases::computer_status(app, context, &request.bot_id)
        .await?
        .ok_or_else(ApiError::not_found)?;
    if let Some(mb) = request.memory_limit_mb {
        boma_domain::validate_memory_limit(mb).map_err(|_| ApiError::bad_request())?;
        computer.memory_limit_mb = mb;
    }
    if let Some(cpu) = request.cpu_limit {
        boma_domain::validate_cpu_limit(cpu).map_err(|_| ApiError::bad_request())?;
        computer.cpu_limit = cpu;
    }
    if let Some(minutes) = request.idle_minutes {
        boma_domain::validate_idle_minutes(minutes).map_err(|_| ApiError::bad_request())?;
        computer.idle_minutes = minutes;
    }
    computer.updated_at = app.services.clock.now();
    app.stores
        .computers
        .update(context, &computer)
        .await
        .map_err(ApiError::from)?;
    Ok(ComputerSettingsReply {
        memory_limit_mb: computer.memory_limit_mb,
        cpu_limit: computer.cpu_limit,
        idle_minutes: computer.idle_minutes,
        mode: if computer.private {
            "private".to_owned()
        } else {
            "team".to_owned()
        },
    })
}

/// `computer.provider` — which providers this deployment can run (REQ-027).
///
/// `frame` is true only when a frame command is configured for the image; `screenshot` is the
/// same capability under its wire name. `shell` is true for every non-`none` provider.
fn computer_provider(app: &boma_app::App) -> ComputerProviderReply {
    let active = app.services.computer.as_deref().map_or_else(
        || "none".to_owned(),
        |runtime| runtime.provider().to_owned(),
    );
    let (frame, screenshot) = app
        .services
        .computer
        .as_deref()
        .map_or((false, false), |runtime| {
            (
                runtime.frame_command().is_some(),
                runtime.frame_command().is_some(),
            )
        });
    let shell = app
        .services
        .computer
        .as_deref()
        .is_some_and(|runtime| runtime.provider() != "none");
    ComputerProviderReply {
        active,
        providers: vec![
            ComputerProviderInfo {
                name: "none".to_owned(),
                available: true,
                frame: false,
                shell: false,
                screenshot: false,
            },
            ComputerProviderInfo {
                name: "docker".to_owned(),
                available: app
                    .services
                    .computer
                    .as_deref()
                    .is_some_and(|runtime| runtime.provider() == "docker"),
                frame,
                shell,
                screenshot,
            },
        ],
    }
}

/// The body of `computer.status` (REQ-027).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ComputerStatusRequest {
    /// The bot whose computer is asked about.
    bot_id: String,
    /// `private` (default) or `team` (only meaningful for `computer.start`).
    #[serde(default)]
    mode: Option<String>,
}

/// Validates the bot-scoped computer body, or answers the uniform not-found.
fn computer_request(body: &Bytes) -> Result<ComputerStatusRequest, ApiError> {
    parse(body)
}

/// `computer.status` — where this bot's computer is (REQ-026, REQ-027).
///
/// A deployment without a computer backend answers `COMPUTERS_UNAVAILABLE`; a bot whose computer
/// has never started answers `stopped`, never an error. Another space's bot is `NOT_FOUND`.
async fn computer_status(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<ComputerStatusReply, ApiError> {
    let request = computer_request(&body)?;
    let app = state.app();
    if app
        .services
        .computer
        .as_deref()
        .is_none_or(|runtime| runtime.provider() == "none")
    {
        return Err(ApiError::computers_unavailable());
    }
    let computer = usecases::computer_status(app, &auth.actor.context, &request.bot_id).await?;
    Ok(computer.map_or_else(
        || ComputerStatusReply {
            status: "stopped".to_owned(),
            provider: app.services.computer.as_deref().map_or_else(
                || "none".to_owned(),
                |runtime| runtime.provider().to_owned(),
            ),
            started_at: None,
            screen_generation: None,
        },
        |computer| ComputerStatusReply::for_computer(&computer),
    ))
}

/// `computer.start` — start (or reuse) the bot's computer (REQ-026).
///
/// The boot is lazy by design: this procedure is the explicit ask for one, and the same computer
/// is reused by every later call. A deployment without a backend answers `COMPUTERS_UNAVAILABLE`.
async fn computer_start(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<ComputerStatusReply, ApiError> {
    let request = computer_request(&body)?;
    let app = state.app();
    if app
        .services
        .computer
        .as_deref()
        .is_none_or(|runtime| runtime.provider() == "none")
    {
        return Err(ApiError::computers_unavailable());
    }
    let private = match request.mode.as_deref() {
        None | Some("private") => true,
        Some("team") => false,
        Some(_) => {
            return Err(ApiError::new(
                crate::error::ErrorCode::BadRequest,
                "mode must be \"private\" or \"team\".",
            ));
        }
    };
    let outcome =
        usecases::ensure_computer(app, &auth.actor.context, &request.bot_id, private).await?;
    Ok(ComputerStatusReply::for_computer(&outcome.computer))
}

/// `computer.stop` — stop the bot's computer, preserving its home (REQ-026).
///
/// Idempotent: nothing to stop answers `stopped` with no timestamps.
async fn computer_stop(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<ComputerStatusReply, ApiError> {
    let request = computer_request(&body)?;
    let app = state.app();
    if app
        .services
        .computer
        .as_deref()
        .is_none_or(|runtime| runtime.provider() == "none")
    {
        return Err(ApiError::computers_unavailable());
    }
    let computer = usecases::stop_computer(app, &auth.actor.context, &request.bot_id).await?;
    Ok(ComputerStatusReply::for_computer(&computer))
}

/// `computer.openScreen` — mint a capability that opens the bot's screen (REQ-030).
///
/// The capability is returned once and the server stores only its hash. The computer must be
/// running: without a screen there is nothing to open.
async fn computer_open_screen(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<ComputerOpenScreenReply, ApiError> {
    let request = computer_request(&body)?;
    let app = state.app();
    if app
        .services
        .computer
        .as_deref()
        .is_none_or(|runtime| runtime.provider() == "none")
    {
        return Err(ApiError::computers_unavailable());
    }
    let issued = usecases::issue_screen_capability(app, &auth.actor.context, &request.bot_id)
        .await
        .map_err(|error| match error {
            boma_app::AppError::Rule(_) => ApiError::new(
                crate::error::ErrorCode::Conflict,
                "The computer is not running.",
            ),
            other => crate::error::ApiError::from(other),
        })?;
    Ok(ComputerOpenScreenReply {
        capability: issued.token,
        screen_generation: issued.screen_generation,
    })
}

/// The body of `routines.runNow` (REQ-025).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RoutinesRunNowRequest {
    /// The routine to fire, once, as a normal turn.
    routine_id: String,
}

/// `routines.runNow` — fire a routine once as a normal turn (REQ-025).
async fn routines_run_now(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<RoutineRunNowReply, ApiError> {
    let request: RoutinesRunNowRequest = parse(&body)?;
    let outcome =
        usecases::routines_run_now(state.app(), &auth.actor.context, &request.routine_id).await?;
    Ok(RoutineRunNowReply::from(&outcome))
}

/// The body of `threads.react`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ThreadsReactRequest {
    /// The message reacted to.
    message_id: String,
    /// The emoji to toggle.
    emoji: String,
    /// Whether to add or remove.
    action: ReactionActionRequest,
}

/// The two things a member may do to a reaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
enum ReactionActionRequest {
    /// Add the emoji for this member.
    Add,
    /// Remove the emoji for this member.
    Remove,
}

/// `threads.react` — add or remove an emoji reaction on a message (REQ-008).
async fn threads_react(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<ReactReply, ApiError> {
    let request: ThreadsReactRequest = parse(&body)?;
    let action = match request.action {
        ReactionActionRequest::Add => boma_app::ports::ReactionAction::Add,
        ReactionActionRequest::Remove => boma_app::ports::ReactionAction::Remove,
    };
    let outcome = usecases::react_to_message(
        state.app(),
        &auth.actor.context,
        &request.message_id,
        &request.emoji,
        action,
    )
    .await?;
    Ok(ReactReply {
        message_id: outcome.message_id.as_str().to_owned(),
        reactions: outcome
            .reactions
            .iter()
            .map(crate::wire::ReactionView::from)
            .collect(),
    })
}

/// `runs.list` — the activity projection, newest first.
async fn runs_list(state: AppState, auth: AuthContext, body: Bytes) -> Result<RunsReply, ApiError> {
    let request: RunsListRequest = parse(&body)?;
    let page = usecases::list_runs(
        state.app(),
        &auth.actor.context,
        request.bot_id.as_deref(),
        request.cursor.as_deref(),
        request.limit,
    )
    .await?;

    Ok(RunsReply {
        runs: page
            .runs
            .iter()
            .map(|activity| RunView::new(&activity.run, activity.prompt_snippet.clone()))
            .collect(),
        next_cursor: page.next_cursor,
    })
}

/// `runs.cancel` — stop a run, wherever it is (REQ-006).
async fn runs_cancel(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<CancelReply, ApiError> {
    let request: RunsCancelRequest = parse(&body)?;
    let run = usecases::cancel_run(state.app(), &auth.actor.context, &request.run_id).await?;
    Ok(CancelReply {
        id: run.id.as_str().to_owned(),
        status: run.status.as_str().to_owned(),
    })
}

/// The body of `runs.cancel`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RunsCancelRequest {
    /// The run the caller wants stopped.
    run_id: String,
}

/// The body of `runs.retry`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RunsRetryRequest {
    /// The run to re-queue.
    run_id: String,
}

/// `runs.retry` — re-queue a failed run for one more attempt (REQ-012).
async fn runs_retry(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<RunRetryReply, ApiError> {
    let request: RunsRetryRequest = parse(&body)?;
    let run = usecases::retry_run(state.app(), &auth.actor.context, &request.run_id).await?;
    Ok(RunRetryReply::from(&run))
}

/// The body of `runs.list`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RunsListRequest {
    /// The bot whose runs the caller wants.
    #[serde(default)]
    bot_id: Option<String>,
    /// The cursor of the older page.
    #[serde(default)]
    cursor: Option<String>,
    /// How many rows the caller wants.
    #[serde(default)]
    limit: Option<usize>,
}

/// Parses a procedure body.
///
/// An unknown field is refused: the schema is strict, so a client that sends a field the
/// deployment does not know about hears about it instead of having it silently dropped
/// (spec/03-api-contract.md §3.1).
fn parse<T: serde::de::DeserializeOwned>(body: &Bytes) -> Result<T, ApiError> {
    serde_json::from_slice(body).map_err(|error| {
        tracing::debug!(%error, "a procedure body did not match its schema");
        ApiError::bad_request()
    })
}

/// The header a client sends to choose the space.
pub const SPACE_HEADER: &str = "x-space-id";

/// The header the space can also be named with, spelled with the product prefix.
pub const SPACE_HEADER_PREFIXED: &str = "x-boma-space-id";

/// The cookie that carries the session.
pub const SESSION_COOKIE: &str = "session";

/// Reads the space a caller acts in from the headers.
#[must_use]
pub fn requested_space(headers: &axum::http::HeaderMap) -> Option<String> {
    headers
        .get(SPACE_HEADER)
        .or_else(|| headers.get(SPACE_HEADER_PREFIXED))
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
}

// Secrets for bots (REQ-046). Member-side only: the value the member sends is write-only, it is
// sealed under the space's data-encryption key and is never echoed back — not in a reply, not in
// an error, not in a log line. The list answers names and timestamps only.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SecretsSetRequest {
    /// The bot the secret belongs to.
    bot_id: String,
    /// The name the secret is stored under; must look like an environment variable.
    name: String,
    /// The value to seal. Plaintext in the wire body, ciphertext at rest.
    value: String,
}

/// `botSecrets.set` — seal and store a value under a bot-scoped name (REQ-046).
async fn secrets_set(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<SecretSetReply, ApiError> {
    let request: SecretsSetRequest = parse(&body)?;
    usecases::bot_secret_set(
        state.app(),
        &auth.actor.context,
        &request.bot_id,
        &request.name,
        secrecy::SecretString::from(request.value),
    )
    .await?;
    Ok(SecretSetReply { name: request.name })
}

/// The body of `botSecrets.list` and of a bot-scoped forget (REQ-046).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SecretsBotRequest {
    /// The bot whose secrets to answer for.
    bot_id: String,
}

/// `botSecrets.list` — the names of the bot's secrets, newest first (REQ-046).
async fn secrets_list(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<SecretsReply, ApiError> {
    let request: SecretsBotRequest = parse(&body)?;
    let summaries =
        usecases::bot_secret_list(state.app(), &auth.actor.context, &request.bot_id).await?;
    Ok(SecretsReply {
        secrets: summaries
            .into_iter()
            .map(|summary| SecretView {
                name: summary.name,
                created_at: summary.created_at,
            })
            .collect(),
    })
}

/// The body of `botSecrets.forget` (REQ-046).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SecretsForgetRequest {
    /// The bot whose secret is forgotten.
    bot_id: String,
    /// The name to forget.
    name: String,
}

/// `botSecrets.forget` — remove a secret; forgetting a missing one succeeds (REQ-046).
async fn secrets_forget(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<SecretForgetReply, ApiError> {
    let request: SecretsForgetRequest = parse(&body)?;
    let removed = usecases::bot_secret_forget(
        state.app(),
        &auth.actor.context,
        &request.bot_id,
        &request.name,
    )
    .await?;
    Ok(SecretForgetReply {
        name: request.name,
        removed,
    })
}

/// The body of `skills.list` (REQ-035): nothing, `{}`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SkillsListRequest {}

/// `skills.list` — the built-ins and the space's own, by name.
async fn skills_list(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<SkillsReply, ApiError> {
    let _: SkillsListRequest = parse(&body)?;
    let skills = usecases::skills_list(state.app(), &auth.actor.context).await?;
    Ok(SkillsReply {
        skills: skills
            .into_iter()
            .map(|summary| SkillView {
                name: summary.name,
                description: summary.description,
                origin: summary.origin,
                plugin_source: summary.plugin_source,
                created_at: summary.created_at,
            })
            .collect(),
    })
}

/// The body of `skills.get` (REQ-035).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SkillsGetRequest {
    /// The wire name to fetch.
    name: String,
}

/// `skills.get` — one skill with its instructions.
async fn skills_get(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<SkillReply, ApiError> {
    let request: SkillsGetRequest = parse(&body)?;
    let skill = usecases::skill_get(state.app(), &auth.actor.context, &request.name).await?;
    Ok(SkillReply {
        name: skill.name,
        description: skill.description,
        instructions: skill.instructions,
        origin: skill.origin,
        plugin_source: skill.plugin_source,
    })
}

/// The body of `skills.teach` (REQ-035).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SkillsTeachRequest {
    /// The wire name the bot will call.
    name: String,
    /// One line a person reads.
    description: String,
    /// The instructions handed to the model.
    instructions: String,
}

/// `skills.teach` — teach the space a skill; a built-in's name is refused.
async fn skills_teach(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<TeachSkillReply, ApiError> {
    let request: SkillsTeachRequest = parse(&body)?;
    usecases::teach_skill(
        state.app(),
        &auth.actor.context,
        &request.name,
        &request.description,
        &request.instructions,
        boma_domain::SkillOrigin::Taught,
        None,
    )
    .await?;
    Ok(TeachSkillReply { name: request.name })
}

/// The body of `mcp.servers.list` (REQ-033): nothing, `{}`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct McpServersListRequest {}

fn mcp_view(summary: boma_app::ports::McpServerSummary) -> McpServerView {
    McpServerView {
        id: summary.id,
        name: summary.name,
        transport: summary.transport,
        url: summary.url,
        command: summary.command,
        auth_state: summary.auth_state,
        allow_private: summary.allow_private,
        oauth_configured: summary.oauth_configured,
        tool_count: summary.tool_count,
        created_at: summary.created_at,
    }
}

/// `mcp.servers.list` — the servers of the space, name ascending.
async fn mcp_servers_list(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<McpServersListReply, ApiError> {
    let _: McpServersListRequest = parse(&body)?;
    let servers = usecases::mcp_servers_list(state.app(), &auth.actor.context).await?;
    Ok(McpServersListReply {
        servers: servers.into_iter().map(mcp_view).collect(),
    })
}

/// The OAuth endpoints a member registers for a server (REQ-033).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct McpOAuthConfigInput {
    /// The authorization endpoint the member's browser opens.
    authorization_url: String,
    /// The token endpoint the exchange calls.
    token_url: String,
    /// The client id the member registered at the server.
    client_id: String,
    /// The client secret, when the server issues confidential ones.
    client_secret: Option<String>,
}

/// The body of `mcp.servers.add` (REQ-033).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct McpServersAddRequest {
    /// The member's display name for the server.
    name: String,
    /// `http` or `stdio`. Local command servers are refused in this build.
    transport: String,
    /// The HTTPS URL, for `http`.
    url: Option<String>,
    /// The command, for `stdio`. Always refused by this build.
    command: Option<String>,
    /// The bearer token, sealed per space at rest.
    bearer_token: Option<String>,
    /// The member's explicit allowance for a private endpoint (defect D-05).
    #[serde(default)]
    allow_private: bool,
    /// The OAuth endpoints, when the server uses a registered client.
    oauth: Option<McpOAuthConfigInput>,
}

/// `mcp.servers.add` — register a server; a stdio or otherwise refused registration fails with why.
async fn mcp_servers_add(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<McpServerReply, ApiError> {
    let request: McpServersAddRequest = parse(&body)?;
    let transport = request
        .transport
        .parse::<boma_domain::McpTransport>()
        .map_err(|error| ApiError::from(boma_app::AppError::from(error)))?;
    let summary = usecases::mcp_server_add(
        state.app(),
        &auth.actor.context,
        &request.name,
        transport,
        request.url,
        request.command,
        request.bearer_token.map(secrecy::SecretString::from),
        request.allow_private,
        request.oauth.map(|oauth| boma_app::ports::McpOAuthConfig {
            authorization_url: oauth.authorization_url,
            token_url: oauth.token_url,
            client_id: oauth.client_id,
            client_secret: oauth.client_secret.map(secrecy::SecretString::from),
        }),
    )
    .await?;
    Ok(McpServerReply {
        server: mcp_view(summary),
    })
}

/// The body of `mcp.servers.remove` and `mcp.servers.probe` (REQ-033).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct McpServersServerRequest {
    /// The server's id.
    server_id: String,
}

/// `mcp.servers.remove` — remove a server; removing a missing one is a success.
async fn mcp_servers_remove(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<McpServersRemoveReply, ApiError> {
    let request: McpServersServerRequest = parse(&body)?;
    let removed =
        usecases::mcp_server_remove(state.app(), &auth.actor.context, &request.server_id).await?;
    Ok(McpServersRemoveReply { removed })
}

/// `mcp.servers.probe` — initialize and discover the server's tools.
async fn mcp_servers_probe(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<McpServerReply, ApiError> {
    let request: McpServersServerRequest = parse(&body)?;
    let summary =
        usecases::mcp_server_probe(state.app(), &auth.actor.context, &request.server_id).await?;
    Ok(McpServerReply {
        server: mcp_view(summary),
    })
}

/// The body of `connections.list` (REQ-032): nothing, `{}`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ConnectionsListRequest {}

fn connector_view(summary: boma_app::ports::ConnectorSummary) -> ConnectorView {
    ConnectorView {
        connector_id: summary.connector_id,
        name: summary.name,
        description: summary.description,
        auth_kind: summary.auth_kind,
        installed: summary.installed,
    }
}

/// `connections.list` — the catalogue with this space's install state.
async fn connections_list(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<ConnectionsListReply, ApiError> {
    let _: ConnectionsListRequest = parse(&body)?;
    let connectors = usecases::connections_list(state.app(), &auth.actor.context).await?;
    Ok(ConnectionsListReply {
        connections: connectors.into_iter().map(connector_view).collect(),
    })
}

/// The body of `connections.install` (REQ-032).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ConnectionsInstallRequest {
    /// The catalogue id to install.
    connector_id: String,
    /// The bearer key for an `api_key` connector. Never returned by any API.
    #[serde(default)]
    api_key: Option<String>,
}

/// `connections.install` — install or re-key a connector.
async fn connections_install(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<ConnectionsInstallReply, ApiError> {
    let request: ConnectionsInstallRequest = parse(&body)?;
    let connector = usecases::connections_install(
        state.app(),
        &auth.actor.context,
        &request.connector_id,
        request.api_key.map(secrecy::SecretString::from),
    )
    .await?;
    Ok(ConnectionsInstallReply {
        connector: connector_view(connector),
    })
}

/// The body of `connections.remove` (REQ-032).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ConnectionsRemoveRequest {
    /// The catalogue id to revoke.
    connector_id: String,
}

/// `connections.remove` — revoke the installation; revoking a missing one is a success.
async fn connections_remove(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<ConnectionsRemoveReply, ApiError> {
    let request: ConnectionsRemoveRequest = parse(&body)?;
    let removed =
        usecases::connections_remove(state.app(), &auth.actor.context, &request.connector_id)
            .await?;
    Ok(ConnectionsRemoveReply { removed })
}

/// The body of `marketplace.list` (REQ-034): nothing, `{}`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MarketplaceListRequest {}

fn plugin_install_view(outcome: boma_app::ports::PluginInstallOutcome) -> PluginInstallView {
    PluginInstallView {
        name: outcome.name,
        source: outcome.source,
        resolved_sha: outcome.resolved_sha,
        skill_count: outcome.skill_count,
        mcp_server_count: outcome.mcp_server_count,
        skipped: outcome.skipped,
        created_at: String::new(),
    }
}

/// `marketplace.list` — the catalogue and this space's installs.
async fn marketplace_list(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<MarketplaceListReply, ApiError> {
    let _: MarketplaceListRequest = parse(&body)?;
    let (connectors, installed) =
        usecases::marketplace_list(state.app(), &auth.actor.context).await?;
    Ok(MarketplaceListReply {
        connectors: connectors.into_iter().map(connector_view).collect(),
        installed_plugins: installed
            .into_iter()
            .map(|summary| PluginInstallView {
                name: summary.name,
                source: summary.source,
                resolved_sha: summary.resolved_sha,
                skill_count: summary
                    .parts
                    .get("skills")
                    .and_then(serde_json::Value::as_u64)
                    .map_or(0, |count| u32::try_from(count).unwrap_or(u32::MAX)),
                mcp_server_count: summary
                    .parts
                    .get("mcpServers")
                    .and_then(serde_json::Value::as_u64)
                    .map_or(0, |count| u32::try_from(count).unwrap_or(u32::MAX)),
                skipped: summary.skipped.as_array().cloned().unwrap_or_default(),
                created_at: summary.created_at,
            })
            .collect(),
    })
}

/// The body of `marketplace.submit` (REQ-034).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MarketplaceSubmitRequest {
    /// The source reference; the reply carries an id and nothing about who submitted.
    source: String,
}

/// `marketplace.submit` — submit a plugin for review (authed, ten per hour per submitter).
async fn marketplace_submit(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<MarketplaceSubmitReply, ApiError> {
    let request: MarketplaceSubmitRequest = parse(&body)?;
    let id =
        usecases::marketplace_submit(state.app(), &auth.actor.context, &request.source).await?;
    Ok(MarketplaceSubmitReply { ok: true, id })
}

/// The body of `marketplace.install` (REQ-034).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MarketplaceInstallRequest {
    /// The source reference.
    source: String,
}

/// `marketplace.install` — fetch, validate and install; the no-local-code rule applies.
async fn marketplace_install(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<MarketplaceInstallReply, ApiError> {
    let request: MarketplaceInstallRequest = parse(&body)?;
    let outcome =
        usecases::marketplace_install(state.app(), &auth.actor.context, &request.source).await?;
    Ok(MarketplaceInstallReply {
        install: plugin_install_view(outcome),
    })
}

/// The body of `mcp.oauth.start` (REQ-033).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct McpOAuthStartRequest {
    /// The server to sign in to.
    server_id: String,
}

/// `mcp.oauth.start` — open the sign-in URL for a remote server with OAuth.
async fn mcp_oauth_start(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<McpOAuthStartReply, ApiError> {
    let request: McpOAuthStartRequest = parse(&body)?;
    let started =
        usecases::mcp_oauth_start(state.app(), &auth.actor.context, &request.server_id).await?;
    Ok(McpOAuthStartReply {
        authorization_url: started.authorization_url,
        state: started.state,
    })
}

/// The body of `mcp.oauth.complete` (REQ-033).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct McpOAuthCompleteRequest {
    /// The server to sign in to.
    server_id: String,
    /// The `code` from the pasted callback URL.
    code: String,
    /// The `state` from the pasted callback URL.
    state: String,
}

/// `mcp.oauth.complete` — exchange the code for a token, seal it, discover the tools.
async fn mcp_oauth_complete(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<McpServerReply, ApiError> {
    let request: McpOAuthCompleteRequest = parse(&body)?;
    let summary = usecases::mcp_oauth_complete(
        state.app(),
        &auth.actor.context,
        &request.server_id,
        &request.code,
        &request.state,
    )
    .await?;
    Ok(McpServerReply {
        server: mcp_view(summary),
    })
}
