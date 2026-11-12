//! The ports the use cases need from the outside world.
//!
//! Every trait here is an interface the product owns: the application layer never names a
//! concrete adapter, so a test can drive a use case with an in-memory fake and the Postgres
//! implementation can be swapped without touching a business rule.
//!
//! Two rules shape these signatures:
//!
//! * **Sequences are allocated by the store, not by the caller.** A per-thread sequence that the
//!   caller computes is a race (REQ-005): two concurrent sends would claim the same number. The
//!   store allocates it, in the same transaction as the row it belongs to.
//! * **A multi-table write is one port call.** Creating a bot also creates its thread, in one
//!   transaction (REQ-004): a bot without a conversation is a half-created bot.

use async_trait::async_trait;
use boma_domain::{
    Bot, Card, CardId, Email, Event, ExternalEffect, Message, MessagePage, Reaction, Run, RunId,
    RunStatus, Session, Space, SpaceRole, TenantContext, Thread, ToolSpec, User, UserId,
};

use crate::error::AppError;

/// A user to create.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewUser {
    /// The person's id.
    pub id: UserId,
    /// The address they sign in with.
    pub email: Email,
    /// The hash of their password.
    pub password_hash: String,
    /// The name a person sees.
    pub name: String,
    /// The locale the interface is shown in.
    pub locale: String,
    /// When the account was created.
    pub created_at: String,
}

/// An organization to create: the billing entity that owns spaces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewOrganization {
    /// The organization's id.
    pub id: boma_domain::OrganizationId,
    /// The name a person sees.
    pub name: String,
    /// When it was created.
    pub created_at: String,
}

/// A space to create.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewSpace {
    /// The tenant's id.
    pub id: boma_domain::SpaceId,
    /// The billing entity that owns it.
    pub organization_id: boma_domain::OrganizationId,
    /// The name a person sees.
    pub name: String,
    /// Whether this is the space a member lands in.
    pub is_default: bool,
    /// When it was created.
    pub created_at: String,
}

/// Everything a new account needs, written in one transaction.
///
/// The user, their organization, their first space and their membership in it belong together: a
/// person with no space cannot do anything, and a space with no member is invisible (REQ-042).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewAccount {
    /// The person.
    pub user: NewUser,
    /// The billing entity.
    pub organization: NewOrganization,
    /// The first space.
    pub space: NewSpace,
}

/// The result of creating an account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    /// The person.
    pub user: User,
    /// The space they land in.
    pub space: Space,
}

/// A session to create.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewSession {
    /// The session's id.
    pub id: boma_domain::SessionId,
    /// The person it belongs to.
    pub user_id: UserId,
    /// The hash of the cookie's token.
    pub token_hash: Vec<u8>,
    /// When it expires.
    pub expires_at: String,
    /// When it was created.
    pub created_at: String,
}

/// A bot to create.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewBot {
    /// The bot's id.
    pub id: boma_domain::BotId,
    /// The tenant it belongs to.
    pub space_id: boma_domain::SpaceId,
    /// The member who creates it.
    pub user_id: UserId,
    /// The name a person sees.
    pub name: String,
    /// What the bot is for.
    pub description: Option<String>,
    /// The colour a person sees.
    pub color: Option<String>,
    /// When it was created.
    pub created_at: String,
}

/// A thread to create.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewThread {
    /// The thread's id.
    pub id: boma_domain::ThreadId,
    /// The tenant it belongs to.
    pub space_id: boma_domain::SpaceId,
    /// The bot whose conversation this is.
    pub bot_id: boma_domain::BotId,
    /// The member who owns it.
    pub user_id: UserId,
    /// When it was created.
    pub created_at: String,
}

