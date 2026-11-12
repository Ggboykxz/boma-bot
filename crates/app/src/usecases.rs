//! The use cases: what the product does, composed out of pure rules and ports.
//!
//! Every function here takes the [`App`] (its ports) and the caller's tenant context. Nothing
//! here knows about HTTP, SQL or the database: the API layer maps errors, the infra layer
//! implements the ports, and the domain holds the rules.
//!
//! The M1 vocabulary (REQ-001, REQ-002, REQ-005, REQ-012, REQ-020, REQ-040, REQ-041, REQ-042,
//! REQ-043):
//!
//! * [`sign_up`] and [`sign_in`] mint a session and hand back an opaque token;
//! * [`create_space`] writes a space and the caller's membership in one transaction;
//! * [`create_bot`] writes a bot *and* its thread in one transaction;
//! * [`send_message`] writes one message and one run, and replays the same answer for the same
//!   client nonce;
//! * [`page_messages`] returns the newest messages first, with a cursor;
//! * [`list_runs`] projects runs into the activity list;
//! * [`cancel_run`] stops a run: once it reaches `cancelled`, nothing lands on it again
//!   (REQ-006).

use boma_domain::{
    Bot, CardId, CardStatus, Email, EventType, Message, MessagePage, Run, RunStatus, RunTrigger,
    Space, SpaceRole, TenantContext, Thread, ToolSpec, User, parse_cursor, validate_message_text,
};

pub use crate::accounts::{SignIn, SignUp};

use crate::{
    accounts::Accounts,
    error::AppError,
    ports::{AnswerCardOutcome, NewBot, NewEffect, NewMessage, NewRun, NewThread, ReactionAction},
    service::{Actor, App, SendOutcome},
};

/// The account service, built from an application.
#[must_use]
pub fn accounts(app: &App, session_ttl_seconds: i64) -> Accounts {
    Accounts::new(app.clone(), session_ttl_seconds)
}

/// Creates an account and signs the person in.
///
/// # Errors
///
/// See [`Accounts::sign_up`].
pub async fn sign_up(
    app: &App,
    session_ttl_seconds: i64,
    request: &SignUp,
) -> Result<crate::accounts::SessionGrant, AppError> {
    accounts(app, session_ttl_seconds).sign_up(request).await
}

/// Signs an existing person in.
///
/// # Errors
///
/// See [`Accounts::sign_in`].
pub async fn sign_in(
    app: &App,
    session_ttl_seconds: i64,
    request: &SignIn,
) -> Result<crate::accounts::SessionGrant, AppError> {
    accounts(app, session_ttl_seconds).sign_in(request).await
}

/// Resolves a session token into the person it belongs to.
///
/// An expired or unknown token is `None`, never an error: the caller is simply not signed in.
///
/// # Errors
///
/// Returns [`AppError::Unavailable`] when the store cannot answer.
pub async fn resolve_session(app: &App, token: &str) -> Result<Option<(User, String)>, AppError> {
    let hash = app.services.tokens.hash(token)?;
    let Some(session) = app.stores.sessions.find_by_token_hash(&hash).await? else {
        return Ok(None);
    };
    if session.expires_at.as_str() <= app.services.clock.now().as_str() {
        return Ok(None);
    }
    let Some(user) = app.stores.users.find(&session.user_id).await? else {
        return Ok(None);
    };
    if user.deactivated_at.is_some() {
        return Ok(None);
    }
    Ok(Some((user, session.expires_at)))
}

/// The space a caller acts in, and the context that proves it.
///
/// The header wins when it names a space the caller belongs to; otherwise the caller's default
/// space is used. A space the caller does not belong to is *not found*, never forbidden: the
/// difference would confirm that the space exists (REQ-043).
///
/// # Errors
///
/// Returns [`AppError::NotFound`] when the caller belongs to no space at all.
pub async fn resolve_actor(
    app: &App,
    user: &User,
    requested_space: Option<&str>,
) -> Result<Actor, AppError> {
    let space = match requested_space {
        Some(space_id) => {
            let space_id =
                boma_domain::SpaceId::new(space_id.to_owned()).map_err(|_| AppError::NotFound)?;
            app.stores
                .spaces
                .find(&space_id, &user.id)
                .await?
                .ok_or(AppError::NotFound)?
        }
        None => app
            .stores
            .spaces
            .default_for(&user.id)
            .await?
            .ok_or(AppError::NotFound)?,
    };

    Ok(Actor {
        user: user.clone(),
        context: TenantContext::Member {
            space_id: space.id.clone(),
            user_id: user.id.clone(),
        },
    })
}

/// A space, plus the role the caller has in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpaceCreated {
    /// The space.
    pub space: Space,
    /// The caller's role in it.
    pub role: SpaceRole,
}

