//! The in-memory stores: every port of the application, backed by rows in memory.
//!
//! The product's ports are written so that a use case never names an adapter. This module is the
//! adapter a test uses: the same rows, the same rules, the same tenant isolation, without a
//! database.
//!
//! The rules are the ones the database enforces, and they are the ones that matter:
//!
//! * **The store allocates the sequences.** A message takes its number from the thread, in the
//!   same critical section as the insert, so two concurrent sends never claim the same number
//!   (REQ-005). The event sequence works the same way (REQ-005, REQ-020).
//! * **The nonce makes a send idempotent.** A message that already carries the nonce is returned
//!   as it is, with `created: false`, and no sequence is allocated (REQ-002). A run that already
//!   carries the nonce is returned unchanged (REQ-002).
//! * **The claim is atomic.** The oldest queued run moves to `leased` under the lock, with a
//!   fence that grows by one, so two workers never claim the same run (REQ-012).
//! * **The fence guards the writes.** A worker whose fence no longer matches is refused
//!   (REQ-012).
//! * **The tenant decides what a caller sees.** A member context only sees the rows of its own
//!   space; a row of another tenant is *not found*, never forbidden, because the difference would
//!   confirm that the identifier exists (REQ-043).
//!
//! People, sessions and organizations are not tenant data: they belong to a person, not to a
//! space, and their stores ignore the context.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock, RwLock, RwLockReadGuard, RwLockWriteGuard};

use boma_app::{
    AppError,
    ports::{
        Account, AccountStore, ActivityPage, BotStore, CardStore, Clock, EffectStore,
        EventPublisher, EventStore, Invitation, InvitationStore, MemoryStore, MessageStore,
        NewAccount, NewBot, NewEffect, NewEvent, NewInvitation, NewMemoryDocument, NewMessage,
        NewRun, NewSession, NewSpace, NewThread, ReactionAction, ReactionStore, ReadStateStore,
        RosterStore, RoutineOutcome, RoutineStore, RunActivity, RunStore, SessionStore, SpaceStore,
        StoredMessage, ThreadStore, ToolStore, UserStore,
    },
};
use boma_domain::DomainError;
use boma_domain::{
    Block, Bot, BotId, Card, CardId, CardStatus, Email, Event, ExternalEffect, MemoryDocument,
    Message, MessageId, MessagePage, OrganizationId, Reaction, Run, RunId, RunStatus, Session,
    SessionId, Space, SpaceId, SpaceRole, TenantContext, Thread, ThreadId, ToolKind, ToolSpec,
    User, UserId, cursor_from_seq, validate_memory_path,
};
use tokio::sync::broadcast;

/// How many characters of a prompt the activity list shows.
const PROMPT_SNIPPET_LENGTH: usize = 120;

/// How long a claim holds a run before it may be taken back.
///
/// The lease is what a reconciler uses to recover a run whose worker died (REQ-014). One hour is
/// long enough for a slow answer and short enough that a dead worker is replaced.
pub const LEASE_TTL_SECONDS: i64 = 3_600;

/// How many events a thread's channel keeps before a slow subscriber misses one.
const EVENT_CHANNEL_CAPACITY: usize = 256;

/// A person, with the hash of their password.
#[derive(Debug, Clone)]
struct UserRow {
    /// The person.
    user: User,
    /// The hash of their password.
    password_hash: String,
}

/// A billing entity, as the in-memory database holds it.
///
/// The product has no domain type for an organization: only its identifier travels. The testkit
/// keeps the name and the creation time so that a test can assert on them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Organization {
    /// The organization's id.
    pub id: OrganizationId,
    /// The name a person sees.
    pub name: String,
    /// When it was created.
    pub created_at: String,
}

/// A billing entity.
#[derive(Debug, Clone)]
struct OrganizationRow {
    /// The organization's id.
    id: OrganizationId,
    /// The name a person sees.
    name: String,
    /// When it was created.
    created_at: String,
}

/// A tenant.
#[derive(Debug, Clone)]
struct SpaceRow {
    /// The space.
    space: Space,
    /// When it was created.
    created_at: String,
}

/// Who belongs to which tenant.
#[derive(Debug, Clone)]
struct MembershipRow {
    /// The tenant.
    space_id: SpaceId,
    /// The member.
    user_id: UserId,
    /// What the member may do.
    role: SpaceRole,
}

/// A bot.
#[derive(Debug, Clone)]
struct BotRow {
    /// The bot.
    bot: Bot,
}

/// A conversation, with the sequences it allocates.
#[derive(Debug, Clone)]
struct ThreadRow {
    /// The thread.
    thread: Thread,
}

/// A message, with the tenant it belongs to.
#[derive(Debug, Clone)]
struct MessageRow {
    /// The message.
    message: Message,
    /// The tenant.
    space_id: SpaceId,
}

/// A reaction somebody left on a message (REQ-008).
#[derive(Debug, Clone)]
struct ReactionRow {
    /// The emoji.
    emoji: String,
    /// Who reacted.
    user_id: UserId,
    /// The message reacted to.
    message_id: MessageId,
    /// The tenant the message belongs to.
    space_id: SpaceId,
}

/// The read position of one person in one conversation (REQ-009).
#[derive(Debug, Clone)]
struct ReadStateRow {
    /// The conversation.
    thread_id: ThreadId,
    /// The tenant it belongs to.
    space_id: SpaceId,
    /// The reader.
    user_id: UserId,
    /// The highest sequence the reader had read.
    last_seq: i64,
}

/// A conversation a member snoozed (REQ-009).
#[derive(Debug, Clone)]
struct SnoozeRow {
    /// The conversation.
    thread_id: ThreadId,
    /// The tenant it belongs to.
    space_id: SpaceId,
    /// The member who snoozed it.
    user_id: UserId,
    /// Until when the snooze lasts (RFC3339).
    snoozed_until: String,
}

/// A memory document, with the tenant it belongs to (REQ-022).
#[derive(Debug, Clone)]
struct MemoryDocumentMemRow {
    /// The tenant.
    space_id: SpaceId,
    /// The owner bot, or none for the space-shared memory.
    bot_id: Option<BotId>,
    /// Its path.
    path: String,
    /// The content at the newest revision.
    content: String,
    /// The revision the content is at.
    revision: u64,
    /// When it was last written.
    updated_at: String,
}

/// A routine, with its tenant (REQ-023).
#[derive(Debug, Clone)]
struct RoutineRow {
    /// The routine.
    routine: boma_app::ports::Routine,
}

/// A screen capability row, with its tenant (REQ-030).
#[derive(Debug, Clone)]
struct ScreenCapabilityRow {
    /// The capability.
    computer_id: boma_domain::ComputerId,
    /// The tenant it opens for.
    space_id: SpaceId,
    /// The hash of the bearer token.
    token_hash: Vec<u8>,
    /// The generation the capability was minted against.
    screen_generation: u64,
}

/// A computer, with its tenant (REQ-026).
#[derive(Debug, Clone)]
struct ComputerRow {
    /// The computer.
    computer: boma_domain::Computer,
}

/// One recorded outcome of a wake, for the audit trail.
// The dispatcher reads these rows in the next slice (REQ-024); the structure is load-bearing.
#[allow(dead_code)]
#[derive(Debug, Clone)]
struct RoutineRunRow {
    /// The routine.
    routine_id: boma_domain::RoutineId,
    /// The tenant.
    space_id: SpaceId,
    /// The run the wake produced, when it did.
    run_id: Option<RunId>,
    /// `fired`, `coalesced`, `skipped`, `failed`, `paused`.
    outcome: String,
    /// Free-text detail.
    detail: Option<String>,
    /// When it happened.
    occurred_at: String,
}

/// A run.
#[derive(Debug, Clone)]
struct RunRow {
    /// The run.
    run: Run,
}

/// A card, with the tenant it belongs to.
#[derive(Debug, Clone)]
struct CardRow {
    /// The card.
    card: Card,
    /// The tenant.
    space_id: SpaceId,
}

/// An effect a run performs, keyed so it can never execute twice.
#[derive(Debug, Clone)]
struct EffectRow {
    /// The effect.
    effect: ExternalEffect,
}

/// One tool the space installed and can grant (REQ-016).
#[derive(Debug, Clone)]
struct ToolInstallRow {
    /// The space.
    space_id: SpaceId,
    /// The key the catalogue and the grants use.
    tool_key: String,
    /// What the tool does.
    description: String,
    /// Its family.
    kind: ToolKind,
    /// Whether it is enabled for the space.
    enabled: bool,
}

/// An event, with the tenant it belongs to.
#[derive(Debug, Clone)]
struct EventRow {
    /// The event.
    event: Event,
    /// The tenant.
    space_id: SpaceId,
}

/// A session, with the hash of its token.
#[derive(Debug, Clone)]
struct SessionRow {
    /// The session.
    session: Session,
    /// The hash of the token.
    token_hash: Vec<u8>,
}

/// An invitation, with the hash of its token.
#[derive(Debug, Clone)]
struct InvitationRow {
    /// The invitation.
    invitation: Invitation,
    /// The hash of the token.
    token_hash: Vec<u8>,
    /// When it was created.
    created_at: String,
}

/// The rows the in-memory database holds.
#[derive(Debug, Default)]
struct State {
    /// The people, with the hash of their password.
    users: Vec<UserRow>,
    /// The billing entities.
    organizations: Vec<OrganizationRow>,
    /// The tenants.
    spaces: Vec<SpaceRow>,
    /// Who belongs to which tenant.
    memberships: Vec<MembershipRow>,
    /// The bots.
    bots: Vec<BotRow>,
    /// The conversations, with the sequences they allocate.
    threads: Vec<ThreadRow>,
    /// The messages.
    messages: Vec<MessageRow>,
    /// The runs.
    runs: Vec<RunRow>,
    /// The events.
    events: Vec<EventRow>,
    /// The sessions, with the hash of their token.
    sessions: Vec<SessionRow>,
    /// The invitations, with the hash of their token.
    invitations: Vec<InvitationRow>,
    /// The cards, with the space they belong to.
    cards: Vec<CardRow>,
    /// The reactions people left on messages.
    reactions: Vec<ReactionRow>,
    /// The read position of each person, per conversation.
    read_states: Vec<ReadStateRow>,
    /// The snoozes of each person, per conversation.
    snoozes: Vec<SnoozeRow>,
    /// The screen capabilities minted against computers.
    screen_capabilities: Vec<ScreenCapabilityRow>,
    /// The memory documents people wrote.
    memory_documents: Vec<MemoryDocumentMemRow>,
    /// The computers of bots.
    computers: Vec<ComputerRow>,
    /// The routines of each bot.
    routines: Vec<RoutineRow>,
    /// The audit trail of routine wakes.
    routine_runs: Vec<RoutineRunRow>,
    /// The effects runs perform.
    effects: Vec<EffectRow>,
    /// The tools the spaces installed.
    tool_installs: Vec<ToolInstallRow>,
}

impl State {
    /// The thread an identifier names.
    fn thread(&self, id: &ThreadId) -> Option<&ThreadRow> {
        self.threads.iter().find(|row| row.thread.id == *id)
    }

    /// The thread an identifier names, ready to be written to.
    fn thread_mut(&mut self, id: &ThreadId) -> Option<&mut ThreadRow> {
        self.threads.iter_mut().find(|row| row.thread.id == *id)
    }

    /// The message an identifier names.
    fn message(&self, id: &MessageId) -> Option<&MessageRow> {
        self.messages.iter().find(|row| row.message.id == *id)
    }

    /// The membership of a person in a tenant.
    fn membership(&self, space_id: &SpaceId, user_id: &UserId) -> Option<&MembershipRow> {
        self.memberships
            .iter()
            .find(|row| row.space_id == *space_id && row.user_id == *user_id)
    }
}

/// Whether the context may see the rows of a tenant.
///
/// A member context is limited to its own space. A system context crosses tenants, for a reason
/// the product owns.
fn visible(context: &TenantContext, space_id: &SpaceId) -> bool {
    match context {
        TenantContext::Member {
            space_id: allowed, ..
        } => allowed == space_id,
        TenantContext::System { .. } => true,
    }
}

/// The database the stores built with `default()` share.
///
/// An application assembled from default stores behaves like one assembled from a
/// [`MemoryDatabase`]: the stores see the same rows. The database lives as long as the process,
/// so a test that needs its own rows builds its own [`MemoryDatabase`] instead.
fn default_database() -> &'static MemoryDatabase {
    static DATABASE: OnceLock<MemoryDatabase> = OnceLock::new();
    DATABASE.get_or_init(MemoryDatabase::new)
}

/// Locks the database for reading, recovering from a poisoned lock.
fn read(state: &RwLock<State>) -> RwLockReadGuard<'_, State> {
    match state.read() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// Locks the database for writing, recovering from a poisoned lock.
fn write(state: &RwLock<State>) -> RwLockWriteGuard<'_, State> {
    match state.write() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// The in-memory database every store of the testkit shares.
///
/// The stores hold a handle on the same rows: what one store writes, another one reads. That is
/// what makes a test possible without a database, and what keeps the rules of the ports honest:
/// the message store and the run store write the same thread the bot store wrote.
#[derive(Debug, Clone, Default)]
pub struct MemoryDatabase {
    /// The rows.
    state: Arc<RwLock<State>>,
}

impl MemoryDatabase {
    /// Builds an empty database.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The store of accounts.
    #[must_use]
    pub fn accounts(&self) -> MemoryAccountStore {
        MemoryAccountStore::new(self)
    }

    /// The store of people.
    #[must_use]
    pub fn users(&self) -> MemoryUserStore {
        MemoryUserStore::new(self)
    }

    /// The store of sessions.
    #[must_use]
    pub fn sessions(&self) -> MemorySessionStore {
        MemorySessionStore::new(self)
    }

    /// The store of tenants.
    #[must_use]
    pub fn spaces(&self) -> MemorySpaceStore {
        MemorySpaceStore::new(self)
    }

    /// The store of bots.
    #[must_use]
    pub fn bots(&self) -> MemoryBotStore {
        MemoryBotStore::new(self)
    }

