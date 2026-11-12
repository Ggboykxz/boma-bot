//! The rows the database returns, and how they become domain types.
//!
//! Every read goes through one of these structs: the shape of the row is written down once, and
//! the mapping to a domain type is written down once, so a change of column name breaks in one
//! place instead of in every query.

use std::str::FromStr;

use boma_app::AppError;
use boma_domain::{
    Block, Bot, BotId, Card, CardId, CardKind, CardStatus, Email, Event, EventType, ExternalEffect,
    InviteId, Message, MessageId, MessageRole, OrganizationId, Run, RunId, RunStatus, RunTrigger,
    Session, SessionId, Space, SpaceId, SpaceRole, Thread, ThreadId, ToolKind, ToolSpec, User,
    UserId,
};
use serde_json::Value;
use sqlx::types::Json;

/// A row of `users`.
#[derive(Debug, sqlx::FromRow)]
pub(crate) struct UserRow {
    pub(crate) id: String,
    pub(crate) email: String,
    pub(crate) name: String,
    pub(crate) email_verified: bool,
    pub(crate) locale: String,
    pub(crate) password_hash: String,
    pub(crate) deactivated_at: Option<String>,
}

/// A row of `sessions`.
#[derive(Debug, sqlx::FromRow)]
pub(crate) struct SessionRow {
    pub(crate) id: String,
    pub(crate) user_id: String,
    pub(crate) token_hash: Vec<u8>,
    pub(crate) expires_at: String,
}

/// A row of `spaces`.
#[derive(Debug, sqlx::FromRow)]
pub(crate) struct SpaceRow {
    pub(crate) id: String,
    pub(crate) organization_id: String,
    pub(crate) name: String,
    pub(crate) is_default: bool,
}

/// A row of `space_members`.
#[derive(Debug, sqlx::FromRow)]
pub(crate) struct MembershipRow {
    pub(crate) role: String,
}

/// A row of `bots`.
#[derive(Debug, sqlx::FromRow)]
pub(crate) struct BotRow {
    pub(crate) id: String,
    pub(crate) space_id: String,
    pub(crate) user_id: String,
    pub(crate) name: String,
    pub(crate) description: Option<String>,
    pub(crate) color: Option<String>,
    pub(crate) thread_id: Option<String>,
    pub(crate) archived_at: Option<String>,
    pub(crate) created_at: String,
    pub(crate) enabled_tools: Vec<String>,
    pub(crate) model_provider: Option<String>,
    pub(crate) model_id: Option<String>,
    pub(crate) model_pinned: bool,
}

/// A row of `threads`.
#[derive(Debug, sqlx::FromRow)]
pub(crate) struct ThreadRow {
    pub(crate) id: String,
    pub(crate) space_id: String,
    pub(crate) bot_id: String,
    pub(crate) user_id: String,
    pub(crate) next_message_seq: i64,
    pub(crate) next_event_seq: i64,
    pub(crate) created_at: String,
}

/// A row of `messages`.
#[derive(Debug, sqlx::FromRow)]
pub(crate) struct MessageRow {
    pub(crate) id: String,
    pub(crate) thread_id: String,
    pub(crate) seq: i64,
    pub(crate) role: String,
    pub(crate) blocks: Json<Value>,
    pub(crate) bot_id: Option<String>,
    pub(crate) author_user_id: Option<String>,
    pub(crate) run_id: Option<String>,
    pub(crate) client_nonce: Option<String>,
    pub(crate) created_at: String,
    pub(crate) reply_to_message_id: Option<String>,
    pub(crate) reply_quote: Option<String>,
}

/// A row of `runs`.
#[derive(Debug, sqlx::FromRow)]
pub(crate) struct RunRow {
    pub(crate) id: String,
    pub(crate) space_id: String,
    pub(crate) bot_id: String,
    pub(crate) thread_id: String,
    pub(crate) user_id: String,
    pub(crate) status: String,
    pub(crate) trigger: String,
    pub(crate) client_nonce: Option<String>,
    pub(crate) source_message_id: Option<String>,
    pub(crate) lease_owner: Option<String>,
    pub(crate) lease_fence: i64,
    pub(crate) lease_expires_at: Option<String>,
    pub(crate) started_at: Option<String>,
    pub(crate) completed_at: Option<String>,
    pub(crate) error: Option<String>,
    pub(crate) steering_from_seq: Option<i64>,
    pub(crate) steering_consumed_seq: i64,
    pub(crate) model_provider: Option<String>,
    pub(crate) model_id: Option<String>,
    pub(crate) model_pinned: bool,
    pub(crate) parent_run_id: Option<String>,
    pub(crate) created_at: String,
}