/// Creates a space and makes the caller its owner.
///
/// The membership is written in the same transaction as the space: a space nobody belongs to is
/// invisible (REQ-042).
///
/// # Errors
///
/// Returns [`AppError::Rule`] when the name is refused, and [`AppError::Unavailable`] when the
/// store cannot answer.
pub async fn create_space(
    app: &App,
    context: &TenantContext,
    name: &str,
) -> Result<SpaceCreated, AppError> {
    Space::validate(name)?;
    let name = name.trim().to_owned();
    let organization_id = organization_of(app, context).await?;
    let created = app
        .stores
        .spaces
        .create(
            crate::ports::NewSpace {
                id: boma_domain::SpaceId::new(app.services.ids.new_id("spc"))?,
                organization_id,
                name,
                is_default: false,
                created_at: app.services.clock.now(),
            },
            &context.user_id().cloned().ok_or(AppError::NotFound)?,
        )
        .await?;
    Ok(SpaceCreated {
        space: created,
        role: SpaceRole::Owner,
    })
}

/// The organization the caller's spaces belong to.
async fn organization_of(
    app: &App,
    context: &TenantContext,
) -> Result<boma_domain::OrganizationId, AppError> {
    let space_id = context.space_id().ok_or(AppError::NotFound)?;
    let user_id = context.user_id().cloned().ok_or(AppError::NotFound)?;
    let space = app
        .stores
        .spaces
        .find(space_id, &user_id)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(space.organization_id)
}

/// An invitation, as the caller sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvitationCreated {
    /// The invitation's id.
    pub id: boma_domain::InviteId,
    /// When it expires.
    pub expires_at: String,
}

/// Invites somebody into the caller's space.
///
/// The token never leaves this function: it is hashed and stored, and only the hash is kept
/// (REQ-042).
///
/// # Errors
///
/// Returns [`AppError::Rule`] when the address is refused, and [`AppError::Unavailable`] when the
/// store cannot answer.
pub async fn invite(
    app: &App,
    context: &TenantContext,
    email: &str,
    role: SpaceRole,
    ttl_seconds: i64,
) -> Result<InvitationCreated, AppError> {
    let email = Email::new(email.to_owned())?;
    let space_id = context.space_id().ok_or(AppError::NotFound)?;
    let token = app.services.tokens.generate()?;
    let token_hash = app.services.tokens.hash(&token)?;
    let invitation = app
        .stores
        .invitations
        .create(crate::ports::NewInvitation {
            id: boma_domain::InviteId::new(app.services.ids.new_id("inv"))?,
            space_id: space_id.clone(),
            email,
            token_hash,
            role,
            expires_at: app.services.clock.now_plus_seconds(ttl_seconds),
            created_at: app.services.clock.now(),
        })
        .await?;
    Ok(InvitationCreated {
        id: invitation.id,
        expires_at: invitation.expires_at,
    })
}

/// A bot, its thread, and what the caller asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BotCreated {
    /// The bot.
    pub bot: Bot,
    /// Its conversation.
    pub thread: Thread,
}

/// Creates a bot and its thread, in one transaction.
///
/// A bot without a conversation is a half-created bot, so the two rows are written together
/// (REQ-004, REQ-042).
///
/// # Errors
///
/// Returns [`AppError::Rule`] when the name is refused, and [`AppError::Unavailable`] when the
/// store cannot answer.
pub async fn create_bot(
    app: &App,
    context: &TenantContext,
    name: &str,
    description: Option<String>,
    color: Option<String>,
) -> Result<BotCreated, AppError> {
    Bot::validate_name(name)?;
    let name = name.trim().to_owned();
    let space_id = context.space_id().ok_or(AppError::NotFound)?;
    let user_id = context.user_id().cloned().ok_or(AppError::NotFound)?;
    let now = app.services.clock.now();
    let bot_id = boma_domain::BotId::new(app.services.ids.new_id("bot"))?;
    let thread_id = boma_domain::ThreadId::new(app.services.ids.new_id("thr"))?;

    let (bot, thread) = app
        .stores
        .bots
        .create_with_thread(
            NewBot {
                id: bot_id.clone(),
                space_id: space_id.clone(),
                user_id: user_id.clone(),
                name,
                description,
                color,
                created_at: now.clone(),
            },
            NewThread {
                id: thread_id,
                space_id: space_id.clone(),
                bot_id,
                user_id,
                created_at: now,
            },
        )
        .await?;

    Ok(BotCreated { bot, thread })
}

/// Finds a bot inside the caller's tenant.
///
/// A bot in another tenant is not found (REQ-043).
///
/// # Errors
///
/// Returns [`AppError::Unavailable`] when the store cannot answer.
pub async fn find_bot(app: &App, context: &TenantContext, bot_id: &str) -> Result<Bot, AppError> {
    let bot_id = boma_domain::BotId::new(bot_id.to_owned()).map_err(|_| AppError::NotFound)?;
    app.stores
        .bots
        .find(context, &bot_id)
        .await?
        .ok_or(AppError::NotFound)
}

/// A message to send.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SendMessage {
    /// The bot that receives it.
    pub bot_id: String,
    /// What the person wrote.
    pub text: String,
    /// The nonce that makes the send idempotent.
    pub client_nonce: Option<String>,
    /// The message this one answers, when it is a reply.
    pub reply_to_message_id: Option<String>,
    /// A short quote of the message being answered.
    pub reply_quote: Option<String>,
}