/// A message to store.
///
/// The sequence is *not* here: the store allocates it, atomically with the insert (REQ-005).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewMessage {
    /// The message's id.
    pub id: boma_domain::MessageId,
    /// The tenant it belongs to.
    pub space_id: boma_domain::SpaceId,
    /// The thread it belongs to.
    pub thread_id: boma_domain::ThreadId,
    /// Who wrote it.
    pub role: boma_domain::MessageRole,
    /// The blocks it is made of.
    pub blocks: Vec<boma_domain::Block>,
    /// The bot that wrote it.
    pub bot_id: Option<boma_domain::BotId>,
    /// The person who wrote it.
    pub author_user_id: Option<UserId>,
    /// The run that produced it.
    pub run_id: Option<boma_domain::RunId>,
    /// The nonce that makes the send idempotent.
    pub client_nonce: Option<String>,
    /// When it was written.
    pub created_at: String,
    /// The message this one answers, when it is a reply.
    pub reply_to_message_id: Option<boma_domain::MessageId>,
    /// A short quote of the message being answered.
    pub reply_quote: Option<String>,
}

/// The outcome of storing a message: the message, and whether it was new.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredMessage {
    /// The stored message.
    pub message: Message,
    /// `false` when the message already existed for that nonce.
    pub created: bool,
}

/// A run to create.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewRun {
    /// The run's id.
    pub id: boma_domain::RunId,
    /// The tenant it belongs to.
    pub space_id: boma_domain::SpaceId,
    /// The bot that works.
    pub bot_id: boma_domain::BotId,
    /// The thread it writes to.
    pub thread_id: boma_domain::ThreadId,
    /// The member who started it.
    pub user_id: UserId,
    /// What caused it.
    pub trigger: boma_domain::RunTrigger,
    /// The nonce that makes the start idempotent.
    pub client_nonce: Option<String>,
    /// The message that started it.
    pub source_message_id: Option<boma_domain::MessageId>,
    /// When it was created.
    pub created_at: String,
    /// The provider the run resolved, or `None` when the resolver has not run yet.
    pub model_provider: Option<String>,
    /// The model the run resolved, or `None` when the resolver has not run yet.
    pub model_id: Option<String>,
    /// Whether the resolved model is pinned: a pinned decision never falls back silently.
    pub model_pinned: bool,
    /// The run that delegated to this one, when this run is a helper (REQ-019).
    pub parent_run_id: Option<boma_domain::RunId>,
    /// The routine this run wakes for (REQ-023), when it woke for one.
    pub routine_id: Option<boma_domain::RoutineId>,
}

/// An event to append.
///
/// The sequence is allocated by the store, atomically with the append (REQ-005, REQ-020).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewEvent {
    /// The tenant it belongs to.
    pub space_id: boma_domain::SpaceId,
    /// The thread it belongs to.
    pub thread_id: boma_domain::ThreadId,
    /// What it announces.
    pub kind: boma_domain::EventType,
    /// Its body.
    pub payload: serde_json::Value,
    /// When it happened.
    pub created_at: String,
}

/// An invitation to create.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewInvitation {
    /// The invitation's id.
    pub id: boma_domain::InviteId,
    /// The tenant it belongs to.
    pub space_id: boma_domain::SpaceId,
    /// Who is invited.
    pub email: Email,
    /// The hash of the token; the token itself is never stored.
    pub token_hash: Vec<u8>,
    /// The role the invitee will have.
    pub role: SpaceRole,
    /// When it expires.
    pub expires_at: String,
    /// When it was created.
    pub created_at: String,
}

/// An invitation, as the caller sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invitation {
    /// The invitation's id.
    pub id: boma_domain::InviteId,
    /// The tenant it belongs to.
    pub space_id: boma_domain::SpaceId,
    /// Who is invited.
    pub email: Email,
    /// The role the invitee will have.
    pub role: SpaceRole,
    /// When it expires.
    pub expires_at: String,
    /// Whether it is still pending.
    pub pending: bool,
}

/// A row of the activity projection (REQ-011).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunActivity {
    /// The run.
    pub run: Run,
    /// The beginning of the message that started it, short enough to render on one line.
    pub prompt_snippet: Option<String>,
}

/// A page of activity, newest first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivityPage {
    /// The rows.
    pub runs: Vec<RunActivity>,
    /// The cursor for the page before this one, when there is one.
    pub next_cursor: Option<String>,
}

/// Creates and reads accounts.
#[async_trait]
pub trait AccountStore: Send + Sync {
    /// Creates the person, their organization, their first space and their membership.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Conflict`] when the address is already registered.
    async fn create_account(&self, account: NewAccount) -> Result<Account, AppError>;
}

