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

/// Registers one hit against a shared rate limit and answers with the recorded count (D-01).
///
/// The count comes from the backend all replicas share — never from this process' memory — so
/// refusing an over-limit caller is an honest decision no matter which replica serves them.
/// The decision itself belongs to the caller: the auth endpoints own their limits.
///
/// # Errors
///
/// Returns [`AppError::Unavailable`] when a store cannot answer.
pub async fn hit_rate_limit(
    app: &App,
    key: &str,
    window_seconds: u64,
) -> Result<crate::ports::RateLimitDecision, AppError> {
    let now = app.services.clock.now();
    app.stores.rate_limits.hit(key, &now, window_seconds).await
}

/// Records a plugin submission for review (REQ-034).
///
/// `source` arrives already parsed: the callers decide how to report a malformed reference,
/// because the public endpoint pins the exact body `Invalid source.` while the authed RPC uses
/// the ordinary boundary rules. An empty `name` or `contact` is stored as nothing.
///
/// # Errors
///
/// Returns an error when a store cannot answer.
pub async fn submit_plugin(
    app: &App,
    source: &boma_domain::PluginSourceRef,
    name: Option<String>,
    contact: Option<String>,
    submitter: Option<boma_domain::UserId>,
) -> Result<String, AppError> {
    let id = app.services.ids.new_id("sub");
    app.stores
        .plugin_submissions
        .record(crate::ports::NewPluginSubmission {
            id: id.clone(),
            source: source.canonical(),
            name: normalize_optional(name, MAX_PLUGIN_NAME_LENGTH),
            contact: normalize_optional(contact, MAX_PLUGIN_CONTACT_LENGTH),
            submitter_user_id: submitter,
            created_at: app.services.clock.now(),
        })
        .await?;
    Ok(id)
}

/// The skills a member sees (REQ-035): the built-ins first, then the space's own, by name.
///
/// # Errors
///
/// Returns an error when a store cannot answer.
pub async fn skills_list(
    app: &App,
    context: &TenantContext,
) -> Result<Vec<crate::ports::SkillSummary>, AppError> {
    let mut skills: Vec<crate::ports::SkillSummary> = boma_domain::builtin_skills()
        .into_iter()
        .map(|skill| crate::ports::SkillSummary {
            name: skill.name,
            description: skill.description,
            origin: boma_domain::SkillOrigin::Builtin.as_str().to_owned(),
            plugin_source: None,
            created_at: String::new(),
        })
        .collect();
    skills.extend(app.stores.skills.list(context).await?);
    skills.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(skills)
}

/// One skill with its instructions (REQ-035): built-in first, then the space's own.
///
/// # Errors
///
/// Returns [`AppError::NotFound`] when no skill of that name exists.
pub async fn skill_get(
    app: &App,
    context: &TenantContext,
    name: &str,
) -> Result<crate::ports::SkillRecord, AppError> {
    if let Some(skill) = boma_domain::builtin_skills()
        .into_iter()
        .find(|skill| skill.name == name)
    {
        return Ok(crate::ports::SkillRecord {
            name: skill.name,
            description: skill.description,
            instructions: skill.instructions,
            origin: boma_domain::SkillOrigin::Builtin.as_str().to_owned(),
            plugin_source: None,
            created_at: String::new(),
        });
    }
    app.stores
        .skills
        .get(context, name)
        .await?
        .ok_or(AppError::NotFound)
}

/// Teaches a skill into the space (REQ-035).
///
/// A built-in's name is reserved: teaching over `interrogate` would silently change what a
/// deployment means. The caller decides the origin — a member teaching in conversation teaches
/// `taught`; a plugin install installs `plugin` with its source.
///
/// # Errors
///
/// Returns [`AppError::Rule`] when a field fails validation, [`AppError::Conflict`] when the
/// name belongs to a built-in, and an error when a store cannot answer.
pub async fn teach_skill(
    app: &App,
    context: &TenantContext,
    name: &str,
    description: &str,
    instructions: &str,
    origin: boma_domain::SkillOrigin,
    plugin_source: Option<String>,
) -> Result<(), AppError> {
    boma_domain::validate_skill_name(name)?;
    boma_domain::validate_skill_description(description)?;
    boma_domain::validate_skill_instructions(instructions)?;
    if boma_domain::builtin_skills()
        .into_iter()
        .any(|skill| skill.name == name)
    {
        return Err(AppError::Conflict);
    }
    app.stores
        .skills
        .put(
            context,
            crate::ports::NewSkill {
                id: app.services.ids.new_id("sk"),
                name: name.to_owned(),
                description: description.trim().to_owned(),
                instructions: instructions.to_owned(),
                origin,
                plugin_source,
                created_at: app.services.clock.now(),
            },
        )
        .await
}

/// The names and one-line descriptions of every skill this context may load (REQ-035).
///
/// # Errors
///
/// Returns an error when a store cannot answer.
pub async fn skill_names(
    app: &App,
    context: &TenantContext,
) -> Result<Vec<crate::ports::SkillContext>, AppError> {
    Ok(skills_list(app, context)
        .await?
        .into_iter()
        .map(|summary| crate::ports::SkillContext {
            name: summary.name,
            description: summary.description,
        })
        .collect())
}

/// The longest display name a submission carries.
const MAX_PLUGIN_NAME_LENGTH: usize = 120;

/// The longest contact address a submission carries.
const MAX_PLUGIN_CONTACT_LENGTH: usize = 320;

/// Trims and caps an optional free-text field; empty becomes absent.
fn normalize_optional(value: Option<String>, max: usize) -> Option<String> {
    value
        .map(|value| value.trim().chars().take(max).collect::<String>())
        .filter(|value| !value.is_empty())
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
            routine_id: None,
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

/// The shape a secret's name must follow: it becomes an env var name (REQ-046).
fn validate_secret_name(name: &str) -> Result<(), AppError> {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return Err(AppError::Rule(boma_domain::DomainError::InvalidName(
            "the secret's name is empty".to_owned(),
        )));
    };
    if !first.is_ascii_uppercase()
        || !chars.all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
        || name.len() > 64
    {
        return Err(AppError::Rule(boma_domain::DomainError::InvalidName(
            "the secret's name must look like an environment variable (A-Z, digits, underscore)"
                .to_owned(),
        )));
    }
    Ok(())
}

/// Sets a secret for a bot, write-only (REQ-046).
///
/// The plaintext never reaches the database, the log, an event or a message: it is sealed
/// immediately and only the ciphertext is stored. The value is *never* echoed back — not in a
/// response, not in an error, not in any later read (`REQ-046`).
///
/// # Errors
///
/// Returns [`AppError::NotFound`] when the bot does not exist or the caller cannot see it, and
/// [`AppError::Rule`] when the name is refused.
pub async fn bot_secret_set(
    app: &App,
    context: &TenantContext,
    bot_id: &str,
    name: &str,
    value: secrecy::SecretString,
) -> Result<(), AppError> {
    let bot_id = boma_domain::BotId::new(bot_id.to_owned()).map_err(|_| AppError::NotFound)?;
    validate_secret_name(name)?;
    app.stores
        .bots
        .find(context, &bot_id)
        .await?
        .ok_or(AppError::NotFound)?;
    app.stores
        .secrets
        .put(
            context,
            crate::ports::NewBotSecret {
                id: app.services.ids.new_id("sec"),
                bot_id,
                name: name.to_owned(),
                value,
                created_at: app.services.clock.now(),
            },
        )
        .await
}

/// Lists the names of a bot's secrets, newest first. Never the values or the ciphertexts (REQ-046).
///
/// # Errors
///
/// Returns [`AppError::NotFound`] when the bot does not exist or the caller cannot see it.
pub async fn bot_secret_list(
    app: &App,
    context: &TenantContext,
    bot_id: &str,
) -> Result<Vec<crate::ports::SecretSummary>, AppError> {
    let bot_id = boma_domain::BotId::new(bot_id.to_owned()).map_err(|_| AppError::NotFound)?;
    app.stores
        .bots
        .find(context, &bot_id)
        .await?
        .ok_or(AppError::NotFound)?;
    app.stores.secrets.list(context, &bot_id).await
}

/// Forgets a secret. Deleting a name that is not there is a success, not an error (REQ-046).
///
/// # Errors
///
/// Returns [`AppError::NotFound`] when the bot does not exist or the caller cannot see it.
pub async fn bot_secret_forget(
    app: &App,
    context: &TenantContext,
    bot_id: &str,
    name: &str,
) -> Result<bool, AppError> {
    let bot_id = boma_domain::BotId::new(bot_id.to_owned()).map_err(|_| AppError::NotFound)?;
    validate_secret_name(name)?;
    app.stores
        .bots
        .find(context, &bot_id)
        .await?
        .ok_or(AppError::NotFound)?;
    app.stores.secrets.delete(context, &bot_id, name).await
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

/// The outcome of `routines.create` (REQ-023).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutineCreateOutcome {
    /// The created routine.
    pub routine: crate::ports::Routine,
}

/// The outcome of `routines.remove` (REQ-023).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutineRemoveOutcome {
    /// The removed routine's id.
    pub routine_id: boma_domain::RoutineId,
    /// Whether anything was there.
    pub removed: bool,
}