/// Picks the model for one run, from the bot's pinned or chosen model (REQ-015).
///
/// # Errors
///
/// Returns [`AppError::Rule`] when the pinned model is one this deployment cannot reach.
fn model_choice_for(
    bot: &boma_domain::Bot,
    models: &boma_domain::model::DeploymentModels,
) -> Result<boma_domain::model::ModelChoice, AppError> {
    let preferences = boma_domain::model::ModelPreferences {
        bot: bot.model_id.clone().map(|model| {
            let provider = bot
                .model_provider
                .clone()
                .unwrap_or_else(|| "custom".to_owned());
            (format!("{provider}/{model}"), bot.model_pinned)
        }),
        space: None,
        person: None,
    };
    Ok(boma_domain::model::resolve(&preferences, models)?)
}

/// Sends a message to a bot and starts the run that answers it.
///
/// The send returns before any model call: one message row and one queued run, then the worker
/// picks the run up (REQ-001). The same `clientNonce` returns the same message and the same run,
/// so a client that retries after a timeout does not create a second turn (REQ-002).
///
/// # Errors
///
/// Returns [`AppError::NotFound`] when the bot does not exist in the caller's tenant,
/// [`AppError::Rule`] when the text is refused, and [`AppError::Unavailable`] when a store cannot
/// answer.
pub async fn send_message(
    app: &App,
    context: &TenantContext,
    request: &SendMessage,
) -> Result<SendOutcome, AppError> {
    validate_message_text(&request.text)?;

    let bot_id = boma_domain::BotId::new(request.bot_id.clone()).map_err(|_| AppError::NotFound)?;
    let bot = app
        .stores
        .bots
        .find(context, &bot_id)
        .await?
        .ok_or(AppError::NotFound)?;
    if bot.is_archived() {
        return Err(AppError::NotFound);
    }
    let thread = app
        .stores
        .threads
        .for_bot(context, &bot.id)
        .await?
        .ok_or(AppError::NotFound)?;
    let space_id = context.space_id().ok_or(AppError::NotFound)?.clone();
    let user_id = context.user_id().cloned().ok_or(AppError::NotFound)?;
    let now = app.services.clock.now();

    // The message is written first; a reply carries the reference and the quote together.
    let reply_to_message_id = request
        .reply_to_message_id
        .clone()
        .map(|id| boma_domain::MessageId::new(id).map_err(|_| AppError::NotFound))
        .transpose()?;
    let reply_quote = request.reply_quote.clone();

    let message = app
        .stores
        .messages
        .insert(
            context,
            NewMessage {
                id: boma_domain::MessageId::new(app.services.ids.new_id("msg"))?,
                space_id: space_id.clone(),
                thread_id: thread.id.clone(),
                role: boma_domain::MessageRole::User,
                blocks: vec![boma_domain::Block::Text {
                    text: request.text.clone(),
                }],
                bot_id: None,
                author_user_id: Some(user_id.clone()),
                run_id: None,
                client_nonce: request.client_nonce.clone(),
                created_at: now.clone(),
                reply_to_message_id,
                reply_quote,
            },
        )
        .await?;

    // A conversational run that is already working takes this message as steering input instead
    // of queueing a second run behind it (REQ-003). The message is in the thread, the run is
    // marked, and the client sees one working run.
    if let Some(active) = app
        .stores
        .runs
        .find_active_for_thread(context, &thread.id)
        .await?
    {
        app.stores
            .runs
            .set_steering_from(context, &active.id, message.message.seq)
            .await?;
        return Ok(SendOutcome {
            message: message.message,
            run: active,
        });
    }

    // The model is picked before the run exists (REQ-015): a pinned choice the deployment cannot
    // reach fails visibly instead of silently running elsewhere.
    let choice = model_choice_for(&bot, &app.services.models)?;

    let run = app
        .stores
        .runs
        .create(NewRun {
            id: boma_domain::RunId::new(app.services.ids.new_id("run"))?,
            space_id,
            bot_id: bot.id,
            thread_id: thread.id,
            user_id,
            trigger: RunTrigger::User,
            client_nonce: request.client_nonce.clone(),
            source_message_id: Some(message.message.id.clone()),
            created_at: now,
            model_provider: Some(choice.provider),
            model_id: Some(choice.model),
            model_pinned: choice.pinned,
            parent_run_id: None,
        })
        .await?;

    Ok(SendOutcome {
        message: message.message,
        run,
    })
}
/// A page of the thread's history, newest first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessagesPage {
    /// The messages.
    pub messages: Vec<Message>,
    /// The cursor for the older page, when there is one.
    pub next_cursor: Option<String>,
}