/// A row of `events`.
#[derive(Debug, sqlx::FromRow)]
pub(crate) struct EventRow {
    pub(crate) thread_id: String,
    pub(crate) seq: i64,
    // The column is named `type`; the domain calls it `kind`.
    #[sqlx(rename = "type")]
    pub(crate) kind: String,
    pub(crate) payload: Json<Value>,
    pub(crate) created_at: String,
}

/// A row of `invitations`.
#[derive(Debug, sqlx::FromRow)]
pub(crate) struct InvitationRow {
    pub(crate) id: String,
    pub(crate) space_id: String,
    pub(crate) email: String,
    pub(crate) role: String,
    pub(crate) expires_at: String,
    pub(crate) status: String,
}

/// A row of `cards`.
#[derive(Debug, sqlx::FromRow)]
pub(crate) struct CardRow {
    pub(crate) id: String,
    pub(crate) space_id: String,
    pub(crate) thread_id: String,
    pub(crate) run_id: Option<String>,
    pub(crate) kind: String,
    pub(crate) status: String,
    pub(crate) answer: Option<String>,
    pub(crate) created_at: String,
    pub(crate) answered_at: Option<String>,
}

/// A row of `external_effects`.
#[derive(Debug, sqlx::FromRow)]
pub(crate) struct EffectRow {
    pub(crate) run_id: String,
    pub(crate) effect_key: String,
    pub(crate) kind: String,
    pub(crate) payload: Json<Value>,
    pub(crate) applied_at: Option<String>,
}

/// A row of the activity projection: a run and the beginning of the message that started it.
#[derive(Debug, sqlx::FromRow)]
pub(crate) struct ActivityRow {
    #[sqlx(flatten)]
    pub(crate) run: RunRow,
    pub(crate) prompt_blocks: Option<Json<Value>>,
}

/// Turns a stored row into the person it describes.
pub(crate) fn user(row: UserRow) -> Result<User, AppError> {
    Ok(User {
        id: UserId::new(row.id)?,
        email: Email::new(row.email)?,
        name: row.name,
        email_verified: row.email_verified,
        locale: row.locale,
        deactivated_at: row.deactivated_at,
    })
}

/// Turns a stored row into the session it describes.
pub(crate) fn session(row: SessionRow) -> Result<Session, AppError> {
    Ok(Session {
        id: SessionId::new(row.id)?,
        user_id: UserId::new(row.user_id)?,
        expires_at: row.expires_at,
    })
}

/// Turns a stored row into the space it describes.
pub(crate) fn space(row: SpaceRow) -> Result<Space, AppError> {
    Ok(Space {
        id: SpaceId::new(row.id)?,
        organization_id: OrganizationId::new(row.organization_id)?,
        name: row.name,
        is_default: row.is_default,
    })
}

/// Turns a stored row into the role it describes.
pub(crate) fn role(value: &str) -> Result<SpaceRole, AppError> {
    Ok(SpaceRole::parse(value)?)
}

/// Turns a stored row into the bot it describes.
pub(crate) fn bot(row: BotRow) -> Result<Bot, AppError> {
    Ok(Bot {
        id: BotId::new(row.id)?,
        space_id: SpaceId::new(row.space_id)?,
        user_id: UserId::new(row.user_id)?,
        name: row.name,
        description: row.description,
        color: row.color,
        thread_id: ThreadId::new(row.thread_id.unwrap_or_default())?,
        archived_at: row.archived_at,
        created_at: row.created_at,
        enabled_tools: row.enabled_tools,
        model_provider: row.model_provider,
        model_id: row.model_id,
        model_pinned: row.model_pinned,
    })
}

