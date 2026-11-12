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
    Bot, Email, Event, Message, MessagePage, Run, RunStatus, Session, Space, SpaceRole, Thread,
    User, UserId,
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