    /// The store of conversations.
    #[must_use]
    pub fn threads(&self) -> MemoryThreadStore {
        MemoryThreadStore::new(self)
    }

    /// The store of messages.
    #[must_use]
    pub fn messages(&self) -> MemoryMessageStore {
        MemoryMessageStore::new(self)
    }

    /// The store of runs.
    ///
    /// The store needs a clock to stamp the expiry of a lease.
    #[must_use]
    pub fn runs(&self, clock: Arc<dyn Clock>) -> MemoryRunStore {
        MemoryRunStore::new(self, clock)
    }

    /// The store of events.
    ///
    /// The store needs a publisher, so that a subscriber sees the events a use case writes.
    #[must_use]
    pub fn events(&self, publisher: Arc<dyn EventPublisher>) -> MemoryEventStore {
        MemoryEventStore::new(self, publisher)
    }

    /// The store of invitations.
    #[must_use]
    pub fn invitations(&self) -> MemoryInvitationStore {
        MemoryInvitationStore::new(self)
    }

    /// The store of cards.
    #[must_use]
    pub fn cards(&self) -> MemoryCardStore {
        MemoryCardStore::new(self)
    }

    /// The store of effects.
    #[must_use]
    pub fn effects(&self) -> MemoryEffectStore {
        MemoryEffectStore::new(self)
    }

    /// The store of tool installs.
    #[must_use]
    pub fn tools(&self) -> MemoryToolStore {
        MemoryToolStore::new(self)
    }

    /// The store of reactions.
    #[must_use]
    pub fn reactions(&self) -> MemoryReactionStore {
        MemoryReactionStore::new(self)
    }

    /// The store of read positions.
    #[must_use]
    pub fn read_states(&self) -> MemoryReadStateStore {
        MemoryReadStateStore::new(self)
    }

    /// The roster the rail renders from.
    #[must_use]
    pub fn roster(&self) -> MemoryRosterStore {
        MemoryRosterStore::new(self)
    }

    /// The store of memory documents.
    #[must_use]
    pub fn memory(&self) -> MemoryMemoryStore {
        MemoryMemoryStore::new(self)
    }

    /// The store of routines.
    #[must_use]
    pub fn routines(&self) -> MemoryRoutineStore {
        MemoryRoutineStore::new(self)
    }

    /// The store of computers.
    #[must_use]
    pub fn computers(&self) -> MemoryComputerStore {
        MemoryComputerStore::new(self)
    }

    /// The store of screen capabilities.
    #[must_use]
    pub fn screen_capabilities(&self) -> MemoryScreenCapabilityStore {
        MemoryScreenCapabilityStore::new(self)
    }

    /// Seeds a conversation with explicit sequences.
    ///
    /// The product allocates the sequences itself; a golden replay needs to decide them, because
    /// the fixture pins the numbers it captured.
    pub fn seed_thread(&self, thread: Thread) {
        write(&self.state).threads.push(ThreadRow { thread });
    }

    /// Seeds a message with an explicit sequence.
    pub fn seed_message(&self, space_id: SpaceId, message: &Message) {
        let mut state = write(&self.state);
        let thread_id = message.thread_id.clone();
        state.messages.push(MessageRow {
            message: message.clone(),
            space_id,
        });
        if let Some(thread) = state.thread_mut(&thread_id) {
            let next = message.seq + 1;
            if thread.thread.next_message_seq < next {
                thread.thread.next_message_seq = next;
            }
        }
    }

    /// Seeds a run exactly as the fixture describes it.
    pub fn seed_run(&self, run: Run) {
        write(&self.state).runs.push(RunRow { run });
    }

    /// Seeds an event with an explicit sequence.
    pub fn seed_event(&self, space_id: SpaceId, event: &Event) {
        let mut state = write(&self.state);
        let thread_id = event.thread_id.clone();
        state.events.push(EventRow {
            event: event.clone(),
            space_id,
        });
        if let Some(thread) = state.thread_mut(&thread_id) {
            let next = event.seq + 1;
            if thread.thread.next_event_seq < next {
                thread.thread.next_event_seq = next;
            }
        }
    }

    /// Seeds a session with the hash of a known token.
    pub fn seed_session(&self, session: Session, token_hash: Vec<u8>) {
        write(&self.state).sessions.push(SessionRow {
            session,
            token_hash,
        });
    }

    /// Seeds a memory document exactly as the fixture describes it (REQ-022).
    pub fn seed_memory_document(&self, document: &MemoryDocument) {
        let mut state = write(&self.state);
        state.memory_documents.push(MemoryDocumentMemRow {
            space_id: document.space_id.clone(),
            bot_id: document.bot_id.clone(),
            path: document.path.clone(),
            content: document.content.clone(),
            revision: document.revision,
            updated_at: document.updated_at.clone(),
        });
    }

    /// Seeds a card with the tenant it belongs to.
    pub fn seed_card(&self, space_id: SpaceId, card: Card) {
        write(&self.state).cards.push(CardRow { card, space_id });
    }

    /// Seeds a tool install.
    pub fn seed_tool_install(
        &self,
        space_id: SpaceId,
        tool_key: &str,
        description: &str,
        kind: ToolKind,
        enabled: bool,
    ) {
        write(&self.state).tool_installs.push(ToolInstallRow {
            space_id,
            tool_key: tool_key.to_owned(),
            description: description.to_owned(),
            kind,
            enabled,
        });
    }

    /// Sets a bot's grant: the catalogue keys it may use (empty means every install).
    ///
    /// # Panics
    ///
    /// Panics when the bot does not exist; a test that seeds a grant for a bot that was never
    /// created is a bug in the seed.
    pub fn seed_bot_grant(&self, bot_id: &BotId, enabled_tools: Vec<String>) {
        let mut state = write(&self.state);
        #[allow(clippy::expect_used)]
        let bot = state
            .bots
            .iter_mut()
            .find(|row| row.bot.id == *bot_id)
            .expect("the bot exists");
        bot.bot.enabled_tools = enabled_tools;
    }

    /// Sets a bot's model preference: the catalogue key and whether it is pinned.
    ///
    /// # Panics
    ///
    /// Panics when the bot does not exist; a test that seeds a model for a bot that was never
    /// created is a bug in the seed.
    pub fn seed_bot_model(&self, bot_id: &BotId, key: &str, pinned: bool) {
        let (provider, model) = key.split_once('/').unwrap_or(("custom", key));
        let mut state = write(&self.state);
        #[allow(clippy::expect_used)]
        let bot = state
            .bots
            .iter_mut()
            .find(|row| row.bot.id == *bot_id)
            .expect("the bot exists");
        bot.bot.model_provider = Some(provider.to_owned());
        bot.bot.model_id = Some(model.to_owned());
        bot.bot.model_pinned = pinned;
    }

    /// The organization the spaces of a person belong to.
    ///
    /// It is the organization of the first space the person belongs to, and `None` when the person
    /// belongs to no space at all.
    #[must_use]
    pub fn organization_for(&self, user_id: &UserId) -> Option<OrganizationId> {
        let state = read(&self.state);
        for membership in &state.memberships {
            if membership.user_id != *user_id {
                continue;
            }
            if let Some(space) = state
                .spaces
                .iter()
                .find(|row| row.space.id == membership.space_id)
            {
                return Some(space.space.organization_id.clone());
            }
        }
        None
    }

    /// The organizations the database holds.
    #[must_use]
    pub fn organizations(&self) -> Vec<Organization> {
        let state = read(&self.state);
        state
            .organizations
            .iter()
            .map(|row| Organization {
                id: row.id.clone(),
                name: row.name.clone(),
                created_at: row.created_at.clone(),
            })
            .collect()
    }

    /// Registers an organization, and returns it.
    ///
    /// The in-memory database has no foreign keys: a space may name an organization that was
    /// never created, exactly as the account store creates its own. A test that cares about the
    /// organization rows registers them here.
    #[must_use]
    pub fn create_organization(
        &self,
        id: &OrganizationId,
        name: &str,
        created_at: &str,
    ) -> Organization {
        let organization = Organization {
            id: id.clone(),
            name: name.to_owned(),
            created_at: created_at.to_owned(),
        };
        let mut state = write(&self.state);
        state.organizations.push(OrganizationRow {
            id: organization.id.clone(),
            name: organization.name.clone(),
            created_at: organization.created_at.clone(),
        });
        organization
    }
}

/// The store of accounts: the person, their organization, their first space and their membership.
///
/// The four rows belong together: a person with no space cannot do anything, and a space with no
/// member is invisible (REQ-042). They are written under one lock, so an account is never
/// half-created.
#[derive(Debug, Clone)]
pub struct MemoryAccountStore {
    /// The rows.
    state: Arc<RwLock<State>>,
}

impl MemoryAccountStore {
    /// Builds the store on a database.
    #[must_use]
    pub fn new(database: &MemoryDatabase) -> Self {
        Self {
            state: Arc::clone(&database.state),
        }
    }
}

impl Default for MemoryAccountStore {
    fn default() -> Self {
        Self::new(default_database())
    }
}

#[async_trait::async_trait]
impl AccountStore for MemoryAccountStore {
    async fn create_account(&self, account: NewAccount) -> Result<Account, AppError> {
        let mut state = write(&self.state);

        // The address is taken: nothing at all is written (REQ-040).
        if state
            .users
            .iter()
            .any(|row| row.user.email == account.user.email)
        {
            return Err(AppError::Conflict);
        }
        if state.users.iter().any(|row| row.user.id == account.user.id) {
            return Err(AppError::Conflict);
        }
        if state
            .spaces
            .iter()
            .any(|row| row.space.id == account.space.id)
        {
            return Err(AppError::Conflict);
        }

        let user = User {
            id: account.user.id.clone(),
            email: account.user.email.clone(),
            name: account.user.name.clone(),
            email_verified: false,
            locale: account.user.locale.clone(),
            deactivated_at: None,
        };
        let space = Space {
            id: account.space.id.clone(),
            organization_id: account.space.organization_id.clone(),
            name: account.space.name.clone(),
            is_default: account.space.is_default,
        };

        state.users.push(UserRow {
            user: user.clone(),
            password_hash: account.user.password_hash.clone(),
        });
        state.organizations.push(OrganizationRow {
            id: account.organization.id.clone(),
            name: account.organization.name.clone(),
            created_at: account.organization.created_at.clone(),
        });
        state.spaces.push(SpaceRow {
            space: space.clone(),
            created_at: account.space.created_at.clone(),
        });
        state.memberships.push(MembershipRow {
            space_id: account.space.id.clone(),
            user_id: account.user.id.clone(),
            role: SpaceRole::Owner,
        });

        Ok(Account { user, space })
    }
}

/// The store of people.
///
/// A person is not tenant data: the row belongs to the person, not to a space, so the store
/// ignores the tenant context.
#[derive(Debug, Clone)]
pub struct MemoryUserStore {
    /// The rows.
    state: Arc<RwLock<State>>,
}

impl MemoryUserStore {
    /// Builds the store on a database.
    #[must_use]
    pub fn new(database: &MemoryDatabase) -> Self {
        Self {
            state: Arc::clone(&database.state),
        }
    }
}

impl Default for MemoryUserStore {
    fn default() -> Self {
        Self::new(default_database())
    }
}

#[async_trait::async_trait]
impl UserStore for MemoryUserStore {
    async fn find_by_email(&self, email: &Email) -> Result<Option<(User, String)>, AppError> {
        let state = read(&self.state);
        Ok(state
            .users
            .iter()
            .find(|row| row.user.email == *email)
            .map(|row| (row.user.clone(), row.password_hash.clone())))
    }

    async fn find(&self, id: &UserId) -> Result<Option<User>, AppError> {
        let state = read(&self.state);
        Ok(state
            .users
            .iter()
            .find(|row| row.user.id == *id)
            .map(|row| row.user.clone()))
    }
}

/// The store of sessions.
///
/// A session belongs to a person, not to a space: the store ignores the tenant context. The row
/// holds the hash of the token, never the token itself.
#[derive(Debug, Clone)]
pub struct MemorySessionStore {
    /// The rows.
    state: Arc<RwLock<State>>,
}

impl MemorySessionStore {
    /// Builds the store on a database.
    #[must_use]
    pub fn new(database: &MemoryDatabase) -> Self {
        Self {
            state: Arc::clone(&database.state),
        }
    }
}

impl Default for MemorySessionStore {
    fn default() -> Self {
        Self::new(default_database())
    }
}

#[async_trait::async_trait]
impl SessionStore for MemorySessionStore {
    async fn create(&self, session: NewSession) -> Result<Session, AppError> {
        let mut state = write(&self.state);
        if state
            .sessions
            .iter()
            .any(|row| row.token_hash == session.token_hash)
        {
            return Err(AppError::Conflict);
        }
        let stored = Session {
            id: session.id.clone(),
            user_id: session.user_id.clone(),
            expires_at: session.expires_at.clone(),
        };
        state.sessions.push(SessionRow {
            session: stored.clone(),
            token_hash: session.token_hash,
        });
        Ok(stored)
    }

    async fn find_by_token_hash(&self, token_hash: &[u8]) -> Result<Option<Session>, AppError> {
        let state = read(&self.state);
        Ok(state
            .sessions
            .iter()
            .find(|row| row.token_hash == token_hash)
            .map(|row| row.session.clone()))
    }

    async fn delete(&self, id: &SessionId) -> Result<(), AppError> {
        let mut state = write(&self.state);
        state.sessions.retain(|row| row.session.id != *id);
        Ok(())
    }
}

/// The store of tenants and of their members.
#[derive(Debug, Clone)]
pub struct MemorySpaceStore {
    /// The rows.
    state: Arc<RwLock<State>>,
}

impl MemorySpaceStore {
    /// Builds the store on a database.
    #[must_use]
    pub fn new(database: &MemoryDatabase) -> Self {
        Self {
            state: Arc::clone(&database.state),
        }
    }
}

impl Default for MemorySpaceStore {
    fn default() -> Self {
        Self::new(default_database())
    }
}