/// Creates a routine and computes its next wake (REQ-023).
///
/// # Errors
///
/// Returns [`AppError::NotFound`] when the bot does not exist in the caller's tenant, and
/// [`AppError::Rule`] when the name, the prompt, the triggers, or the timezone is refused.
#[allow(clippy::too_many_arguments)]
pub async fn routines_create(
    app: &App,
    context: &TenantContext,
    bot_id: &str,
    name: &str,
    prompt: &str,
    crons: &[String],
    timezone: &str,
    active: bool,
    notify: bool,
) -> Result<RoutineCreateOutcome, AppError> {
    boma_domain::validate_routine_name(name)?;
    boma_domain::validate_routine_prompt(prompt)?;
    boma_domain::validate_timezone(timezone)?;
    boma_domain::validate_triggers(crons)?;
    let bot_id = boma_domain::BotId::new(bot_id.to_owned()).map_err(|_| AppError::NotFound)?;
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
    let now = app.services.clock.now();
    let next_run_at = if active {
        Some(boma_domain::next_run_at(crons, timezone, &now)?)
    } else {
        None
    };
    let routine_id = boma_domain::RoutineId::new(app.services.ids.new_id("routine"))?;
    let routine = app
        .stores
        .routines
        .create(
            context,
            crate::ports::NewRoutine {
                id: routine_id,
                space_id: bot.space_id.clone(),
                bot_id: bot.id,
                thread_id: thread.id,
                user_id: context.user_id().cloned().ok_or(AppError::NotFound)?,
                name: name.to_owned(),
                prompt: prompt.to_owned(),
                crons: crons.to_vec(),
                timezone: timezone.to_owned(),
                active,
                notify,
                next_run_at,
                created_at: now,
            },
        )
        .await?;
    Ok(RoutineCreateOutcome { routine })
}

/// Lists a bot's routines, creation order (REQ-023).
///
/// # Errors
///
/// Returns [`AppError::NotFound`] when the bot does not exist in the caller's tenant.
pub async fn routines_list(
    app: &App,
    context: &TenantContext,
    bot_id: &str,
) -> Result<Vec<crate::ports::Routine>, AppError> {
    let bot_id = boma_domain::BotId::new(bot_id.to_owned()).map_err(|_| AppError::NotFound)?;
    app.stores
        .bots
        .find(context, &bot_id)
        .await?
        .ok_or(AppError::NotFound)?;
    app.stores.routines.list(context, &bot_id).await
}

/// Removes a routine (REQ-023).
///
/// # Errors
///
/// Returns [`AppError::NotFound`] when it does not exist in the caller's tenant.
pub async fn routines_remove(
    app: &App,
    context: &TenantContext,
    id: &str,
) -> Result<RoutineRemoveOutcome, AppError> {
    let routine = app
        .stores
        .routines
        .find(
            context,
            &boma_domain::RoutineId::new(id.to_owned()).map_err(|_| AppError::NotFound)?,
        )
        .await?
        .ok_or(AppError::NotFound)?;
    app.stores.routines.remove(context, &routine.id).await?;
    Ok(RoutineRemoveOutcome {
        routine_id: routine.id,
        removed: true,
    })
}

/// The failure threshold at which a routine pauses itself (REQ-024).
///
/// Five consecutive account-level failures pause the routine: a cron that keeps blowing up stops
/// flooding the space with model errors.
const ROUTINE_FAILURE_THRESHOLD: u32 = 5;

/// The context a routine-run acts for, derived from its owner.
fn routine_context(
    user_id: &boma_domain::UserId,
    space_id: &boma_domain::SpaceId,
) -> TenantContext {
    TenantContext::Member {
        space_id: space_id.clone(),
        user_id: user_id.clone(),
    }
}

/// Formats `now + delay` as the RFC3339 instant at second precision, `Z`, like the clock writes.
fn render_utc_plus(now: &str, delay_secs: u64) -> Result<String, AppError> {
    use time::{Duration, OffsetDateTime, UtcOffset, format_description::well_known::Rfc3339};

    let at = OffsetDateTime::parse(now, &Rfc3339).map_err(|_| AppError::Unavailable)?
        + Duration::seconds(i64::try_from(delay_secs).unwrap_or(i64::MAX));
    at.replace_nanosecond(0)
        .map_err(|_| AppError::Unavailable)?
        .to_offset(UtcOffset::UTC)
        .format(&Rfc3339)
        .map_err(|_| AppError::Unavailable)
}

/// Re-queues a routine-woken run for its next attempt, with backoff (REQ-024).
///
/// Returns `true` when the run was re-queued, `false` when it burned the attempts the schedule
/// owes it and the failure must count toward the routine's budget instead. The re-queued run
/// may not be claimed before `not_before`: a failing backend is not stampeded by its own
/// routine.
///
/// # Errors
///
/// Returns [`AppError::NotFound`] when the run is not in the caller's tenant,
/// [`AppError::Conflict`] when it is not `failed`, and [`AppError::Unavailable`] when the clock
/// or the store cannot answer.
pub async fn auto_retry_routine_run(app: &App, run: &boma_domain::Run) -> Result<bool, AppError> {
    if run.attempt >= boma_domain::run::MAX_ATTEMPTS {
        return Ok(false);
    }
    let delay = boma_domain::run::retry_delay_seconds(run.attempt);
    let not_before = render_utc_plus(&app.services.clock.now(), delay)?;
    app.stores
        .runs
        .retry(
            &routine_context(&run.user_id, &run.space_id),
            &run.id,
            Some(&not_before),
        )
        .await?;
    Ok(true)
}

/// Posts a system notice into the routine's thread (REQ-024: the owner is told).
///
/// The notice is a `System` message: it lands in the thread like any other line and it never
/// starts a run of its own. It carries no text a person wrote, so no redaction is due.
async fn post_routine_notice(
    app: &App,
    context: &TenantContext,
    routine: &crate::ports::Routine,
    run: &boma_domain::Run,
    text: String,
) -> Result<(), AppError> {
    let stored = app
        .stores
        .messages
        .insert(
            context,
            crate::ports::NewMessage {
                id: boma_domain::MessageId::new(app.services.ids.new_id("msg"))?,
                space_id: routine.space_id.clone(),
                thread_id: routine.thread_id.clone(),
                role: boma_domain::MessageRole::System,
                blocks: vec![boma_domain::Block::Text { text }],
                bot_id: Some(routine.bot_id.clone()),
                author_user_id: None,
                run_id: Some(run.id.clone()),
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
                space_id: routine.space_id.clone(),
                thread_id: routine.thread_id.clone(),
                kind: boma_domain::EventType::Message,
                payload: serde_json::json!({
                    "messageId": stored.message.id.as_str(),
                    "seq": stored.message.seq,
                }),
                created_at: app.services.clock.now(),
            },
        )
        .await?;
    Ok(())
}

/// Marks a routine run as completed: the routine's failure counter resets (REQ-024).
///
/// # Errors
///
/// Returns [`AppError::Unavailable`] when a store cannot answer.
pub async fn record_routine_completed(app: &App, run: &boma_domain::Run) -> Result<(), AppError> {
    let Some(routine_id) = &run.routine_id else {
        return Ok(());
    };
    if run.trigger != boma_domain::RunTrigger::Routine {
        // A trial run starts from the routine but is a normal turn: it does not move the
        // failure counters (REQ-025).
        return Ok(());
    }
    let context = routine_context(&run.user_id, &run.space_id);
    let routine = app.stores.routines.find(&context, routine_id).await?;
    app.stores
        .routines
        .record_completed(
            &context,
            routine_id,
            &app.services.clock.now(),
            Some(&run.id),
        )
        .await?;
    if let Some(routine) = routine
        && routine.notify
    {
        post_routine_notice(
            app,
            &context,
            &routine,
            run,
            format!("Routine notice: '{}' finished.", routine.name),
        )
        .await?;
    }
    Ok(())
}

/// Marks a routine run as failed: the failure counter grows, and past the threshold the routine
/// pauses itself (REQ-024).
///
/// # Errors
///
/// Returns [`AppError::Unavailable`] when a store cannot answer.
pub async fn record_routine_failed(app: &App, run: &boma_domain::Run) -> Result<(), AppError> {
    let Some(routine_id) = &run.routine_id else {
        return Ok(());
    };
    if run.trigger != boma_domain::RunTrigger::Routine {
        // A trial run starts from the routine but is a normal turn: it does not move the
        // failure counters (REQ-025).
        return Ok(());
    }
    let context = routine_context(&run.user_id, &run.space_id);
    let before = app.stores.routines.find(&context, routine_id).await?;
    let updated = app
        .stores
        .routines
        .record_failure(
            &context,
            routine_id,
            &app.services.clock.now(),
            Some(&run.id),
            ROUTINE_FAILURE_THRESHOLD,
        )
        .await?;
    let paused_now = before.is_some_and(|routine| routine.active) && !updated.active;
    if paused_now {
        // REQ-024: when the threshold pauses the routine, the owner is told — this notice does
        // not depend on the routine's `notify` preference.
        post_routine_notice(
            app,
            &context,
            &updated,
            run,
            format!(
                "Routine notice: '{}' was paused after {} consecutive failures. Re-enable it to keep the schedule.",
                updated.name, updated.account_failures
            ),
        )
        .await?;
    } else if updated.notify {
        let plural = if updated.account_failures == 1 {
            ""
        } else {
            "s"
        };
        post_routine_notice(
            app,
            &context,
            &updated,
            run,
            format!(
                "Routine notice: '{}' failed ({} consecutive failure{plural}).",
                updated.name, updated.account_failures
            ),
        )
        .await?;
    }
    Ok(())
}

/// The context the dispatcher acts under for a routine-run.
fn dispatch_context(routine: &crate::ports::Routine) -> TenantContext {
    TenantContext::Member {
        space_id: routine.space_id.clone(),
        user_id: routine.user_id.clone(),
    }
}