/// Stores and reads people.
#[async_trait]
pub trait UserStore: Send + Sync {
    /// Finds a person by address, with the hash of their password.
    ///
    /// The hash travels with the person so that sign-in never has to fetch it separately, and it
    /// never travels further: the use case hashes the password and compares.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn find_by_email(&self, email: &Email) -> Result<Option<(User, String)>, AppError>;

    /// Finds a person by id.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn find(&self, id: &UserId) -> Result<Option<User>, AppError>;
}

/// Stores and reads sessions.
#[async_trait]
pub trait SessionStore: Send + Sync {
    /// Creates a session.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn create(&self, session: NewSession) -> Result<Session, AppError>;

    /// Finds the session a token hash belongs to.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn find_by_token_hash(&self, token_hash: &[u8]) -> Result<Option<Session>, AppError>;

    /// Deletes a session.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn delete(&self, id: &boma_domain::SessionId) -> Result<(), AppError>;
}

/// Stores and reads spaces.
#[async_trait]
pub trait SpaceStore: Send + Sync {
    /// Creates a space and the creator's membership.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn create(&self, space: NewSpace, owner: &UserId) -> Result<Space, AppError>;

    /// Finds a space inside a tenant.
    ///
    /// A space in another tenant is *not found* (REQ-043).
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn find(
        &self,
        space_id: &boma_domain::SpaceId,
        user_id: &UserId,
    ) -> Result<Option<Space>, AppError>;

    /// The role a member has in a space, if any.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn membership(
        &self,
        space_id: &boma_domain::SpaceId,
        user_id: &UserId,
    ) -> Result<Option<SpaceRole>, AppError>;

    /// The space a member lands in.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn default_for(&self, user_id: &UserId) -> Result<Option<Space>, AppError>;
}

/// Stores and reads bots.
#[async_trait]
pub trait BotStore: Send + Sync {
    /// Creates a bot and its thread, in one transaction.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn create_with_thread(
        &self,
        bot: NewBot,
        thread: NewThread,
    ) -> Result<(Bot, Thread), AppError>;

    /// Finds a bot inside a tenant.
    ///
    /// A bot in another tenant is *not found*: the difference between "forbidden" and "not
    /// found" would confirm that the id exists (REQ-043).
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn find(
        &self,
        context: &boma_domain::TenantContext,
        id: &boma_domain::BotId,
    ) -> Result<Option<Bot>, AppError>;
}

/// Stores and reads threads.
#[async_trait]
pub trait ThreadStore: Send + Sync {
    /// Finds a thread inside a tenant.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn find(
        &self,
        context: &boma_domain::TenantContext,
        id: &boma_domain::ThreadId,
    ) -> Result<Option<Thread>, AppError>;

    /// The conversation of a bot.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn for_bot(
        &self,
        context: &boma_domain::TenantContext,
        bot_id: &boma_domain::BotId,
    ) -> Result<Option<Thread>, AppError>;
}

/// Stores and reads messages.
#[async_trait]
pub trait MessageStore: Send + Sync {
    /// Stores a message, or returns the one that already carries that nonce.
    ///
    /// The sequence is allocated by the store, atomically with the insert, so two concurrent
    /// sends cannot claim the same number (REQ-005).
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn insert(
        &self,
        context: &boma_domain::TenantContext,
        message: NewMessage,
    ) -> Result<StoredMessage, AppError>;

    /// The message a nonce already produced, if any.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn find_by_nonce(
        &self,
        context: &boma_domain::TenantContext,
        thread_id: &boma_domain::ThreadId,
        client_nonce: &str,
    ) -> Result<Option<Message>, AppError>;

    /// The number of visible messages above `seq` in this thread (REQ-009).
    ///
    /// Background steps (role `tool`) are not counted: the unread divider and its badge
    /// describe the conversation the person can see.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn count_above(
        &self,
        context: &boma_domain::TenantContext,
        thread_id: &boma_domain::ThreadId,
        seq: i64,
    ) -> Result<u64, AppError>;

    /// A page of the thread's history, newest first.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn page(
        &self,
        context: &boma_domain::TenantContext,
        thread_id: &boma_domain::ThreadId,
        before: Option<i64>,
        limit: usize,
    ) -> Result<MessagePage, AppError>;

    /// Replaces the text of a message, for a streamed reply.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn set_text(
        &self,
        context: &boma_domain::TenantContext,
        message_id: &boma_domain::MessageId,
        text: &str,
    ) -> Result<(), AppError>;
}