/// Turns a roster row into the summary the rail renders (REQ-009).
pub(crate) fn bot_summary(row: BotSummaryRow) -> Result<boma_app::ports::BotSummary, AppError> {
    let bot_row = BotRow {
        id: row.id,
        space_id: row.space_id,
        user_id: row.user_id,
        name: row.name,
        description: row.description,
        color: row.color,
        thread_id: row.thread_id.clone(),
        archived_at: row.archived_at,
        created_at: row.created_at,
        enabled_tools: row.enabled_tools,
        model_provider: row.model_provider,
        model_id: row.model_id,
        model_pinned: row.model_pinned,
    };
    Ok(boma_app::ports::BotSummary {
        bot: bot(bot_row)?,
        thread_id: ThreadId::new(row.thread_id.unwrap_or_default())?,
        snoozed_until: row.snoozed_until,
        unread_count: u64::try_from(row.unread_count).unwrap_or(u64::MAX),
    })
}

/// Turns a stored row into the thread it describes.
pub(crate) fn thread(row: ThreadRow) -> Result<Thread, AppError> {
    Ok(Thread {
        id: ThreadId::new(row.id)?,
        space_id: SpaceId::new(row.space_id)?,
        bot_id: BotId::new(row.bot_id)?,
        user_id: UserId::new(row.user_id)?,
        next_message_seq: row.next_message_seq,
        next_event_seq: row.next_event_seq,
        created_at: row.created_at,
    })
}

/// Turns a stored row into the message it describes.
pub(crate) fn message(row: MessageRow) -> Result<Message, AppError> {
    let blocks: Vec<Block> = serde_json::from_value(row.blocks.0).map_err(|error| {
        tracing::debug!(error = %error, "a stored message carries blocks the product cannot read");
        AppError::Unavailable
    })?;
    Ok(Message {
        id: MessageId::new(row.id)?,
        thread_id: ThreadId::new(row.thread_id)?,
        seq: row.seq,
        role: MessageRole::parse(&row.role)?,
        blocks,
        bot_id: row.bot_id.map(BotId::new).transpose()?,
        author_user_id: row.author_user_id.map(UserId::new).transpose()?,
        run_id: row.run_id.map(RunId::new).transpose()?,
        reply_to_message_id: row.reply_to_message_id.map(MessageId::new).transpose()?,
        reply_quote: row.reply_quote,
        client_nonce: row.client_nonce,
        created_at: row.created_at,
        reactions: Vec::new(),
    })
}

/// Turns a stored row into the run it describes.
pub(crate) fn run(row: RunRow) -> Result<Run, AppError> {
    Ok(Run {
        id: RunId::new(row.id)?,
        space_id: SpaceId::new(row.space_id)?,
        bot_id: BotId::new(row.bot_id)?,
        thread_id: ThreadId::new(row.thread_id)?,
        user_id: UserId::new(row.user_id)?,
        status: RunStatus::from_str(&row.status)?,
        trigger: RunTrigger::from_str(&row.trigger)?,
        client_nonce: row.client_nonce,
        source_message_id: row.source_message_id.map(MessageId::new).transpose()?,
        created_at: row.created_at,
        started_at: row.started_at,
        completed_at: row.completed_at,
        error: row.error,
        lease_owner: row.lease_owner,
        lease_fence: row.lease_fence,
        lease_expires_at: row.lease_expires_at,
        steering_from_seq: row.steering_from_seq,
        steering_consumed_seq: row.steering_consumed_seq,
        model_provider: row.model_provider,
        model_id: row.model_id,
        model_pinned: row.model_pinned,
        parent_run_id: row.parent_run_id.map(RunId::new).transpose()?,
    })
}

/// Turns a stored row into the event it describes.
pub(crate) fn event(row: EventRow) -> Result<Event, AppError> {
    Ok(Event {
        thread_id: ThreadId::new(row.thread_id)?,
        seq: row.seq,
        kind: EventType::parse(&row.kind)?,
        payload: row.payload.0,
        created_at: row.created_at,
    })
}

/// Turns a stored row into the invitation it describes.
pub(crate) fn invitation(row: InvitationRow) -> Result<boma_app::ports::Invitation, AppError> {
    Ok(boma_app::ports::Invitation {
        id: InviteId::new(row.id)?,
        space_id: SpaceId::new(row.space_id)?,
        email: Email::new(row.email)?,
        role: SpaceRole::parse(&row.role)?,
        expires_at: row.expires_at,
        pending: row.status == "pending",
    })
}