/// Claims every routine that is due and enqueues a run for each of them (REQ-024).
///
/// A due routine with an already open routine-run in the same thread is coalesced instead of
/// firing again; the skip is recorded in the routine's audit log and the wake is rescheduled.
///
/// Returns the number of runs the call enqueued.
///
/// # Errors
///
/// Returns [`AppError::Unavailable`] when a store cannot answer, and [`AppError::Rule`] when a
/// cron expression stopped being valid.
pub async fn dispatch_routines(app: &App) -> Result<u64, AppError> {
    let now = app.services.clock.now();
    let due = app.stores.routines.due(&now).await?;
    let mut dispatched = 0;
    for routine in due {
        // `@once` has no next wake; otherwise compute the next one from the cron.
        let next = if routine.crons.iter().any(|entry| entry == boma_domain::ONCE) {
            None
        } else {
            Some(boma_domain::next_run_at(
                &routine.crons,
                &routine.timezone,
                &now,
            )?)
        };
        let context = dispatch_context(&routine);

        // A routine that already has an open routine-trigger run must not fire a second one: the
        // attempt joins the open one when it finishes, and the skip is recorded.
        if app
            .stores
            .runs
            .find_open_routine(&context, &routine.id)
            .await?
            .is_some()
        {
            app.stores
                .routines
                .record_skip(
                    &context,
                    &routine.id,
                    crate::ports::RoutineOutcome::Coalesced,
                    &now,
                    None,
                    next.as_deref(),
                    Some("a routine run was already open"),
                )
                .await?;
            continue;
        }

        let run = app
            .stores
            .runs
            .create(crate::ports::NewRun {
                id: boma_domain::RunId::new(app.services.ids.new_id("run"))?,
                space_id: routine.space_id.clone(),
                bot_id: routine.bot_id.clone(),
                thread_id: routine.thread_id.clone(),
                user_id: routine.user_id.clone(),
                trigger: boma_domain::RunTrigger::Routine,
                client_nonce: None,
                source_message_id: None,
                created_at: now.clone(),
                model_provider: None,
                model_id: None,
                model_pinned: false,
                parent_run_id: None,
                routine_id: Some(routine.id.clone()),
            })
            .await?;
        dispatched += 1;
        app.stores
            .routines
            .record_fired(&context, &routine.id, &now, Some(&run.id), next.as_deref())
            .await?;
    }
    Ok(dispatched)
}

/// The outcome of `routines.runNow` — a one-shot turn with the routine's prompt (REQ-025).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutineRunNowOutcome {
    /// The routine that was fired.
    pub routine: crate::ports::Routine,
    /// The run the trial started.
    pub run: boma_domain::Run,
}

/// Starts one turn with the routine's prompt, without changing the schedule (REQ-025).
///
/// # Errors
///
/// Returns [`AppError::NotFound`] when the routine does not exist in the caller's tenant, and
/// [`AppError::Rule`] when the run cannot be started.
pub async fn routines_run_now(
    app: &App,
    context: &TenantContext,
    routine_id: &str,
) -> Result<RoutineRunNowOutcome, AppError> {
    let routine = app
        .stores
        .routines
        .find(
            context,
            &boma_domain::RoutineId::new(routine_id.to_owned()).map_err(|_| AppError::NotFound)?,
        )
        .await?
        .ok_or(AppError::NotFound)?;
    let run = app
        .stores
        .runs
        .create(crate::ports::NewRun {
            id: boma_domain::RunId::new(app.services.ids.new_id("run"))?,
            space_id: routine.space_id.clone(),
            bot_id: routine.bot_id.clone(),
            thread_id: routine.thread_id.clone(),
            user_id: context.user_id().cloned().ok_or(AppError::NotFound)?,
            // A trial run is a normal member-initiated turn; the counters don't move (REQ-025).
            trigger: boma_domain::RunTrigger::User,
            client_nonce: None,
            source_message_id: None,
            created_at: app.services.clock.now(),
            model_provider: None,
            model_id: None,
            model_pinned: false,
            parent_run_id: None,
            routine_id: Some(routine.id.clone()),
        })
        .await?;
    Ok(RoutineRunNowOutcome { routine, run })
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
    app.stores.runs.retry(context, &id, None).await
}

/// The outcome of one computer ensure call (REQ-026).
#[derive(Debug, Clone, PartialEq)]
pub struct ComputerOutcome {
    /// The computer, as the store holds it after the call.
    pub computer: boma_domain::Computer,
    /// Whether this call booted the computer.
    pub started: bool,
}

/// The deployment's computer backend, or unavailable when none is configured (REQ-027).
fn computers_or_unavailable(app: &App) -> Result<&dyn crate::ports::ComputerRuntime, AppError> {
    let runtime = app
        .services
        .computer
        .as_deref()
        .ok_or(AppError::Unavailable)?;
    if runtime.provider() == "none" {
        return Err(AppError::Unavailable);
    }
    Ok(runtime)
}

/// Starts (or reuses) the bot's computer (REQ-026).
///
/// A bot's computer boots on the first call that needs it and reuses whatever it finds running.
/// The same computer row is written back either way: `started` tells the caller whether it booted.
///
/// # Errors
///
/// Returns [`AppError::NotFound`] when the bot does not exist in the caller's tenant,
/// [`AppError::Unavailable`] when the deployment has no computer backend, and
/// [`AppError::Conflict`] when the computer exists but does not start.
pub async fn ensure_computer(
    app: &App,
    context: &TenantContext,
    bot_id: &str,
    private: bool,
) -> Result<ComputerOutcome, AppError> {
    let bot_id = boma_domain::BotId::new(bot_id.to_owned()).map_err(|_| AppError::NotFound)?;
    app.stores
        .bots
        .find(context, &bot_id)
        .await?
        .ok_or(AppError::NotFound)?;
    let runtime = computers_or_unavailable(app)?;

    let now = app.services.clock.now();
    let space_id = context.space_id().cloned().ok_or(AppError::NotFound)?;
    let found = if private {
        app.stores.computers.find(context, &bot_id).await?
    } else {
        app.stores.computers.find_team(context, &space_id).await?
    };
    let mut computer = match found {
        Some(computer) => computer,
        None if !private => {
            app.stores
                .computers
                .create(
                    context,
                    crate::ports::NewComputer {
                        id: boma_domain::ComputerId::new(app.services.ids.new_id("cmp"))?,
                        space_id: space_id.clone(),
                        bot_id: bot_id.clone(),
                        provider: runtime.provider().to_owned(),
                        private: false,
                        memory_limit_mb: 512,
                        cpu_limit: 1.0,
                        idle_minutes: 10,
                        created_at: now.clone(),
                        updated_at: now.clone(),
                    },
                )
                .await?
        }
        None => {
            app.stores
                .computers
                .create(
                    context,
                    crate::ports::NewComputer {
                        id: boma_domain::ComputerId::new(app.services.ids.new_id("cmp"))?,
                        space_id,
                        bot_id,
                        provider: runtime.provider().to_owned(),
                        private: true,
                        memory_limit_mb: 512,
                        cpu_limit: 1.0,
                        idle_minutes: 10,
                        created_at: now.clone(),
                        updated_at: now.clone(),
                    },
                )
                .await?
        }
    };

    let spec = crate::ports::ComputerSpec {
        computer_id: computer.id.as_str().to_owned(),
        memory_limit_mb: computer.memory_limit_mb,
        cpu_limit: computer.cpu_limit,
    };
    let handle = runtime.ensure(&spec).await?;
    computer.external_id = Some(handle.external_id);
    computer.home_id = Some(handle.home_id);
    computer.status = boma_domain::ComputerStatus::Running;
    if handle.started {
        // A real boot: the screen generation moves and older capabilities stop being valid.
        computer.screen_generation = computer.screen_generation.saturating_add(1);
        computer.started_at = Some(now.clone());
        computer.stopped_at = None;
    }
    computer.last_used_at = Some(now.clone());
    computer.updated_at = now;
    app.stores.computers.update(context, &computer).await?;
    Ok(ComputerOutcome {
        computer,
        started: handle.started,
    })
}

/// The outcome of one observation: the frame and the generation it belongs to (REQ-028).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComputerObservation {
    /// The PNG bytes of the frame.
    pub png: Vec<u8>,
    /// The screen generation the frame belongs to.
    pub screen_generation: u64,
}

/// The outcome of a DOM snapshot: the html and the generation it belongs to (REQ-028).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComputerDomSnapshot {
    /// The outer HTML of the document's root.
    pub html: String,
    /// The screen generation the snapshot was taken in.
    pub screen_generation: u64,
}

/// The outcome of an evaluation: the value, if the expression produced one (REQ-028).
#[derive(Debug, Clone, PartialEq)]
pub struct ComputerEvaluation {
    /// The value of `expression`, or null when the expression produced nothing.
    pub value: serde_json::Value,
    /// The screen generation the evaluation ran in.
    pub screen_generation: u64,
}

/// Takes a screenshot of the bot's computer (REQ-028).
///
/// # Errors
///
/// Returns [`AppError::NotFound`] when the bot does not exist in the caller's tenant,
/// [`AppError::Rule`] when the computer is not running or a hold is in effect, and
/// [`AppError::Unavailable`] when the deployment has no computer backend.
pub async fn computer_observe(
    app: &App,
    context: &TenantContext,
    bot_id: &str,
) -> Result<ComputerObservation, AppError> {
    computer_caller(app, context, bot_id).await?;
    let computer = resolve_bot_computer(app, context, bot_id).await?;
    let external_id = computer.external_id.clone().ok_or(AppError::Unavailable)?;
    let runtime = computers_or_unavailable(app)?;
    let png = runtime.observe(&external_id).await?;
    Ok(ComputerObservation {
        png,
        screen_generation: computer.screen_generation,
    })
}