/// Stores and reads runs.
#[async_trait]
pub trait RunStore: Send + Sync {
    /// Creates a run, or returns the run this nonce already started.
    ///
    /// The nonce is unique per thread, so a retried send finds the run it already created instead
    /// of starting a second one (REQ-002). The store returns the *existing* run in that case, and
    /// the caller must not create anything else.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn create(&self, run: NewRun) -> Result<Run, AppError>;

    /// Finds a run inside a tenant.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn find(
        &self,
        context: &boma_domain::TenantContext,
        id: &boma_domain::RunId,
    ) -> Result<Option<Run>, AppError>;

    /// Re-queues a `failed` run for one more attempt (REQ-012).
    ///
    /// The only transition out of a terminal state: `failed → queued`, atomically, with one
    /// attempt added. `completed` and `cancelled` refuse with [`AppError::Conflict`].
    ///
    /// # Errors
    ///
    /// Returns [`AppError::NotFound`] when the run does not exist in the tenant,
    /// [`AppError::Conflict`] when it is not `failed`, and an error when the database cannot
    /// answer.
    async fn retry(
        &self,
        context: &boma_domain::TenantContext,
        id: &boma_domain::RunId,
    ) -> Result<Run, AppError>;

    /// The run a nonce already started, if any.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn find_by_nonce(
        &self,
        context: &boma_domain::TenantContext,
        thread_id: &boma_domain::ThreadId,
        client_nonce: &str,
    ) -> Result<Option<Run>, AppError>;

    /// The newest run on a thread that is working and can take a steering message (REQ-003).
    ///
    /// A routine, a webhook or a bot-to-bot turn is not conversational, so a message written
    /// during one waits for the next turn instead of riding it.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn find_active_for_thread(
        &self,
        context: &boma_domain::TenantContext,
        thread_id: &boma_domain::ThreadId,
    ) -> Result<Option<Run>, AppError>;

    /// Marks the furthest message seq that still joins this run's attempt (REQ-003).
    ///
    /// The first pending steering message decides: later ones never move it forward.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn set_steering_from(
        &self,
        context: &boma_domain::TenantContext,
        id: &boma_domain::RunId,
        seq: i64,
    ) -> Result<(), AppError>;

    /// Marks how far the worker has read for this run (REQ-003).
    ///
    /// The fence decides who may write, like every other worker write (REQ-012).
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer, or the fence no longer matches.
    async fn set_steering_consumed(
        &self,
        context: &boma_domain::TenantContext,
        id: &boma_domain::RunId,
        seq: i64,
        fence: Option<i64>,
    ) -> Result<(), AppError>;

    /// Claims the oldest queued run, if there is one.
    ///
    /// The claim is atomic: the run moves from `queued` to `leased` only if nobody else
    /// claimed it first, and the returned lease is the proof (REQ-012).
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn claim_next_queued(&self, owner: &str) -> Result<Option<Run>, AppError>;

    /// Finds a routine-trigger run for `routine_id` that is not terminal, if any (REQ-024).
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn find_open_routine(
        &self,
        context: &boma_domain::TenantContext,
        routine_id: &boma_domain::RoutineId,
    ) -> Result<Option<Run>, AppError>;

    /// Moves a run to a new status, if the lease still holds.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    ///
    /// `error` carries the short, safe reason a failure is recorded (REQ-021). Passing `None`
    /// never erases a reason already written: only a transition that carries one records it.
    async fn set_status(
        &self,
        context: &boma_domain::TenantContext,
        id: &boma_domain::RunId,
        status: RunStatus,
        at: &str,
        fence: Option<i64>,
        error: Option<String>,
    ) -> Result<(), AppError>;

    /// A page of the activity projection, newest first.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn list(
        &self,
        context: &boma_domain::TenantContext,
        bot_id: Option<&boma_domain::BotId>,
        before: Option<&str>,
        limit: usize,
    ) -> Result<ActivityPage, AppError>;
}

