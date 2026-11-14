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
    Bot, Card, CardId, CardKind, Email, Event, ExternalEffect, Message, MessagePage, Reaction, Run,
    RunId, RunStatus, Session, Space, SpaceId, SpaceRole, TenantContext, Thread, ThreadId,
    ToolSpec, User, UserId,
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
    ///
    /// `not_before` lets the automatic retry hold the re-queued run back (REQ-024); `None`
    /// makes it due immediately, which is what a member-triggered retry wants.
    async fn retry(
        &self,
        context: &boma_domain::TenantContext,
        id: &boma_domain::RunId,
        not_before: Option<&str>,
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

    /// Persists a card exactly once.
    ///
    /// At most one pending card per `(thread_id, run_id)` may exist; asking for a second raises
    /// [`AppError::Conflict`], and re-opening a closed card replays the original on the caller's
    /// side.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Conflict`] when a pending card already exists for this thread and run,
    /// and an error when the database cannot answer.
    async fn create(&self, context: &TenantContext, card: NewCard) -> Result<Card, AppError>;

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

/// What a runtime should make true (REQ-026).
#[derive(Debug, Clone, PartialEq)]
pub struct ComputerSpec {
    /// The computer's id in this deployment.
    pub computer_id: String,
    /// The RAM cap, in megabytes.
    pub memory_limit_mb: u64,
    /// The CPU cap, in CPUs.
    pub cpu_limit: f64,
}

/// What the provider did (REQ-026).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComputerHandle {
    /// The provider-side handle: a container id, for docker.
    pub external_id: String,
    /// The provider-side storage backing the computer's home.
    pub home_id: String,
    /// Whether this call started the computer (false when it was already running).
    pub started: bool,
}

/// The computer backend this deployment is configured with (REQ-027).
///
/// A runtime is provider-shaped: the local Docker supervisor, a cloud provider or `none`. The
/// trait is deliberately small; the computer's lifecycle, observation surface and takeover
/// semantics live on their own ports (REQ-026, REQ-028, REQ-029, REQ-030).
#[async_trait]
pub trait ComputerRuntime: Send + Sync {
    /// The provider's name, for `/ready` and operator logs: `docker`, `none`.
    fn provider(&self) -> &'static str;

    /// Answers whether this deployment can run computers right now.
    ///
    /// A `none` runtime answers `Ok(())` — the absence of computers is a legitimate, working
    /// configuration.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Unavailable`] when the provider is configured but cannot be reached.
    async fn ping(&self) -> Result<(), AppError>;

    /// Makes the computer exist and be running (REQ-026).
    ///
    /// Idempotent in one meaningful sense: a computer that is already running is not booted a
    /// second time; the handle comes back with `started == false`.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Unavailable`] when the provider cannot be reached, and
    /// [`AppError::Rule`] when the spec names an image this provider cannot boot.
    async fn ensure(&self, spec: &ComputerSpec) -> Result<ComputerHandle, AppError>;

    /// Stops the computer. The home backing it is preserved (REQ-026).
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Unavailable`] when the provider cannot be reached.
    async fn stop(&self, external_id: &str) -> Result<(), AppError>;

    /// Captures the computer's current frame (REQ-030).
    ///
    /// The bytes come from the configured frame command run inside the container. A deployment
    /// without a frame command answers [`AppError::Unavailable`] rather than fake pixels.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Unavailable`] when the provider cannot be reached or no frame command
    /// is configured, and [`AppError::Rule`] when the command produced no usable frame.
    async fn screenshot(&self, external_id: &str) -> Result<Vec<u8>, AppError>;

    /// Runs a shell command inside the computer and returns stdout (REQ-028).
    ///
    /// Output is capped at `output_cap` bytes. The exit code is not an error condition: the
    /// caller reads stdout or nothing, exactly like a shell tool. Environment variables are
    /// passed to the process; values that cross this boundary never enter a message or the log.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Unavailable`] when the provider cannot be reached.
    async fn exec(
        &self,
        external_id: &str,
        command: &str,
        output_cap: usize,
        env: &[(String, String)],
    ) -> Result<String, AppError>;

    /// Whether a frame command is configured for the images this runtime boots.
    ///
    /// `None` means the runtime cannot produce frames; the deployment reports so honestly.
    fn frame_command(&self) -> Option<&str>;

    /// Captures the page as a PNG through the browser's CDP endpoint.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Unavailable`] when the computer has no CDP endpoint, it is unreachable,
    /// or the browser refuses the capture.
    async fn observe(&self, external_id: &str) -> Result<Vec<u8>, AppError>;

    /// Navigates the computer's page to `url`.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Unavailable`] for the same reasons as [`ComputerRuntime::observe`].
    async fn navigate(&self, external_id: &str, url: &str) -> Result<(), AppError>;

    /// Evaluates a JavaScript expression and returns its value, when there is one.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Unavailable`] for the same reasons as [`ComputerRuntime::observe`],
    /// and [`AppError::Rule`] when the expression raises inside the page.
    async fn evaluate(
        &self,
        external_id: &str,
        expression: &str,
    ) -> Result<serde_json::Value, AppError>;

    /// Clicks at a viewport point.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Unavailable`] for the same reasons as [`ComputerRuntime::observe`].
    async fn click(&self, external_id: &str, x: f64, y: f64) -> Result<(), AppError>;

    /// Types the text into the focused element, character by character.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Unavailable`] for the same reasons as [`ComputerRuntime::observe`].
    async fn type_text(&self, external_id: &str, text: &str) -> Result<(), AppError>;

    /// Scrolls the page by `delta_y` pixels.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Unavailable`] for the same reasons as [`ComputerRuntime::observe`].
    async fn scroll(&self, external_id: &str, delta_y: f64) -> Result<(), AppError>;

    /// Returns the page's serialized DOM.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Unavailable`] for the same reasons as [`ComputerRuntime::observe`].
    async fn dom_snapshot(&self, external_id: &str) -> Result<String, AppError>;
}