#[async_trait::async_trait]
impl SpaceStore for MemorySpaceStore {
    async fn create(&self, space: NewSpace, owner: &UserId) -> Result<Space, AppError> {
        let mut state = write(&self.state);
        if state.spaces.iter().any(|row| row.space.id == space.id) {
            return Err(AppError::Conflict);
        }
        let stored = Space {
            id: space.id.clone(),
            organization_id: space.organization_id.clone(),
            name: space.name.clone(),
            is_default: space.is_default,
        };
        state.spaces.push(SpaceRow {
            space: stored.clone(),
            created_at: space.created_at.clone(),
        });
        // The membership belongs to the same write: a space nobody belongs to is invisible
        // (REQ-042).
        state.memberships.push(MembershipRow {
            space_id: space.id.clone(),
            user_id: owner.clone(),
            role: SpaceRole::Owner,
        });
        Ok(stored)
    }

    async fn find(&self, space_id: &SpaceId, user_id: &UserId) -> Result<Option<Space>, AppError> {
        let state = read(&self.state);
        // The space is only visible to a member: a space the caller does not belong to is not
        // found (REQ-043).
        if state.membership(space_id, user_id).is_none() {
            return Ok(None);
        }
        Ok(state
            .spaces
            .iter()
            .find(|row| row.space.id == *space_id)
            .map(|row| row.space.clone()))
    }

    async fn membership(
        &self,
        space_id: &SpaceId,
        user_id: &UserId,
    ) -> Result<Option<SpaceRole>, AppError> {
        let state = read(&self.state);
        Ok(state.membership(space_id, user_id).map(|row| row.role))
    }

    async fn default_for(&self, user_id: &UserId) -> Result<Option<Space>, AppError> {
        let state = read(&self.state);
        let mut spaces: Vec<&SpaceRow> = state
            .spaces
            .iter()
            .filter(|row| state.membership(&row.space.id, user_id).is_some())
            .collect();
        // The default space first, then the oldest one.
        spaces.sort_by(|left, right| {
            right
                .space
                .is_default
                .cmp(&left.space.is_default)
                .then_with(|| left.created_at.cmp(&right.created_at))
                .then_with(|| left.space.id.cmp(&right.space.id))
        });
        Ok(spaces.first().map(|row| row.space.clone()))
    }
}

/// The store of bots.
///
/// A bot and its conversation belong together: a bot without a conversation is a half-created
/// bot, so the two rows are written under one lock (REQ-004, REQ-042).
#[derive(Debug, Clone)]
pub struct MemoryBotStore {
    /// The rows.
    state: Arc<RwLock<State>>,
}

impl MemoryBotStore {
    /// Builds the store on a database.
    #[must_use]
    pub fn new(database: &MemoryDatabase) -> Self {
        Self {
            state: Arc::clone(&database.state),
        }
    }
}

impl Default for MemoryBotStore {
    fn default() -> Self {
        Self::new(default_database())
    }
}

#[async_trait::async_trait]
impl BotStore for MemoryBotStore {
    async fn create_with_thread(
        &self,
        bot: NewBot,
        thread: NewThread,
    ) -> Result<(Bot, Thread), AppError> {
        let mut state = write(&self.state);

        // The checks come first: a refused write leaves nothing behind.
        if state.bots.iter().any(|row| row.bot.id == bot.id) {
            return Err(AppError::Conflict);
        }
        if state.threads.iter().any(|row| row.thread.id == thread.id) {
            return Err(AppError::Conflict);
        }
        if bot.space_id != thread.space_id {
            return Err(AppError::NotFound);
        }

        let stored_bot = Bot {
            id: bot.id.clone(),
            space_id: bot.space_id.clone(),
            user_id: bot.user_id.clone(),
            name: bot.name.clone(),
            description: bot.description.clone(),
            color: bot.color.clone(),
            thread_id: thread.id.clone(),
            archived_at: None,
            created_at: bot.created_at.clone(),
            enabled_tools: Vec::new(),
            model_provider: None,
            model_id: None,
            model_pinned: false,
        };
        let stored_thread = Thread {
            id: thread.id.clone(),
            space_id: thread.space_id.clone(),
            bot_id: thread.bot_id.clone(),
            user_id: thread.user_id.clone(),
            // The first message takes the number one, the first event too (REQ-005).
            next_message_seq: 1,
            next_event_seq: 1,
            created_at: thread.created_at.clone(),
        };

        state.bots.push(BotRow {
            bot: stored_bot.clone(),
        });
        state.threads.push(ThreadRow {
            thread: stored_thread.clone(),
        });
        Ok((stored_bot, stored_thread))
    }

    async fn find(&self, context: &TenantContext, id: &BotId) -> Result<Option<Bot>, AppError> {
        let state = read(&self.state);
        let Some(row) = state.bots.iter().find(|row| row.bot.id == *id) else {
            return Ok(None);
        };
        // A bot of another tenant is not found: the difference between "forbidden" and "not
        // found" would confirm that the identifier exists (REQ-043).
        if !visible(context, &row.bot.space_id) {
            return Ok(None);
        }
        Ok(Some(row.bot.clone()))
    }
}

/// The store of conversations.
#[derive(Debug, Clone)]
pub struct MemoryThreadStore {
    /// The rows.
    state: Arc<RwLock<State>>,
}

impl MemoryThreadStore {
    /// Builds the store on a database.
    #[must_use]
    pub fn new(database: &MemoryDatabase) -> Self {
        Self {
            state: Arc::clone(&database.state),
        }
    }
}

impl Default for MemoryThreadStore {
    fn default() -> Self {
        Self::new(default_database())
    }
}

#[async_trait::async_trait]
impl ThreadStore for MemoryThreadStore {
    async fn find(
        &self,
        context: &TenantContext,
        id: &ThreadId,
    ) -> Result<Option<Thread>, AppError> {
        let state = read(&self.state);
        let Some(row) = state.thread(id) else {
            return Ok(None);
        };
        if !visible(context, &row.thread.space_id) {
            return Ok(None);
        }
        Ok(Some(row.thread.clone()))
    }

    async fn for_bot(
        &self,
        context: &TenantContext,
        bot_id: &BotId,
    ) -> Result<Option<Thread>, AppError> {
        let state = read(&self.state);
        let mut threads: Vec<&ThreadRow> = state
            .threads
            .iter()
            .filter(|row| row.thread.bot_id == *bot_id)
            .collect();
        threads.sort_by(|left, right| {
            left.thread
                .created_at
                .cmp(&right.thread.created_at)
                .then_with(|| left.thread.id.cmp(&right.thread.id))
        });
        Ok(threads
            .into_iter()
            .find(|row| visible(context, &row.thread.space_id))
            .map(|row| row.thread.clone()))
    }
}

/// The store of messages.
///
/// Two rules shape it: the store allocates the sequence of a message, and the nonce makes a send
/// idempotent (REQ-002, REQ-005).
#[derive(Debug, Clone)]
pub struct MemoryMessageStore {
    /// The rows.
    state: Arc<RwLock<State>>,
}

impl MemoryMessageStore {
    /// Builds the store on a database.
    #[must_use]
    pub fn new(database: &MemoryDatabase) -> Self {
        Self {
            state: Arc::clone(&database.state),
        }
    }
}

impl Default for MemoryMessageStore {
    fn default() -> Self {
        Self::new(default_database())
    }
}

#[async_trait::async_trait]
impl MessageStore for MemoryMessageStore {
    async fn insert(
        &self,
        context: &TenantContext,
        message: NewMessage,
    ) -> Result<StoredMessage, AppError> {
        let mut state = write(&self.state);

        // A write into another tenant is refused, and refused as "not found" (REQ-043).
        if !visible(context, &message.space_id) {
            return Err(AppError::NotFound);
        }

        let thread_id = message.thread_id.clone();

        // The nonce makes the send idempotent: the message that already carries it comes back
        // as it is, and no sequence is allocated (REQ-002).
        if let Some(nonce) = message.client_nonce.as_deref()
            && let Some(existing) = message_with_nonce(&state, &thread_id, nonce)
        {
            return Ok(StoredMessage {
                message: existing.message.clone(),
                created: false,
            });
        }

        let Some(thread) = state.thread_mut(&thread_id) else {
            return Err(AppError::NotFound);
        };
        if !visible(context, &thread.thread.space_id) {
            return Err(AppError::NotFound);
        }
        // The sequence comes from the thread, in the same critical section as the insert: two
        // concurrent sends never claim the same number (REQ-005).
        let seq = thread.thread.next_message_seq;
        thread.thread.next_message_seq += 1;

        let stored = Message {
            id: message.id.clone(),
            thread_id,
            seq,
            role: message.role,
            blocks: message.blocks.clone(),
            bot_id: message.bot_id.clone(),
            author_user_id: message.author_user_id.clone(),
            run_id: message.run_id.clone(),
            client_nonce: message.client_nonce.clone(),
            created_at: message.created_at.clone(),
            reply_to_message_id: message.reply_to_message_id.clone(),
            reply_quote: message.reply_quote.clone(),
            reactions: Vec::new(),
        };
        state.messages.push(MessageRow {
            message: stored.clone(),
            space_id: message.space_id.clone(),
        });
        Ok(StoredMessage {
            message: stored,
            created: true,
        })
    }

    async fn find_by_nonce(
        &self,
        context: &TenantContext,
        thread_id: &ThreadId,
        client_nonce: &str,
    ) -> Result<Option<Message>, AppError> {
        let state = read(&self.state);
        let Some(thread) = state.thread(thread_id) else {
            return Ok(None);
        };
        if !visible(context, &thread.thread.space_id) {
            return Ok(None);
        }
        Ok(message_with_nonce(&state, thread_id, client_nonce).map(|row| row.message.clone()))
    }

    async fn count_above(
        &self,
        context: &TenantContext,
        thread_id: &ThreadId,
        seq: i64,
    ) -> Result<u64, AppError> {
        let state = read(&self.state);
        let count = state
            .messages
            .iter()
            .filter(|row| visible(context, &row.space_id))
            .filter(|row| row.message.thread_id == *thread_id && row.message.seq > seq)
            .filter(|row| row.message.role != boma_domain::MessageRole::Tool)
            .count();
        Ok(u64::try_from(count).unwrap_or(u64::MAX))
    }

    async fn page(
        &self,
        context: &TenantContext,
        thread_id: &ThreadId,
        before: Option<i64>,
        limit: usize,
    ) -> Result<MessagePage, AppError> {
        let state = read(&self.state);
        let Some(thread) = state.thread(thread_id) else {
            return Ok(MessagePage {
                messages: Vec::new(),
                next_cursor: None,
            });
        };
        if !visible(context, &thread.thread.space_id) {
            return Ok(MessagePage {
                messages: Vec::new(),
                next_cursor: None,
            });
        }

        let mut messages: Vec<&MessageRow> = state
            .messages
            .iter()
            .filter(|row| row.message.thread_id == *thread_id)
            .filter(|row| before.is_none_or(|before| row.message.seq < before))
            .collect();
        // The sequence is the only ordering a client may trust (REQ-005).
        messages.sort_by_key(|row| std::cmp::Reverse(row.message.seq));
        let selected: Vec<&MessageRow> = messages.into_iter().take(limit).collect();

        // The cursor points at the oldest message of the page, and only when older messages are
        // left: a client that follows it gets the next page.
        let next_cursor = selected.last().and_then(|oldest| {
            let oldest_seq = oldest.message.seq;
            let has_more = state
                .messages
                .iter()
                .any(|row| row.message.thread_id == *thread_id && row.message.seq < oldest_seq);
            has_more.then(|| cursor_from_seq(oldest_seq))
        });

        Ok(MessagePage {
            messages: selected
                .into_iter()
                .map(|row| row.message.clone())
                .collect(),
            next_cursor,
        })
    }

    async fn set_text(
        &self,
        context: &TenantContext,
        message_id: &MessageId,
        text: &str,
    ) -> Result<(), AppError> {
        let mut state = write(&self.state);
        let Some(row) = state
            .messages
            .iter_mut()
            .find(|row| row.message.id == *message_id)
        else {
            return Err(AppError::NotFound);
        };
        if !visible(context, &row.space_id) {
            return Err(AppError::NotFound);
        }
        // A streamed reply replaces the text of the message (REQ-020).
        row.message.blocks = vec![Block::Text {
            text: text.to_owned(),
        }];
        Ok(())
    }
}

/// The message a nonce already produced, in a thread.
fn message_with_nonce<'rows>(
    state: &'rows State,
    thread_id: &ThreadId,
    nonce: &str,
) -> Option<&'rows MessageRow> {
    state.messages.iter().find(|row| {
        row.message.thread_id == *thread_id && row.message.client_nonce.as_deref() == Some(nonce)
    })
}

/// The store of runs.
///
/// Three rules shape it: the nonce makes a start idempotent, the claim is atomic, and the fence
/// guards the writes (REQ-002, REQ-012).
#[derive(Clone)]
pub struct MemoryRunStore {
    /// The rows.
    state: Arc<RwLock<State>>,
    /// The clock that stamps the expiry of a lease.
    clock: Arc<dyn Clock>,
}

impl MemoryRunStore {
    /// Builds the store on a database.
    ///
    /// The store needs a clock to stamp the expiry of a lease.
    #[must_use]
    pub fn new(database: &MemoryDatabase, clock: Arc<dyn Clock>) -> Self {
        Self {
            state: Arc::clone(&database.state),
            clock,
        }
    }
}

impl Default for MemoryRunStore {
    fn default() -> Self {
        Self::new(
            default_database(),
            Arc::new(crate::scripts::ScriptedClock::fixed(
                crate::scripts::DEFAULT_INSTANT,
            )),
        )
    }
}