/// Turns a stored row into the card it describes.
pub(crate) fn card(row: CardRow) -> Result<Card, AppError> {
    let kind = match row.kind.as_str() {
        "question" => CardKind::Question,
        "approval" => CardKind::Approval,
        "purchase" => CardKind::Purchase,
        "takeover" => CardKind::Takeover,
        _ => {
            return Err(AppError::Rule(boma_domain::DomainError::UnknownEventType(
                row.kind,
            )));
        }
    };
    let status = match row.status.as_str() {
        "pending" => CardStatus::Pending,
        "answered" => CardStatus::Answered,
        "declined" => CardStatus::Declined,
        _ => {
            return Err(AppError::Rule(boma_domain::DomainError::UnknownEventType(
                row.status,
            )));
        }
    };
    Ok(Card {
        id: CardId::new(row.id)?,
        space_id: SpaceId::new(row.space_id)?,
        thread_id: ThreadId::new(row.thread_id)?,
        run_id: row.run_id.map(RunId::new).transpose()?,
        kind,
        status,
        answer: row.answer,
        created_at: row.created_at,
        answered_at: row.answered_at,
    })
}

/// A row of `bots`, with the member-specific state the rail renders (REQ-009).
#[derive(Debug, sqlx::FromRow)]
pub(crate) struct BotSummaryRow {
    pub(crate) id: String,
    pub(crate) space_id: String,
    pub(crate) user_id: String,
    pub(crate) name: String,
    pub(crate) description: Option<String>,
    pub(crate) color: Option<String>,
    pub(crate) thread_id: Option<String>,
    pub(crate) archived_at: Option<String>,
    pub(crate) created_at: String,
    pub(crate) enabled_tools: Vec<String>,
    pub(crate) model_provider: Option<String>,
    pub(crate) model_id: Option<String>,
    pub(crate) model_pinned: bool,
    pub(crate) snoozed_until: Option<String>,
    pub(crate) unread_count: i64,
}

/// A row of `tool_installs`.
#[derive(Debug, sqlx::FromRow)]
pub(crate) struct ToolInstallRow {
    pub(crate) tool_key: String,
    pub(crate) description: String,
    pub(crate) kind: String,
}

/// A row of `message_reactions`.
#[derive(Debug, sqlx::FromRow)]
pub(crate) struct ReactionRow {
    pub(crate) emoji: String,
    pub(crate) count: i64,
    pub(crate) reacted: bool,
}

/// A row of `message_reactions` read for hydration, attributed to the caller in Rust.
#[derive(Debug, sqlx::FromRow)]
pub(crate) struct ReactionAttributionRow {
    pub(crate) message_id: String,
    pub(crate) emoji: String,
    pub(crate) user_id: String,
}

/// Turns a stored row into the effect it describes.
pub(crate) fn effect(row: EffectRow) -> Result<ExternalEffect, AppError> {
    Ok(ExternalEffect {
        run_id: RunId::new(row.run_id)?,
        effect_key: row.effect_key,
        kind: row.kind,
        payload: row.payload.0,
        applied_at: row.applied_at,
    })
}

/// Turns a stored row into the tool spec it describes.
pub(crate) fn tool_spec(row: ToolInstallRow) -> Result<ToolSpec, AppError> {
    Ok(ToolSpec {
        key: row.tool_key,
        description: row.description,
        kind: ToolKind::parse(&row.kind)?,
    })
}

/// The first text of a message, shortened to one line: the activity list shows it as a hint.
pub(crate) fn prompt_snippet(blocks: &Value) -> Option<String> {
    let blocks: Vec<Block> = serde_json::from_value(blocks.clone()).ok()?;
    let text = blocks
        .iter()
        .filter_map(Block::as_text)
        .collect::<Vec<_>>()
        .join(" ");
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.is_empty() {
        return None;
    }
    Some(text.chars().take(PROMPT_SNIPPET_LENGTH).collect())
}

/// How many characters of a prompt the activity list shows.
const PROMPT_SNIPPET_LENGTH: usize = 120;