/// A computer row as a store writes it (REQ-026).
#[derive(Debug, Clone, PartialEq)]
pub struct NewComputer {
    /// The computer's id.
    pub id: boma_domain::ComputerId,
    /// The tenant it belongs to.
    pub space_id: boma_domain::SpaceId,
    /// The bot whose computer this is.
    pub bot_id: boma_domain::BotId,
    /// The provider key: `docker`, `none`, or a cloud provider.
    pub provider: String,
    /// Whether the computer is isolated from the space's trust boundary (REQ-031).
    pub private: bool,
    /// The RAM cap, in megabytes.
    pub memory_limit_mb: u64,
    /// The CPU cap, in CPUs.
    pub cpu_limit: f64,
    /// Minutes the computer may sit idle before the sweeper releases it.
    pub idle_minutes: u64,
    /// When the computer row was created.
    pub created_at: String,
    /// When the computer row last changed.
    pub updated_at: String,
}

/// The computers of this deployment (REQ-026).
#[async_trait]
pub trait ComputerStore: Send + Sync {
    /// Finds the computer of a bot inside a tenant.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn find(
        &self,
        context: &boma_domain::TenantContext,
        bot_id: &boma_domain::BotId,
    ) -> Result<Option<boma_domain::Computer>, AppError>;

    /// Finds the team computer shared by a space, when there is one.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn find_team(
        &self,
        context: &boma_domain::TenantContext,
        space_id: &boma_domain::SpaceId,
    ) -> Result<Option<boma_domain::Computer>, AppError>;

    /// Finds a computer by id inside a tenant.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn find_by_id(
        &self,
        context: &boma_domain::TenantContext,
        id: &boma_domain::ComputerId,
    ) -> Result<Option<boma_domain::Computer>, AppError>;

    /// Creates the row for a computer that does not exist yet.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer, or when the computer already exists.
    async fn create(
        &self,
        context: &boma_domain::TenantContext,
        computer: NewComputer,
    ) -> Result<boma_domain::Computer, AppError>;

    /// Writes the row back: status, handle, generation, timestamps.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn update(
        &self,
        context: &boma_domain::TenantContext,
        computer: &boma_domain::Computer,
    ) -> Result<(), AppError>;

    /// The running computers whose idle window has passed, as of `as_of` (system context).
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn running_idle(&self, as_of: &str) -> Result<Vec<boma_domain::Computer>, AppError>;
}

/// A screen capability row, as the stores hold it (REQ-030).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenCapability {
    /// The computer this capability opens.
    pub computer_id: boma_domain::ComputerId,
    /// The tenant the computer belongs to.
    pub space_id: boma_domain::SpaceId,
    /// The screen generation this capability was minted against.
    pub screen_generation: u64,
}

/// The screen capability minted against a computer, ready to store (REQ-030).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewScreenCapability {
    /// The computer this capability opens.
    pub computer_id: boma_domain::ComputerId,
    /// The tenant the computer belongs to.
    pub space_id: boma_domain::SpaceId,
    /// The generation of the screen at minting time.
    pub screen_generation: u64,
    /// The hash of the bearer token; the token itself is never stored.
    pub token_hash: Vec<u8>,
    /// When the capability was minted.
    pub created_at: String,
}

/// Screen capabilities: sealed bearer tokens that authorise the screen stream (REQ-030).
#[async_trait]
pub trait ScreenCapabilityStore: Send + Sync {
    /// Stores a freshly minted capability.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn issue(
        &self,
        context: &boma_domain::TenantContext,
        capability: NewScreenCapability,
    ) -> Result<(), AppError>;

    /// Finds a capability by hash, alone, in a system context.
    ///
    /// The bearer token *is* the authentication here; there is no tenant session yet.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn find_by_hash(&self, token_hash: &[u8]) -> Result<Option<ScreenCapability>, AppError>;
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

/// A fresh card ready to persist (REQ-007, REQ-016).
#[derive(Debug, Clone)]
pub struct NewCard {
    /// The card id.
    pub id: CardId,
    /// The tenant the card lives in.
    pub space_id: SpaceId,
    /// The conversation the card asks its question in.
    pub thread_id: ThreadId,
    /// What the card asks.
    pub kind: CardKind,
    /// When the card was created.
    pub created_at: String,
    /// The run the card blocks, when there is one.
    pub run_id: Option<RunId>,
}

/// The body needed to register a tool call waiting for an approval (REQ-017).
#[derive(Debug, Clone)]
pub struct NewPendingToolCall {
    /// The run that invoked the tool.
    pub run_id: RunId,
    /// The provider's correlation id for the call.
    pub call_id: String,
    /// The tenant the run lives in.
    pub space_id: SpaceId,
    /// The conversation it happens in.
    pub thread_id: ThreadId,
    /// The tool's key as it appears in the catalogue.
    pub name: String,
    /// The arguments the provider produced.
    pub arguments: serde_json::Value,
    /// When the call was registered.
    pub created_at: String,
}

/// A tool call waiting on an approval, as a relation (REQ-017).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingToolCall {
    /// The run the call belongs to.
    pub run_id: RunId,
    /// The provider's correlation id for the call.
    pub call_id: String,
    /// The tenant the call lives in.
    pub space_id: SpaceId,
    /// The conversation it happens in.
    pub thread_id: ThreadId,
    /// The tool's key.
    pub name: String,
    /// The arguments the call carries.
    pub arguments: serde_json::Value,
    /// Where the call stands.
    pub status: boma_domain::PendingToolCallStatus,
    /// What the model read as the result, once applied.
    pub result: Option<serde_json::Value>,
    /// When the call was registered.
    pub created_at: String,
    /// When the member answered, when it did.
    pub answered_at: Option<String>,
}

