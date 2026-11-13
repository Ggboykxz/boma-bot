//! The application's composition root and its shared services.
//!
//! One object owns every port the use cases need. The binary builds it from concrete adapters;
//! a test builds it from fakes. Nothing else in the workspace constructs one.

use std::sync::Arc;

use boma_domain::{Bot, Message, Run, Thread, User};

use crate::{
    error::AppError,
    ports::{
        AccountStore, BotStore, CardStore, Clock, ComputerStore, EffectStore, EventPublisher,
        EventStore, IdGenerator, InvitationStore, MemoryStore, MessageStore, PasswordHasher,
        PendingToolCallStore, RateLimitStore, ReactionStore, ReadStateStore, RosterStore,
        RoutineStore, RunStore, ScreenCapabilityStore, SessionStore, SpaceStore, ThreadStore,
        TokenGenerator, ToolStore, UserStore,
    },
};

/// The stores: the durable side of the product.
#[derive(Clone)]
pub struct Stores {
    /// Accounts, organizations and their first space.
    pub accounts: Arc<dyn AccountStore>,
    /// People.
    pub users: Arc<dyn UserStore>,
    /// Sessions.
    pub sessions: Arc<dyn SessionStore>,
    /// Spaces and memberships.
    pub spaces: Arc<dyn SpaceStore>,
    /// Bots.
    pub bots: Arc<dyn BotStore>,
    /// Threads.
    pub threads: Arc<dyn ThreadStore>,
    /// Messages.
    pub messages: Arc<dyn MessageStore>,
    /// Runs.
    pub runs: Arc<dyn RunStore>,
    /// Events.
    pub events: Arc<dyn EventStore>,
    /// Invitations.
    pub invitations: Arc<dyn InvitationStore>,
    /// Approval and question cards.
    pub cards: Arc<dyn CardStore>,
    /// Tool calls waiting on an approval (REQ-017).
    pub pending_tool_calls: Arc<dyn PendingToolCallStore>,
    /// Shared rate-limit counters (D-01).
    pub rate_limits: Arc<dyn RateLimitStore>,
    /// Side effects a run performs, exactly once per effect key.
    pub effects: Arc<dyn EffectStore>,
    /// The per-run tool catalogue.
    pub tools: Arc<dyn ToolStore>,
    /// Emoji reactions on messages (REQ-008).
    pub reactions: Arc<dyn ReactionStore>,
    /// Per-member read positions (REQ-009).
    pub read_states: Arc<dyn ReadStateStore>,
    /// The roster the rail renders from (REQ-009).
    pub roster: Arc<dyn RosterStore>,
    /// The memory documents of bots and of the space (REQ-022).
    pub memory: Arc<dyn MemoryStore>,
    /// The routines of bots (REQ-023, REQ-024, REQ-025).
    pub routines: Arc<dyn RoutineStore>,
    /// The computers of bots (REQ-026).
    pub computers: Arc<dyn ComputerStore>,
    /// The screen capabilities of bots' computers (REQ-030).
    pub screen_capabilities: Arc<dyn ScreenCapabilityStore>,
}

impl Stores {
    /// Builds the stores from already-constructed adapters.
    ///
    /// The list is long because the product has many stores; a deployment wires each one to its
    /// adapter exactly once, at startup, and nowhere else.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        accounts: Arc<dyn AccountStore>,
        users: Arc<dyn UserStore>,
        sessions: Arc<dyn SessionStore>,
        spaces: Arc<dyn SpaceStore>,
        bots: Arc<dyn BotStore>,
        threads: Arc<dyn ThreadStore>,
        messages: Arc<dyn MessageStore>,
        runs: Arc<dyn RunStore>,
        events: Arc<dyn EventStore>,
        invitations: Arc<dyn InvitationStore>,
        cards: Arc<dyn CardStore>,
        pending_tool_calls: Arc<dyn PendingToolCallStore>,
        effects: Arc<dyn EffectStore>,
        tools: Arc<dyn ToolStore>,
        rate_limits: Arc<dyn RateLimitStore>,
        reactions: Arc<dyn ReactionStore>,
        read_states: Arc<dyn ReadStateStore>,
        roster: Arc<dyn RosterStore>,
        memory: Arc<dyn MemoryStore>,
        routines: Arc<dyn RoutineStore>,
        computers: Arc<dyn ComputerStore>,
        screen_capabilities: Arc<dyn ScreenCapabilityStore>,
    ) -> Self {
        Self {
            accounts,
            users,
            sessions,
            spaces,
            bots,
            threads,
            messages,
            runs,
            events,
            invitations,
            cards,
            pending_tool_calls,
            rate_limits,
            effects,
            tools,
            reactions,
            read_states,
            roster,
            memory,
            routines,
            computers,
            screen_capabilities,
        }
    }
}