/// Appends and reads the events behind a subscription.
#[async_trait]
pub trait EventStore: Send + Sync {
    /// Appends an event to a thread's stream.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn append(
        &self,
        context: &boma_domain::TenantContext,
        event: NewEvent,
    ) -> Result<Event, AppError>;

    /// The events a thread published after a sequence.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn since(
        &self,
        context: &boma_domain::TenantContext,
        thread_id: &boma_domain::ThreadId,
        after_seq: i64,
    ) -> Result<Vec<Event>, AppError>;
}

/// Stores and reads invitations.
#[async_trait]
pub trait InvitationStore: Send + Sync {
    /// Creates an invitation.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn create(&self, invitation: NewInvitation) -> Result<Invitation, AppError>;

    /// The invitation a token hash belongs to.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn find_by_token_hash(&self, token_hash: &[u8]) -> Result<Option<Invitation>, AppError>;
}

/// Hands events to the live subscribers.
pub trait EventPublisher: Send + Sync {
    /// Publishes an event to whoever is listening on its thread.
    fn publish(&self, event: Event);

    /// Subscribes to the events of one thread.
    ///
    /// The subscription starts *now*: a client that reconnects replays the log from its last
    /// sequence first, then listens (REQ-005, REQ-020).
    fn subscribe(
        &self,
        thread_id: &boma_domain::ThreadId,
    ) -> tokio::sync::broadcast::Receiver<Event>;
}

/// Stores and reads approval and question cards (REQ-007, REQ-017).
#[async_trait]
pub trait CardStore: Send + Sync {
    /// Finds a card inside a tenant.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn find(&self, context: &TenantContext, id: &CardId) -> Result<Option<Card>, AppError>;

    /// Marks a card as answered with `answer`, refusing a card that was already answered.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::NotFound`] when the card does not exist in the tenant, and
    /// [`AppError::Conflict`] when it was already answered.
    async fn answer(
        &self,
        context: &TenantContext,
        id: &CardId,
        answer: &str,
    ) -> Result<Card, AppError>;
}

/// The reactions on a message (REQ-008).
#[async_trait]
pub trait ReactionStore: Send + Sync {
    /// Adds or removes one emoji reaction for the caller.
    ///
    /// Returns the full reaction list for the message after the change, so one RPC answers with
    /// the truth rather than making the client count.
    ///
    /// # Errors
    ///
    /// Returns an error when the message does not exist in the tenant, or the emoji is refused.
    async fn react(
        &self,
        context: &boma_domain::TenantContext,
        message_id: &boma_domain::MessageId,
        user_id: &boma_domain::UserId,
        emoji: &str,
        action: ReactionAction,
    ) -> Result<Vec<Reaction>, AppError>;

    /// The reactions a set of messages carries, attributed to the caller (REQ-008).
    ///
    /// One attribution is what `reacted` means: whether the caller personally reacted with the
    /// emoji. The map holds a list only for the messages that carry reactions.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn list_for_messages(
        &self,
        context: &boma_domain::TenantContext,
        message_ids: &[boma_domain::MessageId],
    ) -> Result<std::collections::HashMap<boma_domain::MessageId, Vec<Reaction>>, AppError>;
}

/// Whether to add or remove a reaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReactionAction {
    /// Add the emoji for this person.
    Add,
    /// Remove the emoji for this person.
    Remove,
}

/// The per-member read position of a conversation (REQ-009).
#[async_trait]
pub trait ReadStateStore: Send + Sync {
    /// The caller's read position in this thread; zero when they never read it.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn position(
        &self,
        context: &boma_domain::TenantContext,
        thread_id: &boma_domain::ThreadId,
    ) -> Result<i64, AppError>;

    /// Sets the caller's read position, returning the value stored.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn set(
        &self,
        context: &boma_domain::TenantContext,
        thread_id: &boma_domain::ThreadId,
        last_seq: i64,
    ) -> Result<i64, AppError>;
}