/// Stores the tool calls a run is waiting to apply (REQ-017).
#[async_trait]
pub trait PendingToolCallStore: Send + Sync {
    /// Registers a pending call exactly once per `(run_id, call_id)`.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn create(
        &self,
        context: &TenantContext,
        call: NewPendingToolCall,
    ) -> Result<PendingToolCall, AppError>;

    /// The still-pending calls of one run, oldest first.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn find_pending(
        &self,
        context: &TenantContext,
        run_id: &RunId,
    ) -> Result<Vec<PendingToolCall>, AppError>;

    /// Marks a call applied with its result; refuses to apply an already-applied call.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Conflict`] when the call is not pending any more, and an error when
    /// the database cannot answer.
    async fn mark_applied(
        &self,
        context: &TenantContext,
        run_id: &RunId,
        call_id: &str,
        result: serde_json::Value,
        answered_at: &str,
    ) -> Result<PendingToolCall, AppError>;

    /// Marks a call refused, no effect running. Refuses to refuse an already-answered call.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Conflict`] when the call is not pending any more, and an error when
    /// the database cannot answer.
    async fn mark_refused(
        &self,
        context: &TenantContext,
        run_id: &RunId,
        call_id: &str,
        answered_at: &str,
    ) -> Result<PendingToolCall, AppError>;
}

/// What one rate-limit hit recorded (D-01).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RateLimitDecision {
    /// Hits registered in this window, including this one.
    pub count: u64,
    /// The window this hit belongs to, in seconds since the epoch, aligned.
    pub bucket: i64,
}

/// A shared counter for rate limits, shared across replicas (D-01).
///
/// The expensive part of "rate limit" is remembering how many hits there were. This store is
/// the cheapest honest answer: a bucket per key, a row per bucket, an upsert per hit. Process
/// memory is not a correct answer for a replicated deployment; this is the same counter no
/// matter which replica serves the request.
#[async_trait]
pub trait RateLimitStore: Send + Sync {
    /// Registers one hit for `key` in the `window_seconds`-aligned bucket of `now` (RFC 3339),
    /// and answers with the count it recorded.
    ///
    /// # Errors
    ///
    /// Returns an error when the backend cannot answer.
    async fn hit(
        &self,
        key: &str,
        now: &str,
        window_seconds: u64,
    ) -> Result<RateLimitDecision, AppError>;
}

/// What a store keeps for a sealed secret: the ciphertext and its own nonce.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SealedSecret {
    /// The AES-256-GCM ciphertext.
    pub ciphertext: Vec<u8>,
    /// The 12-byte nonce this ciphertext was sealed under.
    pub nonce: Vec<u8>,
}

/// A secret as the store exposes it: a name, never the value (REQ-046).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretSummary {
    /// The identifier stored under the bot (e.g. `OPENAI_KEY`).
    pub name: String,
    /// When it was written.
    pub created_at: String,
}

/// What a caller hands in to seal and store (REQ-046).
///
/// The plaintext crosses the port boundary in-process (same address space, never the log or a
/// socket); the store seals it under the space's data-encryption key before anything persists.
#[derive(Debug, Clone)]
pub struct NewBotSecret {
    /// The secret's id.
    pub id: String,
    /// The bot the secret belongs to.
    pub bot_id: boma_domain::BotId,
    /// The name the secret will be stored under.
    pub name: String,
    /// The value to seal.
    pub value: secrecy::SecretString,
    /// When the secret was written.
    pub created_at: String,
}

/// Stores bot secrets write-only (REQ-046).
#[async_trait]
pub trait SecretStore: Send + Sync {
    /// Seals and stores a value under the bot-scoped name, replacing any previous one.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::NotFound`] when the bot does not exist in the caller's tenant, and an
    /// error when the database refuses the write.
    async fn put(&self, context: &TenantContext, secret: NewBotSecret) -> Result<(), AppError>;

    /// The names of the bot's secrets, newest first. Never values, never ciphertexts.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn list(
        &self,
        context: &TenantContext,
        bot_id: &boma_domain::BotId,
    ) -> Result<Vec<SecretSummary>, AppError>;

    /// Removes a secret. Deleting a name that is not there is a success, not an error.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn delete(
        &self,
        context: &TenantContext,
        bot_id: &boma_domain::BotId,
        name: &str,
    ) -> Result<bool, AppError>;

    /// The value, for the computer executor only.
    ///
    /// Never called for an API response: the only caller is the shell-out path, which places the
    /// value in the process environment (REQ-046). Returns `None` for an unknown secret.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer or the seal does not authenticate.
    async fn reveal(
        &self,
        context: &TenantContext,
        bot_id: &boma_domain::BotId,
        name: &str,
    ) -> Result<Option<secrecy::SecretString>, AppError>;
}

/// One skill the model may load, as the provider advertisement sees it (REQ-035).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillContext {
    /// The wire name the model passes to `skill.load`.
    pub name: String,
    /// One line about what the skill does.
    pub description: String,
}

/// What a caller hands in to teach or install a skill (REQ-035).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewSkill {
    /// The skill's id.
    pub id: String,
    /// The wire name; unique per space.
    pub name: String,
    /// One line a person reads.
    pub description: String,
    /// The instructions handed to the model.
    pub instructions: String,
    /// `taught` or `plugin`: where the skill came from.
    pub origin: boma_domain::SkillOrigin,
    /// The plugin source it was installed from, for plugin skills.
    pub plugin_source: Option<String>,
    /// When the skill was written.
    pub created_at: String,
}

/// What the API exposes about a skill: the name and its one line. Instructions go over the wire
/// only to a `skills.get` call or to the model's tool loop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillSummary {
    /// The wire name.
    pub name: String,
    /// One line a person reads.
    pub description: String,
    /// `builtin`, `taught` or `plugin`.
    pub origin: String,
    /// The plugin source, for plugin skills.
    pub plugin_source: Option<String>,
    /// When the skill was written.
    pub created_at: String,
}

/// A whole skill as the stores expose it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillRecord {
    /// The wire name.
    pub name: String,
    /// One line a person reads.
    pub description: String,
    /// The instructions handed to the model.
    pub instructions: String,
    /// `builtin`, `taught` or `plugin`.
    pub origin: String,
    /// The plugin source, for plugin skills.
    pub plugin_source: Option<String>,
    /// When the skill was written.
    pub created_at: String,
}