#[async_trait::async_trait]
impl RunStore for MemoryRunStore {
    async fn create(&self, run: NewRun) -> Result<Run, AppError> {
        let mut state = write(&self.state);

        // The nonce already started a run: the caller replays a send, and the run it already
        // has comes back unchanged (REQ-002).
        if let Some(nonce) = run.client_nonce.as_deref()
            && let Some(existing) = run_with_nonce(&state, &run.thread_id, nonce)
        {
            return Ok(existing.run.clone());
        }
        if state.runs.iter().any(|row| row.run.id == run.id) {
            return Err(AppError::Conflict);
        }

        let stored = Run {
            id: run.id.clone(),
            space_id: run.space_id.clone(),
            bot_id: run.bot_id.clone(),
            thread_id: run.thread_id.clone(),
            user_id: run.user_id.clone(),
            status: RunStatus::Queued,
            trigger: run.trigger,
            client_nonce: run.client_nonce.clone(),
            source_message_id: run.source_message_id.clone(),
            created_at: run.created_at.clone(),
            started_at: None,
            completed_at: None,
            error: None,
            lease_owner: None,
            lease_fence: 0,
            lease_expires_at: None,
            steering_from_seq: None,
            steering_consumed_seq: 0,
            attempt: 1,
            model_provider: run.model_provider,
            model_id: run.model_id,
            model_pinned: run.model_pinned,
            parent_run_id: run.parent_run_id,
            routine_id: run.routine_id,
            not_before: None,
        };
        state.runs.push(RunRow {
            run: stored.clone(),
        });
        Ok(stored)
    }

    async fn retry(
        &self,
        context: &TenantContext,
        id: &RunId,
        not_before: Option<&str>,
    ) -> Result<Run, AppError> {
        let mut state = write(&self.state);
        let Some(row) = state
            .runs
            .iter_mut()
            .find(|row| row.run.id == *id && visible(context, &row.run.space_id))
        else {
            return Err(AppError::NotFound);
        };
        match row.run.status {
            RunStatus::Failed => {}
            _ => return Err(AppError::Conflict),
        }
        row.run.status = RunStatus::Queued;
        row.run.attempt += 1;
        row.run.started_at = None;
        row.run.completed_at = None;
        row.run.lease_owner = None;
        row.run.lease_fence = 0;
        row.run.lease_expires_at = None;
        row.run.steering_from_seq = None;
        row.run.steering_consumed_seq = 0;
        row.run.not_before = not_before.map(str::to_owned);
        Ok(row.run.clone())
    }

    async fn find(&self, context: &TenantContext, id: &RunId) -> Result<Option<Run>, AppError> {
        let state = read(&self.state);
        let Some(row) = state.runs.iter().find(|row| row.run.id == *id) else {
            return Ok(None);
        };
        if !visible(context, &row.run.space_id) {
            return Ok(None);
        }
        Ok(Some(row.run.clone()))
    }

    async fn find_by_nonce(
        &self,
        context: &TenantContext,
        thread_id: &ThreadId,
        client_nonce: &str,
    ) -> Result<Option<Run>, AppError> {
        let state = read(&self.state);
        let Some(existing) = run_with_nonce(&state, thread_id, client_nonce) else {
            return Ok(None);
        };
        if !visible(context, &existing.run.space_id) {
            return Ok(None);
        }
        Ok(Some(existing.run.clone()))
    }

    async fn find_active_for_thread(
        &self,
        context: &TenantContext,
        thread_id: &ThreadId,
    ) -> Result<Option<Run>, AppError> {
        let state = read(&self.state);
        let mut candidate: Option<&Run> = None;
        for row in &state.runs {
            let run = &row.run;
            if run.thread_id != *thread_id || !visible(context, &run.space_id) {
                continue;
            }
            if !matches!(run.status, RunStatus::Leased | RunStatus::Running) {
                continue;
            }
            if !run.trigger.accepts_steering() {
                continue;
            }
            candidate = match candidate {
                None => Some(run),
                Some(current) => {
                    if current.created_at < run.created_at
                        || (current.created_at == run.created_at && current.id < run.id)
                    {
                        Some(run)
                    } else {
                        Some(current)
                    }
                }
            };
        }
        Ok(candidate.cloned())
    }

    async fn set_steering_from(
        &self,
        context: &TenantContext,
        id: &RunId,
        seq: i64,
    ) -> Result<(), AppError> {
        let mut state = write(&self.state);
        let Some(row) = state.runs.iter_mut().find(|row| row.run.id == *id) else {
            return Err(AppError::NotFound);
        };
        if !visible(context, &row.run.space_id) {
            return Err(AppError::NotFound);
        }
        // The first pending message decides: a later one never moves the window back or forth.
        row.run.steering_from_seq = Some(match row.run.steering_from_seq {
            Some(current) => current.min(seq),
            None => seq,
        });
        Ok(())
    }

    async fn set_steering_consumed(
        &self,
        context: &TenantContext,
        id: &RunId,
        seq: i64,
        fence: Option<i64>,
    ) -> Result<(), AppError> {
        let mut state = write(&self.state);
        let Some(row) = state.runs.iter_mut().find(|row| row.run.id == *id) else {
            return Err(AppError::NotFound);
        };
        if !visible(context, &row.run.space_id) {
            return Err(AppError::NotFound);
        }
        if row.run.status.is_terminal() {
            return Err(AppError::Conflict);
        }
        if let Some(fence) = fence
            && row.run.lease_fence != fence
        {
            return Err(AppError::Conflict);
        }
        row.run.steering_consumed_seq = row.run.steering_consumed_seq.max(seq);
        Ok(())
    }

    async fn claim_next_queued(&self, owner: &str) -> Result<Option<Run>, AppError> {
        let now = self.clock.now();
        let expires_at = self.clock.now_plus_seconds(LEASE_TTL_SECONDS);
        let mut state = write(&self.state);

        // An expired lease is no longer held: its worker disappeared mid-turn, and the run is
        // the next turn the queue owes anybody (REQ-013, REQ-014). A live lease still wins: two
        // workers never claim the same run (REQ-012).
        let expired_lease = |run: &Run| {
            matches!(run.status, RunStatus::Leased | RunStatus::Running)
                && run
                    .lease_expires_at
                    .as_deref()
                    .is_some_and(|expires| expires < now.as_str())
        };
        // The oldest candidate: the creation time decides, the identifier breaks the tie.
        let mut candidate: Option<usize> = None;
        for (index, row) in state.runs.iter().enumerate() {
            if row.run.status != RunStatus::Queued && !expired_lease(&row.run) {
                continue;
            }
            if row.run.status == RunStatus::Queued
                && let Some(not_before) = row.run.not_before.as_deref()
                && not_before > now.as_str()
            {
                // The automatic retry scheduled it for later (REQ-024): not yet due.
                continue;
            }
            match candidate {
                None => candidate = Some(index),
                Some(current) => {
                    if is_older(&row.run, &state.runs[current].run) {
                        candidate = Some(index);
                    }
                }
            }
        }
        let Some(index) = candidate else {
            return Ok(None);
        };

        // The claim happens under the lock: two workers never claim the same run (REQ-012).
        let row = &mut state.runs[index];
        row.run.status = RunStatus::Leased;
        row.run.lease_owner = Some(owner.to_owned());
        row.run.lease_fence += 1;
        row.run.lease_expires_at = Some(expires_at);
        if row.run.started_at.is_none() {
            row.run.started_at = Some(now);
        }
        Ok(Some(row.run.clone()))
    }

    async fn set_status(
        &self,
        context: &TenantContext,
        id: &RunId,
        status: RunStatus,
        at: &str,
        fence: Option<i64>,
        error: Option<String>,
    ) -> Result<(), AppError> {
        let mut state = write(&self.state);
        let Some(row) = state.runs.iter_mut().find(|row| row.run.id == *id) else {
            return Err(AppError::NotFound);
        };
        if !visible(context, &row.run.space_id) {
            return Err(AppError::NotFound);
        }
        if row.run.status.is_terminal() {
            // A cancelled or finished run absorbs nothing else: once the chat sees a terminal
            // state, no further write may land on the row (REQ-006).
            return Err(AppError::Conflict);
        }
        if let Some(fence) = fence
            && row.run.lease_fence != fence
        {
            // Another worker took the run over: this write is refused (REQ-012).
            return Err(AppError::Conflict);
        }
        row.run.status = status;
        if status.is_terminal() {
            row.run.completed_at = Some(at.to_owned());
        }
        if let Some(error) = error {
            row.run.error = Some(error);
        }
        Ok(())
    }

    async fn find_open_routine(
        &self,
        context: &TenantContext,
        routine_id: &boma_domain::RoutineId,
    ) -> Result<Option<Run>, AppError> {
        let state = read(&self.state);
        Ok(state
            .runs
            .iter()
            .find(|row| {
                row.run.routine_id.as_ref() == Some(routine_id)
                    && matches!(
                        row.run.status,
                        RunStatus::Queued
                            | RunStatus::Leased
                            | RunStatus::Running
                            | RunStatus::WaitingInput
                            | RunStatus::WaitingTakeover
                    )
                    && visible(context, &row.run.space_id)
            })
            .map(|row| row.run.clone()))
    }

    async fn list(
        &self,
        context: &TenantContext,
        bot_id: Option<&BotId>,
        before: Option<&str>,
        limit: usize,
    ) -> Result<ActivityPage, AppError> {
        let start = before.map(parse_run_cursor).transpose()?;
        let state = read(&self.state);

        let mut rows: Vec<&RunRow> = state
            .runs
            .iter()
            .filter(|row| visible(context, &row.run.space_id))
            .filter(|row| bot_id.is_none_or(|bot| row.run.bot_id == *bot))
            .collect();
        // The newest run first: the creation time decides, the identifier breaks the tie.
        rows.sort_by(|left, right| {
            right
                .run
                .created_at
                .cmp(&left.run.created_at)
                .then_with(|| right.run.id.cmp(&left.run.id))
        });

        // The cursor names the row the page ends at; the next page starts after it.
        let offset = match start {
            None => 0,
            Some((at, id)) => rows
                .iter()
                .position(|row| row.run.created_at == at && row.run.id.as_str() == id)
                .map_or(0, |position| position + 1),
        };
        let offset = offset.min(rows.len());
        let total = rows.len();
        let selected: Vec<&RunRow> = rows.into_iter().skip(offset).take(limit).collect();

        let mut runs = Vec::with_capacity(selected.len());
        for row in &selected {
            let prompt_snippet = row
                .run
                .source_message_id
                .as_ref()
                .and_then(|id| state.message(id))
                .and_then(prompt_snippet_of);
            runs.push(RunActivity {
                run: row.run.clone(),
                prompt_snippet,
            });
        }

        // The cursor names the oldest row of the page, and only when rows are left.
        let next_cursor = if offset + runs.len() < total {
            selected
                .last()
                .map(|row| run_cursor(&row.run.created_at, row.run.id.as_str()))
        } else {
            None
        };

        Ok(ActivityPage { runs, next_cursor })
    }
}

/// The run a nonce already started, in a thread.
fn run_with_nonce<'rows>(
    state: &'rows State,
    thread_id: &ThreadId,
    nonce: &str,
) -> Option<&'rows RunRow> {
    state.runs.iter().find(|row| {
        row.run.thread_id == *thread_id && row.run.client_nonce.as_deref() == Some(nonce)
    })
}

/// The cursor of the activity list: the instant and the id of the oldest row of a page.
///
/// The format is opaque to the client, like every other cursor of the product.
fn run_cursor(created_at: &str, id: &str) -> String {
    format!("run:{created_at}|{id}")
}

/// Reads an activity cursor back.
fn parse_run_cursor(cursor: &str) -> Result<(String, String), AppError> {
    let rest = cursor.strip_prefix("run:").ok_or_else(|| {
        AppError::Rule(boma_domain::DomainError::InvalidCursor(cursor.to_owned()))
    })?;
    let (at, id) = rest.split_once('|').ok_or_else(|| {
        AppError::Rule(boma_domain::DomainError::InvalidCursor(cursor.to_owned()))
    })?;
    Ok((at.to_owned(), id.to_owned()))
}

/// Whether a run was created before another one.
fn is_older(left: &Run, right: &Run) -> bool {
    left.created_at < right.created_at
        || (left.created_at == right.created_at && left.id < right.id)
}

