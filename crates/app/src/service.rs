//! The application's composition root and its shared services.
//!
//! One object owns every port the use cases need. The binary builds it from concrete adapters;
//! a test builds it from fakes. Nothing else in the workspace constructs one.

use std::sync::Arc;

use boma_domain::{Bot, Message, Run, Thread, User};

use crate::{
    error::AppError,
    ports::{
        AccountStore, ArtifactStore, AuditStore, BillingStore, BotStore, CardStore, Clock,
        ComputerStore, ConnectorStore, EffectStore, EventPublisher, EventStore, IdGenerator,
        InvitationStore, MailStore, McpServerStore, MemoryStore, MessageStore, MessagingStore,
        ModelStore, PageStore, PasswordHasher, PendingToolCallStore, PluginInstallStore,
        PluginSubmissionStore, RateLimitStore, ReactionStore, ReadStateStore, RosterStore,
        RoutineStore, RunStore, ScreenCapabilityStore, SecretStore, SessionStore, SkillStore,
        SpaceStore, ThreadStore, TicketStore, TokenGenerator, ToolStore, UserStore,
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
    /// The bot secrets vault (REQ-046).
    pub secrets: Arc<dyn SecretStore>,
    /// Anonymous plugin submissions awaiting review (REQ-034).
    pub plugin_submissions: Arc<dyn PluginSubmissionStore>,
    /// Taught and plugin skills of the space (REQ-035).
    pub skills: Arc<dyn SkillStore>,
    /// The MCP servers a space connects to (REQ-033).
    pub mcp_servers: Arc<dyn McpServerStore>,
    /// The audit trail of the escape hatches (defect D-05).
    pub audit: Arc<dyn AuditStore>,
    /// The connector installs of the space (REQ-032).
    pub connectors: Arc<dyn ConnectorStore>,
    /// The plugin installs of the space (REQ-034).
    pub plugin_installs: Arc<dyn PluginInstallStore>,
    /// The artifact rows of the space (REQ-036).
    pub artifacts: Arc<dyn ArtifactStore>,
    /// The pages of the space and their immutable versions (REQ-037).
    pub pages: Arc<dyn PageStore>,
    /// The inbound-mail rules and held messages (REQ-047).
    pub mail: Arc<dyn MailStore>,
    /// The messaging installations and their external threads (REQ-048).
    pub messaging: Arc<dyn MessagingStore>,
    /// The model credentials and the person's preference (REQ-052).
    pub models: Arc<dyn ModelStore>,
    /// The spending limit, the payments and the subscription (REQ-018, REQ-051).
    pub billing: Arc<dyn BillingStore>,
    /// The ruled parts of one account export (REQ-044).
    pub account_export: Arc<dyn crate::ports::AccountExportStore>,
    /// The admin surfaces: members, invitations, grants, plugin review (REQ-058).
    pub admin: Arc<dyn crate::ports::AdminStore>,
    /// The password recovery tokens (REQ-040).
    pub password_resets: Arc<dyn crate::ports::PasswordResetStore>,
    /// The tickets of the board (REQ-038). Only meaningful when the deployment enables the board.
    pub tickets: Arc<dyn TicketStore>,
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
        secrets: Arc<dyn SecretStore>,
        plugin_submissions: Arc<dyn PluginSubmissionStore>,
        skills: Arc<dyn SkillStore>,
        mcp_servers: Arc<dyn McpServerStore>,
        audit: Arc<dyn AuditStore>,
        connectors: Arc<dyn ConnectorStore>,
        plugin_installs: Arc<dyn PluginInstallStore>,
        artifacts: Arc<dyn ArtifactStore>,
        pages: Arc<dyn PageStore>,
        tickets: Arc<dyn TicketStore>,
        mail: Arc<dyn MailStore>,
        messaging: Arc<dyn MessagingStore>,
        reactions: Arc<dyn ReactionStore>,
        read_states: Arc<dyn ReadStateStore>,
        roster: Arc<dyn RosterStore>,
        memory: Arc<dyn MemoryStore>,
        routines: Arc<dyn RoutineStore>,
        computers: Arc<dyn ComputerStore>,
        screen_capabilities: Arc<dyn ScreenCapabilityStore>,
        models: Arc<dyn ModelStore>,
        billing: Arc<dyn BillingStore>,
        account_export: Arc<dyn crate::ports::AccountExportStore>,
        admin: Arc<dyn crate::ports::AdminStore>,
        password_resets: Arc<dyn crate::ports::PasswordResetStore>,
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
            secrets,
            plugin_submissions,
            skills,
            mcp_servers,
            audit,
            connectors,
            plugin_installs,
            artifacts,
            pages,
            mail,
            messaging,
            models,
            billing,
            account_export,
            admin,
            password_resets,
            tickets,
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
// The settings one deployment carries, as bools, intentionally: each flag is one operator
// knob, not a state machine. The spec treats flags as first-class (board, messaging, billing).
#[allow(clippy::struct_excessive_bools)]
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
    /// The transport that calls MCP servers (REQ-033); a test doubles it, a deployment wires
    /// the dispatching transport (HTTP, plus stdio behind the allow-list).
    pub mcp_transport: Arc<dyn crate::ports::McpTransport>,
    /// The deployment's stdio allow-list (`MCP_STDIO_ALLOWED_COMMANDS`): the bare program
    /// names this deployment permits for local MCP servers. Empty by default.
    pub stdio_allowed: Vec<String>,
    /// Whether the ticket board is enabled (`BOMA_BOARD`). Off by default: without the flag the
    /// board surface is absent — every board procedure answers 404, signed in or not (REQ-038).
    pub board_enabled: bool,
    /// The inbound-mail settings (`BOMA_EMAIL_RELAY_TOKEN` + `BOMA_EMAIL_DOMAIN`). `None` means
    /// the relay route does not exist (REQ-047).
    pub email_inbound: Option<crate::ports::EmailInboundSettings>,
    /// The runner for connector tools (REQ-032), when this deployment is configured to
    /// execute them. `None` means connector tools are never dispatched, and a call answers with
    /// a clear refusal rather than a fake success.
    pub connector_runner: Option<Arc<dyn crate::ports::ConnectorRunner>>,
    /// The fetcher that resolves plugin sources (REQ-034). A test doubles it; a deployment
    /// wires the GitHub fetcher.
    pub plugin_fetcher: Arc<dyn crate::ports::PluginFetcher>,
    /// The OAuth client for MCP servers (REQ-033); a test doubles it, a deployment wires the
    /// form-encoded token-endpoint client.
    pub oauth_client: Arc<dyn crate::ports::OAuthClient>,
    /// The messaging transport (REQ-048): a test doubles it, a deployment wires the HTTP
    /// client. The flag gates the surface, not the transport.
    pub messaging_transport: Arc<dyn crate::ports::MessagingTransport>,
    /// The Slack app credentials (REQ-048). `None` means Slack is not configured for this
    /// deployment: installs refuse with a clear error, and inbound Slack deliveries cannot
    /// verify.
    pub slack: Option<crate::ports::SlackAppConfig>,
    /// Whether the messaging surface exists (`BOMA_MESSAGING`). Off by default: without the
    /// flag the messaging routes and procedures answer 404, signed in or not.
    pub messaging_enabled: bool,
    /// Whether the SCIM sync surface exists (`BOMA_SCIM`), and the bearer an enterprise
    /// directory presents on it. `None` means the routes are uniformly absent (REQ-045).
    pub scim_token: Option<secrecy::SecretString>,
    /// The counters the process keeps; exposed at `/metrics` when the deployment enables it.
    pub metrics: Arc<crate::metrics::Metrics>,
    /// Whether `GET /metrics` answers (BOMA_METRICS). Off by default; the endpoint is absent,
    /// not degraded, when the flag is absent (REQ-054).
    pub metrics_enabled: bool,
    /// Whether the one-time recovery link is written to the process output (BOMA_AUTH_PRINT_RESET_LINK).
    ///
    /// The default delivery channel of this deployment: it has no outbound mail, so the operator's
    /// process output is the mailbox this deployment has. Off by default; only an operator who wants
    /// the link on stdout turns it on, and the line carries the account id and the link, never the
    /// address.
    pub print_recovery_link: bool,
    /// The public base URL this deployment is reached at, for webhooks members must paste into
    /// a platform (REQ-048). A loopback default for development; set it in every deployment
    /// that installs a messaging platform.
    pub public_base_url: String,
    /// The out-of-band artifact bytes (REQ-036). Content-addressed; a test doubles it with a
    /// RAM map, a deployment wires the filesystem store.
    pub artifact_content: Arc<dyn crate::ports::ArtifactContentStore>,
    /// The model credential probe (REQ-052): a test doubles it, a deployment wires the HTTP
    /// client.
    pub model_transport: Arc<dyn crate::ports::ModelTransport>,
    /// The checkout provider (REQ-051); `None` means checkouts refuse with a clear error.
    pub billing_provider: Option<Arc<dyn crate::ports::BillingProvider>>,
    /// The billing webhook secret (`BOMA_BILLING_WEBHOOK_SECRET`); without it the webhook
    /// cannot verify anything and every call is uniform 401.
    pub billing_webhook_secret: Option<secrecy::SecretString>,
    /// Whether the billing surface exists (`BOMA_BILLING`). Off by default: without the flag
    /// the billing routes and procedures answer 404, signed in or not.
    pub billing_enabled: bool,
}

impl Services {
    /// Builds the services from already-constructed adapters.
    #[must_use]
    #[allow(clippy::too_many_arguments)] // the composition root; the list is the contract
    pub fn new(
        publisher: Arc<dyn EventPublisher>,
        hasher: Arc<dyn PasswordHasher>,
        tokens: Arc<dyn TokenGenerator>,
        ids: Arc<dyn IdGenerator>,
        clock: Arc<dyn Clock>,
        models: boma_domain::model::DeploymentModels,
        mcp_transport: Arc<dyn crate::ports::McpTransport>,
        plugin_fetcher: Arc<dyn crate::ports::PluginFetcher>,
        oauth_client: Arc<dyn crate::ports::OAuthClient>,
        artifact_content: Arc<dyn crate::ports::ArtifactContentStore>,
        messaging_transport: Arc<dyn crate::ports::MessagingTransport>,
        model_transport: Arc<dyn crate::ports::ModelTransport>,
    ) -> Self {
        Self {
            publisher,
            hasher,
            tokens,
            ids,
            clock,
            models,
            computer: None,
            mcp_transport,
            stdio_allowed: Vec::new(),
            board_enabled: false,
            email_inbound: None,
            connector_runner: None,
            plugin_fetcher,
            oauth_client,
            artifact_content,
            messaging_transport,
            model_transport,
            billing_provider: None,
            billing_webhook_secret: None,
            billing_enabled: false,
            scim_token: None,
            metrics: Arc::new(crate::metrics::Metrics::new()),
            metrics_enabled: false,
            print_recovery_link: false,
            slack: None,
            messaging_enabled: false,
            public_base_url: "http://127.0.0.1:3000".to_owned(),
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

    /// Builds the services with the connector runner attached (REQ-032).
    ///
    /// A deployment that leaves it out never dispatches a connector tool.
    #[must_use]
    pub fn with_connectors(mut self, runner: Arc<dyn crate::ports::ConnectorRunner>) -> Self {
        self.connector_runner = Some(runner);
        self
    }

    /// Builds the services with the stdio allow-list attached (REQ-033).
    ///
    /// The list is the deployment's answer to "may a local MCP command run here"; it is empty
    /// by default, and the default is the safe one.
    #[must_use]
    pub fn with_stdio_commands(mut self, commands: Vec<String>) -> Self {
        self.stdio_allowed = commands;
        self
    }

    /// Builds the services with the board flag attached (REQ-038).
    #[must_use]
    pub fn with_board(mut self, enabled: bool) -> Self {
        self.board_enabled = enabled;
        self
    }

    /// Builds the services with the inbound-mail settings attached (REQ-047).
    #[must_use]
    pub fn with_email_inbound(
        mut self,
        settings: Option<crate::ports::EmailInboundSettings>,
    ) -> Self {
        self.email_inbound = settings;
        self
    }

    /// Builds the services with the SCIM sync surface enabled (REQ-045).
    #[must_use]
    pub fn with_scim(mut self, token: secrecy::SecretString) -> Self {
        self.scim_token = Some(token);
        self
    }

    /// Builds the services with the SCIM surface wired from deployment config, when present.
    #[must_use]
    pub fn with_scim_option(mut self, token: Option<String>) -> Self {
        self.scim_token = token.map(secrecy::SecretString::from);
        self
    }

    /// Builds the services with the recovery-link printing enabled (REQ-040).
    #[must_use]
    pub fn with_print_recovery_link(mut self, enabled: bool) -> Self {
        self.print_recovery_link = enabled;
        self
    }

    /// Builds the services with the metrics endpoint exposed (REQ-054).
    #[must_use]
    pub fn with_metrics(mut self, enabled: bool) -> Self {
        self.metrics_enabled = enabled;
        self
    }

    /// Builds the services with the messaging surface enabled or not (REQ-048).
    #[must_use]
    pub fn with_messaging(mut self, enabled: bool) -> Self {
        self.messaging_enabled = enabled;
        self
    }

    /// Builds the services with the Slack app credentials attached (REQ-048).
    #[must_use]
    pub fn with_slack(mut self, slack: Option<crate::ports::SlackAppConfig>) -> Self {
        self.slack = slack;
        self
    }

    /// Builds the services with the billing surface and its settings attached (REQ-051).
    #[must_use]
    pub fn with_billing(
        mut self,
        enabled: bool,
        provider: Option<Arc<dyn crate::ports::BillingProvider>>,
        webhook_secret: Option<secrecy::SecretString>,
    ) -> Self {
        self.billing_enabled = enabled;
        self.billing_provider = provider;
        self.billing_webhook_secret = webhook_secret;
        self
    }

    /// Builds the services with the public base URL attached (REQ-048).
    #[must_use]
    pub fn with_public_base_url(mut self, url: String) -> Self {
        self.public_base_url = url;
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
            AppError::StdioDisabled => "stdio_disabled",
            AppError::RateLimited => "rate_limited",
            AppError::Unavailable => "dependency_unavailable",
            AppError::NotFound => "not_found",
            AppError::Forbidden => "forbidden",
        };
        Self {
            reason,
            detail: error.to_string(),
        }
    }
}