/// Stores taught and plugin skills, per space (REQ-035).
///
/// Built-in skills live in the domain; this store carries only what a space owns.
#[async_trait]
pub trait SkillStore: Send + Sync {
    /// The space's skills, name ascending.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn list(&self, context: &TenantContext) -> Result<Vec<SkillSummary>, AppError>;

    /// One skill by name, with its instructions.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn get(
        &self,
        context: &TenantContext,
        name: &str,
    ) -> Result<Option<SkillRecord>, AppError>;

    /// Teaches or installs a skill, replacing a taught skill of the same name.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Conflict`] when a plugin skill of the same name exists (a reinstall
    /// replaces; two plugins never share a name) and an error when the database refuses the write.
    async fn put(&self, context: &TenantContext, skill: NewSkill) -> Result<(), AppError>;

    /// Removes a skill by name. Removing a missing skill is a success.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn remove(&self, context: &TenantContext, name: &str) -> Result<bool, AppError>;
}

/// What a caller hands in to submit a plugin for review (REQ-034).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewPluginSubmission {
    /// The submission's id.
    pub id: String,
    /// The source reference, exactly as submitted (parse or not).
    pub source: String,
    /// The plugin's display name, when the author gave one.
    pub name: Option<String>,
    /// The contact address, when the author gave one. Never returned by any API.
    pub contact: Option<String>,
    /// The submitting member, when there was one (anonymous submissions carry `None`).
    pub submitter_user_id: Option<boma_domain::UserId>,
    /// When the submission was recorded.
    pub created_at: String,
}

/// What the API exposes about a submission: an id. Nothing about the submitter is returned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginSubmissionSummary {
    /// The submission's id.
    pub id: String,
}

/// Stores plugin submissions (REQ-034).
///
/// Submissions are not tenant data: the anonymous submission path has no space. The review is
/// an admin concern; this store records rows, nothing more.
#[async_trait]
pub trait PluginSubmissionStore: Send + Sync {
    /// Records a submission.
    ///
    /// # Errors
    ///
    /// Returns an error when the database refuses the write.
    async fn record(
        &self,
        submission: NewPluginSubmission,
    ) -> Result<PluginSubmissionSummary, AppError>;
}

/// One audit event written by an escape hatch (defect D-05).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewAuditEvent {
    /// The event's id.
    pub id: String,
    /// The space the event belongs to.
    pub space_id: boma_domain::SpaceId,
    /// The member who caused it, when there was one.
    pub actor_user_id: Option<boma_domain::UserId>,
    /// What happened.
    pub kind: boma_domain::AuditKind,
    /// What it happened to: the server name, the tool, the command — never the secret.
    pub target: String,
    /// Extra structure the row records (host, tool, run id). Never secret material.
    pub metadata: serde_json::Value,
    /// When it happened.
    pub created_at: String,
}

/// Stores audit events on the escape hatches (defect D-05).
///
/// Insert-only: an audit trail a caller can rewrite is not an audit trail.
#[async_trait]
pub trait AuditStore: Send + Sync {
    /// Appends one event.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn append(&self, event: NewAuditEvent) -> Result<(), AppError>;
}

/// The credential the caller hands in for an MCP server (REQ-033).
#[derive(Debug, Clone)]
pub struct NewMcpBearer {
    /// The bearer token the server expects. Sealed at rest, never returned.
    pub token: secrecy::SecretString,
}

/// The OAuth endpoints a member registered for a server (REQ-033).
#[derive(Debug, Clone)]
pub struct McpOAuthConfig {
    /// The authorization endpoint the member's browser opens.
    pub authorization_url: String,
    /// The token endpoint the exchange calls.
    pub token_url: String,
    /// The client id the member registered at the server.
    pub client_id: String,
    /// The client secret, when the server issues confidential ones.
    pub client_secret: Option<secrecy::SecretString>,
}

/// What a caller hands in to register an MCP server (REQ-033).
#[derive(Debug, Clone)]
pub struct NewMcpServer {
    /// The server's id.
    pub id: String,
    /// The member's display name for it.
    pub name: String,
    /// `http` or `stdio`.
    pub transport: boma_domain::McpTransport,
    /// The HTTPS URL, for `http`.
    pub url: Option<String>,
    /// The command, for `stdio`.
    pub command: Option<String>,
    /// The bearer token, when the server uses one.
    pub bearer: Option<secrecy::SecretString>,
    /// The OAuth endpoints, when the server uses a registered client.
    pub oauth: Option<McpOAuthConfig>,
    /// The member's explicit allowance for a private endpoint (D-05).
    pub allow_private: bool,
    /// When the server was registered.
    pub created_at: String,
}

/// What the API exposes about a server. Never the credentials.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpServerSummary {
    /// The server's id.
    pub id: String,
    /// The member's display name.
    pub name: String,
    /// `http` or `stdio`.
    pub transport: String,
    /// The HTTPS URL, when it is one.
    pub url: Option<String>,
    /// The command, when it is a stdio server.
    pub command: Option<String>,
    /// `none`, `oauth_pending`, `connected` or `error`.
    pub auth_state: String,
    /// The member's explicit allowance for a private endpoint (D-05).
    pub allow_private: bool,
    /// Whether an OAuth client is configured (implies `oauth` in the add request).
    pub oauth_configured: bool,
    /// The number of tools the last probe discovered, when any.
    pub tool_count: Option<i32>,
    /// When the server was registered.
    pub created_at: String,
}