/// Returns a page of the thread's history.
///
/// The page is ordered by the thread's sequence, newest first, never by insertion time: `seq` is
/// the only ordering a client may trust (REQ-005).
///
/// # Errors
///
/// Returns [`AppError::Rule`] when the cursor or the page size is refused, and
/// [`AppError::Unavailable`] when the store cannot answer.
pub async fn page_messages(
    app: &App,
    context: &TenantContext,
    thread_id: &str,
    cursor: Option<&str>,
    limit: Option<usize>,
) -> Result<MessagesPage, AppError> {
    let thread_id =
        boma_domain::ThreadId::new(thread_id.to_owned()).map_err(|_| AppError::NotFound)?;
    let before = cursor.map(parse_cursor).transpose()?;
    let limit = limit.unwrap_or(boma_domain::thread::DEFAULT_PAGE_SIZE);
    boma_domain::thread::validate_page_size(limit)?;
    let page: MessagePage = app
        .stores
        .messages
        .page(context, &thread_id, before, limit)
        .await?;
    let messages = hydrate_reactions(app, context, page.messages).await?;
    Ok(MessagesPage {
        messages,
        next_cursor: page.next_cursor,
    })
}

/// Attaches the reactions a page of messages carries (REQ-008).
///
/// One attribution per page: the caller sees, for each emoji, how many people reacted and
/// whether they did.
async fn hydrate_reactions(
    app: &App,
    context: &TenantContext,
    mut messages: Vec<boma_domain::Message>,
) -> Result<Vec<boma_domain::Message>, AppError> {
    if messages.is_empty() {
        return Ok(messages);
    }
    let ids: Vec<boma_domain::MessageId> =
        messages.iter().map(|message| message.id.clone()).collect();
    let reactions = app
        .stores
        .reactions
        .list_for_messages(context, &ids)
        .await?;
    for message in &mut messages {
        if let Some(list) = reactions.get(&message.id) {
            message.reactions = list.clone();
        }
    }
    Ok(messages)
}

/// The head of a thread as the caller sees it (REQ-009).
///
/// `lastSeq` is the highest sequence the thread has allocated, `readSeq` is the caller's
/// position in it and `unreadCount` counts the visible messages above it. Background steps
/// (role `tool`) are not counted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadHead {
    /// The thread's id.
    pub thread_id: String,
    /// The highest sequence the thread has allocated.
    pub last_seq: i64,
    /// When the newest message was written, when there is one.
    pub last_message_at: Option<String>,
    /// The caller's read position.
    pub read_seq: i64,
    /// How many visible messages sit above the caller's position.
    pub unread_count: u64,
}

/// Reads the head of a thread for the caller (REQ-009).
///
/// # Errors
///
/// Returns [`AppError::NotFound`] when the thread does not exist in the caller's tenant.
pub async fn thread_head(
    app: &App,
    context: &TenantContext,
    thread_id: &str,
) -> Result<ThreadHead, AppError> {
    head_with(app, context, thread_id, None).await
}

/// Marks the caller's read position to the newest message of the thread (REQ-009).
///
/// # Errors
///
/// Returns [`AppError::NotFound`] when the thread does not exist in the caller's tenant.
pub async fn mark_read(
    app: &App,
    context: &TenantContext,
    thread_id: &str,
) -> Result<ThreadHead, AppError> {
    head_with(app, context, thread_id, Some(true)).await
}

/// Marks the caller's read position to zero (REQ-009).
///
/// # Errors
///
/// Returns [`AppError::NotFound`] when the thread does not exist in the caller's tenant.
pub async fn mark_unread(
    app: &App,
    context: &TenantContext,
    thread_id: &str,
) -> Result<ThreadHead, AppError> {
    head_with(app, context, thread_id, Some(false)).await
}

/// The head a caller sees; `mark` decides whether to move the read position first.
async fn head_with(
    app: &App,
    context: &TenantContext,
    thread_id: &str,
    mark: Option<bool>,
) -> Result<ThreadHead, AppError> {
    let thread_id =
        boma_domain::ThreadId::new(thread_id.to_owned()).map_err(|_| AppError::NotFound)?;
    let thread = app
        .stores
        .threads
        .find(context, &thread_id)
        .await?
        .ok_or(AppError::NotFound)?;
    // The thread must belong to the caller's space. The store's own guards are the defense
    // in fact for the writes; this check is the same for the reads, and holds even where
    // the row level security does not apply (a superuser connection).
    if let Some(space_id) = context.space_id()
        && thread.space_id != *space_id
    {
        return Err(AppError::NotFound);
    }
    let page = app
        .stores
        .messages
        .page(context, &thread_id, None, 1)
        .await?;
    let last = page.messages.first();
    let last_seq = last.map_or(0, |message| message.seq);
    let last_message_at = last.map(|message| message.created_at.clone());
    let read_seq = match mark {
        Some(true) => {
            app.stores
                .read_states
                .set(context, &thread_id, last_seq)
                .await?
        }
        Some(false) => app.stores.read_states.set(context, &thread_id, 0).await?,
        None => app.stores.read_states.position(context, &thread_id).await?,
    };
    let unread_count = app
        .stores
        .messages
        .count_above(context, &thread_id, read_seq)
        .await?;
    Ok(ThreadHead {
        thread_id: thread_id.as_str().to_owned(),
        last_seq,
        last_message_at,
        read_seq,
        unread_count,
    })
}

/// One memory document written (REQ-022).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryWriteOutcome {
    /// The document as it stands after the write.
    pub document: boma_domain::MemoryDocument,
}

/// The outcome of a memory removal (REQ-022).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryRemoveOutcome {
    /// The path that was removed.
    pub path: String,
    /// Whether anything was there.
    pub removed: bool,
}