/// Navigates the bot's computer to `url` (REQ-028).
///
/// # Errors
///
/// Returns [`AppError::NotFound`] when the bot does not exist in the caller's tenant,
/// [`AppError::Rule`] when the computer is not running or a hold is in effect.
pub async fn computer_navigate(
    app: &App,
    context: &TenantContext,
    bot_id: &str,
    url: &str,
) -> Result<boma_domain::Computer, AppError> {
    computer_caller(app, context, bot_id).await?;
    if url.is_empty() || url.len() > 4096 {
        return Err(AppError::Rule(boma_domain::DomainError::InvalidName(
            "the url is not usable".to_owned(),
        )));
    }
    let computer = resolve_bot_computer(app, context, bot_id).await?;
    let external_id = computer.external_id.clone().ok_or(AppError::Unavailable)?;
    let runtime = computers_or_unavailable(app)?;
    runtime.navigate(&external_id, url).await?;
    computer_status(app, context, bot_id)
        .await?
        .ok_or(AppError::NotFound)
}

/// Evaluates a JavaScript expression in the bot's computer (REQ-028).
///
/// # Errors
///
/// Returns [`AppError::NotFound`] when the bot does not exist in the caller's tenant,
/// [`AppError::Rule`] when the expression raises or a hold is in effect.
pub async fn computer_evaluate(
    app: &App,
    context: &TenantContext,
    bot_id: &str,
    expression: &str,
) -> Result<ComputerEvaluation, AppError> {
    computer_caller(app, context, bot_id).await?;
    if expression.is_empty() || expression.len() > 64 * 1024 {
        return Err(AppError::Rule(boma_domain::DomainError::InvalidName(
            "the expression is not usable".to_owned(),
        )));
    }
    let computer = resolve_bot_computer(app, context, bot_id).await?;
    let external_id = computer.external_id.clone().ok_or(AppError::Unavailable)?;
    let runtime = computers_or_unavailable(app)?;
    let value = runtime.evaluate(&external_id, expression).await?;
    Ok(ComputerEvaluation {
        value,
        screen_generation: computer.screen_generation,
    })
}

/// Clicks at a viewport point in the bot's computer (REQ-028).
///
/// # Errors
///
/// Returns [`AppError::NotFound`] when the bot does not exist in the caller's tenant,
/// [`AppError::Rule`] when the computer is not running or a hold is in effect.
pub async fn computer_click(
    app: &App,
    context: &TenantContext,
    bot_id: &str,
    x: f64,
    y: f64,
) -> Result<boma_domain::Computer, AppError> {
    computer_caller(app, context, bot_id).await?;
    if !x.is_finite() || !y.is_finite() || x < 0.0 || y < 0.0 {
        return Err(AppError::Rule(boma_domain::DomainError::InvalidName(
            "the point is not usable".to_owned(),
        )));
    }
    let computer = resolve_bot_computer(app, context, bot_id).await?;
    let external_id = computer.external_id.clone().ok_or(AppError::Unavailable)?;
    let runtime = computers_or_unavailable(app)?;
    runtime.click(&external_id, x, y).await?;
    computer_status(app, context, bot_id)
        .await?
        .ok_or(AppError::NotFound)
}

/// Types text into the focused element of the bot's computer (REQ-028).
///
/// # Errors
///
/// Returns [`AppError::NotFound`] when the bot does not exist in the caller's tenant,
/// [`AppError::Rule`] when the computer is not running or a hold is in effect.
pub async fn computer_type(
    app: &App,
    context: &TenantContext,
    bot_id: &str,
    text: &str,
) -> Result<boma_domain::Computer, AppError> {
    computer_caller(app, context, bot_id).await?;
    if text.len() > 64 * 1024 {
        return Err(AppError::Rule(boma_domain::DomainError::InvalidName(
            "the text is too large".to_owned(),
        )));
    }
    let computer = resolve_bot_computer(app, context, bot_id).await?;
    let external_id = computer.external_id.clone().ok_or(AppError::Unavailable)?;
    let runtime = computers_or_unavailable(app)?;
    runtime.type_text(&external_id, text).await?;
    computer_status(app, context, bot_id)
        .await?
        .ok_or(AppError::NotFound)
}

/// Scrolls the bot's computer by `delta_y` pixels (REQ-028).
///
/// # Errors
///
/// Returns [`AppError::NotFound`] when the bot does not exist in the caller's tenant,
/// [`AppError::Rule`] when the computer is not running or a hold is in effect.
pub async fn computer_scroll(
    app: &App,
    context: &TenantContext,
    bot_id: &str,
    delta_y: f64,
) -> Result<boma_domain::Computer, AppError> {
    computer_caller(app, context, bot_id).await?;
    if !delta_y.is_finite() || delta_y == 0.0 {
        return Err(AppError::Rule(boma_domain::DomainError::InvalidName(
            "the scroll delta is not usable".to_owned(),
        )));
    }
    let computer = resolve_bot_computer(app, context, bot_id).await?;
    let external_id = computer.external_id.clone().ok_or(AppError::Unavailable)?;
    let runtime = computers_or_unavailable(app)?;
    runtime.scroll(&external_id, delta_y).await?;
    computer_status(app, context, bot_id)
        .await?
        .ok_or(AppError::NotFound)
}

/// Snapshots the bot's computer DOM (REQ-028).
///
/// # Errors
///
/// Returns [`AppError::NotFound`] when the bot does not exist in the caller's tenant,
/// [`AppError::Rule`] when the computer is not running or a hold is in effect.
pub async fn computer_dom_snapshot(
    app: &App,
    context: &TenantContext,
    bot_id: &str,
) -> Result<ComputerDomSnapshot, AppError> {
    computer_caller(app, context, bot_id).await?;
    let computer = resolve_bot_computer(app, context, bot_id).await?;
    let external_id = computer.external_id.clone().ok_or(AppError::Unavailable)?;
    let runtime = computers_or_unavailable(app)?;
    let html = runtime.dom_snapshot(&external_id).await?;
    Ok(ComputerDomSnapshot {
        html,
        screen_generation: computer.screen_generation,
    })
}

/// Resolves the computer a bot addresses, for the observe-family tools: its own, or the
/// shared team computer (REQ-028/031).
async fn resolve_bot_computer(
    app: &App,
    context: &TenantContext,
    bot_id: &str,
) -> Result<boma_domain::Computer, AppError> {
    let bot_id = boma_domain::BotId::new(bot_id.to_owned()).map_err(|_| AppError::NotFound)?;
    resolve_computer(app, context, &bot_id)
        .await?
        .ok_or(AppError::Unavailable)
}

/// Finds the computer a bot addresses: its own, or the space's team computer (REQ-031).
async fn resolve_computer(
    app: &App,
    context: &TenantContext,
    bot_id: &boma_domain::BotId,
) -> Result<Option<boma_domain::Computer>, AppError> {
    if let Some(computer) = app.stores.computers.find(context, bot_id).await? {
        return Ok(Some(computer));
    }
    let Some(space_id) = context.space_id() else {
        return Ok(None);
    };
    let Some(team) = app.stores.computers.find_team(context, space_id).await? else {
        return Ok(None);
    };
    Ok(Some(team))
}

/// Stops the bot's computer, preserving its home (REQ-026).
///
/// Idempotent: a bot with no computer, and a computer already stopped, both answer `stopped`.
///
/// # Errors
///
/// Returns [`AppError::NotFound`] when the bot does not exist in the caller's tenant, and
/// [`AppError::Unavailable`] when the deployment has no computer backend or the provider does
/// not answer.
pub async fn stop_computer(
    app: &App,
    context: &TenantContext,
    bot_id: &str,
) -> Result<boma_domain::Computer, AppError> {
    let bot_id = boma_domain::BotId::new(bot_id.to_owned()).map_err(|_| AppError::NotFound)?;
    app.stores
        .bots
        .find(context, &bot_id)
        .await?
        .ok_or(AppError::NotFound)?;
    let runtime = computers_or_unavailable(app)?;

    let now = app.services.clock.now();
    let Some(mut computer) = resolve_computer(app, context, &bot_id).await? else {
        // Nothing was ever started: the answer is `stopped` with no timestamps (the golden says so).
        return Ok(boma_domain::Computer {
            id: boma_domain::ComputerId::new(app.services.ids.new_id("cmp"))?,
            space_id: context.space_id().cloned().ok_or(AppError::NotFound)?,
            bot_id,
            provider: runtime.provider().to_owned(),
            external_id: None,
            status: boma_domain::ComputerStatus::Stopped,
            screen_generation: 0,
            home_id: None,
            takeover_by_user: None,
            takeover_at: None,
            private: false,
            memory_limit_mb: 512,
            cpu_limit: 1.0,
            idle_minutes: 10,
            started_at: None,
            last_used_at: None,
            stopped_at: None,
            created_at: now.clone(),
            updated_at: now,
        });
    };
    if computer.status != boma_domain::ComputerStatus::Stopped {
        if let Some(external_id) = computer.external_id.clone() {
            runtime.stop(&external_id).await?;
        }
        computer.status = boma_domain::ComputerStatus::Stopped;
        computer.started_at = None;
        computer.last_used_at = None;
        computer.stopped_at = Some(now.clone());
        computer.updated_at = now;
        app.stores.computers.update(context, &computer).await?;
    }
    Ok(computer)
}

/// Reads where the bot's computer is (REQ-026).
///
/// A bot that never started one answers `stopped`, never an error.
///
/// # Errors
///
/// Returns [`AppError::NotFound`] when the bot does not exist in the caller's tenant.
pub async fn computer_status(
    app: &App,
    context: &TenantContext,
    bot_id: &str,
) -> Result<Option<boma_domain::Computer>, AppError> {
    let bot_id = boma_domain::BotId::new(bot_id.to_owned()).map_err(|_| AppError::NotFound)?;
    app.stores
        .bots
        .find(context, &bot_id)
        .await?
        .ok_or(AppError::NotFound)?;
    resolve_computer(app, context, &bot_id).await
}