/// One server as the stores persist it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpServerRecord {
    /// The server's id.
    pub id: String,
    /// The member's display name.
    pub name: String,
    /// `http` or `stdio`.
    pub transport: String,
    /// The HTTPS URL, when it is one.
    pub url: Option<String>,
    /// The command, when it is a stdio server.
    pub command: Option<String>,
    /// `none`, `oauth_pending`, `connected` or `error`.
    pub auth_state: String,
    /// The member's explicit allowance for a private endpoint (D-05).
    pub allow_private: bool,
    /// The authorization endpoint, when OAuth is configured.
    pub authorization_url: Option<String>,
    /// The token endpoint, when OAuth is configured.
    pub token_url: Option<String>,
    /// The client id, when OAuth is configured.
    pub client_id: Option<String>,
    /// The number of tools the last probe discovered, when any.
    pub tool_count: Option<i32>,
    /// When the server was registered.
    pub created_at: String,
}

/// One tool a server discovered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpTool {
    /// The tool's name as the server names it.
    pub name: String,
    /// One line about what the tool does.
    pub description: String,
    /// The JSON schema of its arguments.
    pub input_schema: serde_json::Value,
}

/// Stores the MCP servers of a space (REQ-033).
#[async_trait]
pub trait McpServerStore: Send + Sync {
    /// The space's servers, name ascending.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn list(&self, context: &TenantContext) -> Result<Vec<McpServerSummary>, AppError>;

    /// Registers a server.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Conflict`] when the name is already taken, and an error when the
    /// database refuses the write.
    async fn create(
        &self,
        context: &TenantContext,
        server: NewMcpServer,
    ) -> Result<McpServerSummary, AppError>;

    /// One server by id.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn get(
        &self,
        context: &TenantContext,
        id: &str,
    ) -> Result<Option<McpServerRecord>, AppError>;

    /// Removes a server. Removing a missing server is a success.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn remove(&self, context: &TenantContext, id: &str) -> Result<bool, AppError>;

    /// The bearer token, revealed for the wire call only. Never logged.
    ///
    /// # Errors
    ///
    /// Returns an error when the token does not unwrap under this deployment's vault.
    async fn reveal_bearer(
        &self,
        context: &TenantContext,
        id: &str,
    ) -> Result<Option<secrecy::SecretString>, AppError>;

    /// Records the probe outcome: auth state and discovered tool count.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn set_auth_state(
        &self,
        context: &TenantContext,
        id: &str,
        state: &str,
        tool_count: Option<i32>,
    ) -> Result<(), AppError>;

    /// Replaces the tool rows of a server with what the probe discovered.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn replace_tools(
        &self,
        context: &TenantContext,
        server_id: &str,
        tools: &[McpTool],
    ) -> Result<(), AppError>;

    /// The tools the last probe discovered for every connected server of the space, keyed by
    /// server name. Used to build a run's catalogue without a network call.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn catalogue_tools(
        &self,
        context: &TenantContext,
    ) -> Result<Vec<(String, McpTool)>, AppError>;
    /// Seals and stores one pending OAuth attempt for a server (REQ-033).
    ///
    /// The verifier never persists in clear: the store seals it under the deployment's vault.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer or the seal fails.
    async fn store_oauth_session(
        &self,
        context: &TenantContext,
        server_id: &str,
        state: &str,
        verifier: secrecy::SecretString,
        expires_at: &str,
    ) -> Result<(), AppError>;

    /// Atomically takes one pending OAuth session for a server.
    ///
    /// `now` is the caller's clock (RFC 3339, same instant the session was stamped against).
    /// Returns `Some(verifier)` when the session exists and is unexpired; `None` otherwise —
    /// an unknown, already-consumed or expired state. The row is deleted either way, so a
    /// replayed `code` exchanges at most once.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer or the seal does not authenticate.
    async fn take_oauth_session(
        &self,
        context: &TenantContext,
        server_id: &str,
        state: &str,
        now: &str,
    ) -> Result<Option<secrecy::SecretString>, AppError>;

    /// The client secret, revealed for the token exchange only. Never logged.
    ///
    /// # Errors
    ///
    /// Returns an error when the secret does not unwrap under this deployment's vault.
    async fn reveal_client_secret(
        &self,
        context: &TenantContext,
        server_id: &str,
    ) -> Result<Option<secrecy::SecretString>, AppError>;

    /// Seals and stores the server's bearer (from an OAuth exchange) and marks it connected.
    ///
    /// # Errors
    ///
    /// Returns an error when the seal fails or the database cannot answer.
    async fn store_bearer(
        &self,
        context: &TenantContext,
        server_id: &str,
        bearer: secrecy::SecretString,
    ) -> Result<(), AppError>;
}

/// Calls an MCP server (REQ-033).
///
/// Redirects are refused by the implementation, and every call carries the SSRF guard's verdict;
/// this port exists so a deployment can swap transports without touching a business rule.
#[async_trait]
pub trait McpTransport: Send + Sync {
    /// Discovers the tools a server exposes.
    ///
    /// # Errors
    ///
    /// Returns an error when the server cannot be reached, answers with an error, or a URL is
    /// refused by the outbound guard.
    async fn list_tools(
        &self,
        server: &McpServerRecord,
        bearer: Option<&secrecy::SecretString>,
    ) -> Result<Vec<McpTool>, AppError>;

    /// Calls one tool.
    ///
    /// # Errors
    ///
    /// Returns an error when the server cannot be reached, answers with an error, or a URL is
    /// refused by the outbound guard.
    async fn call_tool(
        &self,
        server: &McpServerRecord,
        bearer: Option<&secrecy::SecretString>,
        tool: &str,
        input: &serde_json::Value,
    ) -> Result<serde_json::Value, AppError>;
}

/// One connector of the catalogue as the space sees it (REQ-032).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectorSummary {
    /// The catalogue id.
    pub connector_id: String,
    /// The member's display name.
    pub name: String,
    /// One line about what installing it gives.
    pub description: String,
    /// `none` or `api_key`.
    pub auth_kind: String,
    /// Whether this space installed it.
    pub installed: bool,
}