/// Writes a memory document: one appended revision, the projection updated (REQ-022).
///
/// # Errors
///
/// Returns [`AppError::Rule`] when the scope or the path is refused, and [`AppError::NotFound`]
/// when scope `bot` names a bot that does not exist in the caller's tenant.
pub async fn memory_write(
    app: &App,
    context: &TenantContext,
    scope: &str,
    bot_id: Option<&str>,
    path: &str,
    text: &str,
) -> Result<MemoryWriteOutcome, AppError> {
    let (scope, bot_id) = resolve_memory_scope(app, context, scope, bot_id).await?;
    let _ = scope; // the store derives the scope from the owner id
    let document = app
        .stores
        .memory
        .write(
            context,
            crate::ports::NewMemoryDocument {
                bot_id,
                path: path.to_owned(),
                content: text.to_owned(),
                updated_at: app.services.clock.now(),
            },
        )
        .await?;
    Ok(MemoryWriteOutcome { document })
}

/// Reads one memory document (REQ-022).
///
/// # Errors
///
/// Returns [`AppError::NotFound`] when it does not exist in the caller's tenant, or when scope
/// `bot` names a bot the caller cannot see.
pub async fn memory_read(
    app: &App,
    context: &TenantContext,
    scope: &str,
    bot_id: Option<&str>,
    path: &str,
) -> Result<boma_domain::MemoryDocument, AppError> {
    let (_, bot_id) = resolve_memory_scope(app, context, scope, bot_id).await?;
    app.stores
        .memory
        .read(context, bot_id.as_ref(), path)
        .await?
        .ok_or(AppError::NotFound)
}

/// Lists the documents of a bot, or of the space-shared memory (REQ-022).
///
/// # Errors
///
/// Returns [`AppError::NotFound`] when scope `bot` names a bot the caller cannot see.
pub async fn memory_list(
    app: &App,
    context: &TenantContext,
    scope: &str,
    bot_id: Option<&str>,
) -> Result<Vec<boma_domain::MemoryDocument>, AppError> {
    let (_, bot_id) = resolve_memory_scope(app, context, scope, bot_id).await?;
    app.stores.memory.list(context, bot_id.as_ref()).await
}

/// Removes a memory document and its revision chain (REQ-022).
///
/// # Errors
///
/// Returns [`AppError::NotFound`] when scope `bot` names a bot the caller cannot see.
pub async fn memory_remove(
    app: &App,
    context: &TenantContext,
    scope: &str,
    bot_id: Option<&str>,
    path: &str,
) -> Result<MemoryRemoveOutcome, AppError> {
    let (_, bot_id) = resolve_memory_scope(app, context, scope, bot_id).await?;
    let removed = app
        .stores
        .memory
        .remove(context, bot_id.as_ref(), path)
        .await?;
    Ok(MemoryRemoveOutcome {
        path: path.to_owned(),
        removed,
    })
}

/// Resolves the `bot` or `shared` scope from the wire, checking what it names (REQ-022).
async fn resolve_memory_scope(
    app: &App,
    context: &TenantContext,
    scope: &str,
    bot_id: Option<&str>,
) -> Result<(boma_domain::MemoryScope, Option<boma_domain::BotId>), AppError> {
    match (scope, bot_id) {
        ("bot", Some(id)) => {
            let id = boma_domain::BotId::new(id.to_owned()).map_err(|_| {
                AppError::Rule(boma_domain::DomainError::InvalidName(
                    "the bot id is invalid".to_owned(),
                ))
            })?;
            app.stores
                .bots
                .find(context, &id)
                .await?
                .ok_or(AppError::NotFound)?;
            Ok((boma_domain::MemoryScope::Bot, Some(id)))
        }
        ("bot", None) => Err(AppError::Rule(boma_domain::DomainError::InvalidName(
            "scope bot requires a botId".to_owned(),
        ))),
        ("shared", None) => Ok((boma_domain::MemoryScope::Shared, None)),
        ("shared", Some(_)) => Err(AppError::Rule(boma_domain::DomainError::InvalidName(
            "scope shared takes no botId".to_owned(),
        ))),
        _ => Err(AppError::Rule(boma_domain::DomainError::InvalidName(
            "scope is bot or shared".to_owned(),
        ))),
    }
}

/// The outcome of one snooze call (REQ-009).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnoozeOutcome {
    /// The thread that was (or was not) snoozed.
    pub thread_id: String,
    /// The deadline stored, or `None` when the snooze was cleared.
    pub snoozed_until: Option<String>,
}

/// The roster the rail renders from (REQ-009).
///
/// # Errors
///
/// Returns an error when the database cannot answer, or when the caller is not a member.
pub async fn list_bots(
    app: &App,
    context: &TenantContext,
) -> Result<Vec<crate::ports::BotSummary>, AppError> {
    app.stores.roster.list_bots(context).await
}