/// The beginning of the message that started a run, short enough to render on one line.
fn prompt_snippet_of(row: &MessageRow) -> Option<String> {
    let text = row
        .message
        .blocks
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

/// The store of events.
///
/// The sequence is allocated the way the message sequence is: from the thread, under the same
/// lock as the append (REQ-005, REQ-020). The event is then published, so that a subscriber sees
/// it as it happens.
#[derive(Clone)]
pub struct MemoryEventStore {
    /// The rows.
    state: Arc<RwLock<State>>,
    /// Where the events are published.
    publisher: Arc<dyn EventPublisher>,
}

impl MemoryEventStore {
    /// Builds the store on a database.
    ///
    /// The store needs a publisher, so that a subscriber sees the events a use case writes.
    #[must_use]
    pub fn new(database: &MemoryDatabase, publisher: Arc<dyn EventPublisher>) -> Self {
        Self {
            state: Arc::clone(&database.state),
            publisher,
        }
    }
}

impl Default for MemoryEventStore {
    fn default() -> Self {
        Self::new(default_database(), Arc::new(MemoryEventPublisher::new()))
    }
}

#[async_trait::async_trait]
impl EventStore for MemoryEventStore {
    async fn append(&self, context: &TenantContext, event: NewEvent) -> Result<Event, AppError> {
        let mut state = write(&self.state);
        if !visible(context, &event.space_id) {
            return Err(AppError::NotFound);
        }
        let thread_id = event.thread_id.clone();

        let Some(thread) = state.thread_mut(&thread_id) else {
            return Err(AppError::NotFound);
        };
        if !visible(context, &thread.thread.space_id) {
            return Err(AppError::NotFound);
        }
        // The sequence comes from the thread, in the same critical section as the append
        // (REQ-005, REQ-020).
        let seq = thread.thread.next_event_seq;
        thread.thread.next_event_seq += 1;

        let stored = Event {
            thread_id,
            seq,
            kind: event.kind,
            payload: event.payload.clone(),
            created_at: event.created_at.clone(),
        };
        state.events.push(EventRow {
            event: stored.clone(),
            space_id: event.space_id.clone(),
        });

        // The event is published while the lock is held, so that subscribers see the events in
        // the order of their sequences.
        self.publisher.publish(stored.clone());
        Ok(stored)
    }

    async fn since(
        &self,
        context: &TenantContext,
        thread_id: &ThreadId,
        after_seq: i64,
    ) -> Result<Vec<Event>, AppError> {
        let state = read(&self.state);
        let Some(thread) = state.thread(thread_id) else {
            return Ok(Vec::new());
        };
        if !visible(context, &thread.thread.space_id) {
            return Ok(Vec::new());
        }
        let mut events: Vec<Event> = state
            .events
            .iter()
            .filter(|row| row.event.thread_id == *thread_id && row.event.seq > after_seq)
            .filter(|row| visible(context, &row.space_id))
            .map(|row| row.event.clone())
            .collect();
        events.sort_by_key(|event| event.seq);
        Ok(events)
    }
}

/// The store of invitations.
///
/// The token never leaves the application: it is hashed, and only the hash is stored (REQ-042).
/// The store is not tenant data: the hash is all the caller has, and the space the invitation
/// belongs to is what the answer carries.
#[derive(Debug, Clone)]
pub struct MemoryInvitationStore {
    /// The rows.
    state: Arc<RwLock<State>>,
}

impl MemoryInvitationStore {
    /// Builds the store on a database.
    #[must_use]
    pub fn new(database: &MemoryDatabase) -> Self {
        Self {
            state: Arc::clone(&database.state),
        }
    }
}

impl Default for MemoryInvitationStore {
    fn default() -> Self {
        Self::new(default_database())
    }
}

impl Default for MemoryCardStore {
    fn default() -> Self {
        Self::new(default_database())
    }
}

impl Default for MemoryEffectStore {
    fn default() -> Self {
        Self::new(default_database())
    }
}

impl Default for MemoryToolStore {
    fn default() -> Self {
        Self::new(default_database())
    }
}

#[async_trait::async_trait]
impl InvitationStore for MemoryInvitationStore {
    async fn create(&self, invitation: NewInvitation) -> Result<Invitation, AppError> {
        let mut state = write(&self.state);
        if state
            .invitations
            .iter()
            .any(|row| row.token_hash == invitation.token_hash)
        {
            return Err(AppError::Conflict);
        }
        let stored = Invitation {
            id: invitation.id.clone(),
            space_id: invitation.space_id.clone(),
            email: invitation.email.clone(),
            role: invitation.role,
            expires_at: invitation.expires_at.clone(),
            pending: true,
        };
        state.invitations.push(InvitationRow {
            invitation: stored.clone(),
            token_hash: invitation.token_hash,
            created_at: invitation.created_at.clone(),
        });
        Ok(stored)
    }

    async fn find_by_token_hash(&self, token_hash: &[u8]) -> Result<Option<Invitation>, AppError> {
        let state = read(&self.state);
        let mut matching: Vec<&InvitationRow> = state
            .invitations
            .iter()
            .filter(|row| row.token_hash == token_hash)
            .collect();
        // The newest invitation wins, like the newest row of the table.
        matching.sort_by(|left, right| right.created_at.cmp(&left.created_at));
        Ok(matching.first().map(|row| row.invitation.clone()))
    }
}

/// The store of cards (REQ-007, REQ-017).
#[derive(Debug, Clone)]
pub struct MemoryCardStore {
    state: Arc<RwLock<State>>,
}

impl MemoryCardStore {
    /// Builds the store on the shared rows.
    #[must_use]
    pub fn new(database: &MemoryDatabase) -> Self {
        Self {
            state: Arc::clone(&database.state),
        }
    }
}

#[async_trait::async_trait]
impl CardStore for MemoryCardStore {
    async fn find(&self, context: &TenantContext, id: &CardId) -> Result<Option<Card>, AppError> {
        let state = read(&self.state);
        let Some(row) = state.cards.iter().find(|row| row.card.id == *id) else {
            return Ok(None);
        };
        if !visible(context, &row.space_id) {
            return Ok(None);
        }
        Ok(Some(row.card.clone()))
    }

    async fn answer(
        &self,
        context: &TenantContext,
        id: &CardId,
        answer: &str,
    ) -> Result<Card, AppError> {
        let mut state = write(&self.state);
        let Some(row) = state.cards.iter_mut().find(|row| row.card.id == *id) else {
            return Err(AppError::NotFound);
        };
        if !visible(context, &row.space_id) {
            return Err(AppError::NotFound);
        }
        if row.card.status != CardStatus::Pending {
            return Err(AppError::Conflict);
        }
        row.card.status = CardStatus::Answered;
        row.card.answer = Some(answer.to_owned());
        row.card.answered_at = Some("2026-01-01T00:00:00Z".to_owned());
        Ok(row.card.clone())
    }
}

/// The store of effects, exactly once per effect key (REQ-017).
#[derive(Debug, Clone)]
pub struct MemoryEffectStore {
    state: Arc<RwLock<State>>,
}

impl MemoryEffectStore {
    /// Builds the store on the shared rows.
    #[must_use]
    pub fn new(database: &MemoryDatabase) -> Self {
        Self {
            state: Arc::clone(&database.state),
        }
    }
}

#[async_trait::async_trait]
impl EffectStore for MemoryEffectStore {
    async fn record(
        &self,
        context: &TenantContext,
        effect: NewEffect,
    ) -> Result<ExternalEffect, AppError> {
        let mut state = write(&self.state);
        if state
            .effects
            .iter()
            .any(|row| row.effect.effect_key == effect.effect_key)
        {
            return Err(AppError::Conflict);
        }
        let effect = ExternalEffect {
            run_id: effect.run_id,
            effect_key: effect.effect_key.clone(),
            kind: effect.kind,
            payload: effect.payload,
            applied_at: Some("2026-01-01T00:00:00Z".to_owned()),
        };
        state.effects.push(EffectRow {
            effect: effect.clone(),
        });
        let _ = context;
        Ok(effect)
    }
}

/// The store of reactions (REQ-008).
#[derive(Debug, Clone)]
pub struct MemoryReactionStore {
    state: Arc<RwLock<State>>,
}

impl MemoryReactionStore {
    /// Builds the store on the shared rows.
    #[must_use]
    pub fn new(database: &MemoryDatabase) -> Self {
        Self {
            state: Arc::clone(&database.state),
        }
    }
}

impl Default for MemoryReactionStore {
    fn default() -> Self {
        Self::new(default_database())
    }
}

#[async_trait::async_trait]
impl ReactionStore for MemoryReactionStore {
    async fn react(
        &self,
        context: &TenantContext,
        message_id: &MessageId,
        user_id: &UserId,
        emoji: &str,
        action: ReactionAction,
    ) -> Result<Vec<Reaction>, AppError> {
        if !valid_emoji(emoji) {
            return Err(AppError::Rule(DomainError::InvalidName(
                "the emoji is not valid".to_owned(),
            )));
        }
        let emoji = emoji.trim().to_owned();
        let mut state = write(&self.state);
        let space_id = match state.message(message_id) {
            Some(row) if visible(context, &row.space_id) => row.space_id.clone(),
            _ => return Err(AppError::NotFound),
        };
        match action {
            ReactionAction::Add => {
                if !state.reactions.iter().any(|row| {
                    row.message_id == *message_id && row.user_id == *user_id && row.emoji == emoji
                }) {
                    state.reactions.push(ReactionRow {
                        emoji: emoji.clone(),
                        user_id: user_id.clone(),
                        message_id: message_id.clone(),
                        space_id,
                    });
                }
            }
            ReactionAction::Remove => {
                state.reactions.retain(|row| {
                    !(row.message_id == *message_id
                        && row.user_id == *user_id
                        && row.emoji == emoji)
                });
            }
        }
        let mut counts: std::collections::BTreeMap<&str, (u32, bool)> =
            std::collections::BTreeMap::new();
        for row in state
            .reactions
            .iter()
            .filter(|row| row.message_id == *message_id)
        {
            let entry = counts.entry(row.emoji.as_str()).or_insert((0, false));
            entry.0 += 1;
            if row.user_id == *user_id {
                entry.1 = true;
            }
        }
        Ok(counts
            .into_iter()
            .map(|(emoji, (count, reacted))| Reaction {
                emoji: emoji.to_owned(),
                count,
                reacted,
            })
            .collect())
    }

    async fn list_for_messages(
        &self,
        context: &TenantContext,
        message_ids: &[MessageId],
    ) -> Result<std::collections::HashMap<MessageId, Vec<Reaction>>, AppError> {
        if message_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }
        let caller = context.user_id().cloned();
        let state = read(&self.state);
        let mut grouped: std::collections::HashMap<
            MessageId,
            std::collections::BTreeMap<String, (u32, bool)>,
        > = std::collections::HashMap::new();
        for row in state
            .reactions
            .iter()
            .filter(|row| message_ids.contains(&row.message_id))
            .filter(|row| visible(context, &row.space_id))
        {
            let entry = grouped
                .entry(row.message_id.clone())
                .or_default()
                .entry(row.emoji.clone())
                .or_insert((0, false));
            entry.0 += 1;
            if caller.as_ref().is_some_and(|id| *id == row.user_id) {
                entry.1 = true;
            }
        }
        Ok(grouped
            .into_iter()
            .map(|(message_id, emoji_counts)| {
                let reactions = emoji_counts
                    .into_iter()
                    .map(|(emoji, (count, reacted))| Reaction {
                        emoji,
                        count,
                        reacted,
                    })
                    .collect::<Vec<_>>();
                (message_id, reactions)
            })
            .collect())
    }
}

/// Whether the string is a plausible emoji for this product.
fn valid_emoji(emoji: &str) -> bool {
    let trimmed = emoji.trim();
    if trimmed.is_empty() || trimmed.chars().count() > 16 {
        return false;
    }
    trimmed.chars().all(|c| !c.is_control())
}

/// The store of read positions (REQ-009).
#[derive(Debug, Clone)]
pub struct MemoryReadStateStore {
    state: Arc<RwLock<State>>,
}

impl MemoryReadStateStore {
    /// Builds the store on the shared rows.
    #[must_use]
    pub fn new(database: &MemoryDatabase) -> Self {
        Self {
            state: Arc::clone(&database.state),
        }
    }
}

impl Default for MemoryReadStateStore {
    fn default() -> Self {
        Self::new(default_database())
    }
}

#[async_trait::async_trait]
impl ReadStateStore for MemoryReadStateStore {
    async fn position(
        &self,
        context: &TenantContext,
        thread_id: &ThreadId,
    ) -> Result<i64, AppError> {
        let Some(user_id) = context.user_id().cloned() else {
            return Err(AppError::NotFound);
        };
        let state = read(&self.state);
        let found = state
            .read_states
            .iter()
            .find(|row| row.thread_id == *thread_id && row.user_id == user_id)
            .filter(|row| visible(context, &row.space_id));
        Ok(found.map_or(0, |row| row.last_seq))
    }

    async fn set(
        &self,
        context: &TenantContext,
        thread_id: &ThreadId,
        last_seq: i64,
    ) -> Result<i64, AppError> {
        if last_seq < 0 {
            return Err(AppError::Rule(boma_domain::DomainError::InvalidName(
                "the read position cannot be negative".to_owned(),
            )));
        }
        let Some(user_id) = context.user_id().cloned() else {
            return Err(AppError::NotFound);
        };
        let Some(space_id) = context.space_id().cloned() else {
            return Err(AppError::NotFound);
        };
        let mut state = write(&self.state);
        // The write only lands when the thread belongs to the caller's space.
        match state.thread(thread_id) {
            Some(row) if visible(context, &row.thread.space_id) => {}
            _ => return Err(AppError::NotFound),
        }
        match state
            .read_states
            .iter_mut()
            .find(|row| row.thread_id == *thread_id && row.user_id == user_id)
        {
            Some(row) => row.last_seq = last_seq,
            None => state.read_states.push(ReadStateRow {
                thread_id: thread_id.clone(),
                space_id,
                user_id,
                last_seq,
            }),
        }
        Ok(last_seq)
    }
}

/// The roster the rail renders from (REQ-009).
#[derive(Debug, Clone)]
pub struct MemoryRosterStore {
    state: Arc<RwLock<State>>,
}

impl MemoryRosterStore {
    /// Builds the store on the shared rows.
    #[must_use]
    pub fn new(database: &MemoryDatabase) -> Self {
        Self {
            state: Arc::clone(&database.state),
        }
    }
}

impl Default for MemoryRosterStore {
    fn default() -> Self {
        Self::new(default_database())
    }
}

#[async_trait::async_trait]
impl RosterStore for MemoryRosterStore {
    async fn list_bots(
        &self,
        context: &TenantContext,
    ) -> Result<Vec<boma_app::ports::BotSummary>, AppError> {
        let Some(user_id) = context.user_id().cloned() else {
            return Err(AppError::NotFound);
        };
        let now = time::OffsetDateTime::now_utc();
        let state = read(&self.state);
        let mut summaries: Vec<boma_app::ports::BotSummary> = Vec::new();
        for pair in state
            .bots
            .iter()
            .filter(|row| visible(context, &row.bot.space_id))
        {
            let bot = pair.bot.clone();
            let Some(thread) = state.threads.iter().find(|row| row.thread.bot_id == bot.id) else {
                continue;
            };
            let thread_id = thread.thread.id.clone();
            let snoozed_until = state
                .snoozes
                .iter()
                .find(|row| row.thread_id == thread_id && row.user_id == user_id)
                .filter(|row| visible(context, &row.space_id))
                .filter(|row| {
                    time::OffsetDateTime::parse(
                        &row.snoozed_until,
                        &time::format_description::well_known::Rfc3339,
                    )
                    .is_ok_and(|until| until > now)
                })
                .map(|row| row.snoozed_until.clone());
            let read_seq = state
                .read_states
                .iter()
                .find(|row| row.thread_id == thread_id && row.user_id == user_id)
                .map_or(0, |row| row.last_seq);
            let unread_count = state
                .messages
                .iter()
                .filter(|row| {
                    row.message.thread_id == thread_id
                        && visible(context, &row.space_id)
                        && row.message.role != boma_domain::MessageRole::Tool
                        && row.message.seq > read_seq
                })
                .count();
            summaries.push(boma_app::ports::BotSummary {
                bot,
                thread_id,
                snoozed_until,
                unread_count: u64::try_from(unread_count).unwrap_or(u64::MAX),
            });
        }
        summaries.sort_by(|left, right| {
            left.bot
                .created_at
                .cmp(&right.bot.created_at)
                .then(left.bot.id.as_str().cmp(right.bot.id.as_str()))
        });
        Ok(summaries)
    }