/// Stores the connectors a space installed (REQ-032).
///
/// Credentials are sealed under the space's data-encryption key (ADR-020 machinery); listings
/// never carry them, `connected_credential` is the only read path, used by the executor.
#[async_trait]
pub trait ConnectorStore: Send + Sync {
    /// The catalogue with this space's install state, id ascending.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn list(&self, context: &TenantContext) -> Result<Vec<ConnectorSummary>, AppError>;

    /// Installs (or re-keys) a connector for the space.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Rule`] when an api-key connector is installed without a key, and an
    /// error when the database refuses the write.
    async fn install(
        &self,
        context: &TenantContext,
        connector_id: &str,
        credential: Option<secrecy::SecretString>,
    ) -> Result<ConnectorSummary, AppError>;

    /// Revokes the installation. Removing a missing installation is a success.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn revoke(&self, context: &TenantContext, connector_id: &str) -> Result<bool, AppError>;

    /// Whether the connector is installed and connected in this space.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn is_connected(
        &self,
        context: &TenantContext,
        connector_id: &str,
    ) -> Result<bool, AppError>;

    /// The credential, revealed for the executor only. Never logged.
    ///
    /// # Errors
    ///
    /// Returns an error when the credential does not unwrap under this deployment's vault.
    async fn connected_credential(
        &self,
        context: &TenantContext,
        connector_id: &str,
    ) -> Result<Option<secrecy::SecretString>, AppError>;

    /// The tools the installed, connected connectors expose, filtered by nothing — the bot's
    /// tool grant is applied by the catalogue builder, not here.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn catalogue_tools(
        &self,
        context: &TenantContext,
    ) -> Result<Vec<boma_domain::ToolSpec>, AppError>;
}

/// Runs the tools of an installed connector (REQ-032).
///
/// Implementations decide where the work happens: network, parse, cap. This port exists so the
/// dispatch path never names a concrete adapter.
#[async_trait]
pub trait ConnectorRunner: Send + Sync {
    /// Runs one tool of one connector with the installation's credential, when it has one.
    ///
    /// # Errors
    ///
    /// Returns an error when the connector refuses the call.
    async fn run(
        &self,
        connector_id: &str,
        tool: &str,
        arguments: &serde_json::Value,
        credential: Option<&secrecy::SecretString>,
    ) -> Result<serde_json::Value, AppError>;
}

/// One file inside a fetched plugin repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginRepoFile {
    /// The path relative to the fetched root (the skill folder, the manifest, …).
    pub path: String,
    /// The file's bytes. Binary components are simply not carried: the installer only needs text.
    pub content: Vec<u8>,
}

/// What a fetched plugin looks like when it arrives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchedPlugin {
    /// Every file the fetcher kept, paths relative to the fetched root.
    pub files: Vec<PluginRepoFile>,
    /// The commit SHA the tree resolved to; pinned, and verified when the ref was a full SHA.
    pub resolved_sha: Option<String>,
}

/// Fetches a plugin from its source reference (REQ-034).
///
/// Implementations decide how a source is resolved; the installer never names a network client.
#[async_trait]
pub trait PluginFetcher: Send + Sync {
    /// Fetches the plugin files for one source reference.
    ///
    /// # Errors
    ///
    /// Returns an error when the source cannot be resolved or fetched; nothing is installed on
    /// an error — the caller does not write anything.
    async fn fetch(&self, source: &boma_domain::PluginSourceRef)
    -> Result<FetchedPlugin, AppError>;
}

/// What one installation of a plugin ended up doing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginInstallOutcome {
    /// The install id.
    pub id: String,
    /// The plugin's name from its manifest.
    pub name: String,
    /// The canonical source it came from (`url[@ref][#subdir]`).
    pub source: String,
    /// The commit SHA, when the fetcher reported one.
    pub resolved_sha: Option<String>,
    /// The number of skills installed.
    pub skill_count: u32,
    /// The number of MCP servers registered.
    pub mcp_server_count: u32,
    /// Every part left out, named, with the reason — the no-local-code rule made visible.
    pub skipped: Vec<serde_json::Value>,
}

/// One stored plugin install summary (REQ-034).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginInstallSummary {
    /// The install's id.
    pub id: String,
    /// The plugin's name.
    pub name: String,
    /// The canonical source.
    pub source: String,
    /// The commit SHA, when one was resolved.
    pub resolved_sha: Option<String>,
    /// What was installed: `{skills, mcpServers}`.
    pub parts: serde_json::Value,
    /// What was left out, named with reasons.
    pub skipped: serde_json::Value,
    /// When it was installed or last replaced.
    pub created_at: String,
}

/// What a caller hands in to record a plugin installation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewPluginInstall {
    /// The install's id.
    pub id: String,
    /// The canonical source.
    pub source: String,
    /// The commit SHA, when one was resolved.
    pub resolved_sha: Option<String>,
    /// The plugin's name.
    pub name: String,
    /// What was installed: `{skills, mcpServers}`.
    pub parts: serde_json::Value,
    /// What was left out, named with reasons.
    pub skipped: Vec<serde_json::Value>,
    /// When it was recorded.
    pub created_at: String,
}

/// Stores plugin installations for a space (REQ-034).
#[async_trait]
pub trait PluginInstallStore: Send + Sync {
    /// The space's installs, newest first.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn list(&self, context: &TenantContext) -> Result<Vec<PluginInstallSummary>, AppError>;

    /// Records (or replaces, same source) the install.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn record(
        &self,
        context: &TenantContext,
        install: NewPluginInstall,
    ) -> Result<(), AppError>;

    /// Removes every plugin skill of the given source from the space. Returns how many went.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn remove_plugin_skills(
        &self,
        context: &TenantContext,
        source: &str,
    ) -> Result<u32, AppError>;
}

/// Exchanges an OAuth authorization code for a token, and produces the PKCE pair (REQ-033).
///
/// The deployment decides how the exchange happens: the members' browsers drive the
/// `authorization_endpoint` on the server itself; this port only speaks to the token endpoint
/// and mints the verifier/challenge pair. Tests script the whole thing against a loopback
/// wire server; production wires a plain HTTP client with form encoding.
#[async_trait]
pub trait OAuthClient: Send + Sync {
    /// Mints a fresh `(verifier, challenge)` pair (S256).
    ///
    /// # Errors
    ///
    /// Returns an error when the random source cannot answer.
    async fn pkce_pair(&self) -> Result<(secrecy::SecretString, String), AppError>;