/// Snoozes one conversation for the caller, or clears the snooze (REQ-009).
///
/// # Errors
///
/// Returns [`AppError::NotFound`] when the thread does not exist in the caller's tenant, and
/// [`AppError::Rule`] when `until` is not an RFC3339 instant.
pub async fn snooze_thread(
    app: &App,
    context: &TenantContext,
    thread_id: &str,
    until: Option<&str>,
) -> Result<SnoozeOutcome, AppError> {
    let thread_id =
        boma_domain::ThreadId::new(thread_id.to_owned()).map_err(|_| AppError::NotFound)?;
    if let Some(until) = until {
        validate_until(until)?;
    }
    let thread = app
        .stores
        .threads
        .find(context, &thread_id)
        .await?
        .ok_or(AppError::NotFound)?;
    if let Some(space_id) = context.space_id()
        && thread.space_id != *space_id
    {
        return Err(AppError::NotFound);
    }
    let snoozed_until = app
        .stores
        .roster
        .set_snooze(context, &thread_id, until)
        .await?;
    Ok(SnoozeOutcome {
        thread_id: thread_id.as_str().to_owned(),
        snoozed_until,
    })
}

/// Checks the `until` instant a snooze carries (REQ-009).
fn validate_until(until: &str) -> Result<(), AppError> {
    use time::OffsetDateTime;
    use time::format_description::well_known::Rfc3339;
    OffsetDateTime::parse(until, &Rfc3339)
        .map(|_| ())
        .map_err(|_| {
            AppError::Rule(boma_domain::DomainError::InvalidName(
                "until must be an RFC3339 instant".to_owned(),
            ))
        })
}

/// Re-queues a failed run for one more attempt (REQ-012).
///
/// The only transition out of a terminal state: `failed → queued`. The attempt count grows by
/// one; the fence and the lease are dropped.
///
/// # Errors
///
/// Returns [`AppError::NotFound`] when the run does not exist in the caller's tenant, and
/// [`AppError::Conflict`] when the run is not `failed`.
pub async fn retry_run(app: &App, context: &TenantContext, run_id: &str) -> Result<Run, AppError> {
    let id = boma_domain::RunId::new(run_id.to_owned()).map_err(|_| AppError::NotFound)?;
    app.stores.runs.retry(context, &id).await
}

/// The activity projection: the runs of a bot, newest first (REQ-011).
///
/// # Errors
///
/// Returns [`AppError::Rule`] when the cursor is refused, and [`AppError::Unavailable`] when the
/// store cannot answer.
pub async fn list_runs(
    app: &App,
    context: &TenantContext,
    bot_id: Option<&str>,
    cursor: Option<&str>,
    limit: Option<usize>,
) -> Result<crate::ports::ActivityPage, AppError> {
    let bot = match bot_id {
        Some(value) => {
            Some(boma_domain::BotId::new(value.to_owned()).map_err(|_| AppError::NotFound)?)
        }
        None => None,
    };
    let limit = limit.unwrap_or(boma_domain::thread::DEFAULT_PAGE_SIZE);
    boma_domain::thread::validate_page_size(limit)?;
    app.stores
        .runs
        .list(context, bot.as_ref(), cursor, limit)
        .await
}

/// What a claimed run looks like to the worker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimedRun {
    /// The run.
    pub run: Run,
    /// The bot it works for.
    pub bot: Bot,
    /// The thread it writes to.
    pub thread: Thread,
}

/// Claims the oldest queued run and starts working on it.
///
/// The claim is atomic: the run moves from `queued` to `leased` only if nobody else claimed it,
/// and the lease carries a fence, so a worker that was slow past its expiry cannot write
/// (REQ-012).
///
/// # Errors
///
/// Returns [`AppError::Unavailable`] when a store cannot answer.
pub async fn claim_run(app: &App, owner: &str) -> Result<Option<ClaimedRun>, AppError> {
    let Some(run) = app.stores.runs.claim_next_queued(owner).await? else {
        return Ok(None);
    };
    let context = TenantContext::System {
        reason: boma_domain::SystemReason::ScheduledJob,
        user_id: Some(run.user_id.clone()),
    };
    let bot = app
        .stores
        .bots
        .find(&context, &run.bot_id)
        .await?
        .ok_or(AppError::NotFound)?;
    let thread = app
        .stores
        .threads
        .for_bot(&context, &run.bot_id)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(Some(ClaimedRun { run, bot, thread }))
}

/// Stops a run: nothing further happens to it (REQ-006).
///
/// Cancelling a run that already reached a terminal state is a no-op that returns the run as it
/// is, which is what makes a retried stop safe to render. The member who stops the run sees the
/// `cancelled` status; the stream announces it.
///
/// # Errors
///
/// Returns [`AppError::NotFound`] when the run does not exist in the caller's tenant, and
/// [`AppError::Unavailable`] when a store cannot answer.
pub async fn cancel_run(app: &App, context: &TenantContext, run_id: &str) -> Result<Run, AppError> {
    let run_id = boma_domain::RunId::new(run_id.to_owned()).map_err(|_| AppError::NotFound)?;
    let run = app
        .stores
        .runs
        .find(context, &run_id)
        .await?
        .ok_or(AppError::NotFound)?;
    if run.status.is_terminal() {
        return Ok(run);
    }
    let now = app.services.clock.now();
    app.stores
        .runs
        .set_status(context, &run_id, RunStatus::Cancelled, &now, None, None)
        .await?;
    let run = app
        .stores
        .runs
        .find(context, &run_id)
        .await?
        .ok_or(AppError::Unavailable)?;
    app.stores
        .events
        .append(
            context,
            crate::ports::NewEvent {
                space_id: run.space_id.clone(),
                thread_id: run.thread_id.clone(),
                kind: EventType::Run,
                payload: serde_json::json!({
                    "runId": run.id.as_str(),
                    "status": run.status.as_str(),
                }),
                created_at: now,
            },
        )
        .await?;
    Ok(run)
}