    async fn set_snooze(
        &self,
        context: &TenantContext,
        thread_id: &ThreadId,
        until: Option<&str>,
    ) -> Result<Option<String>, AppError> {
        let Some(user_id) = context.user_id().cloned() else {
            return Err(AppError::NotFound);
        };
        let Some(space_id) = context.space_id().cloned() else {
            return Err(AppError::NotFound);
        };
        let mut state = write(&self.state);
        match state.thread(thread_id) {
            Some(row) if visible(context, &row.thread.space_id) => {}
            _ => return Err(AppError::NotFound),
        }
        if let Some(until) = until {
            time::OffsetDateTime::parse(until, &time::format_description::well_known::Rfc3339)
                .map_err(|_| {
                    AppError::Rule(boma_domain::DomainError::InvalidName(
                        "until must be an RFC3339 instant".to_owned(),
                    ))
                })?;
        }
        state
            .snoozes
            .retain(|row| !(row.thread_id == *thread_id && row.user_id == user_id));
        match until {
            Some(until) => {
                state.snoozes.push(SnoozeRow {
                    thread_id: thread_id.clone(),
                    space_id,
                    user_id,
                    snoozed_until: until.to_owned(),
                });
                Ok(Some(until.to_owned()))
            }
            None => Ok(None),
        }
    }
}

/// The store of memory documents (REQ-022).
#[derive(Debug, Clone)]
pub struct MemoryMemoryStore {
    state: Arc<RwLock<State>>,
}

impl MemoryMemoryStore {
    /// Builds the store on the shared rows.
    #[must_use]
    pub fn new(database: &MemoryDatabase) -> Self {
        Self {
            state: Arc::clone(&database.state),
        }
    }
}

impl Default for MemoryMemoryStore {
    fn default() -> Self {
        Self::new(default_database())
    }
}

#[async_trait::async_trait]
impl MemoryStore for MemoryMemoryStore {
    async fn write(
        &self,
        context: &TenantContext,
        document: NewMemoryDocument,
    ) -> Result<MemoryDocument, AppError> {
        validate_memory_path(&document.path)?;
        let space_id = context.space_id().cloned().ok_or(AppError::NotFound)?;
        let mut state = write(&self.state);
        if let Some(bot_id) = &document.bot_id
            && !state
                .bots
                .iter()
                .any(|row| row.bot.id == *bot_id && visible(context, &row.bot.space_id))
        {
            return Err(AppError::NotFound);
        }
        let key_match = |row: &MemoryDocumentMemRow| {
            row.space_id == space_id && row.bot_id == document.bot_id && row.path == document.path
        };
        if let Some(existing) = state.memory_documents.iter_mut().find(|row| key_match(row)) {
            existing.content = document.content;
            existing.revision += 1;
            existing.updated_at = document.updated_at;
            return Ok(MemoryDocument {
                space_id,
                bot_id: existing.bot_id.clone(),
                path: existing.path.clone(),
                content: existing.content.clone(),
                revision: existing.revision,
                updated_at: existing.updated_at.clone(),
            });
        }
        state.memory_documents.push(MemoryDocumentMemRow {
            space_id: space_id.clone(),
            bot_id: document.bot_id.clone(),
            path: document.path.clone(),
            content: document.content.clone(),
            revision: 1,
            updated_at: document.updated_at.clone(),
        });
        Ok(MemoryDocument {
            space_id,
            bot_id: document.bot_id,
            path: document.path,
            content: document.content,
            revision: 1,
            updated_at: document.updated_at,
        })
    }

    async fn read(
        &self,
        context: &TenantContext,
        bot_id: Option<&BotId>,
        path: &str,
    ) -> Result<Option<MemoryDocument>, AppError> {
        let state = read(&self.state);
        Ok(state
            .memory_documents
            .iter()
            .find(|row| {
                visible(context, &row.space_id) && row.bot_id == bot_id.cloned() && row.path == path
            })
            .map(|row| MemoryDocument {
                space_id: row.space_id.clone(),
                bot_id: row.bot_id.clone(),
                path: row.path.clone(),
                content: row.content.clone(),
                revision: row.revision,
                updated_at: row.updated_at.clone(),
            }))
    }

    async fn list(
        &self,
        context: &TenantContext,
        bot_id: Option<&BotId>,
    ) -> Result<Vec<MemoryDocument>, AppError> {
        let state = read(&self.state);
        let mut documents: Vec<MemoryDocument> = state
            .memory_documents
            .iter()
            .filter(|row| visible(context, &row.space_id) && row.bot_id == bot_id.cloned())
            .map(|row| MemoryDocument {
                space_id: row.space_id.clone(),
                bot_id: row.bot_id.clone(),
                path: row.path.clone(),
                content: row.content.clone(),
                revision: row.revision,
                updated_at: row.updated_at.clone(),
            })
            .collect();
        documents.sort_by(|left, right| left.path.cmp(&right.path));
        Ok(documents)
    }

    async fn remove(
        &self,
        context: &TenantContext,
        bot_id: Option<&BotId>,
        path: &str,
    ) -> Result<bool, AppError> {
        let mut state = write(&self.state);
        let before = state.memory_documents.len();
        state.memory_documents.retain(|row| {
            !(visible(context, &row.space_id) && row.bot_id == bot_id.cloned() && row.path == path)
        });
        Ok(state.memory_documents.len() != before)
    }
}

/// The store of routines (REQ-023).
#[derive(Debug, Clone)]
pub struct MemoryRoutineStore {
    state: Arc<RwLock<State>>,
}

impl MemoryRoutineStore {
    /// Builds the store on the shared rows.
    #[must_use]
    pub fn new(database: &MemoryDatabase) -> Self {
        Self {
            state: Arc::clone(&database.state),
        }
    }
}

impl Default for MemoryRoutineStore {
    fn default() -> Self {
        Self::new(default_database())
    }
}

#[async_trait::async_trait]
impl RoutineStore for MemoryRoutineStore {
    async fn create(
        &self,
        context: &TenantContext,
        routine: boma_app::ports::NewRoutine,
    ) -> Result<boma_app::ports::Routine, AppError> {
        let space_id = routine.space_id.clone();
        let mut state = write(&self.state);
        if !state
            .bots
            .iter()
            .any(|row| row.bot.id == routine.bot_id && visible(context, &row.bot.space_id))
        {
            return Err(AppError::NotFound);
        }
        if state
            .routines
            .iter()
            .any(|row| row.routine.id == routine.id)
        {
            return Err(AppError::Conflict);
        }
        let created_at = routine.created_at.clone();
        let record = boma_app::ports::Routine {
            id: routine.id,
            space_id: space_id.clone(),
            bot_id: routine.bot_id,
            thread_id: routine.thread_id,
            user_id: routine.user_id,
            name: routine.name,
            prompt: routine.prompt,
            crons: routine.crons,
            timezone: routine.timezone,
            active: routine.active,
            notify: routine.notify,
            last_run_at: None,
            next_run_at: routine.next_run_at,
            account_failures: 0,
            created_at: created_at.clone(),
            updated_at: created_at,
        };
        state.routines.push(RoutineRow {
            routine: record.clone(),
        });
        Ok(record)
    }

    async fn find(
        &self,
        context: &TenantContext,
        id: &boma_domain::RoutineId,
    ) -> Result<Option<boma_app::ports::Routine>, AppError> {
        let state = read(&self.state);
        Ok(state
            .routines
            .iter()
            .find(|row| row.routine.id == *id && visible(context, &row.routine.space_id))
            .map(|row| row.routine.clone()))
    }

    async fn remove(
        &self,
        context: &TenantContext,
        id: &boma_domain::RoutineId,
    ) -> Result<bool, AppError> {
        let mut state = write(&self.state);
        let before = state.routines.len();
        state
            .routines
            .retain(|row| !(row.routine.id == *id && visible(context, &row.routine.space_id)));
        Ok(state.routines.len() != before)
    }

    async fn list(
        &self,
        context: &TenantContext,
        bot_id: &boma_domain::BotId,
    ) -> Result<Vec<boma_app::ports::Routine>, AppError> {
        let state = read(&self.state);
        let mut routines: Vec<boma_app::ports::Routine> = state
            .routines
            .iter()
            .filter(|row| visible(context, &row.routine.space_id) && row.routine.bot_id == *bot_id)
            .map(|row| row.routine.clone())
            .collect();
        routines.sort_by(|left, right| {
            left.created_at
                .cmp(&right.created_at)
                .then(left.id.as_str().cmp(right.id.as_str()))
        });
        Ok(routines)
    }

    async fn due(&self, as_of: &str) -> Result<Vec<boma_app::ports::Routine>, AppError> {
        let state = read(&self.state);
        let mut routines: Vec<boma_app::ports::Routine> = state
            .routines
            .iter()
            .filter(|row| {
                row.routine.active
                    && row
                        .routine
                        .next_run_at
                        .as_deref()
                        .is_some_and(|at| at <= as_of)
            })
            .map(|row| row.routine.clone())
            .collect();
        routines.sort_by(|left, right| {
            left.next_run_at
                .cmp(&right.next_run_at)
                .then(left.id.as_str().cmp(right.id.as_str()))
        });
        Ok(routines)
    }

    async fn record_fired(
        &self,
        context: &TenantContext,
        id: &boma_domain::RoutineId,
        occurred_at: &str,
        run_id: Option<&RunId>,
        next_run_at: Option<&str>,
    ) -> Result<boma_app::ports::Routine, AppError> {
        let mut state = write(&self.state);
        let Some(index) = state
            .routines
            .iter()
            .position(|row| row.routine.id == *id && visible(context, &row.routine.space_id))
        else {
            return Err(AppError::NotFound);
        };
        let updated_at = occurred_at.to_owned();
        let routine = &mut state.routines[index].routine;
        routine.last_run_at = Some(occurred_at.to_owned());
        routine.next_run_at = next_run_at.map(str::to_owned);
        if next_run_at.is_none() {
            routine.active = false;
        }
        routine.account_failures = 0;
        routine.updated_at = updated_at;
        let updated = routine.clone();
        let space_id = updated.space_id.clone();
        state.routine_runs.push(RoutineRunRow {
            routine_id: id.clone(),
            space_id,
            run_id: run_id.cloned(),
            outcome: "fired".to_owned(),
            detail: None,
            occurred_at: occurred_at.to_owned(),
        });
        Ok(updated)
    }

    async fn record_failure(
        &self,
        context: &TenantContext,
        id: &boma_domain::RoutineId,
        occurred_at: &str,
        run_id: Option<&RunId>,
        threshold: u32,
    ) -> Result<boma_app::ports::Routine, AppError> {
        let mut state = write(&self.state);
        let Some(index) = state
            .routines
            .iter()
            .position(|row| row.routine.id == *id && visible(context, &row.routine.space_id))
        else {
            return Err(AppError::NotFound);
        };
        let updated_at = occurred_at.to_owned();
        let routine = &mut state.routines[index].routine;
        routine.account_failures += 1;
        if routine.account_failures >= threshold {
            routine.active = false;
        }
        routine.updated_at = updated_at;
        let updated = routine.clone();
        let space_id = updated.space_id.clone();
        let outcome = if updated.active { "failed" } else { "paused" };
        state.routine_runs.push(RoutineRunRow {
            routine_id: id.clone(),
            space_id,
            run_id: run_id.cloned(),
            outcome: outcome.to_owned(),
            detail: None,
            occurred_at: occurred_at.to_owned(),
        });
        Ok(updated)
    }

    async fn record_completed(
        &self,
        context: &TenantContext,
        id: &boma_domain::RoutineId,
        occurred_at: &str,
        run_id: Option<&RunId>,
    ) -> Result<(), AppError> {
        let mut state = write(&self.state);
        let Some(record) = state
            .routines
            .iter_mut()
            .find(|row| row.routine.id == *id && visible(context, &row.routine.space_id))
        else {
            return Err(AppError::NotFound);
        };
        record.routine.account_failures = 0;
        occurred_at.clone_into(&mut record.routine.updated_at);
        let space_id = record.routine.space_id.clone();
        state.routine_runs.push(RoutineRunRow {
            routine_id: id.clone(),
            space_id,
            run_id: run_id.cloned(),
            outcome: "fired".to_owned(),
            detail: None,
            occurred_at: occurred_at.to_owned(),
        });
        Ok(())
    }

    async fn record_skip(
        &self,
        context: &TenantContext,
        id: &boma_domain::RoutineId,
        outcome: RoutineOutcome,
        occurred_at: &str,
        run_id: Option<&RunId>,
        next_run_at: Option<&str>,
        detail: Option<&str>,
    ) -> Result<(), AppError> {
        let mut state = write(&self.state);
        let Some(record) = state
            .routines
            .iter_mut()
            .find(|row| row.routine.id == *id && visible(context, &row.routine.space_id))
        else {
            return Err(AppError::NotFound);
        };
        record.routine.next_run_at = next_run_at.map(str::to_owned);
        if next_run_at.is_none() {
            record.routine.active = false;
        }
        occurred_at.clone_into(&mut record.routine.updated_at);
        let space_id = record.routine.space_id.clone();
        let outcome = match outcome {
            RoutineOutcome::Fired => "fired",
            RoutineOutcome::Coalesced => "coalesced",
            RoutineOutcome::Skipped | RoutineOutcome::Once => "skipped",
            RoutineOutcome::Failed => "failed",
            RoutineOutcome::Paused => "paused",
        };
        state.routine_runs.push(RoutineRunRow {
            routine_id: id.clone(),
            space_id,
            run_id: run_id.cloned(),
            outcome: outcome.to_owned(),
            detail: detail.map(str::to_owned),
            occurred_at: occurred_at.to_owned(),
        });
        Ok(())
    }
}