/// The services: everything that is not a store.
#[derive(Clone)]
pub struct Services {
    /// Hands events to the live subscribers.
    pub publisher: Arc<dyn EventPublisher>,
    /// Hashes and checks passwords.
    pub hasher: Arc<dyn PasswordHasher>,
    /// Mints the opaque tokens the product hands out.
    pub tokens: Arc<dyn TokenGenerator>,
    /// Mints the opaque ids the product hands out.
    pub ids: Arc<dyn IdGenerator>,
    /// Tells the time.
    pub clock: Arc<dyn Clock>,
    /// The models this deployment can reach, and the fallback it executes on.
    pub models: boma_domain::model::DeploymentModels,
    /// The computer backend, when this deployment is configured with one (REQ-027).
    pub computer: Option<Arc<dyn crate::ports::ComputerRuntime>>,
}

impl Services {
    /// Builds the services from already-constructed adapters.
    #[must_use]
    pub fn new(
        publisher: Arc<dyn EventPublisher>,
        hasher: Arc<dyn PasswordHasher>,
        tokens: Arc<dyn TokenGenerator>,
        ids: Arc<dyn IdGenerator>,
        clock: Arc<dyn Clock>,
        models: boma_domain::model::DeploymentModels,
    ) -> Self {
        Self {
            publisher,
            hasher,
            tokens,
            ids,
            clock,
            models,
            computer: None,
        }
    }

    /// Builds the services with a computer backend attached.
    ///
    /// A deployment that is not configured with one leaves it `None` and every computer answer
    /// stays a clear "computers unavailable" (REQ-027).
    #[must_use]
    pub fn with_computers(mut self, computer: Arc<dyn crate::ports::ComputerRuntime>) -> Self {
        self.computer = Some(computer);
        self
    }
}

/// The application: the use cases, with their ports.
#[derive(Clone)]
pub struct App {
    /// The durable side.
    pub stores: Stores,
    /// Everything else.
    pub services: Services,
}

impl App {
    /// Builds the application.
    #[must_use]
    pub fn new(stores: Stores, services: Services) -> Self {
        Self { stores, services }
    }
}

/// The identity a request carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Actor {
    /// The signed-in person.
    pub user: User,
    /// The tenant they act in.
    pub context: boma_domain::TenantContext,
}

/// The result of a send: what the client needs to follow the turn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SendOutcome {
    /// The message that was written.
    pub message: Message,
    /// The run that was started for it.
    pub run: Run,
}

/// The bot and its conversation, as one pair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BotWithThread {
    /// The bot.
    pub bot: Bot,
    /// Its conversation.
    pub thread: Thread,
}

/// A short, safe description of a failure, for logs and audit rows.
///
/// The message never contains personal data: it names the rule, not the value that broke it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    /// A stable, machine-readable reason.
    pub reason: &'static str,
    /// A short explanation, safe to log.
    pub detail: String,
}

impl Failure {
    /// Builds a failure.
    #[must_use]
    pub fn new(reason: &'static str, detail: impl Into<String>) -> Self {
        Self {
            reason,
            detail: detail.into(),
        }
    }
}

impl From<AppError> for Failure {
    fn from(error: AppError) -> Self {
        let reason = match &error {
            AppError::Rule(_) => "rule_violation",
            AppError::Conflict => "conflict",
            AppError::ScreenTaken => "screen_taken",
            AppError::InvalidCredentials => "invalid_credentials",
            AppError::Unavailable => "dependency_unavailable",
            AppError::NotFound => "not_found",
        };
        Self {
            reason,
            detail: error.to_string(),
        }
    }
}