/// Moves a run to a new status, refusing a move the state machine forbids.
///
/// # Errors
///
/// Returns [`AppError::Rule`] when the transition is forbidden, and [`AppError::Unavailable`] when
/// the store cannot answer.
pub async fn set_run_status(
    app: &App,
    context: &TenantContext,
    run_id: &str,
    status: RunStatus,
    fence: Option<i64>,
    error: Option<String>,
) -> Result<(), AppError> {
    let run_id = boma_domain::RunId::new(run_id.to_owned()).map_err(|_| AppError::NotFound)?;
    app.stores
        .runs
        .set_status(
            context,
            &run_id,
            status,
            &app.services.clock.now(),
            fence,
            error,
        )
        .await
}

/// Writes the bot's (or helper's) answer into the thread and announces it.
///
/// Helper runs write with role `tool`, so the person never sees the helper's internal
/// conversation as chat (REQ-019).
///
/// # Errors
///
/// Returns [`AppError::Unavailable`] when a store cannot answer.
pub async fn write_reply(
    app: &App,
    context: &TenantContext,
    thread: &Thread,
    bot: &Bot,
    run_id: &boma_domain::RunId,
    text: &str,
) -> Result<Message, AppError> {
    let space_id = thread.space_id.clone();
    let role = if app
        .stores
        .runs
        .find(context, run_id)
        .await?
        .is_some_and(|run| run.is_helper())
    {
        boma_domain::MessageRole::Tool
    } else {
        boma_domain::MessageRole::Bot
    };

    let stored = app
        .stores
        .messages
        .insert(
            context,
            NewMessage {
                id: boma_domain::MessageId::new(app.services.ids.new_id("msg"))?,
                space_id: space_id.clone(),
                thread_id: thread.id.clone(),
                role,
                blocks: vec![boma_domain::Block::Text {
                    text: text.to_owned(),
                }],
                bot_id: Some(bot.id.clone()),
                author_user_id: None,
                run_id: Some(run_id.clone()),
                client_nonce: None,
                created_at: app.services.clock.now(),
                reply_to_message_id: None,
                reply_quote: None,
            },
        )
        .await?;
    app.stores
        .events
        .append(
            context,
            crate::ports::NewEvent {
                space_id,
                thread_id: thread.id.clone(),
                kind: boma_domain::EventType::Message,
                payload: serde_json::json!({
                    "messageId": stored.message.id.as_str(),
                    "seq": stored.message.seq,
                }),
                created_at: app.services.clock.now(),
            },
        )
        .await?;
    Ok(stored.message)
}

/// Answers a card and, when the answer approves it, records and performs its effect exactly
/// once (REQ-017).
///
/// Answering an already-answered card is idempotent: the recorded effect is never executed a
/// second time, and the answer itself is refused once the card is closed.
///
/// # Errors
///
/// Returns [`AppError::NotFound`] when the card does not exist in the tenant, and
/// [`AppError::Conflict`] when it was already answered.
pub async fn answer_card(
    app: &App,
    context: &TenantContext,
    card_id: &str,
    answer: &str,
) -> Result<AnswerCardOutcome, AppError> {
    let card_id = CardId::new(card_id.to_owned()).map_err(|_| AppError::NotFound)?;
    let card = app
        .stores
        .cards
        .find(context, &card_id)
        .await?
        .ok_or(AppError::NotFound)?;
    if card.status != CardStatus::Pending {
        // The card is closed: no answer lands twice (REQ-017).
        return Err(AppError::Conflict);
    }

    let card = app.stores.cards.answer(context, &card_id, answer).await?;

    let mut effect_recorded = false;
    if answer == "approve"
        && let Some(run_id) = &card.run_id
    {
        // One effect key per card answer: a retried answer never executes twice.
        let effect_key = format!("approve:{}", card_id.as_str());
        match app
            .stores
            .effects
            .record(
                context,
                NewEffect {
                    run_id: run_id.clone(),
                    effect_key,
                    kind: "card_answer".to_owned(),
                    payload: serde_json::json!({
                        "card_id": card_id.as_str(),
                        "answer": answer,
                    }),
                },
            )
            .await
        {
            Ok(_) => effect_recorded = true,
            Err(AppError::Conflict) => {
                // The effect was already recorded: the answer is a replay.
            }
            Err(error) => return Err(error),
        }
        // The run that was waiting on the card resumes (REQ-017).
        set_run_status(
            app,
            context,
            run_id.as_str(),
            RunStatus::Running,
            None,
            None,
        )
        .await?;
    }

    Ok(AnswerCardOutcome {
        card,
        effect_recorded,
    })
}