/// One bot of the caller's space, with the member-specific state the rail shows (REQ-009).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BotSummary {
    /// The bot.
    pub bot: Bot,
    /// The conversation of this bot.
    pub thread_id: boma_domain::ThreadId,
    /// When the caller's snooze ends, when (and only when) it is in the future.
    pub snoozed_until: Option<String>,
    /// How many visible messages the caller has not read yet.
    pub unread_count: u64,
}

/// The roster the rail renders from (REQ-009).
#[async_trait]
pub trait RosterStore: Send + Sync {
    /// The bots of the caller's space, with per-member unread and snooze state.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer, or when the caller is not a member
    /// (a system context has no rail to render).
    async fn list_bots(
        &self,
        context: &boma_domain::TenantContext,
    ) -> Result<Vec<BotSummary>, AppError>;

    /// Snoozes the thread for the caller until `until` (RFC3339); `None` clears the snooze.
    ///
    /// Returns the deadline stored, or `None` when the snooze was cleared.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::NotFound`] when the thread does not exist in the caller's tenant,
    /// and [`AppError::Rule`] when `until` is refused.
    async fn set_snooze(
        &self,
        context: &boma_domain::TenantContext,
        thread_id: &boma_domain::ThreadId,
        until: Option<&str>,
    ) -> Result<Option<String>, AppError>;
}

/// A memory document to write: a path, a content, and who it belongs to (REQ-022).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewMemoryDocument {
    /// The owner bot, or none for the space-shared memory.
    pub bot_id: Option<boma_domain::BotId>,
    /// Its path, relative and POSIX-like.
    pub path: String,
    /// Its content.
    pub content: String,
    /// When it is written, from the application's clock.
    pub updated_at: String,
}

/// The memory documents of a bot or of the space (REQ-022).
#[async_trait]
pub trait MemoryStore: Send + Sync {
    /// Appends a revision and returns the document as it stands.
    ///
    /// The revision chain is append-only: the revision grows by one per write, and the current
    /// document is a projection of it.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Rule`] when the path is refused, and an error when the database cannot
    /// answer.
    async fn write(
        &self,
        context: &boma_domain::TenantContext,
        document: NewMemoryDocument,
    ) -> Result<boma_domain::MemoryDocument, AppError>;

    /// Reads one document inside a tenant.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn read(
        &self,
        context: &boma_domain::TenantContext,
        bot_id: Option<&boma_domain::BotId>,
        path: &str,
    ) -> Result<Option<boma_domain::MemoryDocument>, AppError>;

    /// Lists the documents of a bot, or of the space-shared memory.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn list(
        &self,
        context: &boma_domain::TenantContext,
        bot_id: Option<&boma_domain::BotId>,
    ) -> Result<Vec<boma_domain::MemoryDocument>, AppError>;

    /// Removes one document and its revision chain.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn remove(
        &self,
        context: &boma_domain::TenantContext,
        bot_id: Option<&boma_domain::BotId>,
        path: &str,
    ) -> Result<bool, AppError>;
}

/// A routine, as the app carries it (REQ-023).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Routine {
    /// The routine's id.
    pub id: boma_domain::RoutineId,
    /// The tenant it belongs to.
    pub space_id: boma_domain::SpaceId,
    /// The bot it wakes.
    pub bot_id: boma_domain::BotId,
    /// The conversation it writes in.
    pub thread_id: boma_domain::ThreadId,
    /// Who owns it.
    pub user_id: boma_domain::UserId,
    /// The name a person sees.
    pub name: String,
    /// The prompt the agent receives when it fires.
    pub prompt: String,
    /// The cron expressions, or [`boma_domain::ONCE`].
    pub crons: Vec<String>,
    /// The IANA timezone the cron computes in.
    pub timezone: String,
    /// Whether it wakes at all.
    pub active: bool,
    /// Whether the owner is told about each run.
    pub notify: bool,
    /// When it last fired.
    pub last_run_at: Option<String>,
    /// When it wakes next, in UTC.
    pub next_run_at: Option<String>,
    /// Consecutive failures since the last success.
    pub account_failures: u32,
    /// When it was created.
    pub created_at: String,
    /// When it last changed.
    pub updated_at: String,
}