/// Stops the computers whose idle window has passed (REQ-026).
///
/// The sweeper keeps the deployment inside its capacity budget: a computer nobody has touched
/// for `idle_minutes` stops, and its home survives in the provider's storage.
///
/// Returns the number of computers released.
///
/// # Errors
///
/// Returns [`AppError::Unavailable`] when a store cannot answer.
pub async fn sweep_idle_computers(app: &App) -> Result<u64, AppError> {
    let runtime = match &app.services.computer {
        Some(runtime) if runtime.provider() != "none" => runtime,
        _ => return Ok(0),
    };
    let now = app.services.clock.now();
    let idle = app.stores.computers.running_idle(&now).await?;
    let mut released = 0;
    for mut computer in idle {
        let Some(external_id) = computer.external_id.clone() else {
            continue;
        };
        if runtime.stop(&external_id).await.is_err() {
            continue;
        }
        computer.status = boma_domain::ComputerStatus::Stopped;
        computer.started_at = None;
        computer.last_used_at = None;
        computer.stopped_at = Some(now.clone());
        computer.updated_at = now.clone();
        let context = TenantContext::System {
            reason: boma_domain::SystemReason::ScheduledJob,
            user_id: None,
        };
        app.stores.computers.update(&context, &computer).await?;
        released += 1;
    }
    Ok(released)
}

/// Confines a computer-side path to the bot's home (REQ-028).
///
/// Only absolute paths under `/home/` without `..` segments are accepted; anything else is refused
/// with [`AppError::Rule`].
///
/// # Errors
///
/// Returns [`AppError::Rule`] when the path escapes the home.
pub fn confine_home_path(path: &str) -> Result<&str, AppError> {
    if path.is_empty() || path.len() > 4096 {
        return Err(AppError::Rule(boma_domain::DomainError::InvalidName(
            "the path is not usable".to_owned(),
        )));
    }
    if !path.starts_with('/') || path.contains('\0') {
        return Err(AppError::Rule(boma_domain::DomainError::InvalidName(
            "the path is not absolute".to_owned(),
        )));
    }
    let home = "/home/".to_string();
    let rest = path.strip_prefix(&home).unwrap_or(path);
    if !path.starts_with(&home) || rest.contains("..") {
        return Err(AppError::Rule(boma_domain::DomainError::InvalidName(
            "the path escapes the computer's home".to_owned(),
        )));
    }
    Ok(path)
}

/// A bot's screen under human control (REQ-029).
///
/// While a member holds the screen, computer-driven input refuses to act. The hold is exclusive:
/// only the holder may release it.
///
/// # Errors
///
/// Returns [`AppError::NotFound`] when the bot does not exist in the caller's tenant,
/// [`AppError::ScreenTaken`] when another member holds the screen, and [`AppError::Conflict`] when
/// the computer is not running.
pub async fn computer_takeover(
    app: &App,
    context: &TenantContext,
    bot_id: &str,
) -> Result<boma_domain::Computer, AppError> {
    let bot_id = boma_domain::BotId::new(bot_id.to_owned()).map_err(|_| AppError::NotFound)?;
    app.stores
        .bots
        .find(context, &bot_id)
        .await?
        .ok_or(AppError::NotFound)?;
    let user_id = context.user_id().cloned().ok_or(AppError::NotFound)?;
    let mut computer = resolve_computer(app, context, &bot_id)
        .await?
        .ok_or(AppError::NotFound)?;
    if computer.status != boma_domain::ComputerStatus::Running {
        return Err(AppError::Conflict);
    }
    if let Some(holder) = computer.takeover_by_user.as_ref() {
        if holder == &user_id {
            return Ok(computer); // idempotent: the member already holds the screen
        }
        return Err(AppError::ScreenTaken);
    }
    computer.takeover_by_user = Some(user_id);
    computer.takeover_at = Some(app.services.clock.now());
    computer.updated_at = app.services.clock.now();
    app.stores.computers.update(context, &computer).await?;
    Ok(computer)
}

/// Releases the human hold on a bot's screen (REQ-029).
///
/// Only the holder may release. Releasing a hold that does not exist is a conflict, not an error.
///
/// # Errors
///
/// Returns [`AppError::NotFound`] when the bot does not exist in the caller's tenant,
/// [`AppError::ScreenTaken`] when another member holds the screen, and [`AppError::Conflict`] when
/// no one holds it.
pub async fn computer_release(
    app: &App,
    context: &TenantContext,
    bot_id: &str,
) -> Result<boma_domain::Computer, AppError> {
    let bot_id = boma_domain::BotId::new(bot_id.to_owned()).map_err(|_| AppError::NotFound)?;
    app.stores
        .bots
        .find(context, &bot_id)
        .await?
        .ok_or(AppError::NotFound)?;
    let user_id = context.user_id().cloned().ok_or(AppError::NotFound)?;
    let mut computer = resolve_computer(app, context, &bot_id)
        .await?
        .ok_or(AppError::NotFound)?;
    match computer.takeover_by_user.as_ref() {
        None => return Err(AppError::Conflict),
        Some(holder) if holder != &user_id => return Err(AppError::ScreenTaken),
        Some(_) => {}
    }
    computer.takeover_by_user = None;
    computer.takeover_at = None;
    computer.updated_at = app.services.clock.now();
    app.stores.computers.update(context, &computer).await?;
    Ok(computer)
}

/// Runs a shell command inside the bot's computer (REQ-028).
///
/// Refused while a member holds the screen. The command runs as the computer's user and its
/// stdout, capped at 64 KiB, comes back lossy-decoded.
///
/// # Errors
///
/// Returns [`AppError::NotFound`] when the bot does not exist in the caller's tenant, and
/// [`AppError::Rule`] when the computer is not running or a takeover hold is in effect.
pub async fn computer_shell(
    app: &App,
    context: &TenantContext,
    bot_id: &str,
    command: &str,
) -> Result<String, AppError> {
    computer_caller(app, context, bot_id).await?;
    if command.is_empty() || command.len() > 4096 {
        return Err(AppError::Rule(boma_domain::DomainError::InvalidName(
            "the command is not usable".to_owned(),
        )));
    }
    let computer = app
        .stores
        .computers
        .find(
            context,
            &boma_domain::BotId::new(bot_id.to_owned()).map_err(|_| AppError::NotFound)?,
        )
        .await?
        .ok_or(AppError::NotFound)?;
    let runtime = app
        .services
        .computer
        .as_deref()
        .filter(|runtime| runtime.provider() != "none")
        .ok_or(AppError::Unavailable)?;
    let Some(external_id) = computer.external_id else {
        return Err(AppError::Unavailable);
    };
    // The bot's secrets are available to its computer as environment, and nowhere else: the
    // values never enter a message, the event stream or any tool result (REQ-046).
    let env = secrets_env(app, context, &computer.bot_id).await?;
    runtime.exec(&external_id, command, 64 * 1024, &env).await
}

/// The environment a shell call sees: each of the bot's secrets as its own env var.
///
/// The naming is public on purpose (`BOMA_SECRET_OPENAI_KEY`), the values are not. A secret the
/// member forgot shows up as absent — the shell sees an empty environment, never an error
/// naming which secret existed.
async fn secrets_env(
    app: &App,
    context: &TenantContext,
    bot_id: &boma_domain::BotId,
) -> Result<Vec<(String, String)>, AppError> {
    use secrecy::ExposeSecret;
    let mut env = Vec::new();
    for summary in app.stores.secrets.list(context, bot_id).await? {
        let Some(value) = app
            .stores
            .secrets
            .reveal(context, bot_id, &summary.name)
            .await?
        else {
            continue;
        };
        env.push((
            format!("BOMA_SECRET_{}", summary.name),
            value.expose_secret().to_owned(),
        ));
    }
    Ok(env)
}

/// Reads a file from the bot's computer (REQ-028).
///
/// Refused while a member holds the screen. The path must stay inside `/home/`.
///
/// # Errors
///
/// Returns [`AppError::Rule`] when the path escapes the home or the command fails, and
/// [`AppError::NotFound`] when the bot does not exist in the caller's tenant.
pub async fn computer_read_file(
    app: &App,
    context: &TenantContext,
    bot_id: &str,
    path: &str,
) -> Result<String, AppError> {
    let command = format!("cat -- {path}");
    confine_home_path(path)?;
    computer_shell(app, context, bot_id, &command).await
}

/// Writes a file inside the bot's computer (REQ-028).
///
/// Refused while a member holds the screen. The path must stay inside `/home/`. Content larger
/// than 64 KiB is refused.
///
/// # Errors
///
/// Returns [`AppError::Rule`] when the path escapes the home or the content is too large.
pub async fn computer_write_file(
    app: &App,
    context: &TenantContext,
    bot_id: &str,
    path: &str,
    file_content: &str,
) -> Result<(), AppError> {
    confine_home_path(path)?;
    if file_content.len() > 64 * 1024 {
        return Err(AppError::Rule(boma_domain::DomainError::InvalidName(
            "the file content is too large".to_owned(),
        )));
    }
    let encoded = base64_encode(file_content.as_bytes());
    let script = format!(
        "mkdir -p \"$(dirname -- '{path}')\" && printf '%s' '{encoded}' | base64 -d > '{path}'",
    );
    computer_shell(app, context, bot_id, &script)
        .await
        .map(|_| ())
}

