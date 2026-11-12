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
        AnswerReply, BotReply, BotsListReply, CancelReply, InvitationReply, MemoryDocumentReply,
        MemoryDocumentsReply, MemoryProviderReply, MemoryRemoveReply, MemoryWriteReply,
        MessagesReply, ReactReply, RosterBotView, RunRetryReply, RunView, RunsReply, SendReply,
        SnoozeReply, SpaceReply, ThreadHeadReply,
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