/// A routine to create.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewRoutine {
    /// The routine's id.
    pub id: boma_domain::RoutineId,
    /// The tenant it belongs to.
    pub space_id: boma_domain::SpaceId,
    /// The bot it wakes.
    pub bot_id: boma_domain::BotId,
    /// The conversation it writes in.
    pub thread_id: boma_domain::ThreadId,
    /// Who owns it.
    pub user_id: boma_domain::UserId,
    /// The name a person sees.
    pub name: String,
    /// The prompt the agent receives when it fires.
    pub prompt: String,
    /// The cron expressions, or [`boma_domain::ONCE`].
    pub crons: Vec<String>,
    /// The IANA timezone the cron computes in.
    pub timezone: String,
    /// Whether it wakes at all.
    pub active: bool,
    /// Whether the owner is told about each run.
    pub notify: bool,
    /// When it wakes next, in UTC.
    pub next_run_at: Option<String>,
    /// When it is created.
    pub created_at: String,
}

/// What a routine tick did, for the audit trail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoutineOutcome {
    /// The routine fired and the run is queued.
    Fired,
    /// No run was created because one was already active; the wake was coalesced.
    Coalesced,
    /// The routine was manually skipped by the schedule, or a `@once` slot was consumed.
    Skipped,
    /// The routine was deactivated because it was `@once`.
    Once,
    /// A failure was recorded.
    Failed,
    /// The routine paused itself because the failure threshold was reached.
    Paused,
}

/// The routines of this app (REQ-023, REQ-024, REQ-025).
#[async_trait]
pub trait RoutineStore: Send + Sync {
    /// Creates a routine.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn create(
        &self,
        context: &boma_domain::TenantContext,
        routine: NewRoutine,
    ) -> Result<Routine, AppError>;

    /// Finds a routine inside a tenant.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn find(
        &self,
        context: &boma_domain::TenantContext,
        id: &boma_domain::RoutineId,
    ) -> Result<Option<Routine>, AppError>;

    /// Removes a routine.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn remove(
        &self,
        context: &boma_domain::TenantContext,
        id: &boma_domain::RoutineId,
    ) -> Result<bool, AppError>;

    /// Lists a bot's routines, newest last.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn list(
        &self,
        context: &boma_domain::TenantContext,
        bot_id: &boma_domain::BotId,
    ) -> Result<Vec<Routine>, AppError>;

    /// The routines due at `as_of`, in the order the dispatcher must run them (system context).
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn due(&self, as_of: &str) -> Result<Vec<Routine>, AppError>;

    /// Marks the routine as fired; recomputes the wake, and deactivates a `@once` routine.
    ///
    /// `next_run_at: None` means `@once`: the routine fires and deactivates.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn record_fired(
        &self,
        context: &boma_domain::TenantContext,
        id: &boma_domain::RoutineId,
        occurred_at: &str,
        run_id: Option<&boma_domain::RunId>,
        next_run_at: Option<&str>,
    ) -> Result<Routine, AppError>;

    /// Marks a failure; pauses the routine when `threshold` consecutive failures are recorded.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn record_failure(
        &self,
        context: &boma_domain::TenantContext,
        id: &boma_domain::RoutineId,
        occurred_at: &str,
        run_id: Option<&boma_domain::RunId>,
        threshold: u32,
    ) -> Result<Routine, AppError>;

    /// Marks the routine healthy: after a successful fire its failure counter resets to zero.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn record_completed(
        &self,
        context: &boma_domain::TenantContext,
        id: &boma_domain::RoutineId,
        occurred_at: &str,
        run_id: Option<&boma_domain::RunId>,
    ) -> Result<(), AppError>;

    /// Records one skipped wake (coalesced, deactivated, paused) and advances the wake.
    ///
    /// `next_run_at: None` means this was a `@once` routine and it is now spent.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    #[allow(clippy::too_many_arguments)]
    async fn record_skip(
        &self,
        context: &boma_domain::TenantContext,
        id: &boma_domain::RoutineId,
        outcome: RoutineOutcome,
        occurred_at: &str,
        run_id: Option<&boma_domain::RunId>,
        next_run_at: Option<&str>,
        detail: Option<&str>,
    ) -> Result<(), AppError>;
}