/// The outcome of an emoji reaction a member left on a message (REQ-008).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReactOutcome {
    /// The message reacted to.
    pub message_id: boma_domain::MessageId,
    /// The truth for the message after the change, so one RPC answers completely.
    pub reactions: Vec<boma_domain::Reaction>,
}

/// Adds or removes an emoji reaction a member leaves on a message (REQ-008).
///
/// One person, one emoji, one row: repeating the same `add` changes nothing, which is what makes
/// a retry harmless.
///
/// # Errors
///
/// Returns [`AppError::NotFound`] when the message does not exist in the caller's tenant, and
/// [`AppError::Rule`] when the emoji is refused.
pub async fn react_to_message(
    app: &App,
    context: &TenantContext,
    message_id: &str,
    emoji: &str,
    action: ReactionAction,
) -> Result<ReactOutcome, AppError> {
    let message_id =
        boma_domain::MessageId::new(message_id.to_owned()).map_err(|_| AppError::NotFound)?;
    let user_id = context.user_id().cloned().ok_or(AppError::NotFound)?;
    let reactions = app
        .stores
        .reactions
        .react(context, &message_id, &user_id, emoji, action)
        .await?;
    Ok(ReactOutcome {
        message_id,
        reactions,
    })
}

/// The built-in tools every run receives (REQ-016).
///
/// The spec requires built-in tools to exist but does not enumerate them, so this list is empty
/// at M2: each built-in lands with the milestone that executes it (memory tools with REQ-022,
/// messaging tools with REQ-048). An empty space and an empty bot grant therefore yield exactly
/// these built-ins, as the acceptance criteria require. A silent non-empty list here would be a
/// lie about what exists; each milestone appends, it never removes.
pub const BUILTIN_TOOLS: &[ToolSpec] = &[];

/// Delegates a scoped task to a helper run on the same bot and thread (REQ-019).
///
/// The helper run is a `HelperResult` trigger child of the parent run: it does the work on the
/// same conversation as the parent, but its outcome is recorded as helper output, not as a
/// chat message of the bot. The parent consumes it when it continues (REQ-019 acceptance).
///
/// # Errors
///
/// Returns [`AppError::NotFound`] when the parent run does not exist in the caller's tenant.
pub async fn delegate_helper_run(
    app: &App,
    context: &TenantContext,
    parent_run_id: &str,
    prompt: &str,
) -> Result<Run, AppError> {
    let parent = app
        .stores
        .runs
        .find(
            context,
            &boma_domain::RunId::new(parent_run_id.to_owned()).map_err(|_| AppError::NotFound)?,
        )
        .await?
        .ok_or(AppError::NotFound)?;

    validate_message_text(prompt)?;

    let thread = app
        .stores
        .threads
        .for_bot(context, &parent.bot_id)
        .await?
        .ok_or(AppError::NotFound)?;

    let preferences = boma_domain::model::ModelPreferences {
        bot: parent.model_id.clone().map(|model| {
            (
                format!(
                    "{}/{}",
                    parent
                        .model_provider
                        .clone()
                        .unwrap_or_else(|| "custom".to_owned()),
                    model
                ),
                parent.model_pinned,
            )
        }),
        space: None,
        person: None,
    };
    let choice = boma_domain::model::resolve(&preferences, &app.services.models)?;
    let now = app.services.clock.now();

    let run = app
        .stores
        .runs
        .create(NewRun {
            id: boma_domain::RunId::new(app.services.ids.new_id("hrun"))?,
            space_id: parent.space_id.clone(),
            bot_id: parent.bot_id.clone(),
            thread_id: thread.id.clone(),
            user_id: parent.user_id.clone(),
            trigger: RunTrigger::HelperResult,
            client_nonce: None,
            source_message_id: parent.source_message_id.clone(),
            created_at: now,
            model_provider: Some(choice.provider),
            model_id: Some(choice.model),
            model_pinned: choice.pinned,
            parent_run_id: Some(parent.id.clone()),
        })
        .await?;

    tracing::info!(run = %run.id, parent = %parent.id, "the parent delegated to a helper run");
    Ok(run)
}

/// The tool catalogue of one run: the built-ins plus the tools the bot may use (REQ-016).
///
/// A bot with an empty grant sees every enabled install of its space; a grant is the list of
/// keys it may use instead. A tool the bot may not use never appears in the catalogue.
///
/// # Errors
///
/// Returns [`AppError::NotFound`] when the bot does not exist in the tenant.
pub async fn tool_catalogue(
    app: &App,
    context: &TenantContext,
    bot_id: &str,
) -> Result<Vec<ToolSpec>, AppError> {
    let bot_id = boma_domain::BotId::new(bot_id.to_owned()).map_err(|_| AppError::NotFound)?;
    let bot = app
        .stores
        .bots
        .find(context, &bot_id)
        .await?
        .ok_or(AppError::NotFound)?;
    let mut tools = BUILTIN_TOOLS.to_vec();
    tools.extend(app.stores.tools.catalogue_for(context, &bot.id).await?);
    Ok(tools)
}