/// Base64 encoder: the standard alphabet, no line wrapping.
fn base64_encode(bytes: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// Checks the caller can drive the bot's computer: member of the space, computer running, no
/// takeover hold (REQ-029).
///
/// # Errors
///
/// Returns [`AppError::NotFound`] when the bot does not exist in the caller's tenant, and
/// [`AppError::Rule`] when the computer is not running or a hold is in effect.
pub async fn computer_caller(
    app: &App,
    context: &TenantContext,
    bot_id: &str,
) -> Result<(), AppError> {
    let bot_id = boma_domain::BotId::new(bot_id.to_owned()).map_err(|_| AppError::NotFound)?;
    app.stores
        .bots
        .find(context, &bot_id)
        .await?
        .ok_or(AppError::NotFound)?;
    let computer = resolve_computer(app, context, &bot_id)
        .await?
        .ok_or(AppError::Unavailable)?;
    if computer.status != boma_domain::ComputerStatus::Running {
        return Err(AppError::Rule(boma_domain::DomainError::InvalidName(
            "the computer is not running".to_owned(),
        )));
    }
    if computer.takeover_by_user.is_some() {
        return Err(AppError::Rule(boma_domain::DomainError::InvalidName(
            "the screen is under human control".to_owned(),
        )));
    }
    Ok(())
}

/// A screen capability freshly minted for a computer (REQ-030).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenCapabilityIssued {
    /// The opaque bearer token; only its hash is stored.
    pub token: String,
    /// The generation the capability names.
    pub screen_generation: u64,
    /// The computer this capability opens.
    pub computer_id: boma_domain::ComputerId,
}

/// Mints a capability that opens the bot's screen (REQ-030).
///
/// The computer must be running: a stopped screen opens nothing. The token is returned once and
/// never lands on disk — not in logs, not in a table plain.
///
/// # Errors
///
/// Returns [`AppError::NotFound`] when the bot does not exist in the caller's tenant,
/// [`AppError::Rule`] when the computer is not running, and [`AppError::Unavailable`] when the
/// deployment has no computer backend or the capability cannot be stored.
pub async fn issue_screen_capability(
    app: &App,
    context: &TenantContext,
    bot_id: &str,
) -> Result<ScreenCapabilityIssued, AppError> {
    let bot_id = boma_domain::BotId::new(bot_id.to_owned()).map_err(|_| AppError::NotFound)?;
    app.stores
        .bots
        .find(context, &bot_id)
        .await?
        .ok_or(AppError::NotFound)?;
    let Some(computer) = resolve_computer(app, context, &bot_id).await? else {
        return Err(AppError::Rule(boma_domain::DomainError::InvalidName(
            "the computer is not running".to_owned(),
        )));
    };
    if computer.status != boma_domain::ComputerStatus::Running {
        return Err(AppError::Rule(boma_domain::DomainError::InvalidName(
            "the computer is not running".to_owned(),
        )));
    }
    let space_id = context.space_id().cloned().ok_or(AppError::NotFound)?;
    let token = app.services.tokens.generate()?;
    let token_hash = app.services.tokens.hash(&token)?;
    app.stores
        .screen_capabilities
        .issue(
            context,
            crate::ports::NewScreenCapability {
                computer_id: computer.id.clone(),
                space_id,
                screen_generation: computer.screen_generation,
                token_hash,
                created_at: app.services.clock.now(),
            },
        )
        .await?;
    Ok(ScreenCapabilityIssued {
        token,
        screen_generation: computer.screen_generation,
        computer_id: computer.id,
    })
}

/// Resolves a bearer screen capability (REQ-030).
///
/// The answer is `None` for anything that does not check out: unknown token, a computer that is
/// not running, or a screen generation that moved. The caller answers with one uniform 403.
///
/// # Errors
///
/// Returns [`AppError::Unavailable`] when the stores cannot answer.
pub async fn resolve_screen_capability(
    app: &App,
    token: &str,
) -> Result<Option<crate::ports::ScreenCapability>, AppError> {
    let hash = app.services.tokens.hash(token)?;
    let Some(capability) = app.stores.screen_capabilities.find_by_hash(&hash).await? else {
        return Ok(None);
    };
    let Some(computer) = app
        .stores
        .computers
        .find_by_id(
            &TenantContext::System {
                reason: boma_domain::SystemReason::Authentication,
                user_id: None,
            },
            &capability.computer_id,
        )
        .await?
    else {
        return Ok(None);
    };
    if computer.status != boma_domain::ComputerStatus::Running
        || computer.screen_generation != capability.screen_generation
    {
        return Ok(None);
    }
    Ok(Some(capability))
}