/// The tools a run may use, as the space and the bot enabled them (REQ-016).
#[async_trait]
pub trait ToolStore: Send + Sync {
    /// The catalogue of tools a bot may use in a space.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn catalogue_for(
        &self,
        context: &TenantContext,
        bot_id: &boma_domain::BotId,
    ) -> Result<Vec<ToolSpec>, AppError>;
}

/// Stores the side effects a run performs, exactly once per effect key (REQ-017).
#[async_trait]
pub trait EffectStore: Send + Sync {
    /// Records an effect, once per effect key within a run.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Conflict`] when the effect key already exists for the run, and an
    /// error when the database cannot answer.
    async fn record(
        &self,
        context: &TenantContext,
        effect: NewEffect,
    ) -> Result<ExternalEffect, AppError>;
}

/// What a model provider is asked to answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderRequest {
    /// The bot that is asked.
    pub bot: Bot,
    /// The conversation it happens in.
    pub thread: Thread,
    /// What the person wrote.
    pub prompt: String,
    /// The messages before the prompt, oldest first.
    pub history: Vec<Message>,
    /// The tools this run may use, already filtered (REQ-016).
    pub tools: Vec<ToolSpec>,
}

/// Produces the answer of a bot.
///
/// The provider returns the chunks it would have streamed, in order. The worker publishes each
/// chunk as a `message-update` event before the run completes, so a client sees the answer grow
/// (REQ-020).
#[async_trait]
pub trait ModelProvider: Send + Sync {
    /// Answers a prompt.
    ///
    /// # Errors
    ///
    /// Returns an error when the provider cannot answer.
    async fn complete(&self, request: ProviderRequest) -> Result<Vec<String>, AppError>;
}

/// Hashes and checks passwords.
#[async_trait]
pub trait PasswordHasher: Send + Sync {
    /// Hashes a password.
    ///
    /// # Errors
    ///
    /// Returns an error when the hashing fails.
    async fn hash(&self, password: &str) -> Result<String, AppError>;

    /// Checks a password against a stored hash.
    ///
    /// # Errors
    ///
    /// Returns an error when the hash cannot be read.
    async fn verify(&self, password: &str, hash: &str) -> Result<bool, AppError>;
}

/// Mints the opaque tokens the product hands out.
pub trait TokenGenerator: Send + Sync {
    /// Mints a token.
    ///
    /// # Errors
    ///
    /// Returns an error when the system random source fails.
    fn generate(&self) -> Result<String, AppError>;

    /// The hash a token is stored as.
    ///
    /// # Errors
    ///
    /// Returns an error when the system random source fails.
    fn hash(&self, token: &str) -> Result<Vec<u8>, AppError>;
}

/// Mints the opaque ids the product hands out.
pub trait IdGenerator: Send + Sync {
    /// Mints an id with the given prefix, for example `usr`.
    fn new_id(&self, prefix: &str) -> String;
}

/// Tells the time.
pub trait Clock: Send + Sync {
    /// The current time, as RFC 3339 in UTC.
    fn now(&self) -> String;

    /// The current time shifted by a number of seconds, as RFC 3339 in UTC.
    fn now_plus_seconds(&self, seconds: i64) -> String;
}

/// An effect a run performs, to be recorded exactly once per effect key (REQ-017).
#[derive(Debug, Clone)]
pub struct NewEffect {
    /// The run the effect belongs to.
    pub run_id: RunId,
    /// The idempotency key: the same key never executes twice.
    pub effect_key: String,
    /// What the effect does.
    pub kind: String,
    /// Its payload.
    pub payload: serde_json::Value,
}

/// The outcome of answering a card (REQ-017).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnswerCardOutcome {
    /// The card as it is now.
    pub card: Card,
    /// Whether the effect was recorded; `false` when it was already recorded and the answer was
    /// a replay.
    pub effect_recorded: bool,
}