/// The in-memory publisher: one broadcast channel per thread.
///
/// A subscriber receives the events of the thread it asked for and nothing else, from the moment
/// it subscribed: a client that reconnects replays the log from its last sequence first, then
/// listens (REQ-005, REQ-020).
#[derive(Debug)]
pub struct MemoryEventPublisher {
    /// The channel of each thread.
    channels: Mutex<HashMap<ThreadId, broadcast::Sender<Event>>>,
    /// How many events a channel keeps before a slow subscriber misses one.
    capacity: usize,
}

/// The store of tool installs (REQ-016).
#[derive(Debug, Clone)]
pub struct MemoryToolStore {
    state: Arc<RwLock<State>>,
}

impl MemoryToolStore {
    /// Builds the store on the shared rows.
    #[must_use]
    pub fn new(database: &MemoryDatabase) -> Self {
        Self {
            state: Arc::clone(&database.state),
        }
    }
}

#[async_trait::async_trait]
impl ToolStore for MemoryToolStore {
    async fn catalogue_for(
        &self,
        context: &TenantContext,
        bot_id: &BotId,
    ) -> Result<Vec<ToolSpec>, AppError> {
        // The bot anchors the catalogue: no caller context required (a system context may ask,
        // the worker is one), and the tenant guard is still the row-level security below.
        let state = read(&self.state);
        let Some(row) = state.bots.iter().find(|row| row.bot.id == *bot_id) else {
            return Ok(Vec::new());
        };
        if !visible(context, &row.bot.space_id) {
            return Ok(Vec::new());
        }
        let space_id = row.bot.space_id.clone();
        let grant = row.bot.enabled_tools.clone();
        let catalogue = state
            .tool_installs
            .iter()
            .filter(|row| row.space_id == space_id && row.enabled)
            .filter(|row| grant.is_empty() || grant.iter().any(|key| key == &row.tool_key))
            .map(|row| ToolSpec {
                key: row.tool_key.clone(),
                description: row.description.clone(),
                kind: row.kind.clone(),
            })
            .collect();
        Ok(catalogue)
    }
}

impl MemoryEventPublisher {
    /// Builds the publisher.
    #[must_use]
    pub fn new() -> Self {
        Self::with_capacity(EVENT_CHANNEL_CAPACITY)
    }

    /// Builds the publisher with the size of its channels.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            channels: Mutex::new(HashMap::new()),
            capacity,
        }
    }

    /// The channel of a thread, created the first time it is needed.
    fn sender(&self, thread_id: &ThreadId) -> broadcast::Sender<Event> {
        let mut channels = match self.channels.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        channels
            .entry(thread_id.clone())
            .or_insert_with(|| broadcast::channel(self.capacity).0)
            .clone()
    }
}

impl Default for MemoryEventPublisher {
    fn default() -> Self {
        Self::new()
    }
}

impl EventPublisher for MemoryEventPublisher {
    fn publish(&self, event: Event) {
        let sender = self.sender(&event.thread_id);
        // A thread nobody listens to has no receiver: the event is still written in the log.
        let _ = sender.send(event);
    }

    fn subscribe(&self, thread_id: &ThreadId) -> broadcast::Receiver<Event> {
        self.sender(thread_id).subscribe()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use boma_app::ports::{
        AccountStore, BotStore, EventPublisher, EventStore, MessageStore, NewBot, NewEvent,
        NewMessage, NewRun, NewSpace, NewThread, RunStore, SpaceStore, UserStore,
    };
    use boma_domain::{
        Block, Email, EventType, MessageRole, RunStatus, RunTrigger, SystemReason, TenantContext,
    };
    use tokio::sync::broadcast;

    use super::{
        MemoryAccountStore, MemoryBotStore, MemoryDatabase, MemoryEventPublisher, MemoryEventStore,
        MemoryMessageStore, MemoryRunStore, MemorySpaceStore, MemoryUserStore,
    };
    use crate::scripts::ScriptedClock;

    /// The identifiers a fixture uses.
    fn space_id(value: &str) -> boma_domain::SpaceId {
        boma_domain::SpaceId::new(value).expect("an id is accepted")
    }

    /// The identifiers a fixture uses.
    fn user_id(value: &str) -> boma_domain::UserId {
        boma_domain::UserId::new(value).expect("an id is accepted")
    }

    /// The identifiers a fixture uses.
    fn bot_id(value: &str) -> boma_domain::BotId {
        boma_domain::BotId::new(value).expect("an id is accepted")
    }

    /// The identifiers a fixture uses.
    fn thread_id(value: &str) -> boma_domain::ThreadId {
        boma_domain::ThreadId::new(value).expect("an id is accepted")
    }

    /// The identifiers a fixture uses.
    fn message_id(value: &str) -> boma_domain::MessageId {
        boma_domain::MessageId::new(value).expect("an id is accepted")
    }

    /// The identifiers a fixture uses.
    fn run_id(value: &str) -> boma_domain::RunId {
        boma_domain::RunId::new(value).expect("an id is accepted")
    }

    /// The identifiers a fixture uses.
    fn organization_id(value: &str) -> boma_domain::OrganizationId {
        boma_domain::OrganizationId::new(value).expect("an id is accepted")
    }

    /// The tenant of a member.
    fn member(space: &boma_domain::SpaceId, user: &boma_domain::UserId) -> TenantContext {
        TenantContext::Member {
            space_id: space.clone(),
            user_id: user.clone(),
        }
    }

    /// The rows a test writes before it exercises a store.
    struct Fixture {
        database: MemoryDatabase,
        context: TenantContext,
        space: boma_domain::SpaceId,
        user: boma_domain::UserId,
        bot: boma_domain::BotId,
        thread: boma_domain::ThreadId,
        messages: MemoryMessageStore,
        runs: MemoryRunStore,
        bots: MemoryBotStore,
    }

    /// Writes a space, a bot and its conversation, and hands the stores over.
    async fn fixture() -> Fixture {
        let database = MemoryDatabase::new();
        let space = space_id("spc_1");
        let user = user_id("usr_1");
        let bot = bot_id("bot_1");
        let thread = thread_id("thr_1");

        let spaces = MemorySpaceStore::new(&database);
        spaces
            .create(
                NewSpace {
                    id: space.clone(),
                    organization_id: organization_id("org_1"),
                    name: "Space".to_owned(),
                    is_default: true,
                    created_at: "2026-01-01T00:00:00Z".to_owned(),
                },
                &user,
            )
            .await
            .expect("the space is created");

        let bots = MemoryBotStore::new(&database);
        bots.create_with_thread(
            NewBot {
                id: bot.clone(),
                space_id: space.clone(),
                user_id: user.clone(),
                name: "Assistant".to_owned(),
                description: None,
                color: None,
                created_at: "2026-01-01T00:00:00Z".to_owned(),
            },
            NewThread {
                id: thread.clone(),
                space_id: space.clone(),
                bot_id: bot.clone(),
                user_id: user.clone(),
                created_at: "2026-01-01T00:00:00Z".to_owned(),
            },
        )
        .await
        .expect("the bot is created");

        let clock = Arc::new(ScriptedClock::fixed("2026-01-01T00:00:00Z"));
        let runs = MemoryRunStore::new(&database, clock);
        let messages = MemoryMessageStore::new(&database);

        Fixture {
            database,
            context: member(&space, &user),
            space,
            user,
            bot,
            thread,
            messages,
            runs,
            bots,
        }
    }

    /// A message a test stores.
    fn new_message(fixture: &Fixture, id: &str, nonce: Option<&str>, text: &str) -> NewMessage {
        NewMessage {
            id: message_id(id),
            space_id: fixture.space.clone(),
            thread_id: fixture.thread.clone(),
            role: MessageRole::User,
            blocks: vec![Block::Text {
                text: text.to_owned(),
            }],
            bot_id: None,
            author_user_id: Some(fixture.user.clone()),
            run_id: None,
            client_nonce: nonce.map(str::to_owned),
            created_at: "2026-01-01T00:00:00Z".to_owned(),
            reply_to_message_id: None,
            reply_quote: None,
        }
    }

    /// A run a test starts.
    fn new_run(fixture: &Fixture, id: &str, nonce: Option<&str>) -> NewRun {
        NewRun {
            id: run_id(id),
            space_id: fixture.space.clone(),
            bot_id: fixture.bot.clone(),
            thread_id: fixture.thread.clone(),
            user_id: fixture.user.clone(),
            trigger: RunTrigger::User,
            client_nonce: nonce.map(str::to_owned),
            source_message_id: None,
            created_at: "2026-01-01T00:00:00Z".to_owned(),
            model_provider: None,
            model_id: None,
            model_pinned: false,
            parent_run_id: None,
            routine_id: None,
        }
    }

    #[tokio::test]
    async fn req_005_two_inserts_on_one_thread_get_different_sequences() {
        let fixture = fixture().await;
        let first = fixture
            .messages
            .insert(
                &fixture.context,
                new_message(&fixture, "msg_1", None, "hello"),
            )
            .await
            .expect("the first message is stored");
        let second = fixture
            .messages
            .insert(
                &fixture.context,
                new_message(&fixture, "msg_2", None, "again"),
            )
            .await
            .expect("the second message is stored");

        assert!(first.created, "the first message is new");
        assert!(second.created, "the second message is new");
        assert_ne!(
            first.message.seq, second.message.seq,
            "two messages of one thread never share a sequence"
        );
        assert_eq!(second.message.seq, first.message.seq + 1);
    }

    #[tokio::test]
    async fn req_002_the_same_nonce_returns_the_same_message_and_run() {
        let fixture = fixture().await;

        let first = fixture
            .messages
            .insert(
                &fixture.context,
                new_message(&fixture, "msg_1", Some("nonce-1"), "hello"),
            )
            .await
            .expect("the message is stored");
        let replay = fixture
            .messages
            .insert(
                &fixture.context,
                new_message(&fixture, "msg_2", Some("nonce-1"), "hello"),
            )
            .await
            .expect("the replay is stored");

        assert!(first.created, "the first send creates the message");
        assert!(!replay.created, "the replay creates nothing");
        assert_eq!(first.message.id, replay.message.id);
        assert_eq!(first.message.seq, replay.message.seq);

        let run = fixture
            .runs
            .create(new_run(&fixture, "run_1", Some("nonce-1")))
            .await
            .expect("the run is created");
        let replayed_run = fixture
            .runs
            .create(new_run(&fixture, "run_2", Some("nonce-1")))
            .await
            .expect("the replayed run comes back");

        assert_eq!(
            run.id, replayed_run.id,
            "the same nonce starts the same run"
        );
        assert_eq!(run.status, RunStatus::Queued);
    }

    #[tokio::test]
    async fn req_043_a_member_context_cannot_read_another_tenants_bot() {
        let fixture = fixture().await;

        // Another tenant, with its own bot.
        let other_space = space_id("spc_2");
        let other_user = user_id("usr_2");
        let other_bot = bot_id("bot_2");
        let spaces = MemorySpaceStore::new(&fixture.database);
        spaces
            .create(
                NewSpace {
                    id: other_space.clone(),
                    organization_id: organization_id("org_2"),
                    name: "Other".to_owned(),
                    is_default: true,
                    created_at: "2026-01-01T00:00:00Z".to_owned(),
                },
                &other_user,
            )
            .await
            .expect("the space is created");
        fixture
            .bots
            .create_with_thread(
                NewBot {
                    id: other_bot.clone(),
                    space_id: other_space.clone(),
                    user_id: other_user.clone(),
                    name: "Other assistant".to_owned(),
                    description: None,
                    color: None,
                    created_at: "2026-01-01T00:00:00Z".to_owned(),
                },
                NewThread {
                    id: thread_id("thr_2"),
                    space_id: other_space.clone(),
                    bot_id: other_bot.clone(),
                    user_id: other_user.clone(),
                    created_at: "2026-01-01T00:00:00Z".to_owned(),
                },
            )
            .await
            .expect("the bot is created");

        let found = fixture
            .bots
            .find(&fixture.context, &other_bot)
            .await
            .expect("the store answers");
        assert!(
            found.is_none(),
            "a bot of another tenant is not found, never forbidden"
        );

        let system = TenantContext::System {
            reason: SystemReason::DataMigration,
            user_id: None,
        };
        let found = fixture
            .bots
            .find(&system, &other_bot)
            .await
            .expect("the store answers");
        assert!(found.is_some(), "a system path sees every tenant");
    }

    #[tokio::test]
    async fn req_012_two_claims_never_return_the_same_run() {
        let fixture = fixture().await;
        fixture
            .runs
            .create(new_run(&fixture, "run_1", None))
            .await
            .expect("the run is created");
        fixture
            .runs
            .create(new_run(&fixture, "run_2", None))
            .await
            .expect("the run is created");

        let first = fixture.runs.clone();
        let second = fixture.runs.clone();
        let (left, right) = tokio::join!(
            first.claim_next_queued("worker-1"),
            second.claim_next_queued("worker-2")
        );
        let left = left.expect("the store answers");
        let right = right.expect("the store answers");

        let left = left.expect("the first worker claims a run");
        let right = right.expect("the second worker claims a run");
        assert_ne!(left.id, right.id, "two claims never return the same run");
        assert_eq!(left.status, RunStatus::Leased);
        assert_eq!(right.status, RunStatus::Leased);

        // A third claim finds nothing left: both runs are leased.
        let empty = fixture
            .runs
            .claim_next_queued("worker-3")
            .await
            .expect("the store answers");
        assert!(empty.is_none(), "the queue is empty");
    }

    #[tokio::test]
    async fn req_012_a_stale_fence_cannot_write() {
        let fixture = fixture().await;
        fixture
            .runs
            .create(new_run(&fixture, "run_1", None))
            .await
            .expect("the run is created");

        let claimed = fixture
            .runs
            .claim_next_queued("worker-1")
            .await
            .expect("the store answers")
            .expect("the run is claimed");
        assert_eq!(claimed.lease_fence, 1);

        let refused = fixture
            .runs
            .set_status(
                &fixture.context,
                &claimed.id,
                RunStatus::Running,
                "2026-01-01T00:00:00Z",
                Some(0),
                None,
            )
            .await;
        assert!(
            refused.is_err(),
            "a fence that no longer matches is refused"
        );

        let accepted = fixture
            .runs
            .set_status(
                &fixture.context,
                &claimed.id,
                RunStatus::Running,
                "2026-01-01T00:00:00Z",
                Some(claimed.lease_fence),
                None,
            )
            .await;
        assert!(accepted.is_ok(), "the holder of the lease writes");
    }

    #[tokio::test]
    async fn req_013_an_expired_lease_is_reclaimed_after_a_deploy() {
        let fixture = fixture().await;
        fixture
            .runs
            .create(new_run(&fixture, "run_1", None))
            .await
            .expect("the run is created");

        // The first worker takes the lease, then disappears mid-turn.
        let first = fixture
            .runs
            .claim_next_queued("worker-1")
            .await
            .expect("the claim answers")
            .expect("a run is claimed");
        assert_eq!(first.status, RunStatus::Leased);
        assert_eq!(first.lease_fence, 1);

        // Later, with a clock that stands past the lease's end, the run is handed out again.
        let later = Arc::new(ScriptedClock::fixed("2026-06-01T00:00:00Z"));
        let runs2 = MemoryRunStore::new(&fixture.database, later);
        let reclaimed = runs2
            .claim_next_queued("worker-2")
            .await
            .expect("the claim answers")
            .expect("the stale lease is reclaimed");
        assert_eq!(reclaimed.id, first.id);
        assert_eq!(reclaimed.lease_owner.as_deref(), Some("worker-2"));
        assert_eq!(reclaimed.lease_fence, 2, "the reclaim fences the run again");
        assert_eq!(reclaimed.status, RunStatus::Leased);
        // The original turn keeps its start instant: the story of the run is not rewritten.
        assert_eq!(reclaimed.started_at, first.started_at);
    }

    #[tokio::test]
    async fn req_005_the_events_of_a_thread_are_sequenced_and_published() {
        let fixture = fixture().await;
        let publisher = Arc::new(MemoryEventPublisher::new());
        let events = MemoryEventStore::new(&fixture.database, publisher.clone());
        let mut subscriber = publisher.subscribe(&fixture.thread);

        let first = events
            .append(
                &fixture.context,
                NewEvent {
                    space_id: fixture.space.clone(),
                    thread_id: fixture.thread.clone(),
                    kind: EventType::Message,
                    payload: serde_json::json!({"messageId": "msg_1"}),
                    created_at: "2026-01-01T00:00:00Z".to_owned(),
                },
            )
            .await
            .expect("the event is appended");
        let second = events
            .append(
                &fixture.context,
                NewEvent {
                    space_id: fixture.space.clone(),
                    thread_id: fixture.thread.clone(),
                    kind: EventType::Run,
                    payload: serde_json::json!({}),
                    created_at: "2026-01-01T00:00:01Z".to_owned(),
                },
            )
            .await
            .expect("the event is appended");

        assert_ne!(first.seq, second.seq, "two events never share a sequence");
        let received = subscriber.recv().await.expect("the event is published");
        assert_eq!(received.seq, first.seq);

        let since = events
            .since(&fixture.context, &fixture.thread, first.seq)
            .await
            .expect("the store answers");
        assert_eq!(since.len(), 1);
        assert_eq!(since[0].seq, second.seq);
    }

    #[tokio::test]
    async fn req_040_a_person_is_read_by_address_with_the_hash_of_their_password() {
        let database = MemoryDatabase::new();
        let accounts = database.accounts();
        let users = database.users();
        let email = Email::new("person@example.com").expect("a valid address");
        let user = user_id("usr_1");
        let space = space_id("spc_1");

        accounts
            .create_account(boma_app::ports::NewAccount {
                user: boma_app::ports::NewUser {
                    id: user.clone(),
                    email: email.clone(),
                    password_hash: "hash:secret".to_owned(),
                    name: "Ada".to_owned(),
                    locale: "en-US".to_owned(),
                    created_at: "2026-01-01T00:00:00Z".to_owned(),
                },
                organization: boma_app::ports::NewOrganization {
                    id: organization_id("org_1"),
                    name: "Ada's organization".to_owned(),
                    created_at: "2026-01-01T00:00:00Z".to_owned(),
                },
                space: NewSpace {
                    id: space.clone(),
                    organization_id: organization_id("org_1"),
                    name: "Ada's space".to_owned(),
                    is_default: true,
                    created_at: "2026-01-01T00:00:00Z".to_owned(),
                },
            })
            .await
            .expect("the account is created");

        let found = users
            .find_by_email(&email)
            .await
            .expect("the store answers")
            .expect("the person is found");
        assert_eq!(found.0.id, user);
        assert_eq!(found.1, "hash:secret");

        let default = database
            .spaces()
            .default_for(&user)
            .await
            .expect("the store answers")
            .expect("the person lands in a space");
        assert_eq!(default.id, space);
    }

    #[tokio::test]
    async fn req_042_the_default_stores_share_their_rows() {
        let accounts = MemoryAccountStore::default();
        let users = MemoryUserStore::default();
        let email = Email::new("shared@example.com").expect("a valid address");
        let user = user_id("usr_shared");
        let space = space_id("spc_shared");

        accounts
            .create_account(boma_app::ports::NewAccount {
                user: boma_app::ports::NewUser {
                    id: user.clone(),
                    email: email.clone(),
                    password_hash: "hash:secret".to_owned(),
                    name: "Shared".to_owned(),
                    locale: "en-US".to_owned(),
                    created_at: "2026-01-01T00:00:00Z".to_owned(),
                },
                organization: boma_app::ports::NewOrganization {
                    id: organization_id("org_shared"),
                    name: "Shared's organization".to_owned(),
                    created_at: "2026-01-01T00:00:00Z".to_owned(),
                },
                space: NewSpace {
                    id: space,
                    organization_id: organization_id("org_shared"),
                    name: "Shared's space".to_owned(),
                    is_default: true,
                    created_at: "2026-01-01T00:00:00Z".to_owned(),
                },
            })
            .await
            .expect("the account is created");

        let found = users
            .find_by_email(&email)
            .await
            .expect("the store answers")
            .expect("the person is found");
        assert_eq!(found.0.id, user);
    }

    #[tokio::test]
    async fn req_002_two_runs_with_the_same_nonce_in_two_threads_are_two_runs() {
        let fixture = fixture().await;
        let first = fixture
            .runs
            .create(new_run(&fixture, "run_1", Some("nonce-1")))
            .await
            .expect("the run is created");
        let second = fixture
            .runs
            .create(new_run(&fixture, "run_2", Some("nonce-2")))
            .await
            .expect("the run is created");
        assert_ne!(first.id, second.id, "a nonce is unique per thread");
    }

    #[tokio::test]
    async fn req_011_the_activity_page_shows_the_prompt_and_pages_with_a_cursor() {
        let fixture = fixture().await;
        let message = fixture
            .messages
            .insert(
                &fixture.context,
                new_message(&fixture, "msg_1", None, "plan my week"),
            )
            .await
            .expect("the message is stored");
        let mut run = new_run(&fixture, "run_1", None);
        run.source_message_id = Some(message.message.id.clone());
        fixture.runs.create(run).await.expect("the run is created");

        let page = fixture
            .runs
            .list(&fixture.context, None, None, 10)
            .await
            .expect("the store answers");
        assert_eq!(page.runs.len(), 1);
        assert_eq!(page.runs[0].prompt_snippet.as_deref(), Some("plan my week"));
        assert!(page.next_cursor.is_none(), "the last page has no cursor");
    }

    #[tokio::test]
    async fn req_005_the_events_of_another_tenant_are_not_found() {
        let fixture = fixture().await;
        let publisher = Arc::new(MemoryEventPublisher::new());
        let events = MemoryEventStore::new(&fixture.database, publisher.clone());
        let mut subscriber: broadcast::Receiver<boma_domain::Event> =
            publisher.subscribe(&fixture.thread);

        let other_space = space_id("spc_2");
        let refused = events
            .append(
                &fixture.context,
                NewEvent {
                    space_id: other_space,
                    thread_id: fixture.thread.clone(),
                    kind: EventType::Message,
                    payload: serde_json::json!({}),
                    created_at: "2026-01-01T00:00:00Z".to_owned(),
                },
            )
            .await;
        assert!(refused.is_err(), "a write into another tenant is refused");
        assert!(
            subscriber.try_recv().is_err(),
            "a refused event is not published"
        );
    }
}

/// The computers of each bot, in memory.
#[derive(Debug, Clone)]
pub struct MemoryComputerStore {
    state: Arc<RwLock<State>>,
}

impl MemoryComputerStore {
    /// Builds the store on the shared rows.
    #[must_use]
    pub fn new(database: &MemoryDatabase) -> Self {
        Self {
            state: Arc::clone(&database.state),
        }
    }
}

impl Default for MemoryComputerStore {
    fn default() -> Self {
        Self::new(default_database())
    }
}

#[async_trait::async_trait]
impl boma_app::ports::ComputerStore for MemoryComputerStore {
    async fn find(
        &self,
        context: &TenantContext,
        bot_id: &boma_domain::BotId,
    ) -> Result<Option<boma_domain::Computer>, boma_app::AppError> {
        let state = read(&self.state);
        let row = state
            .computers
            .iter()
            .find(|row| row.computer.bot_id == *bot_id);
        Ok(row
            .filter(|row| visible(context, &row.computer.space_id))
            .map(|row| row.computer.clone()))
    }