    /// Exchanges `code` at `token_url` for the token JSON.
    ///
    /// The form carries `grant_type=authorization_code`, `code`, `redirect_uri`, `client_id`,
    /// `code_verifier`, and `client_secret` when the member registered a confidential client.
    ///
    /// # Errors
    ///
    /// Returns an error when the endpoint cannot be reached or answers with an error.
    async fn exchange_grant(
        &self,
        token_url: &str,
        form: &[(String, String)],
    ) -> Result<serde_json::Value, AppError>;
}

/// The result of beginning an OAuth sign-in for an MCP server (REQ-033).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpOAuthStart {
    /// Where the member's browser opens.
    pub authorization_url: String,
    /// The state token to present back to `mcp.oauth.complete`.
    pub state: String,
}

/// One artifact as the API exposes it (REQ-036).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactSummary {
    /// The artifact's id.
    pub id: String,
    /// The bot that produced it, when one did.
    pub bot_id: Option<String>,
    /// The group thread it belongs to, when one does.
    pub group_id: Option<String>,
    /// The person who owns the artifact.
    pub user_id: String,
    /// The run that produced it, when one did.
    pub run_id: Option<String>,
    /// The member's file name.
    pub name: String,
    /// One line about what the file is.
    pub description: String,
    /// What the client declared.
    pub declared_mime: String,
    /// What the bytes proved (defect D-03).
    pub mime_type: String,
    /// Size in bytes.
    pub size: u64,
    /// `sha256:<hex>` of the content.
    pub hash: String,
    /// The storage key of the bytes (content-addressed; equals the hash).
    pub storage_key: String,
    /// The root of the version family.
    pub root_artifact_id: String,
    /// 1 for the first version, +1 per update, dense per family.
    pub version: i32,
    /// When it was stored.
    pub created_at: String,
}

/// The write payload for one artifact version (REQ-036).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewArtifact {
    /// The artifact's id.
    pub id: String,
    /// The bot that produced it, when one did.
    pub bot_id: Option<String>,
    /// The group thread it belongs to, when one does.
    pub group_id: Option<String>,
    /// The run that produced it, when one did.
    pub run_id: Option<String>,
    /// The member's file name.
    pub name: String,
    /// One line about what the file is.
    pub description: String,
    /// What the client declared.
    pub declared_mime: String,
    /// What the bytes proved.
    pub mime_type: String,
    /// Size in bytes.
    pub size: u64,
    /// `sha256:<hex>` of the content.
    pub hash: String,
    /// The root of the version family; `None` roots a new family at `id`.
    pub root_artifact_id: Option<String>,
    /// The client's idempotency token.
    pub client_nonce: Option<String>,
    /// When it was stored.
    pub created_at: String,
}

/// The bytes-and-proof part of a content write, returned by the out-of-band store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentWrite {
    /// `sha256:<hex>` of what was written.
    pub hash: String,
    /// Size in bytes.
    pub size: u64,
}

/// Artifact metadata rows (REQ-036). Bytes live out of band.
#[async_trait]
pub trait ArtifactStore: Send + Sync {
    /// An existing row for the same (space, user, client_nonce), if any — replay detection.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn find_by_nonce(
        &self,
        context: &TenantContext,
        client_nonce: &str,
    ) -> Result<Option<ArtifactSummary>, AppError>;

    /// Inserts one version, computing the root and version. A `root_artifact_id` that does not
    /// exist in this space roots a new family — guessing roots is not a read primitive.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn put(
        &self,
        context: &TenantContext,
        artifact: NewArtifact,
    ) -> Result<ArtifactSummary, AppError>;

    /// One artifact, the latest version unless `version` pins one.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn get(
        &self,
        context: &TenantContext,
        artifact_id: &str,
        version: Option<i32>,
    ) -> Result<Option<ArtifactSummary>, AppError>;

    /// The row with this exact id. Unlike `get`, the family root does not resolve.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn get_exact(
        &self,
        context: &TenantContext,
        artifact_id: &str,
    ) -> Result<Option<ArtifactSummary>, AppError>;

    /// Every version of the family, newest first (the golden's order).
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn list_versions(
        &self,
        context: &TenantContext,
        artifact_id: &str,
    ) -> Result<Vec<ArtifactSummary>, AppError>;

    /// The latest version of every family in the space, optional bot/group filter, newest first.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn list_space(
        &self,
        context: &TenantContext,
        bot_id: Option<&str>,
        group_id: Option<&str>,
        limit: i64,
    ) -> Result<Vec<ArtifactSummary>, AppError>;

    /// The storage keys of the whole family.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn family_keys(
        &self,
        context: &TenantContext,
        artifact_id: &str,
    ) -> Result<Vec<String>, AppError>;

    /// Removes every row of the family. Returns how many went.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn remove_family(
        &self,
        context: &TenantContext,
        artifact_id: &str,
    ) -> Result<u64, AppError>;

    /// How many rows reference a storage key (cross-family dedup).
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn storage_key_refs(
        &self,
        context: &TenantContext,
        storage_key: &str,
    ) -> Result<i64, AppError>;
}

/// The out-of-band artifact bytes (REQ-036). Content-addressed, idempotent writes.
#[async_trait]
pub trait ArtifactContentStore: Send + Sync {
    /// Writes bytes, hashing them. Writing the same bytes twice answers the same hash.
    ///
    /// # Errors
    ///
    /// Returns an error when the store cannot write.
    async fn put(&self, bytes: &[u8]) -> Result<ContentWrite, AppError>;

    /// Reads a blob by its key.
    ///
    /// # Errors
    ///
    /// Returns an error when the blob cannot be read.
    async fn read(&self, key: &str) -> Result<Vec<u8>, AppError>;