/// Fetches the frame of the computer the capability opens (REQ-030).
///
/// # Errors
///
/// Returns [`AppError::Unavailable`] when the deployment has no computer backend or the frame
/// command fails, [`AppError::Rule`] when the capability does not point at a running computer.
pub async fn screen_frame(
    app: &App,
    capability: &crate::ports::ScreenCapability,
) -> Result<Vec<u8>, AppError> {
    let runtime = app
        .services
        .computer
        .as_deref()
        .filter(|runtime| runtime.provider() != "none")
        .ok_or(AppError::Unavailable)?;
    let context = TenantContext::System {
        reason: boma_domain::SystemReason::Authentication,
        user_id: None,
    };
    let computer = app
        .stores
        .computers
        .find_by_id(&context, &capability.computer_id)
        .await?
        .ok_or(AppError::Unavailable)?;
    let Some(external_id) = computer.external_id else {
        return Err(AppError::Unavailable);
    };
    runtime.screenshot(&external_id).await
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

/// What one tool call produced, to be appended as a `role=tool` message (REQ-016, REQ-017).
#[derive(Debug, Clone)]
pub struct ToolResultReport {
    /// The provider's correlation id for the call.
    pub id: String,
    /// The tool's key.
    pub name: String,
    /// Whether the call succeeded.
    pub ok: bool,
    /// The body the model reads as the result.
    pub body: serde_json::Value,
}

/// Appends one tool result as a `role=tool` message the next pass can read (REQ-016, REQ-017).
///
/// The correlation id and the tool's name are part of the body, so a client and a BYOK adapter
/// both see which call produced which result.
///
/// # Errors
///
/// Returns [`AppError::Unavailable`] when a store cannot answer.
pub async fn append_tool_result(
    app: &App,
    context: &TenantContext,
    thread: &Thread,
    bot_id: &boma_domain::BotId,
    run_id: &boma_domain::RunId,
    report: &ToolResultReport,
) -> Result<Message, AppError> {
    let payload = serde_json::json!({
        "id": report.id,
        "name": report.name,
        "ok": report.ok,
        "body": report.body,
    });
    let stored = app
        .stores
        .messages
        .insert(
            context,
            NewMessage {
                id: boma_domain::MessageId::new(app.services.ids.new_id("msg"))?,
                space_id: thread.space_id.clone(),
                thread_id: thread.id.clone(),
                role: boma_domain::MessageRole::Tool,
                blocks: vec![boma_domain::Block::Text {
                    text: payload.to_string(),
                }],
                bot_id: Some(bot_id.clone()),
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
                space_id: thread.space_id.clone(),
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

/// Parks a run on an approval: pending tool calls, one approval card, the run waiting on input.
///
/// REQ-017: a write-class tool call never has an effect before a person approves. The rows are
/// registered exactly once per `(run_id, call_id)` — a pass replay re-registers the same calls and
/// conflicts on the store's primary key, which is harmless. The approval card is exactly one per
/// run (`cards_thread_run_key`): a replay that finds a card already pending reuses it instead of
/// stacking a second. The run goes to `WaitingInput`; answering the card re-queues it.
///
/// # Errors
///
/// Returns [`AppError::Unavailable`] when a store cannot answer.
pub async fn park_tool_calls(
    app: &App,
    context: &TenantContext,
    claimed: &ClaimedRun,
    calls: Vec<crate::ports::ToolCallRequest>,
) -> Result<(), AppError> {
    let run = &claimed.run;
    let thread = &claimed.thread;
    let bot_id = claimed.bot.id.clone();
    let now = app.services.clock.now();
    for call in calls {
        let pending = crate::ports::NewPendingToolCall {
            run_id: run.id.clone(),
            call_id: call.id.clone(),
            space_id: thread.space_id.clone(),
            thread_id: thread.id.clone(),
            name: call.name.clone(),
            arguments: call.arguments.clone(),
            created_at: now.clone(),
        };
        match app.stores.pending_tool_calls.create(context, pending).await {
            Ok(_) => {}
            Err(AppError::Conflict) => {
                tracing::info!(run = %run.id, call_id = %call.id, "the call was already registered; the replay is absorbed");
            }
            Err(error) => return Err(error),
        }
    }
    let card = match app
        .stores
        .cards
        .create(
            context,
            crate::ports::NewCard {
                id: boma_domain::CardId::new(app.services.ids.new_id("card"))?,
                space_id: thread.space_id.clone(),
                thread_id: thread.id.clone(),
                kind: boma_domain::CardKind::Approval,
                created_at: now.clone(),
                run_id: Some(run.id.clone()),
            },
        )
        .await
    {
        Ok(card) => Some(card),
        Err(AppError::Conflict) => {
            // A card is already pending for this run (crash replay): keep it, the person answers
            // the one they can see, and the flip below stays idempotent.
            tracing::info!(run = %run.id, "an approval card is already pending; the replay is absorbed");
            None
        }
        Err(error) => return Err(error),
    };
    if let Some(card) = card {
        // The person sees the card as part of the thread (the UI renders the block).
        let stored = app
            .stores
            .messages
            .insert(
                context,
                NewMessage {
                    id: boma_domain::MessageId::new(app.services.ids.new_id("msg"))?,
                    space_id: thread.space_id.clone(),
                    thread_id: thread.id.clone(),
                    role: boma_domain::MessageRole::System,
                    blocks: vec![boma_domain::Block::Card {
                        card_id: card.id.as_str().to_owned(),
                        kind: "approval".to_owned(),
                    }],
                    bot_id: Some(bot_id),
                    author_user_id: None,
                    run_id: Some(run.id.clone()),
                    client_nonce: None,
                    created_at: now,
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
                    space_id: thread.space_id.clone(),
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
    }
    set_run_status(
        app,
        context,
        run.id.as_str(),
        RunStatus::WaitingInput,
        Some(run.lease_fence),
        None,
    )
    .await?;
    Ok(())
}

/// Marks the parked calls of a run refused, appends their refusals as `role=tool` messages, and
/// leaves the queue state alone (the resume decides what happens next).
///
/// # Errors
///
/// Returns [`AppError::Unavailable`] when a store cannot answer.
async fn refuse_pending_tool_calls(
    app: &App,
    context: &TenantContext,
    run_id: &boma_domain::RunId,
    bot: &Bot,
    thread: &Thread,
    calls: Vec<crate::ports::PendingToolCall>,
) -> Result<(), AppError> {
    let now = app.services.clock.now();
    for call in calls {
        let refused = serde_json::json!({ "error": "the person refused the call" });
        if let Err(error) = app
            .stores
            .pending_tool_calls
            .mark_refused(context, run_id, &call.call_id, &now)
            .await
        {
            tracing::warn!(run = %run_id, call_id = %call.call_id, %error, "the pending call could not be marked refused");
        }
        append_tool_result(
            app,
            context,
            thread,
            &bot.id,
            run_id,
            &ToolResultReport {
                id: call.call_id.clone(),
                name: call.name.clone(),
                ok: false,
                body: refused,
            },
        )
        .await?;
    }
    Ok(())
}

/// Applies the tool calls a run was parked on, exactly once each (REQ-017).
///
/// For every pending call this run registered: an effect key `tool:{run}:{call}` is recorded —
/// that key, not the answer, is the exactly-once guard — then the tool executes, then the row
/// flips to `applied`. A replay that finds the key already recorded does not re-execute; it marks
/// the row applied with a note instead, because the effect really happened already. When the
/// answer refuses, the calls are marked refused and never execute.
///
/// Returns whether any executed call created an asynchronous child run the parent must wait for
/// (today: `delegate.helper`). The caller leaves the parent suspended in that case; the helper
/// completion re-queues it.
///
/// # Errors
///
/// Returns [`AppError::Unavailable`] when a store cannot answer.
async fn apply_pending_tool_calls(
    app: &App,
    context: &TenantContext,
    run_id: &boma_domain::RunId,
    bot: &Bot,
    thread: &Thread,
    approve: bool,
) -> Result<bool, AppError> {
    let now = app.services.clock.now();
    let pending = app
        .stores
        .pending_tool_calls
        .find_pending(context, run_id)
        .await?;
    if !approve {
        refuse_pending_tool_calls(app, context, run_id, bot, thread, pending).await?;
        return Ok(false);
    }
    let mut child_created = false;
    for call in pending {
        let effect_key = format!("tool:{}:{}", run_id.as_str(), call.call_id);
        let already_applied = match app
            .stores
            .effects
            .record(
                context,
                crate::ports::NewEffect {
                    run_id: run_id.clone(),
                    effect_key,
                    kind: "tool".to_owned(),
                    payload: serde_json::json!({
                        "call_id": call.call_id,
                        "name": call.name,
                        "arguments": call.arguments,
                    }),
                },
            )
            .await
        {
            Ok(_) => false,
            Err(AppError::Conflict) => {
                tracing::warn!(run = %run_id, call_id = %call.call_id, "the effect is already recorded; the call is not executed again");
                true
            }
            Err(error) => return Err(error),
        };
        if already_applied {
            let note = serde_json::json!({ "note": "the effect was already applied" });
            let _ = app
                .stores
                .pending_tool_calls
                .mark_applied(context, run_id, &call.call_id, note.clone(), &now)
                .await;
            append_tool_result(
                app,
                context,
                thread,
                &bot.id,
                run_id,
                &ToolResultReport {
                    id: call.call_id.clone(),
                    name: call.name.clone(),
                    ok: true,
                    body: note,
                },
            )
            .await?;
            continue;
        }
        let (body, created) = execute_approved_tool(app, context, &call).await;
        child_created |= created;
        let ok = body
            .get("ok")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        if let Err(error) = app
            .stores
            .pending_tool_calls
            .mark_applied(context, run_id, &call.call_id, body.clone(), &now)
            .await
        {
            tracing::warn!(run = %run_id, call_id = %call.call_id, %error, "the pending call could not be marked applied");
        }
        append_tool_result(
            app,
            context,
            thread,
            &bot.id,
            run_id,
            &ToolResultReport {
                id: call.call_id.clone(),
                name: call.name.clone(),
                ok,
                body,
            },
        )
        .await?;
    }
    Ok(child_created)
}

/// Teaches a skill on approval (REQ-035): the member approved, the write happens once.
async fn approved_skill_teach(
    app: &App,
    context: &TenantContext,
    call: &crate::ports::PendingToolCall,
) -> (serde_json::Value, bool) {
    let string = |key: &str| -> Option<String> {
        call.arguments
            .get(key)
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
    };
    let Some(name) = string("name") else {
        return (
            serde_json::json!({ "ok": false, "body": { "error": "the tool needs a `name` argument" } }),
            false,
        );
    };
    let Some(description) = string("description") else {
        return (
            serde_json::json!({ "ok": false, "body": { "error": "the tool needs a `description` argument" } }),
            false,
        );
    };
    let Some(instructions) = string("instructions") else {
        return (
            serde_json::json!({ "ok": false, "body": { "error": "the tool needs an `instructions` argument" } }),
            false,
        );
    };
    let body = match teach_skill(
        app,
        context,
        &name,
        &description,
        &instructions,
        boma_domain::SkillOrigin::Taught,
        None,
    )
    .await
    {
        Ok(()) => serde_json::json!({ "ok": true, "body": { "name": name } }),
        Err(error) => {
            serde_json::json!({ "ok": false, "body": { "error": format!("the skill was not taught: {error}") } })
        }
    };
    (body, false)
}

/// Runs one approved tool call and reports what happened in a body shaped `{ ok, body }`,
/// plus whether the call created an asynchronous child run the parent must wait for.
///
/// An approval never fakes a success: the only tool whose effects are implemented here is
/// `memory.write`; anything else is refused inside the tool message instead of skipped silently.
async fn execute_approved_tool(
    app: &App,
    context: &TenantContext,
    call: &crate::ports::PendingToolCall,
) -> (serde_json::Value, bool) {
    let string = |key: &str| -> Option<String> {
        call.arguments
            .get(key)
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
    };
    match call.name.as_str() {
        "memory.write" => {
            let scope = string("scope").unwrap_or_else(|| "shared".to_owned());
            let bot_id = string("botId");
            let Some(path) = string("path") else {
                return (
                    serde_json::json!({ "ok": false, "body": { "error": "the tool needs a `path` argument" } }),
                    false,
                );
            };
            let Some(text) = string("text") else {
                return (
                    serde_json::json!({ "ok": false, "body": { "error": "the tool needs a `text` argument" } }),
                    false,
                );
            };
            let body =
                match memory_write(app, context, &scope, bot_id.as_deref(), &path, &text).await {
                    Ok(outcome) => serde_json::json!({
                        "ok": true,
                        "body": {
                            "path": outcome.document.path,
                            "revision": outcome.document.revision,
                        },
                    }),
                    Err(error) => serde_json::json!({
                        "ok": false,
                        "body": { "error": format!("the write did not happen: {error}") },
                    }),
                };
            (body, false)
        }
        "skill.teach" => approved_skill_teach(app, context, call).await,
        "delegate.helper" => {
            let Some(prompt) = string("prompt") else {
                return (
                    serde_json::json!({ "ok": false, "body": { "error": "the tool needs a `prompt` argument" } }),
                    false,
                );
            };
            match delegate_helper_run(app, context, call.run_id.as_str(), &prompt).await {
                Ok(helper) => (
                    serde_json::json!({
                        "ok": true,
                        "body": {
                            "helperRunId": helper.id.as_str(),
                            "status": helper.status.as_str(),
                        },
                    }),
                    true,
                ),
                Err(error) => (
                    serde_json::json!({
                        "ok": false,
                        "body": { "error": format!("the helper could not be created: {error}") },
                    }),
                    false,
                ),
            }
        }
        other => (
            serde_json::json!({
                "ok": false,
                "body": { "error": format!("the tool {other} cannot be applied yet") },
            }),
            false,
        ),
    }
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
    if let Some(run_id) = &card.run_id {
        // An approval card gates its effect; a question card does not, but both resume the run:
        // the person answered, so the conversation may move on (REQ-007, REQ-017).
        let is_approval = card.kind == boma_domain::CardKind::Approval;
        if is_approval && answer == "approve" {
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
        }
        // Locate the parked conversation: the run id is all the card carries, the thread and
        // the bot answer from it.
        let run = app
            .stores
            .runs
            .find(context, run_id)
            .await?
            .ok_or(AppError::Unavailable)?;
        let bot = app
            .stores
            .bots
            .find(context, &run.bot_id)
            .await?
            .ok_or(AppError::Unavailable)?;
        let thread = app
            .stores
            .threads
            .for_bot(context, &run.bot_id)
            .await?
            .ok_or(AppError::Unavailable)?;
        // Parked calls apply only from the card that parked them: approve applies, anything else
        // refuses; question cards have no parked calls.
        let mut child_created = false;
        if is_approval {
            child_created =
                apply_pending_tool_calls(app, context, run_id, &bot, &thread, answer == "approve")
                    .await?;
        }
        // The run that was waiting on the card resumes (REQ-017): parked on the queue, so the
        // claim picks it up again. A child run created by an approval re-queues the parent when
        // it completes (REQ-019); resuming now would race its result.
        if !child_created {
            set_run_status(app, context, run_id.as_str(), RunStatus::Queued, None, None).await?;
        }
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
/// The memory pair (REQ-022) and the helper delegate (REQ-019) land with the tool loop.
/// Computer reads (REQ-028) are read-only, and always go through the takeover and running gates.
/// `delegate.helper` is write-class: a helper mutates the conversation state machine.
///
/// # Why a function and not a const
///
/// A static of `String`-keyed specs would still allocate on first call; a `Vec` built once per
/// call keeps the catalogue construction simple and lets the loop extend it cheaply.
#[must_use]
pub fn builtin_tools() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            key: "memory.read".to_owned(),
            description: "Read a memory document this bot can see.".to_owned(),
            kind: boma_domain::ToolKind::Builtin,
            effect: boma_domain::ToolEffect::Read,
        },
        ToolSpec {
            key: "memory.write".to_owned(),
            description: "Append a revision to a memory document this bot can write.".to_owned(),
            kind: boma_domain::ToolKind::Memory,
            effect: boma_domain::ToolEffect::Write,
        },
        ToolSpec {
            key: "delegate.helper".to_owned(),
            description: "Ask a scoped helper run to work on the same thread.".to_owned(),
            kind: boma_domain::ToolKind::Builtin,
            effect: boma_domain::ToolEffect::Write,
        },
        ToolSpec {
            key: "computer.observe".to_owned(),
            description: "Capture the current screen of this bot's computer.".to_owned(),
            kind: boma_domain::ToolKind::Builtin,
            effect: boma_domain::ToolEffect::Read,
        },
        ToolSpec {
            key: "skill.load".to_owned(),
            description: "Load the full instructions of a skill this run may use, by name."
                .to_owned(),
            kind: boma_domain::ToolKind::Builtin,
            effect: boma_domain::ToolEffect::Read,
        },
        ToolSpec {
            key: "skill.teach".to_owned(),
            description: "Teach this space a new skill from instructions.".to_owned(),
            kind: boma_domain::ToolKind::Builtin,
            effect: boma_domain::ToolEffect::Write,
        },
    ]
}

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
            routine_id: None,
        })
        .await?;

    tracing::info!(run = %run.id, parent = %parent.id, "the parent delegated to a helper run");
    Ok(run)
}

/// A helper run finished (or failed): record its outcome as a `role=tool` message on the
/// parent thread and re-queue the parent so the worker re-enters it (REQ-019).
///
/// The parent is re-queued only when it is still waiting for input; a parent that was
/// cancelled or already answered is left alone. Returns whether the parent was re-queued.
///
/// `outcome` is the structured result the helper reported: the answer text for a completed
/// helper, or a refusal-safe failure summary. Helper internals still do not surface as chat:
/// the message role is `tool` (REQ-019).
///
/// # Errors
///
/// Returns [`AppError::Unavailable`] when a store cannot answer.
pub async fn complete_helper_run(
    app: &App,
    context: &TenantContext,
    helper: &Run,
    outcome: &str,
) -> Result<bool, AppError> {
    let Some(parent_run_id) = helper.parent_run_id.clone() else {
        return Ok(false);
    };
    let Some(parent) = app.stores.runs.find(context, &parent_run_id).await? else {
        return Ok(false);
    };
    let Some(thread) = app.stores.threads.for_bot(context, &parent.bot_id).await? else {
        return Ok(false);
    };
    let now = app.services.clock.now();
    let body = serde_json::json!({
        "tool": "delegate.helper",
        "helperRunId": helper.id.as_str(),
        "result": outcome,
    });
    let stored = app
        .stores
        .messages
        .insert(
            context,
            NewMessage {
                id: boma_domain::MessageId::new(app.services.ids.new_id("msg"))?,
                space_id: thread.space_id.clone(),
                thread_id: thread.id.clone(),
                role: boma_domain::MessageRole::Tool,
                blocks: vec![boma_domain::Block::Text {
                    text: body.to_string(),
                }],
                bot_id: Some(parent.bot_id.clone()),
                author_user_id: None,
                run_id: Some(parent.id.clone()),
                client_nonce: None,
                created_at: now.clone(),
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
                space_id: thread.space_id.clone(),
                thread_id: thread.id.clone(),
                kind: boma_domain::EventType::Message,
                payload: serde_json::json!({
                    "messageId": stored.message.id.as_str(),
                    "seq": stored.message.seq,
                }),
                created_at: now,
            },
        )
        .await?;

    if parent.status == RunStatus::WaitingInput {
        set_run_status(
            app,
            context,
            parent.id.as_str(),
            RunStatus::Queued,
            None,
            None,
        )
        .await?;
        tracing::info!(parent = %parent.id, helper = %helper.id, "the helper completed; the parent is re-queued");
        return Ok(true);
    }
    Ok(false)
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
    let mut tools = builtin_tools();
    tools.extend(app.stores.tools.catalogue_for(context, &bot.id).await?);
    Ok(tools)
}

/// Dispatches one tool call against the catalogue of this run (REQ-016, REQ-017).
///
/// Read effects execute inline; write and spend effects do not touch state and answer with the
/// approval marker instead, so a tool call can never have a side effect before a member approves.
/// A call to a tool the bot may not use is refused and recorded as such, never executed.
///
/// # Errors
///
/// Returns [`AppError::NotFound`] when the bot does not exist in the caller's tenant. The
/// refusal of an unknown tool is not an error: it is a `ToolCallResult` with `ok: false`, because
/// the loop folds refusals into the next model turn.
pub async fn dispatch_tool(
    app: &App,
    context: &TenantContext,
    bot_id: &str,
    call: &crate::ports::ToolCallRequest,
) -> Result<crate::ports::ToolCallResult, AppError> {
    let catalogue = tool_catalogue(app, context, bot_id).await?;
    let Some(tool) = catalogue.iter().find(|tool| tool.key == call.name) else {
        return Ok(crate::ports::ToolCallResult {
            id: call.id.clone(),
            ok: false,
            body: serde_json::json!({ "error": "the bot may not use this tool" }),
        });
    };
    match tool.effect {
        boma_domain::ToolEffect::Read => dispatch_read_tool(app, context, call).await,
        // Writes and spends never reach a real effect here: they wait for approval (REQ-017).
        boma_domain::ToolEffect::Write => Ok(crate::ports::ToolCallResult {
            id: call.id.clone(),
            ok: true,
            body: serde_json::json!({ "approval": "required" }),
        }),
        boma_domain::ToolEffect::Spend => Ok(crate::ports::ToolCallResult {
            id: call.id.clone(),
            ok: true,
            body: serde_json::json!({ "approval": "required", "kind": "spend" }),
        }),
    }
}

/// Runs the read effects inline against the stores (REQ-016).
async fn dispatch_read_tool(
    app: &App,
    context: &TenantContext,
    call: &crate::ports::ToolCallRequest,
) -> Result<crate::ports::ToolCallResult, AppError> {
    let string = |key: &str| -> Option<String> {
        call.arguments
            .get(key)
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
    };
    match call.name.as_str() {
        "memory.read" => {
            let bot_id = string("botId");
            let scope = string("scope").unwrap_or_else(|| "shared".to_owned());
            let Some(path) = string("path") else {
                return Ok(crate::ports::ToolCallResult {
                    id: call.id.clone(),
                    ok: false,
                    body: serde_json::json!({ "error": "the tool needs a `path` argument" }),
                });
            };
            match memory_read(app, context, &scope, bot_id.as_deref(), &path).await {
                Ok(document) => Ok(crate::ports::ToolCallResult {
                    id: call.id.clone(),
                    ok: true,
                    body: serde_json::json!({
                        "path": document.path,
                        "content": document.content,
                        "revision": document.revision,
                    }),
                }),
                Err(AppError::NotFound) => Ok(crate::ports::ToolCallResult {
                    id: call.id.clone(),
                    ok: false,
                    body: serde_json::json!({ "error": "the document does not exist" }),
                }),
                Err(error) => Err(error),
            }
        }
        "skill.load" => {
            let Some(name) = string("name") else {
                return Ok(crate::ports::ToolCallResult {
                    id: call.id.clone(),
                    ok: false,
                    body: serde_json::json!({ "error": "the tool needs a `name` argument" }),
                });
            };
            match skill_get(app, context, &name).await {
                Ok(skill) => Ok(crate::ports::ToolCallResult {
                    id: call.id.clone(),
                    ok: true,
                    body: serde_json::json!({
                        "name": skill.name,
                        "origin": skill.origin,
                        "instructions": skill.instructions,
                    }),
                }),
                Err(AppError::NotFound) => Ok(crate::ports::ToolCallResult {
                    id: call.id.clone(),
                    ok: false,
                    body: serde_json::json!({ "error": format!("the bot does not know a skill named `{name}`") }),
                }),
                Err(error) => Err(error),
            }
        }
        // Computer reads are only callable from a running computer, and this use case enforces
        // that: dispatching the tool reports that the observation itself needs a computer to
        // answer first. Budgeting and base64 stay out of the model's reach.
        other => Ok(crate::ports::ToolCallResult {
            id: call.id.clone(),
            ok: false,
            body: serde_json::json!({ "error": format!("the read tool {other} is not dispatchable yet") }),
        }),
    }
}