    async fn find_by_id(
        &self,
        context: &TenantContext,
        id: &boma_domain::ComputerId,
    ) -> Result<Option<boma_domain::Computer>, boma_app::AppError> {
        let state = read(&self.state);
        let row = state.computers.iter().find(|row| row.computer.id == *id);
        Ok(row
            .filter(|row| visible(context, &row.computer.space_id))
            .map(|row| row.computer.clone()))
    }

    async fn create(
        &self,
        context: &TenantContext,
        computer: boma_app::ports::NewComputer,
    ) -> Result<boma_domain::Computer, boma_app::AppError> {
        if !visible(context, &computer.space_id) {
            return Err(boma_app::AppError::NotFound);
        }
        let stored = boma_domain::Computer {
            id: computer.id,
            space_id: computer.space_id,
            bot_id: computer.bot_id,
            provider: computer.provider,
            external_id: None,
            status: boma_domain::ComputerStatus::Stopped,
            screen_generation: 0,
            home_id: None,
            private: computer.private,
            memory_limit_mb: computer.memory_limit_mb,
            cpu_limit: computer.cpu_limit,
            idle_minutes: computer.idle_minutes,
            started_at: None,
            last_used_at: None,
            stopped_at: None,
            created_at: computer.created_at,
            updated_at: computer.updated_at,
        };
        let mut state = write(&self.state);
        if state
            .computers
            .iter()
            .any(|row| row.computer.id == stored.id)
        {
            return Err(boma_app::AppError::Conflict);
        }
        state.computers.push(ComputerRow {
            computer: stored.clone(),
        });
        Ok(stored)
    }

    async fn update(
        &self,
        context: &TenantContext,
        computer: &boma_domain::Computer,
    ) -> Result<(), boma_app::AppError> {
        let mut state = write(&self.state);
        let Some(row) = state
            .computers
            .iter_mut()
            .find(|row| row.computer.id == computer.id)
        else {
            return Err(boma_app::AppError::NotFound);
        };
        if !visible(context, &row.computer.space_id) {
            return Err(boma_app::AppError::NotFound);
        }
        row.computer = computer.clone();
        Ok(())
    }

    async fn running_idle(
        &self,
        as_of: &str,
    ) -> Result<Vec<boma_domain::Computer>, boma_app::AppError> {
        let state = read(&self.state);
        let found = state
            .computers
            .iter()
            .map(|row| &row.computer)
            .filter(|computer| computer.status == boma_domain::ComputerStatus::Running)
            .filter_map(|computer| {
                let last_used = computer.last_used_at.as_deref().and_then(parse_rfc3339)?;
                let now = parse_rfc3339(as_of)?;
                let idle = now - last_used;
                (idle >= time::Duration::minutes(computer.idle_minutes.cast_signed()))
                    .then(|| computer.clone())
            })
            .collect();
        Ok(found)
    }
}

/// Parses the RFC3339 instants the clock writes.
fn parse_rfc3339(instant: &str) -> Option<time::OffsetDateTime> {
    use time::format_description::well_known::Rfc3339;

    time::OffsetDateTime::parse(instant, &Rfc3339).ok()
}

/// The screen capabilities of the deployment, in memory.
#[derive(Debug, Clone)]
pub struct MemoryScreenCapabilityStore {
    state: Arc<RwLock<State>>,
}

impl MemoryScreenCapabilityStore {
    /// Builds the store on the shared rows.
    #[must_use]
    pub fn new(database: &MemoryDatabase) -> Self {
        Self {
            state: Arc::clone(&database.state),
        }
    }
}

impl Default for MemoryScreenCapabilityStore {
    fn default() -> Self {
        Self::new(default_database())
    }
}

#[async_trait::async_trait]
impl boma_app::ports::ScreenCapabilityStore for MemoryScreenCapabilityStore {
    async fn issue(
        &self,
        context: &TenantContext,
        capability: boma_app::ports::NewScreenCapability,
    ) -> Result<(), boma_app::AppError> {
        if !visible(context, &capability.space_id) {
            return Err(boma_app::AppError::NotFound);
        }
        let mut state = write(&self.state);
        state.screen_capabilities.push(ScreenCapabilityRow {
            computer_id: capability.computer_id,
            space_id: capability.space_id,
            token_hash: capability.token_hash,
            screen_generation: capability.screen_generation,
        });
        Ok(())
    }

    async fn find_by_hash(
        &self,
        token_hash: &[u8],
    ) -> Result<Option<boma_app::ports::ScreenCapability>, boma_app::AppError> {
        let state = read(&self.state);
        Ok(state
            .screen_capabilities
            .iter()
            .find(|row| row.token_hash == token_hash)
            .map(|row| boma_app::ports::ScreenCapability {
                computer_id: row.computer_id.clone(),
                space_id: row.space_id.clone(),
                screen_generation: row.screen_generation,
            }))
    }
}