    /// Removes a blob. Removing a missing blob is a success.
    ///
    /// # Errors
    ///
    /// Returns an error when the store cannot remove the blob.
    async fn remove(&self, key: &str) -> Result<(), AppError>;
}

/// One page as the member sees it (REQ-037).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageSummary {
    /// The page's id.
    pub id: String,
    /// The bot that produced it, when one did.
    pub bot_id: Option<String>,
    /// Who made it.
    pub created_by: String,
    /// The library label.
    pub name: String,
    /// `private` or `link`.
    pub visibility: String,
    /// The id of the current version.
    pub current_version_id: Option<String>,
    /// When it was shared, when it is.
    pub shared: bool,
    /// The client's idempotency token.
    pub client_nonce: Option<String>,
    /// When it was created.
    pub created_at: String,
    /// When it last changed.
    pub updated_at: String,
}

/// One immutable version of a page's source (REQ-037).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageVersion {
    /// The version's id.
    pub id: String,
    /// The page it belongs to.
    pub page_id: String,
    /// 1 for the first, +1 per update.
    pub version: i32,
    /// The validated block document.
    pub source: serde_json::Value,
    /// When it was compiled.
    pub compiled_at: String,
}

/// What `pages.save` hands in.
#[derive(Debug, Clone, PartialEq)]
pub struct NewPageVersion {
    /// The page to version; `None` creates a page.
    pub page_id: Option<String>,
    /// The library label; required on create, optional on version.
    pub name: Option<String>,
    /// The bot that produced it, when one did.
    pub bot_id: Option<String>,
    /// The new page's id; `None` reuses the page's.
    pub new_id: String,
    /// The new version's id.
    pub new_version_id: String,
    /// The validated document.
    pub source: serde_json::Value,
    /// The client's idempotency token.
    pub client_nonce: Option<String>,
    /// When it was written.
    pub created_at: String,
}

/// Pages and their immutable versions (REQ-037).
#[async_trait]
pub trait PageStore: Send + Sync {
    /// An existing page for the same (space, creator, client_nonce), if any — replay detection.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn find_by_nonce(
        &self,
        context: &TenantContext,
        client_nonce: &str,
    ) -> Result<Option<PageSummary>, AppError>;

    /// Appends a version, creating the page when `page_id` is `None`. A `page_id` that does
    /// not exist in this space is refused.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::NotFound`] when `page_id` is unknown, and an error when the database
    /// cannot answer.
    async fn put_version(
        &self,
        context: &TenantContext,
        version: NewPageVersion,
    ) -> Result<PageSummary, AppError>;

    /// One page with its current (or pinned) version.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn get(
        &self,
        context: &TenantContext,
        page_id: &str,
        version: Option<i32>,
    ) -> Result<Option<(PageSummary, PageVersion)>, AppError>;

    /// Shares the page by link: stores the token hash and flips visibility. Returns whether it
    /// changed anything.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn share(
        &self,
        context: &TenantContext,
        page_id: &str,
        token_hash: &[u8],
    ) -> Result<bool, AppError>;

    /// Revokes the share: the page is private again and the token hash is gone.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn revoke_share(&self, context: &TenantContext, page_id: &str) -> Result<bool, AppError>;

    /// The shared page for a token hash, resolved by a system read. `None` for unknown tokens,
    /// revoked pages and malformed input alike — the callers must not distinguish.
    ///
    /// # Errors
    ///
    /// Returns an error when the database cannot answer.
    async fn shared_by_token_hash(
        &self,
        token_hash: &[u8],
    ) -> Result<Option<PageVersion>, AppError>;
}

/// A memory document carried into a provider turn (REQ-022).
///
/// The scope grammar is the one the memory store enforces; carried, not interpreted here, so the
/// scripted and the BYOK providers prove the same contract against it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryContext {
    /// The document's path, relative and POSIX-like.
    pub path: String,
    /// The content at its newest revision.
    pub content: String,
    /// The scope the document lives in: `bot` or `shared`.
    pub scope: boma_domain::MemoryScope,
}

/// One tool this run may call, as the tool gate names it (REQ-016).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCallRequest {
    /// The provider's correlation id for this call.
    pub id: String,
    /// The tool's key as it appears in the catalogue.
    pub name: String,
    /// The arguments the provider produced, validated by the dispatcher (REQ-016).
    pub arguments: serde_json::Value,
}

/// The outcome of one tool call, handed back to the provider as a tool message (REQ-017).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCallResult {
    /// The provider's correlation id from `ToolCallRequest::id`.
    pub id: String,
    /// Whether the tool executed without a refusal.
    pub ok: bool,
    /// What the model reads as the result.
    pub body: serde_json::Value,
}

/// What one `complete` call hands back: either streamed chunks (the answer), or tool calls.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderTurn {
    /// The assistant's text, in the order it is to be streamed (REQ-020).
    Chunks(Vec<String>),
    /// The assistant wants one or more tools run before it answers (REQ-016, REQ-019).
    ToolCalls(Vec<ToolCallRequest>),
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
    /// Scoped memory available to this turn (REQ-022).
    pub memory: Vec<MemoryContext>,
    /// The skills this run may load, name and one line each (REQ-035). Always carries the
    /// built-ins; taught and plugin skills of the space ride along.
    pub skills: Vec<SkillContext>,
}

/// Produces the next turn of the assistant's answer.
///
/// The provider returns either the ordered chunks to stream (REQ-020), or the tool calls it wants
/// run before it can answer (REQ-016). The worker publishes each chunk as a `message-update`
/// event before the turn completes, so a client sees the answer grow (REQ-020).
#[async_trait]
pub trait ModelProvider: Send + Sync {
    /// Produces the next turn.
    ///
    /// # Errors
    ///
    /// Returns an error when the provider cannot answer.
    async fn complete(&self, request: ProviderRequest) -> Result<ProviderTurn, AppError>;
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
