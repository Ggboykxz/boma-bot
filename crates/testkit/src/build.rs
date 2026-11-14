//! The builder: an application assembled from the in-memory adapters.
//!
//! [`TestApp`] is what a test holds: the application, the stores behind it and the scripted
//! services a test wants to look at. Building it takes one line, and the defaults are sensible:
//! deterministic ids, a fixed clock and deterministic tokens.
//!
//! ```no_run
//! # use boma_testkit::TestApp;
//! # async fn example() {
//! let test = TestApp::builder()
//!     .ids(vec!["usr_1".to_owned(), "org_1".to_owned(), "spc_1".to_owned()])
//!     .build();
//!
//! let user_id = match test.seed_user("person@example.com", "correct horse battery").await {
//!     Ok(user_id) => user_id,
//!     Err(_) => return,
//! };
//! assert_eq!(user_id.as_str(), "usr_1");
//! # }
//! ```

use std::sync::Arc;

use boma_app::{
    App, AppError,
    ports::{Clock, EventPublisher, IdGenerator, PasswordHasher, TokenGenerator},
    service::{Services, Stores},
};

use crate::ScriptedPluginFetcher;
use boma_domain::{BotId, Email, OrganizationId, SpaceId, TenantContext, ThreadId, UserId};

use crate::{
    memory::{MemoryDatabase, MemoryEventPublisher},
    scripts::{FixedHasher, ScriptedClock, ScriptedIds, ScriptedTokens},
};

/// The application a test drives, with the pieces the test wants to look at.
///
/// The application is a plain [`App`]: the use cases do not know that the stores are in memory.
#[derive(Clone)]
pub struct TestApp {
    /// The application.
    app: App,
    /// The rows every store shares.
    database: MemoryDatabase,
    /// The identifiers the application mints.
    ids: Arc<ScriptedIds>,
    /// The clock the application reads.
    clock: Arc<ScriptedClock>,
    /// The tokens the application mints.
    tokens: Arc<ScriptedTokens>,
    /// Where the events are published.
    publisher: Arc<MemoryEventPublisher>,
    /// The hasher the application uses.
    hasher: Arc<FixedHasher>,
}

impl TestApp {
    /// Starts the building of an application.
    #[must_use]
    pub fn builder() -> TestAppBuilder {
        TestAppBuilder::default()
    }

    /// The application, ready to be driven by the use cases.
    #[must_use]
    pub fn app(&self) -> &App {
        &self.app
    }

    /// The identifiers the application mints.
    #[must_use]
    pub fn ids(&self) -> &ScriptedIds {
        self.ids.as_ref()
    }

    /// The clock the application reads.
    #[must_use]
    pub fn clock(&self) -> &ScriptedClock {
        self.clock.as_ref()
    }

    /// The tokens the application mints.
    #[must_use]
    pub fn tokens(&self) -> &ScriptedTokens {
        self.tokens.as_ref()
    }

    /// Where the events are published.
    #[must_use]
    pub fn publisher(&self) -> &MemoryEventPublisher {
        self.publisher.as_ref()
    }

    /// The rows every store shares.
    #[must_use]
    pub fn database(&self) -> &MemoryDatabase {
        &self.database
    }

    /// The tenant a member of a space acts in.
    #[must_use]
    pub fn member_context(space_id: &SpaceId, user_id: &UserId) -> TenantContext {
        TenantContext::Member {
            space_id: space_id.clone(),
            user_id: user_id.clone(),
        }
    }

    /// Creates a person with a known address and password, and returns their identifier.
    ///
    /// The account is created the way the product creates it: the person, their organization,
    /// their first space and their membership in it (REQ-042). The password is hashed with the
    /// testkit's hasher, so the test can sign in with the password it wrote.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Rule`] when the address is refused, and [`AppError::Conflict`] when
    /// the address is already registered.
    pub async fn seed_user(&self, email: &str, password: &str) -> Result<UserId, AppError> {
        let email = Email::new(email.to_owned())?;
        let password_hash = self.hasher.hash(password).await?;
        let user_id = UserId::new(self.ids.new_id("usr"))?;
        let organization_id = OrganizationId::new(self.ids.new_id("org"))?;
        let space_id = SpaceId::new(self.ids.new_id("spc"))?;
        let now = self.clock.now();
        let name = name_of(&email);

        let account = self
            .app
            .stores
            .accounts
            .create_account(boma_app::ports::NewAccount {
                user: boma_app::ports::NewUser {
                    id: user_id.clone(),
                    email: email.clone(),
                    password_hash,
                    name: name.clone(),
                    locale: "en-US".to_owned(),
                    created_at: now.clone(),
                },
                organization: boma_app::ports::NewOrganization {
                    id: organization_id.clone(),
                    name: format!("{name}'s organization"),
                    created_at: now.clone(),
                },
                space: boma_app::ports::NewSpace {
                    id: space_id,
                    organization_id,
                    name: format!("{name}'s space"),
                    is_default: true,
                    created_at: now.clone(),
                },
            })
            .await?;
        Ok(account.user.id)
    }

    /// Creates a space a person owns, and returns its identifier.
    ///
    /// The space joins the organization the person already belongs to, or a new one when the
    /// person belongs to none.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Unavailable`] when the store cannot answer.
    pub async fn seed_space(&self, user_id: &UserId, name: &str) -> Result<SpaceId, AppError> {
        let organization_id = if let Some(organization_id) = self.database.organization_for(user_id)
        {
            organization_id
        } else {
            // The person belongs to no space: the space needs an organization of its own.
            let organization_id = OrganizationId::new(self.ids.new_id("org"))?;
            let _ = self.database.create_organization(
                &organization_id,
                &format!("{name}'s organization"),
                &self.clock.now(),
            );
            organization_id
        };
        let space_id = SpaceId::new(self.ids.new_id("spc"))?;
        let now = self.clock.now();
        let space = self
            .app
            .stores
            .spaces
            .create(
                boma_app::ports::NewSpace {
                    id: space_id.clone(),
                    organization_id,
                    name: name.to_owned(),
                    is_default: false,
                    created_at: now,
                },
                user_id,
            )
            .await?;
        Ok(space.id)
    }

    /// Creates a bot and its conversation, and returns their identifiers.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Conflict`] when the bot already exists, and [`AppError::Unavailable`]
    /// when the store cannot answer.
    pub async fn seed_bot(
        &self,
        space_id: &SpaceId,
        user_id: &UserId,
        name: &str,
    ) -> Result<(BotId, ThreadId), AppError> {
        let bot_id = BotId::new(self.ids.new_id("bot"))?;
        let thread_id = ThreadId::new(self.ids.new_id("thr"))?;
        let now = self.clock.now();
        let (bot, thread) = self
            .app
            .stores
            .bots
            .create_with_thread(
                boma_app::ports::NewBot {
                    id: bot_id.clone(),
                    space_id: space_id.clone(),
                    user_id: user_id.clone(),
                    name: name.to_owned(),
                    description: None,
                    color: None,
                    created_at: now.clone(),
                },
                boma_app::ports::NewThread {
                    id: thread_id,
                    space_id: space_id.clone(),
                    bot_id: bot_id.clone(),
                    user_id: user_id.clone(),
                    created_at: now,
                },
            )
            .await?;
        Ok((bot.id, thread.id))
    }
}

/// The name a seeded person gets: the local part of their address.
fn name_of(email: &Email) -> String {
    email
        .as_str()
        .split('@')
        .next()
        .filter(|part| !part.is_empty())
        .unwrap_or("Person")
        .to_owned()
}

/// The builder of a [`TestApp`].
///
/// Every script is optional: an application built without one still works, with deterministic
/// fallbacks.
#[derive(Clone)]
pub struct TestAppBuilder {
    /// The identifiers the application mints, in order.
    ids: Vec<String>,
    /// The instants the clock hands out, in order.
    instants: Vec<String>,
    /// The tokens the application mints, in order.
    tokens: Vec<String>,
    /// The instant the clock falls back to.
    fixed_instant: Option<String>,
    /// Whether the app builds with a computer backend (REQ-027).
    computers: bool,
    /// The answers the computer runtime hands out, when one is scripted.
    computer_answers: Vec<crate::ComputerAnswer>,
    /// The computer runtime to use instead of a fresh scripted one.
    runtime: Option<Arc<crate::ScriptedComputerRuntime>>,
    /// The MCP transport to use instead of a fresh scripted one. Not `Debug`: a
    /// transport's internals are the server's business.
    mcp_transport: Option<Arc<dyn boma_app::ports::McpTransport>>,
    /// The connector runner to use instead of a fresh scripted one.
    connector_runner: Option<Arc<dyn boma_app::ports::ConnectorRunner>>,
    /// The plugin fetcher to use instead of a fresh scripted one.
    plugin_fetcher: Option<Arc<dyn boma_app::ports::PluginFetcher>>,
    /// The OAuth client to use instead of a fresh scripted one.
    oauth_client: Option<Arc<dyn boma_app::ports::OAuthClient>>,
    /// The content store to use instead of the shared in-RAM one.
    artifact_content: Option<Arc<dyn boma_app::ports::ArtifactContentStore>>,
}

impl Default for TestAppBuilder {
    fn default() -> Self {
        Self {
            ids: Vec::new(),
            instants: Vec::new(),
            tokens: Vec::new(),
            fixed_instant: None,
            computers: true,
            computer_answers: Vec::new(),
            runtime: None,
            mcp_transport: None,
            connector_runner: None,
            plugin_fetcher: None,
            oauth_client: None,
            artifact_content: None,
        }
    }
}

impl TestAppBuilder {
    /// The identifiers the application mints, in order.
    #[must_use]
    pub fn ids(mut self, ids: Vec<String>) -> Self {
        self.ids = ids;
        self
    }

    /// The instants the clock hands out, in order.
    #[must_use]
    pub fn clock(mut self, instants: Vec<String>) -> Self {
        self.instants = instants;
        self
    }

    /// Builds the app without a computer backend: every computer answer is unavailable.
    #[must_use]
    pub fn without_computers(mut self) -> Self {
        self.computers = false;
        self
    }

    /// Scripts the answers the computer backend hands out, in order (REQ-026).
    #[must_use]
    pub fn computer_answers(mut self, answers: Vec<crate::ComputerAnswer>) -> Self {
        self.computers = true;
        self.computer_answers = answers;
        self
    }

    /// Builds the application with this runtime instead of a fresh scripted one.
    #[must_use]
    pub fn computers_runtime(mut self, runtime: Arc<crate::ScriptedComputerRuntime>) -> Self {
        self.computers = true;
        self.runtime = Some(runtime);
        self
    }

    /// Builds the application with this MCP transport instead of a fresh scripted one.
    #[must_use]
    pub fn mcp_transport(mut self, transport: Arc<dyn boma_app::ports::McpTransport>) -> Self {
        self.mcp_transport = Some(transport);
        self
    }

    /// Builds the application with this connector runner instead of a fresh scripted one.
    #[must_use]
    pub fn connector_runner(mut self, runner: Arc<dyn boma_app::ports::ConnectorRunner>) -> Self {
        self.connector_runner = Some(runner);
        self
    }

    /// Builds the application with this plugin fetcher instead of a fresh scripted one.
    #[must_use]
    pub fn plugin_fetcher(mut self, fetcher: Arc<dyn boma_app::ports::PluginFetcher>) -> Self {
        self.plugin_fetcher = Some(fetcher);
        self
    }

    /// Builds the application with this OAuth client instead of a fresh scripted one.
    #[must_use]
    pub fn oauth_client(mut self, client: Arc<dyn boma_app::ports::OAuthClient>) -> Self {
        self.oauth_client = Some(client);
        self
    }

    /// Builds the application with this content store instead of the shared in-RAM one.
    #[must_use]
    pub fn artifact_content(
        mut self,
        content: Arc<dyn boma_app::ports::ArtifactContentStore>,
    ) -> Self {
        self.artifact_content = Some(content);
        self
    }

    /// The instant the clock stands at when its script is exhausted.
    #[must_use]
    pub fn fixed_clock(mut self, instant: &str) -> Self {
        self.fixed_instant = Some(instant.to_owned());
        self
    }

    /// The tokens the application mints, in order.
    #[must_use]
    pub fn tokens(mut self, tokens: Vec<String>) -> Self {
        self.tokens = tokens;
        self
    }

    /// Builds the application.
    #[must_use]
    pub fn build(self) -> TestApp {
        let ids = Arc::new(ScriptedIds::new(self.ids));
        let clock = Arc::new(match &self.fixed_instant {
            Some(instant) => ScriptedClock::fixed(instant),
            None => ScriptedClock::new(self.instants),
        });
        let tokens = Arc::new(ScriptedTokens::new(self.tokens));
        let publisher = Arc::new(MemoryEventPublisher::new());
        let hasher = Arc::new(FixedHasher::new());
        let database = MemoryDatabase::new();

        let stores = Stores::new(
            Arc::new(database.accounts()),
            Arc::new(database.users()),
            Arc::new(database.sessions()),
            Arc::new(database.spaces()),
            Arc::new(database.bots()),
            Arc::new(database.threads()),
            Arc::new(database.messages()),
            Arc::new(database.runs(Arc::clone(&clock) as Arc<dyn Clock>)),
            Arc::new(database.events(Arc::clone(&publisher) as Arc<dyn EventPublisher>)),
            Arc::new(database.invitations()),
            Arc::new(database.cards()),
            Arc::new(database.pending_tool_calls()),
            Arc::new(database.effects()),
            Arc::new(database.tools()),
            Arc::new(database.rate_limits()),
            Arc::new(database.secrets()),
            Arc::new(database.plugin_submissions()),
            Arc::new(database.skills()),
            Arc::new(database.mcp_servers()),
            Arc::new(database.audit()),
            Arc::new(database.connectors()),
            Arc::new(database.plugin_installs()),
            Arc::new(database.artifacts()),
            Arc::new(database.reactions()),
            Arc::new(database.read_states()),
            Arc::new(database.roster()),
            Arc::new(database.memory()),
            Arc::new(database.routines()),
            Arc::new(database.computers()),
            Arc::new(database.screen_capabilities()),
        );
        let services = Services::new(
            Arc::clone(&publisher) as Arc<dyn EventPublisher>,
            Arc::clone(&hasher) as Arc<dyn PasswordHasher>,
            Arc::clone(&tokens) as Arc<dyn TokenGenerator>,
            Arc::clone(&ids) as Arc<dyn IdGenerator>,
            Arc::clone(&clock) as Arc<dyn Clock>,
            boma_domain::model::DeploymentModels {
                reachable: vec![
                    "openai/gpt-5".to_owned(),
                    "anthropic/claude-sonnet".to_owned(),
                ],
                default: "anthropic/claude-sonnet".to_owned(),
            },
            self.mcp_transport
                .clone()
                .unwrap_or_else(|| Arc::new(crate::ScriptedMcpTransport::new())),
            self.plugin_fetcher
                .clone()
                .unwrap_or_else(|| Arc::new(ScriptedPluginFetcher::new())),
            self.oauth_client
                .clone()
                .unwrap_or_else(|| Arc::new(crate::ScriptedOAuthClient::new())),
            self.artifact_content
                .clone()
                .unwrap_or_else(|| Arc::new(database.artifact_content())),
        )
        .with_connectors(
            self.connector_runner
                .clone()
                .unwrap_or_else(|| Arc::new(crate::ScriptedConnectorRunner::new())),
        );
        let services = if let Some(runtime) = self.runtime {
            services.with_computers(runtime as Arc<dyn boma_app::ports::ComputerRuntime>)
        } else if self.computers {
            if self.computer_answers.is_empty() {
                services.with_computers(Arc::new(crate::ScriptedComputerRuntime::default())
                    as Arc<dyn boma_app::ports::ComputerRuntime>)
            } else {
                services.with_computers(Arc::new(crate::ScriptedComputerRuntime::new(
                    self.computer_answers.clone(),
                ))
                    as Arc<dyn boma_app::ports::ComputerRuntime>)
            }
        } else {
            services
        };

        TestApp {
            app: App::new(stores, services),
            database,
            ids,
            clock,
            tokens,
            publisher,
            hasher,
        }
    }
}

#[cfg(test)]
mod tests {
    use boma_app::{
        accounts::SignUp,
        ports::IdGenerator,
        usecases::{SendMessage, create_bot, send_message, sign_up},
    };
    use boma_domain::{RunStatus, SpaceId};

    use super::TestApp;

    #[tokio::test]
    async fn req_001_a_test_app_signs_a_person_in_and_starts_a_run() {
        let test = TestApp::builder()
            .ids(vec![
                "usr_1".to_owned(),
                "org_1".to_owned(),
                "spc_1".to_owned(),
            ])
            .build();

        let grant = sign_up(
            test.app(),
            3_600,
            &SignUp {
                email: "person@example.com".to_owned(),
                password: "correct horse battery".to_owned(),
                name: "Ada".to_owned(),
                locale: "en-US".to_owned(),
            },
        )
        .await
        .expect("the person signs up");
        assert_eq!(grant.user.id.as_str(), "usr_1");

        let space = test
            .app()
            .stores
            .spaces
            .default_for(&grant.user.id)
            .await
            .expect("the store answers")
            .expect("the person lands in a space");
        let context = TestApp::member_context(&space.id, &grant.user.id);

        let bot = create_bot(test.app(), &context, "Assistant", None, None)
            .await
            .expect("the bot is created");

        let outcome = send_message(
            test.app(),
            &context,
            &SendMessage {
                bot_id: bot.bot.id.to_string(),
                text: "plan my week".to_owned(),
                client_nonce: Some("nonce-1".to_owned()),
                reply_to_message_id: None,
                reply_quote: None,
            },
        )
        .await
        .expect("the message is sent");

        assert_eq!(outcome.run.status, RunStatus::Queued);
        assert_eq!(outcome.message.seq, 1);

        // The replay of the same nonce returns the same turn (REQ-002).
        let replay = send_message(
            test.app(),
            &context,
            &SendMessage {
                bot_id: bot.bot.id.to_string(),
                text: "plan my week".to_owned(),
                client_nonce: Some("nonce-1".to_owned()),
                reply_to_message_id: None,
                reply_quote: None,
            },
        )
        .await
        .expect("the replay is sent");
        assert_eq!(replay.message.id, outcome.message.id);
        assert_eq!(replay.run.id, outcome.run.id);
    }

    #[tokio::test]
    async fn req_021_a_run_that_fails_carries_a_legible_reason() {
        use std::sync::Arc;

        use crate::ScriptedProvider;
        use boma_app::worker::Worker;

        let test = TestApp::builder().build();
        let user_id = test
            .seed_user("person@example.com", "correct horse battery")
            .await
            .expect("the person exists");
        let space = test
            .app()
            .stores
            .spaces
            .default_for(&user_id)
            .await
            .expect("the store answers")
            .expect("the person lands in a space");
        let context = TestApp::member_context(&space.id, &user_id);

        let bot = boma_app::usecases::create_bot(test.app(), &context, "Assistant", None, None)
            .await
            .expect("the bot is created");
        let outcome = boma_app::usecases::send_message(
            test.app(),
            &context,
            &boma_app::usecases::SendMessage {
                bot_id: bot.bot.id.to_string(),
                text: "plan my week".to_owned(),
                client_nonce: None,
                reply_to_message_id: None,
                reply_quote: None,
            },
        )
        .await
        .expect("the message is sent");
        assert_eq!(outcome.run.status, RunStatus::Queued);

        // The provider has nothing to say, so the turn fails: the person must still be told,
        // in words safe to show (REQ-021).
        let app = Arc::new(test.app().clone());
        let worker = Worker::new(
            app,
            Arc::new(ScriptedProvider::default()),
            "worker-1".to_owned(),
        );
        assert!(
            worker.step().await.expect("the worker answers"),
            "the worker claimed the queued run"
        );

        let page = test
            .app()
            .stores
            .runs
            .list(&context, Some(&bot.bot.id), None, 10)
            .await
            .expect("the activity answers");
        let run = page.runs.into_iter().next().expect("the run is listed").run;
        assert_eq!(run.status, RunStatus::Failed);
        let reason = run.error.expect("the failure is recorded on the run");
        assert!(!reason.is_empty(), "the reason is not empty");
        assert!(
            !reason.contains("sqlx") && !reason.contains("panic"),
            "the reason names the rule, not the internals of a dependency: {reason}"
        );
    }

    #[tokio::test]
    async fn req_006_a_stopped_run_absorbs_nothing_else() {
        let test = TestApp::builder().build();
        let user_id = test
            .seed_user("person@example.com", "correct horse battery")
            .await
            .expect("the person exists");
        let space = test
            .app()
            .stores
            .spaces
            .default_for(&user_id)
            .await
            .expect("the store answers")
            .expect("the person lands in a space");
        let context = TestApp::member_context(&space.id, &user_id);

        let bot = boma_app::usecases::create_bot(test.app(), &context, "Assistant", None, None)
            .await
            .expect("the bot is created");
        let outcome = boma_app::usecases::send_message(
            test.app(),
            &context,
            &boma_app::usecases::SendMessage {
                bot_id: bot.bot.id.to_string(),
                text: "plan my week".to_owned(),
                client_nonce: None,
                reply_to_message_id: None,
                reply_quote: None,
            },
        )
        .await
        .expect("the message is sent");
        assert_eq!(outcome.run.status, RunStatus::Queued);

        // The person stops the work before the worker ever sees it (REQ-006).
        let stopped = boma_app::usecases::cancel_run(test.app(), &context, outcome.run.id.as_str())
            .await
            .expect("the run is stopped");
        assert_eq!(stopped.status, RunStatus::Cancelled);

        // Stopping again is a no-op that shows the same state.
        let again = boma_app::usecases::cancel_run(test.app(), &context, outcome.run.id.as_str())
            .await
            .expect("the stop is idempotent");
        assert_eq!(again.status, RunStatus::Cancelled);

        // Nothing lands on the run afterwards: a terminal row refuses every write.
        let refused = test
            .app()
            .stores
            .runs
            .set_status(
                &context,
                &outcome.run.id,
                RunStatus::Running,
                &test.app().services.clock.now(),
                None,
                None,
            )
            .await;
        assert!(refused.is_err(), "a cancelled run absorbs nothing");

        // The worker finds nothing left to claim.
        let worker = boma_app::worker::Worker::new(
            std::sync::Arc::new(test.app().clone()),
            std::sync::Arc::new(crate::ScriptedProvider::default()),
            "worker-1".to_owned(),
        );
        assert!(!worker.step().await.expect("the worker answers"));
    }

    #[tokio::test]
    async fn req_003_a_message_during_an_active_run_joins_it() {
        let test = TestApp::builder().build();
        let user_id = test
            .seed_user("person@example.com", "correct horse battery")
            .await
            .expect("the person exists");
        let space = test
            .app()
            .stores
            .spaces
            .default_for(&user_id)
            .await
            .expect("the store answers")
            .expect("the person lands in a space");
        let context = TestApp::member_context(&space.id, &user_id);

        let created = boma_app::usecases::create_bot(test.app(), &context, "Assistant", None, None)
            .await
            .expect("the bot is created");
        let bot = created.bot;
        let first = boma_app::usecases::send_message(
            test.app(),
            &context,
            &boma_app::usecases::SendMessage {
                bot_id: bot.id.to_string(),
                text: "Summarise my week.".to_owned(),
                client_nonce: None,
                reply_to_message_id: None,
                reply_quote: None,
            },
        )
        .await
        .expect("the first message is sent");

        // Claim the run without running it, so the steering window is open.
        let claimed = test
            .app()
            .stores
            .runs
            .claim_next_queued("worker-1")
            .await
            .expect("the claim answers")
            .expect("the run is claimed");
        assert_eq!(claimed.id, first.run.id);
        test.app()
            .stores
            .runs
            .set_status(
                &boma_domain::TenantContext::System {
                    reason: boma_domain::SystemReason::ScheduledJob,
                    user_id: Some(user_id.clone()),
                },
                &first.run.id,
                RunStatus::Running,
                &test.app().services.clock.now(),
                Some(claimed.lease_fence),
                None,
            )
            .await
            .expect("the run is running");

        let second = boma_app::usecases::send_message(
            test.app(),
            &context,
            &boma_app::usecases::SendMessage {
                bot_id: bot.id.to_string(),
                text: "And mention Friday's release.".to_owned(),
                client_nonce: None,
                reply_to_message_id: None,
                reply_quote: None,
            },
        )
        .await
        .expect("the steering message is sent");

        assert_eq!(
            second.run.id, first.run.id,
            "one working run: the steering message joined the active one (REQ-003)"
        );

        let active = test
            .app()
            .stores
            .runs
            .find(&context, &first.run.id)
            .await
            .expect("the run answers")
            .expect("the run exists");
        assert_eq!(active.steering_from_seq, Some(2));
    }

    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn req_003_the_worker_folds_a_steering_message_in_before_it_answers() {
        let test = TestApp::builder().build();
        let user_id = test
            .seed_user("person@example.com", "correct horse battery")
            .await
            .expect("the person exists");
        let space = test
            .app()
            .stores
            .spaces
            .default_for(&user_id)
            .await
            .expect("the store answers")
            .expect("the person lands in a space");
        let context = TestApp::member_context(&space.id, &user_id);

        let created = boma_app::usecases::create_bot(test.app(), &context, "Assistant", None, None)
            .await
            .expect("the bot is created");
        let thread_id = created.thread.id.clone();
        let bot = created.bot;
        let first = boma_app::usecases::send_message(
            test.app(),
            &context,
            &boma_app::usecases::SendMessage {
                bot_id: bot.id.to_string(),
                text: "Summarise my week.".to_owned(),
                client_nonce: None,
                reply_to_message_id: None,
                reply_quote: None,
            },
        )
        .await
        .expect("the first message is sent");

        // The model answers "slowly": the first call waits behind the gate, the member steers
        // while it is thinking, and the fold happens in the passes that follow (REQ-003).
        let scripted = crate::ScriptedProvider::new(vec![vec![" After the steering.".to_owned()]]);
        let (scripted, asked, release) = scripted.gated_first_answer();
        let provider = std::sync::Arc::new(scripted);
        let worker = boma_app::worker::Worker::new(
            std::sync::Arc::new(test.app().clone()),
            provider.clone(),
            "worker-1".to_owned(),
        );
        let step = tokio::spawn(async move { worker.step().await });

        // The first provider call is in flight: steer the run, then let the answer through.
        asked.notified().await;
        let second = boma_app::usecases::send_message(
            test.app(),
            &context,
            &boma_app::usecases::SendMessage {
                bot_id: bot.id.to_string(),
                text: "And mention Friday's release.".to_owned(),
                client_nonce: None,
                reply_to_message_id: None,
                reply_quote: None,
            },
        )
        .await
        .expect("the steering message is sent");
        assert_eq!(second.run.id, first.run.id, "one working run (REQ-003)");
        release
            .send(boma_app::ports::ProviderTurn::Chunks(vec![
                "Considering Friday.".to_owned(),
            ]))
            .expect("the gate opens");
        assert!(
            step.await
                .expect("the worker finishes")
                .expect("the step succeeded")
        );

        let requests = provider.requests();
        assert_eq!(requests.len(), 2, "one pass per pending steering state");
        assert_eq!(requests[0].prompt, "Summarise my week.");
        assert_eq!(requests[1].prompt, "And mention Friday's release.");
        assert_eq!(requests[1].history.len(), 2);

        let page = test
            .app()
            .stores
            .messages
            .page(&context, &thread_id, None, 10)
            .await
            .expect("the page answers");
        let bots = page
            .messages
            .iter()
            .filter(|message| message.role == boma_domain::MessageRole::Bot)
            .count();
        assert_eq!(bots, 1, "one bot reply for both texts");
        let reply = page
            .messages
            .iter()
            .find(|message| message.role == boma_domain::MessageRole::Bot)
            .expect("the bot answered");
        let reply_text = reply
            .blocks
            .iter()
            .filter_map(boma_domain::Block::as_text)
            .collect::<Vec<_>>()
            .join("");
        assert_eq!(reply_text, "Considering Friday. After the steering.");

        let run = test
            .app()
            .stores
            .runs
            .find(&context, &first.run.id)
            .await
            .expect("the run answers")
            .expect("the run exists");
        assert_eq!(run.status, RunStatus::Completed);
        assert_eq!(run.steering_consumed_seq, 2);
    }

    #[tokio::test]
    async fn req_017_answering_a_card_records_the_effect_exactly_once() {
        let test = TestApp::builder().build();
        let user_id = test
            .seed_user("person@example.com", "correct horse battery")
            .await
            .expect("the person exists");
        let space = test
            .app()
            .stores
            .spaces
            .default_for(&user_id)
            .await
            .expect("the store answers")
            .expect("the person lands in a space");
        let context = TestApp::member_context(&space.id, &user_id);

        let created = boma_app::usecases::create_bot(test.app(), &context, "Assistant", None, None)
            .await
            .expect("the bot is created");
        let first = boma_app::usecases::send_message(
            test.app(),
            &context,
            &boma_app::usecases::SendMessage {
                bot_id: created.bot.id.to_string(),
                text: "Do the purchase.".to_owned(),
                client_nonce: None,
                reply_to_message_id: None,
                reply_quote: None,
            },
        )
        .await
        .expect("the message is sent");

        // A pending approval blocks the run; answering it authorizes the effect once.
        let card = boma_domain::Card {
            id: boma_domain::CardId::new("card_golden").expect("an id is accepted"),
            space_id: space.id.clone(),
            thread_id: created.thread.id.clone(),
            run_id: Some(first.run.id.clone()),
            kind: boma_domain::CardKind::Approval,
            status: boma_domain::CardStatus::Pending,
            answer: None,
            created_at: "2026-01-01T00:00:00Z".to_owned(),
            answered_at: None,
        };
        let database = test.database();
        database.seed_card(space.id.clone(), card);

        let outcome =
            boma_app::usecases::answer_card(test.app(), &context, "card_golden", "approve")
                .await
                .expect("the card is answered");
        assert!(outcome.effect_recorded, "the effect runs once on approval");

        // A second answer to the same card is refused: the effect never executes twice.
        let second =
            boma_app::usecases::answer_card(test.app(), &context, "card_golden", "approve").await;
        assert!(second.is_err(), "a closed card absorbs no second answer");
    }

    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn req_017_a_write_tool_waits_for_approval() {
        use std::sync::Arc;

        use crate::ScriptedProvider;
        use boma_app::worker::Worker;

        let test = TestApp::builder().build();
        let user_id = test
            .seed_user("person@example.com", "correct horse battery")
            .await
            .expect("the person exists");
        let space = test
            .app()
            .stores
            .spaces
            .default_for(&user_id)
            .await
            .expect("the store answers")
            .expect("the person lands in a space");
        let context = TestApp::member_context(&space.id, &user_id);

        let created = boma_app::usecases::create_bot(test.app(), &context, "Assistant", None, None)
            .await
            .expect("the bot is created");
        let bot_id = created.bot.id.clone();

        let outcome = boma_app::usecases::send_message(
            test.app(),
            &context,
            &boma_app::usecases::SendMessage {
                bot_id: bot_id.to_string(),
                text: "Remember these commits.".to_owned(),
                client_nonce: None,
                reply_to_message_id: None,
                reply_quote: None,
            },
        )
        .await
        .expect("the message is sent");
        let run_id = outcome.run.id.clone();

        // The model asks for a write-class tool; the run must park, never write.
        let provider = Arc::new(ScriptedProvider::new_turn(vec![
            boma_app::ports::ProviderTurn::ToolCalls(vec![boma_app::ports::ToolCallRequest {
                id: "call_1".to_owned(),
                name: "memory.write".to_owned(),
                arguments: serde_json::json!({
                    "scope": "bot",
                    "botId": bot_id.as_str(),
                    "path": "notes/pending.md",
                    "text": "this must not be written before approval",
                }),
            }]),
        ]));
        let worker = Worker::new(
            std::sync::Arc::new(test.app().clone()),
            provider.clone(),
            "worker-1".to_owned(),
        );
        assert!(worker.step().await.expect("the worker answers"));

        // The run waits on the person; it did not fail and it did not complete.
        let run = test
            .app()
            .stores
            .runs
            .find(&context, &run_id)
            .await
            .expect("the store answers")
            .expect("the run exists");
        assert_eq!(
            run.status,
            RunStatus::WaitingInput,
            "a write-class call parks the run instead of executing"
        );

        // The card is visible as a thread message, carrying its id.
        let page = test
            .app()
            .stores
            .messages
            .page(&context, &created.thread.id, None, 10)
            .await
            .expect("the page answers");
        let card_id = page
            .messages
            .iter()
            .flat_map(|message| message.blocks.iter())
            .find_map(|block| match block {
                boma_domain::Block::Card { card_id, kind } => Some((card_id.clone(), kind.clone())),
                _ => None,
            })
            .expect("an approval card is part of the thread");
        assert_eq!(card_id.1, "approval");

        // No bot reply was written and the document does not exist yet.
        let bot_replies = page
            .messages
            .iter()
            .filter(|message| message.role == boma_domain::MessageRole::Bot)
            .count();
        assert_eq!(bot_replies, 0, "the suspended run does not answer yet");
        assert!(
            boma_app::usecases::memory_read(
                test.app(),
                &context,
                "bot",
                Some(bot_id.as_str()),
                "notes/pending.md",
            )
            .await
            .is_err(),
            "the write did not happen before the approval"
        );
    }

    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn req_017_an_approved_tool_effect_runs_exactly_once() {
        use std::sync::Arc;

        use crate::ScriptedProvider;
        use boma_app::worker::Worker;

        let test = TestApp::builder().build();
        let user_id = test
            .seed_user("person@example.com", "correct horse battery")
            .await
            .expect("the person exists");
        let space = test
            .app()
            .stores
            .spaces
            .default_for(&user_id)
            .await
            .expect("the store answers")
            .expect("the person lands in a space");
        let context = TestApp::member_context(&space.id, &user_id);

        let created = boma_app::usecases::create_bot(test.app(), &context, "Assistant", None, None)
            .await
            .expect("the bot is created");
        let bot_id = created.bot.id.clone();

        let outcome = boma_app::usecases::send_message(
            test.app(),
            &context,
            &boma_app::usecases::SendMessage {
                bot_id: bot_id.to_string(),
                text: "Remember these commits.".to_owned(),
                client_nonce: None,
                reply_to_message_id: None,
                reply_quote: None,
            },
        )
        .await
        .expect("the message is sent");
        let run_id = outcome.run.id.clone();

        let provider = Arc::new(ScriptedProvider::new_turn(vec![
            boma_app::ports::ProviderTurn::ToolCalls(vec![boma_app::ports::ToolCallRequest {
                id: "call_1".to_owned(),
                name: "memory.write".to_owned(),
                arguments: serde_json::json!({
                    "scope": "bot",
                    "botId": bot_id.as_str(),
                    "path": "notes/approved.md",
                    "text": "# Friday release\n- ship\n",
                }),
            }]),
        ]));
        let worker = Worker::new(
            std::sync::Arc::new(test.app().clone()),
            provider.clone(),
            "worker-1".to_owned(),
        );
        assert!(worker.step().await.expect("the worker answers"));

        let page = test
            .app()
            .stores
            .messages
            .page(&context, &created.thread.id, None, 10)
            .await
            .expect("the page answers");
        let card_id = page
            .messages
            .iter()
            .flat_map(|message| message.blocks.iter())
            .find_map(|block| match block {
                boma_domain::Block::Card { card_id, .. } => Some(card_id.clone()),
                _ => None,
            })
            .expect("an approval card is part of the thread");

        let outcome = boma_app::usecases::answer_card(test.app(), &context, &card_id, "approve")
            .await
            .expect("the card is answered");
        assert!(outcome.effect_recorded, "the approval effect is recorded");

        // The run resumes on the queue, not as running (the claim re-picks it up).
        let run = test
            .app()
            .stores
            .runs
            .find(&context, &run_id)
            .await
            .expect("the store answers")
            .expect("the run exists");
        assert_eq!(
            run.status,
            RunStatus::Queued,
            "an approved tool re-queues the run"
        );

        let document = boma_app::usecases::memory_read(
            test.app(),
            &context,
            "bot",
            Some(bot_id.as_str()),
            "notes/approved.md",
        )
        .await
        .expect("the write happened on approval");
        assert_eq!(document.revision, 1, "the effect ran exactly once");

        // The exactly-once effect key is recorded, so a replay records a conflict instead of
        // applying twice.
        let replay = test
            .app()
            .stores
            .effects
            .record(
                &context,
                boma_app::ports::NewEffect {
                    run_id: run_id.clone(),
                    effect_key: format!("tool:{}:call_1", run_id.as_str()),
                    kind: "tool".to_owned(),
                    payload: serde_json::json!({}),
                },
            )
            .await;
        assert!(
            matches!(replay, Err(boma_app::AppError::Conflict)),
            "the effect key prevents a second execution"
        );

        // A second answer to the same card is refused: the effect never executes twice.
        let second =
            boma_app::usecases::answer_card(test.app(), &context, &card_id, "approve").await;
        assert!(second.is_err(), "a closed card absorbs no second answer");
    }

    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn req_019_the_parent_receives_the_helper_result_and_continues() {
        use std::sync::Arc;

        use crate::ScriptedProvider;
        use boma_app::worker::Worker;

        let test = TestApp::builder().build();
        let user_id = test
            .seed_user("person@example.com", "correct horse battery")
            .await
            .expect("the person exists");
        let space = test
            .app()
            .stores
            .spaces
            .default_for(&user_id)
            .await
            .expect("the store answers")
            .expect("the person lands in a space");
        let context = TestApp::member_context(&space.id, &user_id);

        let created = boma_app::usecases::create_bot(test.app(), &context, "Assistant", None, None)
            .await
            .expect("the bot is created");
        let bot_id = created.bot.id.clone();

        let outcome = boma_app::usecases::send_message(
            test.app(),
            &context,
            &boma_app::usecases::SendMessage {
                bot_id: bot_id.to_string(),
                text: "Summarise the commits.".to_owned(),
                client_nonce: None,
                reply_to_message_id: None,
                reply_quote: None,
            },
        )
        .await
        .expect("the message is sent");
        let run_id = outcome.run.id.clone();

        // Parent: delegate. Helper: its answer. Parent: the final reply.
        let provider = Arc::new(ScriptedProvider::new_turn(vec![
            boma_app::ports::ProviderTurn::ToolCalls(vec![boma_app::ports::ToolCallRequest {
                id: "call_1".to_owned(),
                name: "delegate.helper".to_owned(),
                arguments: serde_json::json!({ "prompt": "Research the commits." }),
            }]),
            boma_app::ports::ProviderTurn::Chunks(vec!["The helper found Friday.".to_owned()]),
            boma_app::ports::ProviderTurn::Chunks(vec!["Final: Friday.".to_owned()]),
        ]));
        let worker = Worker::new(
            std::sync::Arc::new(test.app().clone()),
            provider.clone(),
            "worker-1".to_owned(),
        );

        // Pass 1: the parent delegates and parks on the approval card.
        assert!(worker.step().await.expect("the worker answers"));
        let run = test
            .app()
            .stores
            .runs
            .find(&context, &run_id)
            .await
            .expect("the store answers")
            .expect("the run exists");
        assert_eq!(run.status, RunStatus::WaitingInput);

        let page = test
            .app()
            .stores
            .messages
            .page(&context, &created.thread.id, None, 10)
            .await
            .expect("the page answers");
        let card_id = page
            .messages
            .iter()
            .flat_map(|message| message.blocks.iter())
            .find_map(|block| match block {
                boma_domain::Block::Card { card_id, .. } => Some(card_id.clone()),
                _ => None,
            })
            .expect("an approval card is part of the thread");

        // The person approves: the helper run starts, and the parent must not answer yet.
        boma_app::usecases::answer_card(test.app(), &context, &card_id, "approve")
            .await
            .expect("the card is answered");
        let run = test
            .app()
            .stores
            .runs
            .find(&context, &run_id)
            .await
            .expect("the store answers")
            .expect("the run exists");
        assert_eq!(
            run.status,
            RunStatus::WaitingInput,
            "the parent waits for the helper, it does not answer first"
        );

        // Pass 2: the helper answers; the internal text goes to role=tool only.
        assert!(worker.step().await.expect("the worker answers"));

        // The helper completing re-queues the parent.
        let run = test
            .app()
            .stores
            .runs
            .find(&context, &run_id)
            .await
            .expect("the store answers")
            .expect("the run exists");
        assert_eq!(
            run.status,
            RunStatus::Queued,
            "the completed helper re-queues the parent"
        );

        // Pass 3: the parent continues with the helper's outcome in its history.
        assert!(worker.step().await.expect("the worker answers"));

        let page = test
            .app()
            .stores
            .messages
            .page(&context, &created.thread.id, None, 20)
            .await
            .expect("the page answers");
        let bot_texts: Vec<&str> = page
            .messages
            .iter()
            .filter(|message| message.role == boma_domain::MessageRole::Bot)
            .flat_map(|message| message.blocks.iter())
            .filter_map(boma_domain::Block::as_text)
            .collect();
        assert_eq!(
            bot_texts,
            vec!["Final: Friday."],
            "exactly one bot reply, after the helper"
        );
        let tool_texts: Vec<&str> = page
            .messages
            .iter()
            .filter(|message| message.role == boma_domain::MessageRole::Tool)
            .flat_map(|message| message.blocks.iter())
            .filter_map(boma_domain::Block::as_text)
            .collect();
        assert!(
            tool_texts
                .iter()
                .any(|text| text.contains("The helper found Friday.")),
            "the helper's structured result is in a tool message"
        );

        let run = test
            .app()
            .stores
            .runs
            .find(&context, &run_id)
            .await
            .expect("the store answers")
            .expect("the run exists");
        assert_eq!(run.status, RunStatus::Completed);
    }

    #[tokio::test]
    async fn req_019_the_helper_internal_steps_do_not_surface_in_the_parent_chat() {
        use std::sync::Arc;

        use crate::ScriptedProvider;
        use boma_app::worker::Worker;

        let test = TestApp::builder().build();
        let user_id = test
            .seed_user("person@example.com", "correct horse battery")
            .await
            .expect("the person exists");
        let space = test
            .app()
            .stores
            .spaces
            .default_for(&user_id)
            .await
            .expect("the store answers")
            .expect("the person lands in a space");
        let context = TestApp::member_context(&space.id, &user_id);

        let created = boma_app::usecases::create_bot(test.app(), &context, "Assistant", None, None)
            .await
            .expect("the bot is created");
        let bot_id = created.bot.id.clone();

        boma_app::usecases::send_message(
            test.app(),
            &context,
            &boma_app::usecases::SendMessage {
                bot_id: bot_id.to_string(),
                text: "Summarise the commits.".to_owned(),
                client_nonce: None,
                reply_to_message_id: None,
                reply_quote: None,
            },
        )
        .await
        .expect("the message is sent");

        let provider = Arc::new(ScriptedProvider::new_turn(vec![
            boma_app::ports::ProviderTurn::ToolCalls(vec![boma_app::ports::ToolCallRequest {
                id: "call_1".to_owned(),
                name: "delegate.helper".to_owned(),
                arguments: serde_json::json!({ "prompt": "Research the commits." }),
            }]),
            boma_app::ports::ProviderTurn::Chunks(vec!["The helper found Friday.".to_owned()]),
        ]));
        let worker = Worker::new(
            std::sync::Arc::new(test.app().clone()),
            provider.clone(),
            "worker-1".to_owned(),
        );

        // Delegate, approve, let the helper finish.
        assert!(worker.step().await.expect("the worker answers"));
        let page = test
            .app()
            .stores
            .messages
            .page(&context, &created.thread.id, None, 10)
            .await
            .expect("the page answers");
        let card_id = page
            .messages
            .iter()
            .flat_map(|message| message.blocks.iter())
            .find_map(|block| match block {
                boma_domain::Block::Card { card_id, .. } => Some(card_id.clone()),
                _ => None,
            })
            .expect("an approval card is part of the thread");
        boma_app::usecases::answer_card(test.app(), &context, &card_id, "approve")
            .await
            .expect("the card is answered");
        assert!(worker.step().await.expect("the worker answers"));

        // The helper's internal text never appears as a bot message.
        let page = test
            .app()
            .stores
            .messages
            .page(&context, &created.thread.id, None, 10)
            .await
            .expect("the page answers");
        let bot_texts: Vec<&str> = page
            .messages
            .iter()
            .filter(|message| message.role == boma_domain::MessageRole::Bot)
            .flat_map(|message| message.blocks.iter())
            .filter_map(boma_domain::Block::as_text)
            .collect();
        assert!(
            bot_texts.is_empty(),
            "the helper's answer is not a chat reply: {bot_texts:?}"
        );

        let helper_texts: Vec<&str> = page
            .messages
            .iter()
            .filter(|message| message.role == boma_domain::MessageRole::Tool)
            .flat_map(|message| message.blocks.iter())
            .filter_map(boma_domain::Block::as_text)
            .collect();
        assert!(
            helper_texts
                .iter()
                .any(|text| text.contains("The helper found Friday.")),
            "the helper outcome is recorded as a tool message"
        );
    }

    #[tokio::test]
    async fn req_016_the_catalogue_carries_only_granted_tools() {
        let test = TestApp::builder().build();
        let user_id = test
            .seed_user("person@example.com", "correct horse battery")
            .await
            .expect("the person exists");
        let space = test
            .app()
            .stores
            .spaces
            .default_for(&user_id)
            .await
            .expect("the store answers")
            .expect("the person lands in a space");
        let context = TestApp::member_context(&space.id, &user_id);

        let created = boma_app::usecases::create_bot(test.app(), &context, "Assistant", None, None)
            .await
            .expect("the bot is created");
        let bot_id = created.bot.id.clone();

        // The space installs three tools; one is disabled.
        let database = test.database();
        database.seed_tool_install(
            space.id.clone(),
            "mcp.echo",
            "Echo a string",
            boma_domain::ToolKind::Mcp,
            true,
        );
        database.seed_tool_install(
            space.id.clone(),
            "mcp.write",
            "Write the file",
            boma_domain::ToolKind::Mcp,
            true,
        );
        database.seed_tool_install(
            space.id.clone(),
            "connector.github",
            "Open a GitHub pull request",
            boma_domain::ToolKind::Connector,
            false,
        );

        // A bot whose grant is the echo tool: the catalogue carries the built-ins plus
        // whatever the grant allows.
        database.seed_bot_grant(&bot_id, vec!["mcp.echo".to_owned()]);
        let catalogue = boma_app::usecases::tool_catalogue(test.app(), &context, bot_id.as_str())
            .await
            .expect("the catalogue answers");
        let keys: Vec<&str> = catalogue.iter().map(|tool| tool.key.as_str()).collect();
        assert!(keys.contains(&"mcp.echo"));
        assert!(keys.contains(&"memory.read"));
        assert!(!keys.contains(&"connector.github"));
        assert!(!keys.contains(&"mcp.write"));

        // Empty grant: the built-ins plus every enabled install of the space; disabled ones
        // never (REQ-016).
        database.seed_bot_grant(&bot_id, Vec::new());
        let catalogue = boma_app::usecases::tool_catalogue(test.app(), &context, bot_id.as_str())
            .await
            .expect("the catalogue answers");
        let keys: Vec<&str> = catalogue.iter().map(|tool| tool.key.as_str()).collect();
        assert!(keys.contains(&"mcp.echo"));
        assert!(keys.contains(&"mcp.write"));
        assert!(keys.contains(&"memory.read"));
        assert!(keys.contains(&"memory.write"));
        assert!(!keys.contains(&"connector.github"));
    }

    #[tokio::test]
    async fn req_016_the_worker_asks_with_only_granted_tools() {
        use std::sync::Arc;

        let test = TestApp::builder().build();
        let user_id = test
            .seed_user("person@example.com", "correct horse battery")
            .await
            .expect("the person exists");
        let space = test
            .app()
            .stores
            .spaces
            .default_for(&user_id)
            .await
            .expect("the store answers")
            .expect("the person lands in a space");
        let context = TestApp::member_context(&space.id, &user_id);

        let created = boma_app::usecases::create_bot(test.app(), &context, "Assistant", None, None)
            .await
            .expect("the bot is created");
        let thread_id = created.thread.id.clone();
        let bot_id = created.bot.id.clone();

        let database = test.database();
        database.seed_tool_install(
            space.id.clone(),
            "mcp.echo",
            "Echo a string",
            boma_domain::ToolKind::Mcp,
            true,
        );
        database.seed_tool_install(
            space.id.clone(),
            "mcp.write",
            "Write the file",
            boma_domain::ToolKind::Mcp,
            true,
        );
        database.seed_bot_grant(&bot_id, vec!["mcp.write".to_owned()]);

        boma_app::usecases::send_message(
            test.app(),
            &context,
            &boma_app::usecases::SendMessage {
                bot_id: bot_id.to_string(),
                text: "Summarise my week.".to_owned(),
                client_nonce: None,
                reply_to_message_id: None,
                reply_quote: None,
            },
        )
        .await
        .expect("the message is sent");

        let provider =
            Arc::new(crate::ScriptedProvider::default().with_fallback(vec!["Done.".to_owned()]));
        let worker = boma_app::worker::Worker::new(
            std::sync::Arc::new(test.app().clone()),
            provider.clone(),
            "worker-1".to_owned(),
        );
        assert!(worker.step().await.expect("the worker answers"));

        let requests = provider.requests();
        assert_eq!(requests.len(), 1);
        let keys: Vec<&str> = requests[0]
            .tools
            .iter()
            .map(|tool| tool.key.as_str())
            .collect();
        let expected: Vec<String> = boma_app::usecases::builtin_tools()
            .into_iter()
            .chain(std::iter::once(boma_domain::ToolSpec {
                key: "mcp.write".to_owned(),
                description: "Write the file".to_owned(),
                kind: boma_domain::ToolKind::Mcp,
                effect: boma_domain::ToolEffect::Write,
            }))
            .map(|tool| tool.key)
            .collect();
        assert_eq!(
            keys, expected,
            "the model never sees a tool the bot may not use"
        );
        let _ = thread_id;
    }

    #[tokio::test]
    async fn req_008_a_reply_carries_the_message_it_answers_and_the_quote() {
        let test = TestApp::builder().build();
        let user_id = test
            .seed_user("person@example.com", "correct horse battery")
            .await
            .expect("the person exists");
        let space = test
            .app()
            .stores
            .spaces
            .default_for(&user_id)
            .await
            .expect("the store answers")
            .expect("the person lands in a space");
        let context = TestApp::member_context(&space.id, &user_id);

        let created = boma_app::usecases::create_bot(test.app(), &context, "Assistant", None, None)
            .await
            .expect("the bot is created");
        let first = boma_app::usecases::send_message(
            test.app(),
            &context,
            &boma_app::usecases::SendMessage {
                bot_id: created.bot.id.to_string(),
                text: "Summarise my week.".to_owned(),
                client_nonce: None,
                reply_to_message_id: None,
                reply_quote: None,
            },
        )
        .await
        .expect("the first message is sent");

        let reply = boma_app::usecases::send_message(
            test.app(),
            &context,
            &boma_app::usecases::SendMessage {
                bot_id: created.bot.id.to_string(),
                text: "Friday too.".to_owned(),
                client_nonce: None,
                reply_to_message_id: Some(first.message.id.as_str().to_owned()),
                reply_quote: Some("Summarise my week.".to_owned()),
            },
        )
        .await
        .expect("the reply is sent");

        assert_eq!(
            reply
                .message
                .reply_to_message_id
                .as_ref()
                .map(boma_domain::MessageId::as_str),
            Some(first.message.id.as_str())
        );
        assert_eq!(
            reply.message.reply_quote.as_deref(),
            Some("Summarise my week.")
        );
    }

    #[tokio::test]
    async fn req_015_a_pinned_model_the_deployment_cannot_reach_fails_visibly() {
        let test = TestApp::builder().build();
        let user_id = test
            .seed_user("person@example.com", "correct horse battery")
            .await
            .expect("the person exists");
        let space = test
            .app()
            .stores
            .spaces
            .default_for(&user_id)
            .await
            .expect("the store answers")
            .expect("the person lands in a space");
        let context = TestApp::member_context(&space.id, &user_id);

        // The bot is pinned to a model this deployment cannot reach: the send must refuse,
        // and the run must never execute elsewhere silently (REQ-015).
        let created = boma_app::usecases::create_bot(test.app(), &context, "Pinned", None, None)
            .await
            .expect("the bot is created");
        let bot_id = created.bot.id.clone();
        let database = test.database();
        database.seed_bot_model(&bot_id, "openai/gpt-6", true);

        let outcome = boma_app::usecases::send_message(
            test.app(),
            &context,
            &boma_app::usecases::SendMessage {
                bot_id: bot_id.to_string(),
                text: "Summarise my week.".to_owned(),
                client_nonce: None,
                reply_to_message_id: None,
                reply_quote: None,
            },
        )
        .await;
        assert!(outcome.is_err(), "an unreachable pinned model must not run");

        // A reachable unpinned choice falls back to itself: the run executes on the bot's
        // preference, not silently on the deployment default (REQ-015).
        database.seed_bot_model(&bot_id, "openai/gpt-5", false);
        let outcome = boma_app::usecases::send_message(
            test.app(),
            &context,
            &boma_app::usecases::SendMessage {
                bot_id: bot_id.to_string(),
                text: "Summarise my week.".to_owned(),
                client_nonce: None,
                reply_to_message_id: None,
                reply_quote: None,
            },
        )
        .await
        .expect("the fallback resolves");
        assert_eq!(outcome.run.model_provider.as_deref(), Some("openai"));
        assert_eq!(outcome.run.model_id.as_deref(), Some("gpt-5"));
    }

    #[tokio::test]
    async fn req_016_a_builtin_memory_read_grants_the_read_and_the_gate_dispatches_it() {
        let test = TestApp::builder().build();
        let user_id = test
            .seed_user("person@example.com", "correct horse battery")
            .await
            .expect("the person exists");
        let space = test
            .app()
            .stores
            .spaces
            .default_for(&user_id)
            .await
            .expect("the store answers")
            .expect("the person lands in a space");
        let context = TestApp::member_context(&space.id, &user_id);

        let created = boma_app::usecases::create_bot(test.app(), &context, "Assistant", None, None)
            .await
            .expect("the bot is created");

        // The read tool is part of the built-in catalogue with a read effect class (REQ-016).
        let catalogue =
            boma_app::usecases::tool_catalogue(test.app(), &context, created.bot.id.as_str())
                .await
                .expect("the catalogue answers");
        let memory_read = catalogue
            .iter()
            .find(|tool| tool.key == "memory.read")
            .expect("memory.read is a built-in tool");
        assert_eq!(memory_read.effect, boma_domain::ToolEffect::Read);

        // The bot reads a document it owns after a write.
        boma_app::usecases::memory_write(
            test.app(),
            &context,
            "bot",
            Some(created.bot.id.as_str()),
            "notes/commits.md",
            "# Commits",
        )
        .await
        .expect("the memory write happened");
        let result = boma_app::usecases::dispatch_tool(
            test.app(),
            &context,
            created.bot.id.as_str(),
            &boma_app::ports::ToolCallRequest {
                id: "call_1".to_owned(),
                name: "memory.read".to_owned(),
                arguments: serde_json::json!({
                    "scope": "bot",
                    "botId": created.bot.id.as_str(),
                    "path": "notes/commits.md",
                }),
            },
        )
        .await
        .expect("the call dispatches");
        assert_eq!(result.id, "call_1");
        assert!(result.ok);
        assert_eq!(
            result
                .body
                .get("content")
                .and_then(serde_json::Value::as_str),
            Some("# Commits"),
        );
    }

    #[tokio::test]
    async fn req_016_a_tool_the_bot_may_not_use_is_refused_and_recorded() {
        let test = TestApp::builder().build();
        let user_id = test
            .seed_user("person@example.com", "correct horse battery")
            .await
            .expect("the person exists");
        let space = test
            .app()
            .stores
            .spaces
            .default_for(&user_id)
            .await
            .expect("the store answers")
            .expect("the person lands in a space");
        let context = TestApp::member_context(&space.id, &user_id);

        let created = boma_app::usecases::create_bot(test.app(), &context, "Assistant", None, None)
            .await
            .expect("the bot is created");

        let result = boma_app::usecases::dispatch_tool(
            test.app(),
            &context,
            created.bot.id.as_str(),
            &boma_app::ports::ToolCallRequest {
                id: "call_1".to_owned(),
                name: "delete.everything".to_owned(),
                arguments: serde_json::json!({}),
            },
        )
        .await
        .expect("the refusal itself is not an error");
        assert_eq!(result.id, "call_1");
        assert!(!result.ok);
        assert_eq!(
            result.body.get("error").and_then(serde_json::Value::as_str),
            Some("the bot may not use this tool"),
        );
    }

    #[tokio::test]
    async fn req_016_a_write_tool_requires_approval_and_has_no_effect_yet() {
        let test = TestApp::builder().build();
        let user_id = test
            .seed_user("person@example.com", "correct horse battery")
            .await
            .expect("the person exists");
        let space = test
            .app()
            .stores
            .spaces
            .default_for(&user_id)
            .await
            .expect("the store answers")
            .expect("the person lands in a space");
        let context = TestApp::member_context(&space.id, &user_id);

        let created = boma_app::usecases::create_bot(test.app(), &context, "Assistant", None, None)
            .await
            .expect("the bot is created");

        let catalogue =
            boma_app::usecases::tool_catalogue(test.app(), &context, created.bot.id.as_str())
                .await
                .expect("the catalogue answers");
        let memory_write = catalogue
            .iter()
            .find(|tool| tool.key == "memory.write")
            .expect("memory.write is a built-in tool");
        assert_eq!(memory_write.effect, boma_domain::ToolEffect::Write);

        let result = boma_app::usecases::dispatch_tool(
            test.app(),
            &context,
            created.bot.id.as_str(),
            &boma_app::ports::ToolCallRequest {
                id: "call_1".to_owned(),
                name: "memory.write".to_owned(),
                arguments: serde_json::json!({
                    "scope": "bot",
                    "botId": created.bot.id.as_str(),
                    "path": "notes/pending.md",
                    "text": "this must not be written before approval",
                }),
            },
        )
        .await
        .expect("the call dispatches");
        assert_eq!(result.id, "call_1");
        assert!(result.ok);
        assert_eq!(
            result
                .body
                .get("approval")
                .and_then(serde_json::Value::as_str),
            Some("required"),
        );
        // No effect happened: the document does not exist.
        assert!(
            boma_app::usecases::memory_read(
                test.app(),
                &context,
                "bot",
                Some(created.bot.id.as_str()),
                "notes/pending.md",
            )
            .await
            .is_err(),
        );
    }

    #[tokio::test]
    async fn req_022_a_memory_entry_is_available_in_a_later_run_context() {
        let test = TestApp::builder().build();
        let user_id = test
            .seed_user("person@example.com", "correct horse battery")
            .await
            .expect("the person exists");
        let space = test
            .app()
            .stores
            .spaces
            .default_for(&user_id)
            .await
            .expect("the store answers")
            .expect("the person lands in a space");
        let context = TestApp::member_context(&space.id, &user_id);

        let created = boma_app::usecases::create_bot(test.app(), &context, "Assistant", None, None)
            .await
            .expect("the bot is created");
        let bot_id = created.bot.id.clone();

        boma_app::usecases::memory_write(
            test.app(),
            &context,
            "bot",
            Some(bot_id.as_str()),
            "notes/commits.md",
            "# Commits\n- previous work\n",
        )
        .await
        .expect("the memory write happened");
        boma_app::usecases::memory_write(
            test.app(),
            &context,
            "shared",
            None,
            "shared/decisions.md",
            "# Decisions\n- Friday release\n",
        )
        .await
        .expect("the shared memory write happened");

        boma_app::usecases::send_message(
            test.app(),
            &context,
            &boma_app::usecases::SendMessage {
                bot_id: bot_id.to_string(),
                text: "Summarise my week.".to_owned(),
                client_nonce: None,
                reply_to_message_id: None,
                reply_quote: None,
            },
        )
        .await
        .expect("the message is sent");

        let provider = std::sync::Arc::new(crate::ScriptedProvider::default());
        let worker = boma_app::worker::Worker::new(
            std::sync::Arc::new(test.app().clone()),
            provider.clone(),
            "worker-1".to_owned(),
        );
        assert!(worker.step().await.expect("the worker answers"));

        let request = provider
            .requests()
            .into_iter()
            .next()
            .expect("the provider was asked");
        let memory: Vec<&boma_app::ports::MemoryContext> = request.memory.iter().collect();
        assert_eq!(
            memory.len(),
            2,
            "both the bot and the space-shared memory rode into the turn"
        );
        let has = |scope, path, content| {
            memory.iter().any(|document| {
                document.scope == scope && document.path == path && document.content == content
            })
        };
        assert!(
            has(
                boma_domain::MemoryScope::Bot,
                "notes/commits.md",
                "# Commits\n- previous work\n"
            ),
            "the bot's own document reached the turn"
        );
        assert!(
            has(
                boma_domain::MemoryScope::Shared,
                "shared/decisions.md",
                "# Decisions\n- Friday release\n"
            ),
            "the space-shared document reached the turn"
        );
    }

    #[tokio::test]
    async fn req_019_the_tool_loop_is_bounded() {
        use std::sync::Arc;

        use crate::ScriptedProvider;
        use boma_app::worker::Worker;

        let test = TestApp::builder().build();
        let user_id = test
            .seed_user("person@example.com", "correct horse battery")
            .await
            .expect("the person exists");
        let space = test
            .app()
            .stores
            .spaces
            .default_for(&user_id)
            .await
            .expect("the store answers")
            .expect("the person lands in a space");
        let context = TestApp::member_context(&space.id, &user_id);

        let created = boma_app::usecases::create_bot(test.app(), &context, "Assistant", None, None)
            .await
            .expect("the bot is created");
        let bot_id = created.bot.id.clone();

        boma_app::usecases::send_message(
            test.app(),
            &context,
            &boma_app::usecases::SendMessage {
                bot_id: bot_id.to_string(),
                text: "Loop.".to_owned(),
                client_nonce: None,
                reply_to_message_id: None,
                reply_quote: None,
            },
        )
        .await
        .expect("the message is sent");
        let outcome = test
            .app()
            .stores
            .runs
            .list(&context, Some(&bot_id), None, 1)
            .await
            .expect("the store answers");
        let run_id = outcome.runs[0].run.id.clone();

        // The model keeps asking for the same read tool forever; the loop must terminate.
        let call = || boma_app::ports::ToolCallRequest {
            id: "call_1".to_owned(),
            name: "memory.read".to_owned(),
            arguments: serde_json::json!({ "scope": "shared", "path": "never-ends.md" }),
        };
        let provider = Arc::new(ScriptedProvider::new_turn(
            (0..32)
                .map(|_| boma_app::ports::ProviderTurn::ToolCalls(vec![call()]))
                .collect(),
        ));
        let worker = Worker::new(
            std::sync::Arc::new(test.app().clone()),
            provider.clone(),
            "worker-1".to_owned(),
        );
        assert!(worker.step().await.expect("the worker answers"));

        let run = test
            .app()
            .stores
            .runs
            .find(&context, &run_id)
            .await
            .expect("the store answers")
            .expect("the run exists");
        assert_eq!(
            run.status,
            RunStatus::Failed,
            "the run fails legibly instead of looping forever"
        );
        let reason = run.error.expect("the failure records a reason");
        assert!(
            reason.contains("tool calls"),
            "the reason names the rule: {reason}"
        );

        // The tool rounds are bounded exactly at the cap.
        let page = test
            .app()
            .stores
            .messages
            .page(&context, &created.thread.id, None, 50)
            .await
            .expect("the page answers");
        let tool_messages = page
            .messages
            .iter()
            .filter(|message| message.role == boma_domain::MessageRole::Tool)
            .count();
        assert_eq!(
            tool_messages, 8,
            "at most eight rounds of tool calls fold into one run"
        );
    }

    #[tokio::test]
    async fn req_019_delegate_helper_run_creates_a_child_run() {
        let test = TestApp::builder().build();
        let user_id = test
            .seed_user("person@example.com", "correct horse battery")
            .await
            .expect("the person exists");
        let space = test
            .app()
            .stores
            .spaces
            .default_for(&user_id)
            .await
            .expect("the store answers")
            .expect("the person lands in a space");
        let context = TestApp::member_context(&space.id, &user_id);

        let created = boma_app::usecases::create_bot(test.app(), &context, "Assistant", None, None)
            .await
            .expect("the bot is created");
        let parent = boma_app::usecases::send_message(
            test.app(),
            &context,
            &boma_app::usecases::SendMessage {
                bot_id: created.bot.id.to_string(),
                text: "Summarise my week.".to_owned(),
                client_nonce: None,
                reply_to_message_id: None,
                reply_quote: None,
            },
        )
        .await
        .expect("the parent run is created");

        let helper = boma_app::usecases::delegate_helper_run(
            test.app(),
            &context,
            parent.run.id.as_str(),
            "Research the week's commits.",
        )
        .await
        .expect("the helper run is created");

        assert_eq!(helper.trigger, boma_domain::RunTrigger::HelperResult);
        assert_eq!(
            helper
                .parent_run_id
                .as_ref()
                .map(boma_domain::RunId::as_str),
            Some(parent.run.id.as_str())
        );
        assert!(helper.is_helper());
    }

    #[tokio::test]
    async fn req_042_a_seeded_person_owns_the_space_they_land_in() {
        let test = TestApp::builder().build();
        let user_id = test
            .seed_user("person@example.com", "correct horse battery")
            .await
            .expect("the person exists");
        let space_id = test
            .seed_space(&user_id, "Second space")
            .await
            .expect("the space exists");

        let role = test
            .app()
            .stores
            .spaces
            .membership(&space_id, &user_id)
            .await
            .expect("the store answers");
        assert_eq!(role, Some(boma_domain::SpaceRole::Owner));

        let found = test
            .app()
            .stores
            .spaces
            .find(&space_id, &user_id)
            .await
            .expect("the store answers");
        assert_eq!(found.map(|space| space.id), Some(space_id));
    }

    #[tokio::test]
    async fn req_004_a_seeded_bot_carries_its_conversation() {
        let test = TestApp::builder().build();
        let user_id = test
            .seed_user("person@example.com", "correct horse battery")
            .await
            .expect("the person exists");
        let space_id = SpaceId::new(test.ids().new_id("spc")).expect("an id is accepted");
        let (bot_id, thread_id) = test
            .seed_bot(&space_id, &user_id, "Assistant")
            .await
            .expect("the bot exists");

        let bot = test
            .app()
            .stores
            .bots
            .find(&TestApp::member_context(&space_id, &user_id), &bot_id)
            .await
            .expect("the store answers")
            .expect("the bot is found");
        assert_eq!(bot.thread_id, thread_id);
    }
}

#[tokio::test]
async fn req_024_a_due_routine_fires_into_a_routine_run() {
    let test = TestApp::builder()
        .ids(vec![
            "usr_1".to_owned(),
            "org_1".to_owned(),
            "spc_1".to_owned(),
            "bot_1".to_owned(),
            "thr_1".to_owned(),
            "rtn_1".to_owned(),
            "run_1".to_owned(),
        ])
        .fixed_clock("2026-01-01T00:02:00Z")
        .build();
    let user_id = test
        .seed_user("person@example.com", "correct horse battery")
        .await
        .expect("the person exists");
    let space = test
        .app()
        .stores
        .spaces
        .default_for(&user_id)
        .await
        .expect("the store answers")
        .expect("the space exists");
    let context = TestApp::member_context(&space.id, &user_id);

    let bot = boma_app::usecases::create_bot(test.app(), &context, "Analyst", None, None)
        .await
        .expect("the bot is created");

    // An overdue routine: its next wake already passed, so the dispatch claims it.
    let routine = test
        .app()
        .stores
        .routines
        .create(
            &context,
            boma_app::ports::NewRoutine {
                id: boma_domain::RoutineId::new("rtn_1".to_owned()).expect("the id is valid"),
                space_id: space.id.clone(),
                bot_id: bot.bot.id.clone(),
                thread_id: bot.bot.thread_id.clone(),
                user_id: user_id.clone(),
                name: "Morning digest".to_owned(),
                prompt: "Summarise overnight commits.".to_owned(),
                crons: vec!["* * * * *".to_owned()],
                timezone: "UTC".to_owned(),
                active: true,
                notify: false,
                next_run_at: Some("2026-01-01T00:01:00Z".to_owned()),
                created_at: "2026-01-01T00:00:00Z".to_owned(),
            },
        )
        .await
        .expect("the routine is created");

    let dispatched = boma_app::usecases::dispatch_routines(test.app())
        .await
        .expect("the dispatch answers");
    assert_eq!(dispatched, 1);

    let activity = test
        .app()
        .stores
        .runs
        .list(&context, Some(&bot.bot.id), None, 10)
        .await
        .expect("the activity answers");
    let run = activity
        .runs
        .into_iter()
        .next()
        .expect("the routine's run is listed")
        .run;
    assert_eq!(run.trigger, boma_domain::RunTrigger::Routine);
    assert_eq!(
        run.routine_id.map(|id| id.as_str().to_owned()).as_deref(),
        Some("rtn_1")
    );

    assert!(routine.active, "the routine is still active");
}

#[tokio::test]
async fn req_024_a_once_routine_fires_once_and_deactivates_itself() {
    let test = TestApp::builder()
        .ids(vec![
            "usr_1".to_owned(),
            "org_1".to_owned(),
            "spc_1".to_owned(),
            "bot_1".to_owned(),
            "thr_1".to_owned(),
            "rtn_1".to_owned(),
            "run_1".to_owned(),
        ])
        .fixed_clock("2026-01-01T00:02:00Z")
        .build();
    let user_id = test
        .seed_user("person@example.com", "correct horse battery")
        .await
        .expect("the person exists");
    let space = test
        .app()
        .stores
        .spaces
        .default_for(&user_id)
        .await
        .expect("the store answers")
        .expect("the space exists");
    let context = TestApp::member_context(&space.id, &user_id);

    let bot = boma_app::usecases::create_bot(test.app(), &context, "Analyst", None, None)
        .await
        .expect("the bot is created");

    test.app()
        .stores
        .routines
        .create(
            &context,
            boma_app::ports::NewRoutine {
                id: boma_domain::RoutineId::new("rtn_1".to_owned()).expect("the id is valid"),
                space_id: space.id.clone(),
                bot_id: bot.bot.id.clone(),
                thread_id: bot.bot.thread_id.clone(),
                user_id: user_id.clone(),
                name: "One shot".to_owned(),
                prompt: "Do it once.".to_owned(),
                crons: vec!["@once".to_owned()],
                timezone: "UTC".to_owned(),
                active: true,
                notify: false,
                next_run_at: Some("2026-01-01T00:01:00Z".to_owned()),
                created_at: "2026-01-01T00:00:00Z".to_owned(),
            },
        )
        .await
        .expect("the routine is created");

    let dispatched = boma_app::usecases::dispatch_routines(test.app())
        .await
        .expect("the dispatch answers");
    assert_eq!(dispatched, 1);

    let routine = test
        .app()
        .stores
        .routines
        .find(
            &context,
            &boma_domain::RoutineId::new("rtn_1".to_owned()).expect("the id is valid"),
        )
        .await
        .expect("the routine answers")
        .expect("the routine exists");
    assert!(!routine.active, "the @once routine is spent");
    assert_eq!(routine.next_run_at, None);

    // The spent one-shot is not claimed again.
    let dispatched = boma_app::usecases::dispatch_routines(test.app())
        .await
        .expect("the dispatch answers");
    assert_eq!(dispatched, 0);
}

#[tokio::test]
async fn req_024_a_coalesced_routine_records_the_skip_instead_of_stampeding() {
    let test = TestApp::builder()
        .ids(vec![
            "usr_1".to_owned(),
            "org_1".to_owned(),
            "spc_1".to_owned(),
            "bot_1".to_owned(),
            "thr_1".to_owned(),
            "rtn_1".to_owned(),
            "run_open".to_owned(),
            "run_skip".to_owned(),
        ])
        .fixed_clock("2026-01-01T00:02:00Z")
        .build();
    let user_id = test
        .seed_user("person@example.com", "correct horse battery")
        .await
        .expect("the person exists");
    let space = test
        .app()
        .stores
        .spaces
        .default_for(&user_id)
        .await
        .expect("the store answers")
        .expect("the space exists");
    let context = TestApp::member_context(&space.id, &user_id);

    let bot = boma_app::usecases::create_bot(test.app(), &context, "Analyst", None, None)
        .await
        .expect("the bot is created");

    test.app()
        .stores
        .routines
        .create(
            &context,
            boma_app::ports::NewRoutine {
                id: boma_domain::RoutineId::new("rtn_1".to_owned()).expect("the id is valid"),
                space_id: space.id.clone(),
                bot_id: bot.bot.id.clone(),
                thread_id: bot.bot.thread_id.clone(),
                user_id: user_id.clone(),
                name: "Morning digest".to_owned(),
                prompt: "Summarise overnight commits.".to_owned(),
                crons: vec!["* * * * *".to_owned()],
                timezone: "UTC".to_owned(),
                active: true,
                notify: false,
                next_run_at: Some("2026-01-01T00:01:00Z".to_owned()),
                created_at: "2026-01-01T00:00:00Z".to_owned(),
            },
        )
        .await
        .expect("the routine is created");

    // An open routine run already holds the wake; the dispatcher must not stamped a second run.
    test.app()
        .stores
        .runs
        .create(boma_app::ports::NewRun {
            id: boma_domain::RunId::new("run_open".to_owned()).expect("the id is valid"),
            space_id: space.id.clone(),
            bot_id: bot.bot.id.clone(),
            thread_id: bot.bot.thread_id.clone(),
            user_id: user_id.clone(),
            trigger: boma_domain::RunTrigger::Routine,
            client_nonce: None,
            source_message_id: None,
            created_at: "2026-01-01T00:00:00Z".to_owned(),
            model_provider: None,
            model_id: None,
            model_pinned: false,
            parent_run_id: None,
            routine_id: Some(
                boma_domain::RoutineId::new("rtn_1".to_owned()).expect("the id is valid"),
            ),
        })
        .await
        .expect("the open routine run is created");

    let dispatched = boma_app::usecases::dispatch_routines(test.app())
        .await
        .expect("the dispatch answers");
    assert_eq!(dispatched, 0, "the open run coalesces the new wake");

    let routine = test
        .app()
        .stores
        .routines
        .find(
            &context,
            &boma_domain::RoutineId::new("rtn_1".to_owned()).expect("the id is valid"),
        )
        .await
        .expect("the routine answers")
        .expect("the routine exists");
    // The coalesced wake is scheduled onward: it is not due anymore.
    assert_eq!(routine.next_run_at.as_deref(), Some("2026-01-01T00:03:00Z"));
}

#[cfg(test)]
mod routine_retry_tests {
    //! The retry-with-backoff and pause-notice behaviour of routine dispatch (REQ-024).

    use super::*;

    /// What a seeded overdue routine hands back to the tests below.
    struct SeededRoutine {
        context: TenantContext,
        routine_id: boma_domain::RoutineId,
        thread_id: boma_domain::ThreadId,
    }

    /// Seeds a member, a bot and one overdue hourly-grade routine in UTC.
    async fn seed_overdue_routine(
        test: &TestApp,
        name: &str,
        prompt: &str,
        notify: bool,
    ) -> SeededRoutine {
        let user_id = test
            .seed_user("person@example.com", "correct horse battery")
            .await
            .expect("the person exists");
        let space = test
            .app()
            .stores
            .spaces
            .default_for(&user_id)
            .await
            .expect("the store answers")
            .expect("the space exists");
        let context = TestApp::member_context(&space.id, &user_id);
        let routine_id = boma_domain::RoutineId::new("rtn_1".to_owned()).expect("the id is valid");

        let bot = boma_app::usecases::create_bot(test.app(), &context, "Analyst", None, None)
            .await
            .expect("the bot is created");
        test.app()
            .stores
            .routines
            .create(
                &context,
                boma_app::ports::NewRoutine {
                    id: routine_id.clone(),
                    space_id: space.id.clone(),
                    bot_id: bot.bot.id.clone(),
                    thread_id: bot.bot.thread_id.clone(),
                    user_id: user_id.clone(),
                    name: name.to_owned(),
                    prompt: prompt.to_owned(),
                    crons: vec!["0 7 * * *".to_owned()],
                    timezone: "UTC".to_owned(),
                    active: true,
                    notify,
                    next_run_at: Some("2025-12-31T23:59:00Z".to_owned()),
                    created_at: "2026-01-01T00:00:00Z".to_owned(),
                },
            )
            .await
            .expect("the routine is created");
        SeededRoutine {
            context,
            routine_id,
            thread_id: bot.bot.thread_id,
        }
    }

    /// Reads the newest run the space holds.
    async fn reload_latest_run(test: &TestApp, context: &TenantContext) -> boma_domain::Run {
        let page = test
            .app()
            .stores
            .runs
            .list(context, None, None, 10)
            .await
            .expect("the activity answers");
        page.runs.into_iter().next().expect("the run is listed").run
    }

    #[tokio::test]
    async fn req_024_a_retryable_failure_backs_off_before_it_counts() {
        let test = TestApp::builder()
            .ids(vec![
                "usr_1".to_owned(),
                "org_1".to_owned(),
                "spc_1".to_owned(),
                "bot_1".to_owned(),
                "thr_1".to_owned(),
                "rtn_1".to_owned(),
                "run_1".to_owned(),
            ])
            .fixed_clock("2026-01-01T00:00:00Z")
            .build();
        let seeded = seed_overdue_routine(
            &test,
            "Morning digest",
            "Summarise overnight commits.",
            false,
        )
        .await;
        let context = seeded.context;

        assert_eq!(
            boma_app::usecases::dispatch_routines(test.app())
                .await
                .expect("the dispatch answers"),
            1
        );

        // An unscripted provider fails every pass with a dependency error: retryable.
        let app = std::sync::Arc::new(test.app().clone());
        let worker = boma_app::worker::Worker::new(
            app,
            std::sync::Arc::new(crate::ScriptedProvider::default()),
            "worker-1".to_owned(),
        );

        // Attempt one fails and is retried with backoff, not counted.
        assert!(worker.step().await.expect("the worker answers"));
        let run = reload_latest_run(&test, &context).await;
        assert_eq!(run.status, boma_domain::RunStatus::Queued);
        assert_eq!(run.attempt, 2);
        assert_eq!(run.not_before.as_deref(), Some("2026-01-01T00:01:00Z"));

        // The backoff holds: the queue looks empty to a worker standing at the same instant.
        assert!(!worker.step().await.expect("the worker answers"));

        // Attempt two fails the same way; the backoff doubles.
        test.clock().advance_to("2026-01-01T00:02:00Z");
        assert!(worker.step().await.expect("the worker answers"));
        let run = reload_latest_run(&test, &context).await;
        assert_eq!(run.attempt, 3);
        assert_eq!(run.not_before.as_deref(), Some("2026-01-01T00:04:00Z"));

        // Attempt three burns the last retry the schedule owes: the failure counts.
        test.clock().advance_to("2026-01-01T00:05:00Z");
        assert!(worker.step().await.expect("the worker answers"));
        let run = reload_latest_run(&test, &context).await;
        assert_eq!(run.status, boma_domain::RunStatus::Failed);
        let routine = test
            .app()
            .stores
            .routines
            .find(&context, &seeded.routine_id)
            .await
            .expect("the store answers")
            .expect("the routine exists");
        assert_eq!(routine.account_failures, 1);
        assert!(
            routine.active,
            "one counted failure does not pause the routine"
        );
    }

    #[tokio::test]
    async fn req_024_a_pausing_routine_tells_the_owner_in_the_thread() {
        let test = TestApp::builder()
            .ids(vec![
                "usr_1".to_owned(),
                "org_1".to_owned(),
                "spc_1".to_owned(),
                "bot_1".to_owned(),
                "thr_1".to_owned(),
                "rtn_1".to_owned(),
                "run_1".to_owned(),
                "msg_1".to_owned(),
                "msg_2".to_owned(),
                "msg_3".to_owned(),
                "msg_4".to_owned(),
                "msg_5".to_owned(),
            ])
            .fixed_clock("2026-01-01T00:00:00Z")
            .build();
        let seeded =
            seed_overdue_routine(&test, "Nightly triage", "Triage the unassigned bugs.", true)
                .await;
        let context = seeded.context;

        // The dispatch fires the routine into a routine-woken run.
        assert_eq!(
            boma_app::usecases::dispatch_routines(test.app())
                .await
                .expect("the dispatch answers"),
            1
        );
        let run = reload_latest_run(&test, &context).await;

        // Five counted failures: per-run notices until the pause, then the pause notice, once.
        for _ in 0..5 {
            boma_app::usecases::record_routine_failed(test.app(), &run)
                .await
                .expect("the failure is recorded");
        }

        let routine = test
            .app()
            .stores
            .routines
            .find(&context, &seeded.routine_id)
            .await
            .expect("the store answers")
            .expect("the routine exists");
        assert!(!routine.active, "the routine paused itself");
        assert_eq!(routine.account_failures, 5);

        let page = test
            .app()
            .stores
            .messages
            .page(&context, &seeded.thread_id, None, 50)
            .await
            .expect("the thread answers");
        let notices: Vec<String> = page
            .messages
            .iter()
            .filter(|message| message.role == boma_domain::MessageRole::System)
            .flat_map(|message| message.blocks.iter())
            .filter_map(|block| match block {
                boma_domain::Block::Text { text } => Some(text.clone()),
                _ => None,
            })
            .collect();
        assert!(
            notices.iter().any(|text| text.contains("failed (1")),
            "the owner reads the failures as they land: {notices:?}"
        );
        assert_eq!(
            notices
                .iter()
                .filter(|text| text.contains("paused"))
                .count(),
            1,
            "exactly one pause notice is posted: {notices:?}"
        );
        let pause = notices
            .iter()
            .find(|text| text.contains("paused"))
            .expect("the pause notice exists");
        assert!(
            pause.contains("Nightly triage"),
            "the notice names the routine: {pause}"
        );

        // The paused routine no longer wakes.
        assert_eq!(
            boma_app::usecases::dispatch_routines(test.app())
                .await
                .expect("the dispatch answers"),
            0
        );
    }
}

#[tokio::test]
async fn req_026_a_computer_boots_lazily_and_is_reused() {
    let test = TestApp::builder()
        .ids(vec![
            "usr_1".to_owned(),
            "org_1".to_owned(),
            "spc_1".to_owned(),
            "bot_1".to_owned(),
            "thr_1".to_owned(),
            "cmp_1".to_owned(),
        ])
        .computer_answers(vec![
            crate::ComputerAnswer::Started {
                external_id: "ctr_1".to_owned(),
                home_id: "vol_1".to_owned(),
            },
            crate::ComputerAnswer::Restarted {
                external_id: "ctr_1".to_owned(),
                home_id: "vol_1".to_owned(),
            },
        ])
        .fixed_clock("2026-01-01T00:00:00Z")
        .build();
    let user_id = test
        .seed_user("person@example.com", "correct horse battery")
        .await
        .expect("the person exists");
    let space = test
        .app()
        .stores
        .spaces
        .default_for(&user_id)
        .await
        .expect("the store answers")
        .expect("the space exists");
    let context = TestApp::member_context(&space.id, &user_id);
    let bot = boma_app::usecases::create_bot(test.app(), &context, "Analyst", None, None)
        .await
        .expect("the bot is created");

    // The first ask boots the computer: the row goes running with a fresh generation.
    let outcome =
        boma_app::usecases::ensure_computer(test.app(), &context, bot.bot.id.as_str(), true)
            .await
            .expect("the computer answers");
    assert!(outcome.started);
    assert_eq!(
        outcome.computer.status,
        boma_domain::ComputerStatus::Running
    );
    assert_eq!(outcome.computer.screen_generation, 1);
    assert_eq!(outcome.computer.external_id.as_deref(), Some("ctr_1"));
    assert_eq!(outcome.computer.home_id.as_deref(), Some("vol_1"));

    // The second ask reuses it: nothing boots, nothing bumps the generation.
    let outcome =
        boma_app::usecases::ensure_computer(test.app(), &context, bot.bot.id.as_str(), true)
            .await
            .expect("the computer answers");
    assert!(!outcome.started);
    assert_eq!(outcome.computer.screen_generation, 1);
    assert_eq!(outcome.computer.external_id.as_deref(), Some("ctr_1"));

    // The stop releases it; the home survives in the provider's storage.
    let outcome = boma_app::usecases::stop_computer(test.app(), &context, bot.bot.id.as_str())
        .await
        .expect("the computer answers");
    assert_eq!(outcome.status, boma_domain::ComputerStatus::Stopped);
    assert_eq!(outcome.stopped_at.as_deref(), Some("2026-01-01T00:00:00Z"));
    assert!(outcome.started_at.is_none());

    // The store still holds the row: `stopped` with no timestamps is the honest answer.
    let outcome = boma_app::usecases::computer_status(test.app(), &context, bot.bot.id.as_str())
        .await
        .expect("the status answers");
    let computer = outcome.expect("the computer row persists");
    assert_eq!(computer.status, boma_domain::ComputerStatus::Stopped);
    assert!(computer.started_at.is_none());
    assert!(computer.stopped_at.is_some());
}

#[tokio::test]
async fn req_026_the_idle_sweeper_releases_a_computer_past_its_window() {
    let test = TestApp::builder()
        .ids(vec![
            "usr_1".to_owned(),
            "org_1".to_owned(),
            "spc_1".to_owned(),
            "bot_1".to_owned(),
            "thr_1".to_owned(),
            "cmp_1".to_owned(),
        ])
        .computer_answers(vec![crate::ComputerAnswer::Started {
            external_id: "ctr_1".to_owned(),
            home_id: "vol_1".to_owned(),
        }])
        .fixed_clock("2026-01-01T00:00:00Z")
        .build();
    let user_id = test
        .seed_user("person@example.com", "correct horse battery")
        .await
        .expect("the person exists");
    let space = test
        .app()
        .stores
        .spaces
        .default_for(&user_id)
        .await
        .expect("the store answers")
        .expect("the space exists");
    let context = TestApp::member_context(&space.id, &user_id);
    let bot = boma_app::usecases::create_bot(test.app(), &context, "Analyst", None, None)
        .await
        .expect("the bot is created");

    let outcome =
        boma_app::usecases::ensure_computer(test.app(), &context, bot.bot.id.as_str(), true)
            .await
            .expect("the computer answers");
    assert_eq!(
        outcome.computer.status,
        boma_domain::ComputerStatus::Running
    );

    // Nobody touched it for an hour; the idle window is ten minutes by default.
    let mut computer = outcome.computer;
    computer.last_used_at = Some("2025-12-31T00:00:00Z".to_owned());
    test.app()
        .stores
        .computers
        .update(&context, &computer)
        .await
        .expect("the store answers");

    let released = boma_app::usecases::sweep_idle_computers(test.app())
        .await
        .expect("the sweeper answers");
    assert_eq!(released, 1);

    let found = test
        .app()
        .stores
        .computers
        .find(&context, &bot.bot.id)
        .await
        .expect("the store answers")
        .expect("the computer exists");
    assert_eq!(found.status, boma_domain::ComputerStatus::Stopped);
    assert_eq!(found.stopped_at.as_deref(), Some("2026-01-01T00:00:00Z"));
}

#[tokio::test]
async fn req_026_a_scripted_runtime_that_cannot_deliver_answers_unavailable() {
    // No answer scripted: the ensure fails loudly instead of persisting a half-boot.
    let test = TestApp::builder()
        .ids(vec![
            "usr_1".to_owned(),
            "org_1".to_owned(),
            "spc_1".to_owned(),
            "bot_1".to_owned(),
            "thr_1".to_owned(),
            "cmp_1".to_owned(),
        ])
        .build();
    let user_id = test
        .seed_user("person@example.com", "correct horse battery")
        .await
        .expect("the person exists");
    let space = test
        .app()
        .stores
        .spaces
        .default_for(&user_id)
        .await
        .expect("the store answers")
        .expect("the space exists");
    let context = TestApp::member_context(&space.id, &user_id);
    let bot = boma_app::usecases::create_bot(test.app(), &context, "Analyst", None, None)
        .await
        .expect("the bot is created");

    let error =
        boma_app::usecases::ensure_computer(test.app(), &context, bot.bot.id.as_str(), true)
            .await
            .expect_err("the ensure must fail");
    assert!(matches!(error, boma_app::AppError::Unavailable));
}

#[tokio::test]
async fn req_031_a_team_computer_is_shared_within_a_space() {
    let test = TestApp::builder()
        .ids(vec![
            "usr_1".to_owned(),
            "org_1".to_owned(),
            "spc_1".to_owned(),
            "bot_1".to_owned(),
            "thr_1".to_owned(),
            "cmp_1".to_owned(),
            "bot_2".to_owned(),
            "thr_2".to_owned(),
            "cmp_2".to_owned(),
            "cmp_3".to_owned(),
        ])
        .computer_answers(vec![
            crate::ComputerAnswer::Started {
                external_id: "ctr_1".to_owned(),
                home_id: "vol_1".to_owned(),
            },
            crate::ComputerAnswer::Started {
                external_id: "ctr_2".to_owned(),
                home_id: "vol_2".to_owned(),
            },
            crate::ComputerAnswer::Started {
                external_id: "ctr_3".to_owned(),
                home_id: "vol_3".to_owned(),
            },
            crate::ComputerAnswer::Restarted {
                external_id: "ctr_3".to_owned(),
                home_id: "vol_3".to_owned(),
            },
        ])
        .fixed_clock("2026-01-01T00:00:00Z")
        .build();
    let user_id = test
        .seed_user("person@example.com", "correct horse battery")
        .await
        .expect("the person exists");
    let space = test
        .app()
        .stores
        .spaces
        .default_for(&user_id)
        .await
        .expect("the store answers")
        .expect("the space exists");
    let context = TestApp::member_context(&space.id, &user_id);
    let first = boma_app::usecases::create_bot(test.app(), &context, "Analyst", None, None)
        .await
        .expect("the bot is created");
    let second = boma_app::usecases::create_bot(test.app(), &context, "Analyst 2", None, None)
        .await
        .expect("the bot is created");

    // A private computer per bot: different computers, different homes.
    let private_one =
        boma_app::usecases::ensure_computer(test.app(), &context, first.bot.id.as_str(), true)
            .await
            .expect("the computer boots");
    let private_two =
        boma_app::usecases::ensure_computer(test.app(), &context, second.bot.id.as_str(), true)
            .await
            .expect("the computer boots");
    assert_ne!(private_one.computer.id, private_two.computer.id);
    assert_eq!(private_one.computer.screen_generation, 1);
    assert_eq!(private_two.computer.screen_generation, 1);

    // A team computer is shared: the second bot rides the same container and home.
    let team_one =
        boma_app::usecases::ensure_computer(test.app(), &context, first.bot.id.as_str(), false)
            .await
            .expect("the computer boots");
    let team_two =
        boma_app::usecases::ensure_computer(test.app(), &context, second.bot.id.as_str(), false)
            .await
            .expect("the team computer is reused");
    assert_eq!(team_one.computer.id, team_two.computer.id);
    assert!(team_one.started);
    assert_eq!(team_one.computer.screen_generation, 1);
    assert!(!team_two.started);
    assert_eq!(team_two.computer.screen_generation, 1);
}

#[cfg(test)]
mod secrets_tests {
    //! Secrets for bots (REQ-046): write-only storage, environment injection for shells.

    use std::sync::Arc;

    use secrecy::ExposeSecret;

    use super::TestApp;

    // REQ-046 — secrets for bots: write-only storage, environment injection for shells.

    /// The usually-expected seed ids: user, org, space, bot, thread, computer.
    fn req_046_seed_ids() -> Vec<String> {
        vec![
            "usr_1".to_owned(),
            "org_1".to_owned(),
            "spc_1".to_owned(),
            "bot_1".to_owned(),
            "thr_1".to_owned(),
            "cmp_1".to_owned(),
        ]
    }

    #[tokio::test]
    async fn req_046_a_bot_secret_reaches_the_tool_environment_but_never_a_reply() {
        let runtime = Arc::new(crate::ScriptedComputerRuntime::new(vec![
            crate::ComputerAnswer::Started {
                external_id: "ctr_1".to_owned(),
                home_id: "vol_1".to_owned(),
            },
        ]));
        let test = TestApp::builder()
            .ids(req_046_seed_ids())
            .computers_runtime(runtime.clone())
            .fixed_clock("2026-01-01T00:00:00Z")
            .build();
        let user_id = test
            .seed_user("person@example.com", "correct horse battery")
            .await
            .expect("the person exists");
        let space = test
            .app()
            .stores
            .spaces
            .default_for(&user_id)
            .await
            .expect("the store answers")
            .expect("the space exists");
        let context = TestApp::member_context(&space.id, &user_id);
        let bot = boma_app::usecases::create_bot(test.app(), &context, "Analyst", None, None)
            .await
            .expect("the bot is created");
        let bot_id = bot.bot.id.as_str().to_owned();

        boma_app::usecases::bot_secret_set(
            test.app(),
            &context,
            &bot_id,
            "OPENAI_KEY",
            secrecy::SecretString::from("s3cr3t-value".to_owned()),
        )
        .await
        .expect("the secret is stored");

        // The list answers names only, and nothing in the summary carries the value.
        let secrets = boma_app::usecases::bot_secret_list(test.app(), &context, &bot_id)
            .await
            .expect("the list answers");
        assert_eq!(secrets.len(), 1);
        assert_eq!(secrets[0].name, "OPENAI_KEY");
        assert!(!format!("{:?}", secrets[0]).contains("s3cr3t-value"));

        // A shell runs with the variable present, the value never in the transcript of the call.
        boma_app::usecases::ensure_computer(test.app(), &context, &bot_id, true)
            .await
            .expect("the computer answers");
        boma_app::usecases::computer_shell(test.app(), &context, &bot_id, "env")
            .await
            .expect("the shell answers");
        {
            let requests = runtime.requests.lock().expect("the requests are recorded");
            let env_line = requests
                .iter()
                .find(|line| line.starts_with("env="))
                .expect("the shell call recorded its environment")
                .clone();
            assert_eq!(env_line, "env=BOMA_SECRET_OPENAI_KEY");
            assert!(
                !env_line.contains("s3cr3t-value"),
                "the value never crosses in cleartext"
            );
        }

        // A forgotten secret is gone from the environment too (REQ-046).
        let removed =
            boma_app::usecases::bot_secret_forget(test.app(), &context, &bot_id, "OPENAI_KEY")
                .await
                .expect("the forget answers");
        assert!(removed);
        let secrets = boma_app::usecases::bot_secret_list(test.app(), &context, &bot_id)
            .await
            .expect("the list answers");
        assert_eq!(secrets.len(), 0, "no secret remains after the forget");
        boma_app::usecases::computer_shell(test.app(), &context, &bot_id, "env")
            .await
            .expect("the shell answers");
        {
            let requests = runtime.requests.lock().expect("the requests are recorded");
            let env_lines: Vec<&String> = requests
                .iter()
                .filter(|line| line.starts_with("env="))
                .collect();
            assert_eq!(
                env_lines.last().map(|line| line.as_str()),
                Some("env="),
                "after forgetting, the shell sees an empty environment"
            );
        }
    }

    #[tokio::test]
    async fn req_046_setting_the_same_name_twice_replaces_the_value() {
        let test = TestApp::builder()
            .ids(req_046_seed_ids())
            .fixed_clock("2026-01-01T00:00:00Z")
            .build();
        let user_id = test
            .seed_user("person@example.com", "correct horse battery")
            .await
            .expect("the person exists");
        let space = test
            .app()
            .stores
            .spaces
            .default_for(&user_id)
            .await
            .expect("the store answers")
            .expect("the space exists");
        let context = TestApp::member_context(&space.id, &user_id);
        let bot = boma_app::usecases::create_bot(test.app(), &context, "Analyst", None, None)
            .await
            .expect("the bot is created");
        let bot_id = bot.bot.id.as_str().to_owned();

        boma_app::usecases::bot_secret_set(
            test.app(),
            &context,
            &bot_id,
            "SERVICE_TOKEN",
            secrecy::SecretString::from("first-value".to_owned()),
        )
        .await
        .expect("the first value is stored");
        boma_app::usecases::bot_secret_set(
            test.app(),
            &context,
            &bot_id,
            "SERVICE_TOKEN",
            secrecy::SecretString::from("second-value".to_owned()),
        )
        .await
        .expect("the replacement is stored");

        let secrets = boma_app::usecases::bot_secret_list(test.app(), &context, &bot_id)
            .await
            .expect("the list answers");
        assert_eq!(secrets.len(), 1, "replacing a name keeps one row");

        let bot_id_ref = boma_domain::BotId::new(bot_id.clone()).expect("the id is valid");
        let value = test
            .app()
            .stores
            .secrets
            .reveal(&context, &bot_id_ref, "SERVICE_TOKEN")
            .await
            .expect("the reveal answers")
            .expect("the secret is there");
        assert_eq!(value.expose_secret(), "second-value");
    }

    #[tokio::test]
    async fn req_046_secrets_of_another_space_are_neither_writable_nor_listed() {
        let test = TestApp::builder()
            .fixed_clock("2026-01-01T00:00:00Z")
            .build();
        let first_user = test
            .seed_user("person@example.com", "correct horse battery")
            .await
            .expect("the first person exists");
        let second_user = test
            .seed_user("other@example.com", "correct horse battery")
            .await
            .expect("the second person exists");
        let first_space = test
            .app()
            .stores
            .spaces
            .default_for(&first_user)
            .await
            .expect("the store answers")
            .expect("the space exists");
        let second_space = test
            .app()
            .stores
            .spaces
            .default_for(&second_user)
            .await
            .expect("the store answers")
            .expect("the second space exists");
        let first_context = TestApp::member_context(&first_space.id, &first_user);
        let second_context = TestApp::member_context(&second_space.id, &second_user);
        let bot = boma_app::usecases::create_bot(test.app(), &first_context, "Analyst", None, None)
            .await
            .expect("the bot is created");
        let bot_id = bot.bot.id.as_str().to_owned();

        let error = boma_app::usecases::bot_secret_list(test.app(), &second_context, &bot_id)
            .await
            .expect_err("another tenant cannot list the secret names");
        assert!(matches!(error, boma_app::AppError::NotFound));
        let error = boma_app::usecases::bot_secret_set(
            test.app(),
            &second_context,
            &bot_id,
            "OPENAI_KEY",
            secrecy::SecretString::from("cross-tenant".to_owned()),
        )
        .await
        .expect_err("another tenant cannot write the secret");
        assert!(matches!(error, boma_app::AppError::NotFound));
        let error = boma_app::usecases::bot_secret_forget(
            test.app(),
            &second_context,
            &bot_id,
            "OPENAI_KEY",
        )
        .await
        .expect_err("another tenant cannot forget the secret");
        assert!(matches!(error, boma_app::AppError::NotFound));
    }

    #[tokio::test]
    async fn req_046_the_name_must_look_like_an_environment_variable() {
        let test = TestApp::builder()
            .ids(req_046_seed_ids())
            .fixed_clock("2026-01-01T00:00:00Z")
            .build();
        let user_id = test
            .seed_user("person@example.com", "correct horse battery")
            .await
            .expect("the person exists");
        let space = test
            .app()
            .stores
            .spaces
            .default_for(&user_id)
            .await
            .expect("the store answers")
            .expect("the space exists");
        let context = TestApp::member_context(&space.id, &user_id);
        let bot = boma_app::usecases::create_bot(test.app(), &context, "Analyst", None, None)
            .await
            .expect("the bot is created");
        let bot_id = bot.bot.id.as_str().to_owned();

        for bad in [
            "openai_key",
            "OpenAI",
            "1KEY",
            "OPENAI KEY",
            "OPENAI-KEY",
            "_KEY",
        ] {
            let outcome = boma_app::usecases::bot_secret_set(
                test.app(),
                &context,
                &bot_id,
                bad,
                secrecy::SecretString::from("value".to_owned()),
            )
            .await;
            assert!(
                matches!(outcome, Err(boma_app::AppError::Rule(_))),
                "{bad} is refused"
            );
        }
        let too_long = "A".repeat(65);
        let outcome = boma_app::usecases::bot_secret_set(
            test.app(),
            &context,
            &bot_id,
            &too_long,
            secrecy::SecretString::from("value".to_owned()),
        )
        .await;
        assert!(matches!(outcome, Err(boma_app::AppError::Rule(_))));

        let just_right = "A".repeat(64);
        boma_app::usecases::bot_secret_set(
            test.app(),
            &context,
            &bot_id,
            &just_right,
            secrecy::SecretString::from("value".to_owned()),
        )
        .await
        .expect("a 64-character name is stored");
    }

    #[tokio::test]
    async fn req_046_forgetting_a_missing_secret_is_a_success() {
        let test = TestApp::builder()
            .ids(req_046_seed_ids())
            .fixed_clock("2026-01-01T00:00:00Z")
            .build();
        let user_id = test
            .seed_user("person@example.com", "correct horse battery")
            .await
            .expect("the person exists");
        let space = test
            .app()
            .stores
            .spaces
            .default_for(&user_id)
            .await
            .expect("the store answers")
            .expect("the space exists");
        let context = TestApp::member_context(&space.id, &user_id);
        let bot = boma_app::usecases::create_bot(test.app(), &context, "Analyst", None, None)
            .await
            .expect("the bot is created");
        let bot_id = bot.bot.id.as_str().to_owned();

        // Forgetting something that was never stored succeeds quietly.
        let removed =
            boma_app::usecases::bot_secret_forget(test.app(), &context, &bot_id, "NEVER_STORED")
                .await
                .expect("the forget answers");
        assert!(!removed);

        boma_app::usecases::bot_secret_set(
            test.app(),
            &context,
            &bot_id,
            "TO_FORGET",
            secrecy::SecretString::from("value".to_owned()),
        )
        .await
        .expect("the secret is stored");
        let removed =
            boma_app::usecases::bot_secret_forget(test.app(), &context, &bot_id, "TO_FORGET")
                .await
                .expect("the forget answers");
        assert!(removed);
        // Deleting twice is not an error, and the second delete reports nothing there.
        let removed =
            boma_app::usecases::bot_secret_forget(test.app(), &context, &bot_id, "TO_FORGET")
                .await
                .expect("the forget answers");
        assert!(!removed);
    }
}

#[cfg(test)]
mod plugin_submission_tests {
    // REQ-034 — plugin marketplace: the submission is recorded with the canonical source.

    use super::TestApp;

    #[tokio::test]
    async fn req_034_an_anonymous_submission_is_recorded_with_the_canonical_source() {
        let test = TestApp::builder()
            .ids(vec!["sub_atomic".to_owned()])
            .fixed_clock("2026-01-01T00:00:00Z")
            .build();
        let source = boma_domain::parse_plugin_source("https://example.test/plugin.git#main")
            .expect("the source parses");
        let id = boma_app::usecases::submit_plugin(
            test.app(),
            &source,
            Some("  Example  ".to_owned()),
            Some(String::new()),
            None,
        )
        .await
        .expect("the submission is recorded");
        assert_eq!(id, "sub_atomic");
        let recorded = test.database().plugin_submissions().recorded();
        assert_eq!(recorded.len(), 1);
        let row = &recorded[0];
        assert_eq!(row.source, "https://example.test/plugin#main");
        assert_eq!(row.name.as_deref(), Some("Example"));
        assert_eq!(row.contact, None, "an empty contact is stored as absent");
        assert_eq!(
            row.submitter_user_id, None,
            "an anonymous row carries no submitter"
        );
        assert_eq!(row.status, "pending");
        assert_eq!(row.created_at, "2026-01-01T00:00:00Z");
    }

    #[tokio::test]
    async fn req_034_a_submission_with_an_oversized_field_is_capped() {
        let test = TestApp::builder()
            .ids(vec!["sub_capped".to_owned()])
            .fixed_clock("2026-01-01T00:00:00Z")
            .build();
        let source = boma_domain::parse_plugin_source("https://example.test/plugin.git")
            .expect("the source parses");
        let id = boma_app::usecases::submit_plugin(
            test.app(),
            &source,
            Some("n".repeat(500)),
            Some("c".repeat(500)),
            None,
        )
        .await
        .expect("the submission is recorded");
        assert_eq!(id, "sub_capped");
        let recorded = test.database().plugin_submissions().recorded();
        assert_eq!(recorded[0].name.as_ref().map(String::len), Some(120));
        assert_eq!(recorded[0].contact.as_ref().map(String::len), Some(320));
    }
}

#[cfg(test)]
mod skills_tests {
    // REQ-035 — skills: a taught bundle is available in a later run, and its use is visible.

    use std::sync::Arc;

    use super::TestApp;
    use crate::ScriptedProvider;

    #[tokio::test]
    #[allow(clippy::too_many_lines)] // mirrors the approval-flow tests: the scenario is the documentation
    async fn req_035_a_taught_skill_is_available_in_a_later_run() {
        use boma_app::worker::Worker;

        let test = TestApp::builder().build();
        let user_id = test
            .seed_user("person@example.com", "correct horse battery")
            .await
            .expect("the person exists");
        let space = test
            .app()
            .stores
            .spaces
            .default_for(&user_id)
            .await
            .expect("the store answers")
            .expect("the person lands in a space");
        let context = TestApp::member_context(&space.id, &user_id);
        let created = boma_app::usecases::create_bot(test.app(), &context, "Assistant", None, None)
            .await
            .expect("the bot is created");
        let bot_id = created.bot.id.clone();

        // Teach the skill directly, as the member-facing RPC does.
        boma_app::usecases::teach_skill(
            test.app(),
            &context,
            "commits",
            "How to summarise commits for a reviewer.",
            "Read the log with git log --stat; group by intent, quote the riskiest line.",
            boma_domain::SkillOrigin::Taught,
            None,
        )
        .await
        .expect("the skill is taught");

        let outcome = boma_app::usecases::send_message(
            test.app(),
            &context,
            &boma_app::usecases::SendMessage {
                bot_id: bot_id.to_string(),
                text: "Summarise the commits.".to_owned(),
                client_nonce: None,
                reply_to_message_id: None,
                reply_quote: None,
            },
        )
        .await
        .expect("the message is sent");

        let provider = Arc::new(ScriptedProvider::new_turn(vec![
            boma_app::ports::ProviderTurn::ToolCalls(vec![boma_app::ports::ToolCallRequest {
                id: "call_1".to_owned(),
                name: "skill.load".to_owned(),
                arguments: serde_json::json!({ "name": "commits" }),
            }]),
            boma_app::ports::ProviderTurn::Chunks(vec!["Done, grouped by intent.".to_owned()]),
        ]));
        let worker = Worker::new(
            Arc::new(test.app().clone()),
            provider.clone(),
            "worker-1".to_owned(),
        );
        assert!(worker.step().await.expect("the worker answers"));

        // The run's provider turn carried the built-in and the taught skill, by name.
        let requests = provider.requests();
        let names: Vec<&str> = requests[0]
            .skills
            .iter()
            .map(|skill| skill.name.as_str())
            .collect();
        assert!(
            names.contains(&"interrogate"),
            "the built-in rides along: {names:?}"
        );
        assert!(
            names.contains(&"commits"),
            "the taught skill rides along: {names:?}"
        );

        // The load is visible on the thread as a tool message with the full instructions.
        let page = test
            .app()
            .stores
            .messages
            .page(&context, &created.thread.id, None, 20)
            .await
            .expect("the page answers");
        let tool_text = page
            .messages
            .iter()
            .find(|message| message.role == boma_domain::MessageRole::Tool)
            .map(|message| {
                message
                    .blocks
                    .iter()
                    .filter_map(|block| match block {
                        boma_domain::Block::Text { text } => Some(text.clone()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .expect("a tool message carries the load");
        assert!(tool_text.contains("skill.load"), "{tool_text}");
        assert!(tool_text.contains("git log --stat"), "{tool_text}");
        assert!(tool_text.contains("\"ok\":true"), "{tool_text}");

        // And the reply still lands.
        let run = test
            .app()
            .stores
            .runs
            .find(&context, &outcome.run.id)
            .await
            .expect("the run exists")
            .expect("the run exists");
        assert_eq!(run.status, boma_domain::RunStatus::Completed);
    }

    #[tokio::test]
    async fn req_035_an_unknown_skill_is_refused_and_the_refusal_is_visible() {
        use boma_app::worker::Worker;

        let test = TestApp::builder().build();
        let user_id = test
            .seed_user("person@example.com", "correct horse battery")
            .await
            .expect("the person exists");
        let space = test
            .app()
            .stores
            .spaces
            .default_for(&user_id)
            .await
            .expect("the store answers")
            .expect("the person lands in a space");
        let context = TestApp::member_context(&space.id, &user_id);
        let created = boma_app::usecases::create_bot(test.app(), &context, "Assistant", None, None)
            .await
            .expect("the bot is created");
        let bot_id = created.bot.id.clone();

        let outcome = boma_app::usecases::send_message(
            test.app(),
            &context,
            &boma_app::usecases::SendMessage {
                bot_id: bot_id.to_string(),
                text: "Summarise the commits.".to_owned(),
                client_nonce: None,
                reply_to_message_id: None,
                reply_quote: None,
            },
        )
        .await
        .expect("the message is sent");
        let _ = outcome;

        let provider = Arc::new(ScriptedProvider::new_turn(vec![
            boma_app::ports::ProviderTurn::ToolCalls(vec![boma_app::ports::ToolCallRequest {
                id: "call_1".to_owned(),
                name: "skill.load".to_owned(),
                arguments: serde_json::json!({ "name": "does-not-exist" }),
            }]),
            boma_app::ports::ProviderTurn::Chunks(vec!["I do not know that skill.".to_owned()]),
        ]));
        let worker = Worker::new(
            Arc::new(test.app().clone()),
            provider.clone(),
            "worker-1".to_owned(),
        );
        assert!(worker.step().await.expect("the worker answers"));

        let page = test
            .app()
            .stores
            .messages
            .page(&context, &created.thread.id, None, 20)
            .await
            .expect("the page answers");
        let tool_text = page
            .messages
            .iter()
            .find(|message| message.role == boma_domain::MessageRole::Tool)
            .map(|message| {
                message
                    .blocks
                    .iter()
                    .filter_map(|block| match block {
                        boma_domain::Block::Text { text } => Some(text.clone()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .expect("a tool message carries the refusal");
        assert!(tool_text.contains("\"ok\":false"), "{tool_text}");
        assert!(tool_text.contains("does-not-exist"), "{tool_text}");
    }

    #[tokio::test]
    async fn req_035_a_skill_taught_by_the_agent_waits_for_approval_then_lands() {
        use boma_app::worker::Worker;

        let test = TestApp::builder().build();
        let user_id = test
            .seed_user("person@example.com", "correct horse battery")
            .await
            .expect("the person exists");
        let space = test
            .app()
            .stores
            .spaces
            .default_for(&user_id)
            .await
            .expect("the store answers")
            .expect("the person lands in a space");
        let context = TestApp::member_context(&space.id, &user_id);
        let created = boma_app::usecases::create_bot(test.app(), &context, "Assistant", None, None)
            .await
            .expect("the bot is created");
        let bot_id = created.bot.id.clone();

        let outcome = boma_app::usecases::send_message(
            test.app(),
            &context,
            &boma_app::usecases::SendMessage {
                bot_id: bot_id.to_string(),
                text: "Learn how I want releases summarised.".to_owned(),
                client_nonce: None,
                reply_to_message_id: None,
                reply_quote: None,
            },
        )
        .await
        .expect("the message is sent");
        let _ = outcome;

        let provider = Arc::new(ScriptedProvider::new_turn(vec![
            boma_app::ports::ProviderTurn::ToolCalls(vec![boma_app::ports::ToolCallRequest {
                id: "call_1".to_owned(),
                name: "skill.teach".to_owned(),
                arguments: serde_json::json!({
                    "name": "release-notes",
                    "description": "How this space writes release notes.",
                    "instructions": "Lead with the user-visible change; one bullet per fix.",
                }),
            }]),
            boma_app::ports::ProviderTurn::Chunks(vec!["Learned.".to_owned()]),
        ]));
        let worker = Worker::new(
            Arc::new(test.app().clone()),
            provider.clone(),
            "worker-1".to_owned(),
        );
        assert!(worker.step().await.expect("the worker answers"));

        // The teaching parked on an approval card: no skill yet.
        let card_id = test
            .app()
            .stores
            .messages
            .page(&context, &created.thread.id, None, 20)
            .await
            .expect("the page answers")
            .messages
            .into_iter()
            .flat_map(|message| message.blocks.into_iter())
            .find_map(|block| match block {
                boma_domain::Block::Card { card_id, .. } => Some(card_id),
                _ => None,
            })
            .expect("an approval card is part of the thread");
        assert!(
            teaches(&test, &context, "release-notes").await.is_none(),
            "the skill does not exist before the approval"
        );

        let answered = boma_app::usecases::answer_card(test.app(), &context, &card_id, "approve")
            .await
            .expect("the card is answered");
        assert!(answered.effect_recorded);
        assert!(
            teaches(&test, &context, "release-notes").await.is_some(),
            "the skill lands once approved"
        );

        // The run resumes and completes.
        assert!(worker.step().await.expect("the worker answers"));
    }

    /// Whether the named space skill exists as the member sees it.
    #[allow(clippy::expect_used)] // test helper: a failure to list is a broken deployment, not a runtime path
    async fn teaches(
        test: &TestApp,
        context: &boma_domain::TenantContext,
        name: &str,
    ) -> Option<boma_app::ports::SkillSummary> {
        boma_app::usecases::skills_list(test.app(), context)
            .await
            .expect("the list answers")
            .into_iter()
            .find(|skill| skill.name == name)
    }
}

#[cfg(test)]
mod mcp_tests {
    // REQ-033 — MCP servers: registry, discovery, per-space private allow-list (D-05).

    use std::sync::Arc;

    use super::TestApp;
    use crate::ScriptedMcpTransport;

    async fn seeded() -> (
        TestApp,
        boma_domain::UserId,
        boma_domain::TenantContext,
        boma_app::usecases::BotCreated,
    ) {
        let test = TestApp::builder().build();
        let user_id = test
            .seed_user("person@example.com", "correct horse battery")
            .await
            .expect("the person exists");
        let space = test
            .app()
            .stores
            .spaces
            .default_for(&user_id)
            .await
            .expect("the store answers")
            .expect("the person lands in a space");
        let context = TestApp::member_context(&space.id, &user_id);
        let created = boma_app::usecases::create_bot(test.app(), &context, "Assistant", None, None)
            .await
            .expect("the bot is created");
        (test, user_id, context, created)
    }

    #[tokio::test]
    async fn req_033_a_member_registers_a_remote_server_and_manages_it() {
        let (test, _user_id, context, _created) = seeded().await;

        let summary = boma_app::usecases::mcp_server_add(
            test.app(),
            &context,
            "company-files",
            boma_domain::McpTransport::Http,
            Some("https://mcp.example.com/mcp".to_owned()),
            None,
            Some(secrecy::SecretString::from("token-1".to_owned())),
            false,
            None,
        )
        .await
        .expect("the server is registered");
        assert_eq!(summary.transport, "http");
        assert_eq!(summary.auth_state, "none");
        assert!(!summary.allow_private);
        let server_id = summary.id.clone();

        let servers = boma_app::usecases::mcp_servers_list(test.app(), &context)
            .await
            .expect("the list answers");
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].name, "company-files");
        assert!(
            !format!("{servers:?}").contains("token-1"),
            "no credential leaks into the list"
        );

        // The bearer is sealed like every secret: revealed only for the wire call.
        let revealed = test
            .app()
            .stores
            .mcp_servers
            .reveal_bearer(&context, &server_id)
            .await
            .expect("the reveal answers")
            .expect("the token is there");
        assert_eq!(secrecy::ExposeSecret::expose_secret(&revealed), "token-1");

        let removed = boma_app::usecases::mcp_server_remove(test.app(), &context, &server_id)
            .await
            .expect("the remove answers");
        assert!(removed);
        let listed = boma_app::usecases::mcp_servers_list(test.app(), &context)
            .await
            .expect("the list answers");
        assert_eq!(listed.len(), 0);
        // Removing twice is a success.
        assert!(
            !boma_app::usecases::mcp_server_remove(test.app(), &context, &server_id)
                .await
                .expect("the remove answers")
        );
    }

    #[tokio::test]
    async fn req_033_a_stdio_server_is_refused_by_default() {
        let (test, _user_id, context, _created) = seeded().await;

        let outcome = boma_app::usecases::mcp_server_add(
            test.app(),
            &context,
            "local-things",
            boma_domain::McpTransport::Stdio,
            None,
            Some("/usr/bin/python3 -m mcp_server".to_owned()),
            None,
            false,
            None,
        )
        .await;
        let Err(error) = outcome else {
            panic!("a stdio server must be refused by default");
        };
        assert!(
            error.to_string().contains("local command servers"),
            "{error}"
        );
    }

    #[tokio::test]
    async fn req_033_a_private_endpoint_needs_the_allow_list_and_writes_an_audit_event() {
        let (test, _user_id, context, _created) = seeded().await;

        // Refused without it: the URL resolves nowhere, but the refusal comes from the shape check.
        let refused = boma_app::usecases::mcp_server_add(
            test.app(),
            &context,
            "lan-server",
            boma_domain::McpTransport::Http,
            Some("http://192.168.1.10:8080/mcp".to_owned()),
            None,
            None,
            false,
            None,
        )
        .await;
        assert!(
            refused.is_err(),
            "plain HTTP without the allow-list is refused"
        );

        // Allowed with it, and every probe audits.
        let summary = boma_app::usecases::mcp_server_add(
            test.app(),
            &context,
            "lan-server",
            boma_domain::McpTransport::Http,
            Some("http://192.168.1.10:8080/mcp".to_owned()),
            None,
            None,
            true,
            None,
        )
        .await
        .expect("the allow-list releases the shape");
        assert!(summary.allow_private);

        let transport = Arc::new(ScriptedMcpTransport::new());
        // Re-wire the transport for this test's app? No: probe uses the app's own transport, which is scripted-empty here.
        let _ = transport;
        let probed = boma_app::usecases::mcp_server_probe(test.app(), &context, &summary.id).await;
        assert!(
            probed.is_ok(),
            "the probe answers with the scripted transport"
        );

        let events = test.database().audit().events();
        assert!(
            events
                .iter()
                .any(|event| event.kind == "mcp.private_call" && event.target == "lan-server"),
            "every probe of a private endpoint writes an audit event: {events:?}"
        );
    }

    #[tokio::test]
    async fn req_033_a_probe_discovers_tools_and_they_join_the_catalogue() {
        let test = TestApp::builder()
            .mcp_transport(Arc::new(ScriptedMcpTransport::with_tools_and_results(
                vec![
                    boma_app::ports::McpTool {
                        name: "list-issues".to_owned(),
                        description: "Lists the open issues.".to_owned(),
                        input_schema: serde_json::json!({ "type": "object" }),
                    },
                    boma_app::ports::McpTool {
                        name: "../unsafe".to_owned(),
                        description: "Must not appear anywhere.".to_owned(),
                        input_schema: serde_json::json!({ "type": "object" }),
                    },
                ],
                vec![serde_json::json!({ "issues": ["A", "B"] })],
            )))
            .build();
        let user_id = test
            .seed_user("person@example.com", "correct horse battery")
            .await
            .expect("the person exists");
        let space = test
            .app()
            .stores
            .spaces
            .default_for(&user_id)
            .await
            .expect("the store answers")
            .expect("the person lands in a space");
        let context = TestApp::member_context(&space.id, &user_id);
        let created = boma_app::usecases::create_bot(test.app(), &context, "Assistant", None, None)
            .await
            .expect("the bot is created");
        let bot_id = created.bot.id.clone();

        let summary = boma_app::usecases::mcp_server_add(
            test.app(),
            &context,
            "example",
            boma_domain::McpTransport::Http,
            Some("https://mcp.example.com/mcp".to_owned()),
            None,
            None,
            false,
            None,
        )
        .await
        .expect("the server is registered");

        let probed = boma_app::usecases::mcp_server_probe(test.app(), &context, &summary.id)
            .await
            .expect("the probe discovers the tools");
        assert_eq!(
            probed.tool_count,
            Some(1),
            "the unsafe tool was skipped: {probed:?}"
        );
        assert_eq!(probed.auth_state, "connected");

        let catalogue = boma_app::usecases::tool_catalogue(test.app(), &context, bot_id.as_str())
            .await
            .expect("the catalogue answers");
        let keys: Vec<&str> = catalogue.iter().map(|tool| tool.key.as_str()).collect();
        assert!(keys.contains(&"mcp.example.list-issues"), "{keys:?}");
        assert!(!keys.iter().any(|key| key.contains("unsafe")), "{keys:?}");
    }

    #[tokio::test]
    #[allow(clippy::too_many_lines)] // mirrors the approval-flow tests: the scenario is the documentation
    async fn req_033_an_approved_mcp_call_runs_exactly_like_an_approved_write() {
        use boma_app::worker::Worker;

        let test = TestApp::builder()
            .mcp_transport(Arc::new(ScriptedMcpTransport::with_tools_and_results(
                vec![boma_app::ports::McpTool {
                    name: "list-issues".to_owned(),
                    description: "Lists the open issues.".to_owned(),
                    input_schema: serde_json::json!({ "type": "object" }),
                }],
                vec![serde_json::json!({ "issues": ["A"] })],
            )))
            .build();
        let user_id = test
            .seed_user("person@example.com", "correct horse battery")
            .await
            .expect("the person exists");
        let space = test
            .app()
            .stores
            .spaces
            .default_for(&user_id)
            .await
            .expect("the store answers")
            .expect("the person lands in a space");
        let context = TestApp::member_context(&space.id, &user_id);
        let created = boma_app::usecases::create_bot(test.app(), &context, "Assistant", None, None)
            .await
            .expect("the bot is created");
        let bot_id = created.bot.id.clone();

        let summary = boma_app::usecases::mcp_server_add(
            test.app(),
            &context,
            "example",
            boma_domain::McpTransport::Http,
            Some("https://mcp.example.com/mcp".to_owned()),
            None,
            None,
            false,
            None,
        )
        .await
        .expect("the server is registered");
        boma_app::usecases::mcp_server_probe(test.app(), &context, &summary.id)
            .await
            .expect("the probe discovers the tools");

        let outcome = boma_app::usecases::send_message(
            test.app(),
            &context,
            &boma_app::usecases::SendMessage {
                bot_id: bot_id.to_string(),
                text: "What are the open issues?".to_owned(),
                client_nonce: None,
                reply_to_message_id: None,
                reply_quote: None,
            },
        )
        .await
        .expect("the message is sent");

        let provider = Arc::new(crate::ScriptedProvider::new_turn(vec![
            boma_app::ports::ProviderTurn::ToolCalls(vec![boma_app::ports::ToolCallRequest {
                id: "call_1".to_owned(),
                name: "mcp.example.list-issues".to_owned(),
                arguments: serde_json::json!({ "state": "open" }),
            }]),
            boma_app::ports::ProviderTurn::Chunks(vec!["One issue: A.".to_owned()]),
        ]));
        let worker = Worker::new(
            Arc::new(test.app().clone()),
            provider.clone(),
            "worker-1".to_owned(),
        );
        assert!(worker.step().await.expect("the worker answers"));

        // Write-class: the call parked on an approval card; the server was not called yet.
        let page = test
            .app()
            .stores
            .messages
            .page(&context, &created.thread.id, None, 20)
            .await
            .expect("the page answers");
        let card_id = page
            .messages
            .iter()
            .flat_map(|message| message.blocks.iter())
            .find_map(|block| match block {
                boma_domain::Block::Card { card_id, .. } => Some(card_id.clone()),
                _ => None,
            })
            .expect("an approval card is part of the thread");
        let parked = test
            .app()
            .stores
            .runs
            .find(&context, &outcome.run.id)
            .await
            .expect("the store answers")
            .expect("the run exists");
        assert_eq!(parked.status, boma_domain::RunStatus::WaitingInput);

        boma_app::usecases::answer_card(test.app(), &context, card_id.as_str(), "approve")
            .await
            .expect("the card is answered");

        let run = test
            .app()
            .stores
            .runs
            .find(&context, &outcome.run.id)
            .await
            .expect("the store answers")
            .expect("the run exists");
        assert_eq!(run.status, boma_domain::RunStatus::Queued);

        // The resumed run calls the server exactly once.
        assert!(worker.step().await.expect("the worker answers"));

        let page = test
            .app()
            .stores
            .messages
            .page(&context, &created.thread.id, None, 20)
            .await
            .expect("the page answers");
        let tool_text = page
            .messages
            .iter()
            .filter(|message| message.role == boma_domain::MessageRole::Tool)
            .map(|message| {
                message
                    .blocks
                    .iter()
                    .filter_map(|block| match block {
                        boma_domain::Block::Text { text } => Some(text.clone()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(tool_text.contains("list-issues"), "{tool_text}");
        assert!(tool_text.contains("\"A\""), "{tool_text}");
    }
}

#[cfg(test)]
mod connector_tests {
    // REQ-032 — connectors: install, expose, refuse after revoke.

    use std::sync::Arc;

    use super::TestApp;

    async fn seeded() -> (
        TestApp,
        boma_domain::UserId,
        boma_domain::TenantContext,
        boma_app::usecases::BotCreated,
    ) {
        let test = TestApp::builder().build();
        let user_id = test
            .seed_user("person@example.com", "correct horse battery")
            .await
            .expect("the person exists");
        let space = test
            .app()
            .stores
            .spaces
            .default_for(&user_id)
            .await
            .expect("the store answers")
            .expect("the person lands in a space");
        let context = TestApp::member_context(&space.id, &user_id);
        let created = boma_app::usecases::create_bot(test.app(), &context, "Assistant", None, None)
            .await
            .expect("the bot is created");
        (test, user_id, context, created)
    }

    #[tokio::test]
    async fn req_032_installing_a_connector_stores_its_credential_and_exposes_its_tool() {
        let (test, _user_id, context, created) = seeded().await;
        let bot_id = created.bot.id.clone();

        // An api-key connector refuses to install without its key.
        let refused =
            boma_app::usecases::connections_install(test.app(), &context, "http-json", None).await;
        assert!(
            refused.is_err(),
            "an api-key connector without a key is refused"
        );

        let installed = boma_app::usecases::connections_install(
            test.app(),
            &context,
            "http-json",
            Some(secrecy::SecretString::from("key-1".to_owned())),
        )
        .await
        .expect("the install lands");
        assert_eq!(installed.connector_id, "http-json");
        assert!(installed.installed);

        // The list carries no credential, only state.
        let list = boma_app::usecases::connections_list(test.app(), &context)
            .await
            .expect("the list answers");
        assert_eq!(list.len(), 2);
        let http_json = list
            .iter()
            .find(|entry| entry.connector_id == "http-json")
            .expect("the connector is listed");
        assert!(http_json.installed);
        assert_eq!(http_json.auth_kind, "api_key");
        assert!(
            !format!("{list:?}").contains("key-1"),
            "no credential leaks into the list"
        );

        // The tool appears in the bot's catalogue.
        let catalogue = boma_app::usecases::tool_catalogue(test.app(), &context, bot_id.as_str())
            .await
            .expect("the catalogue answers");
        let keys: Vec<&str> = catalogue.iter().map(|tool| tool.key.as_str()).collect();
        assert!(keys.contains(&"connector.http-json.get"), "{keys:?}");

        // The stored row is sealed like every secret.
        let revealed = test
            .app()
            .stores
            .connectors
            .connected_credential(&context, "http-json")
            .await
            .expect("the reveal answers")
            .expect("the credential is there");
        assert_eq!(secrecy::ExposeSecret::expose_secret(&revealed), "key-1");
    }

    #[tokio::test]
    async fn req_032_a_bot_with_a_grant_excludes_the_connector_tool() {
        let (test, _user_id, context, created) = seeded().await;
        let bot_id = created.bot.id.clone();

        boma_app::usecases::connections_install(test.app(), &context, "rss", None)
            .await
            .expect("the install lands");

        // An empty grant sees everything; this grant does not.
        test.database()
            .seed_bot_grant(&bot_id, vec!["memory.read".to_owned()]);
        let catalogue = boma_app::usecases::tool_catalogue(test.app(), &context, bot_id.as_str())
            .await
            .expect("the catalogue answers");
        let keys: Vec<&str> = catalogue.iter().map(|tool| tool.key.as_str()).collect();
        assert!(!keys.contains(&"connector.rss.read"), "{keys:?}");
        assert!(keys.contains(&"memory.read"), "{keys:?}");

        // An empty grant sees the connector again.
        test.database().seed_bot_grant(&bot_id, Vec::new());
        let catalogue = boma_app::usecases::tool_catalogue(test.app(), &context, bot_id.as_str())
            .await
            .expect("the catalogue answers");
        let keys: Vec<&str> = catalogue.iter().map(|tool| tool.key.as_str()).collect();
        assert!(keys.contains(&"connector.rss.read"), "{keys:?}");
    }

    #[tokio::test]
    async fn req_032_a_removed_connector_refuses_the_tool_call() {
        let (test, _user_id, context, created) = seeded().await;
        let bot_id = created.bot.id.clone();

        boma_app::usecases::connections_install(test.app(), &context, "rss", None)
            .await
            .expect("the install lands");
        boma_app::usecases::connections_remove(test.app(), &context, "rss")
            .await
            .expect("the remove answers");

        let catalogue = boma_app::usecases::tool_catalogue(test.app(), &context, bot_id.as_str())
            .await
            .expect("the catalogue answers");
        assert!(
            !catalogue
                .iter()
                .any(|tool| tool.key == "connector.rss.read")
        );

        // Even a direct call is refused, without executing anything.
        let call = boma_app::ports::ToolCallRequest {
            id: "call_1".to_owned(),
            name: "connector.rss.read".to_owned(),
            arguments: serde_json::json!({ "url": "https://example.test/feed" }),
        };
        let result =
            boma_app::usecases::dispatch_tool(test.app(), &context, bot_id.as_str(), &call)
                .await
                .expect("the dispatch answers");
        assert!(
            !result.ok,
            "the revoked connector cannot run: {:?}",
            result.body
        );
        assert_eq!(
            result.body["error"],
            serde_json::json!("the bot may not use this tool")
        );
    }

    #[tokio::test]
    async fn req_032_the_credential_reaches_the_runner_but_never_the_reply() {
        let runner = Arc::new(crate::ScriptedConnectorRunner::new());
        let test = TestApp::builder().connector_runner(runner.clone()).build();
        let user_id = test
            .seed_user("person@example.com", "correct horse battery")
            .await
            .expect("the person exists");
        let space = test
            .app()
            .stores
            .spaces
            .default_for(&user_id)
            .await
            .expect("the store answers")
            .expect("the person lands in a space");
        let context = TestApp::member_context(&space.id, &user_id);
        let created = boma_app::usecases::create_bot(test.app(), &context, "Assistant", None, None)
            .await
            .expect("the bot is created");

        boma_app::usecases::connections_install(
            test.app(),
            &context,
            "http-json",
            Some(secrecy::SecretString::from("key-1".to_owned())),
        )
        .await
        .expect("the install lands");

        let call = boma_app::ports::ToolCallRequest {
            id: "call_1".to_owned(),
            name: "connector.http-json.get".to_owned(),
            arguments: serde_json::json!({ "url": "https://api.example.test/data" }),
        };
        let result =
            boma_app::usecases::dispatch_tool(test.app(), &context, created.bot.id.as_str(), &call)
                .await
                .expect("the dispatch answers");
        assert!(result.ok, "{:?}", result.body);
        let calls = runner.calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "http-json");
        assert_eq!(calls[0].1, "get");
        assert!(calls[0].3, "the credential rode the call");
        assert!(
            !serde_json::to_string(&result.body)
                .expect("the body serialises")
                .contains("key-1"),
            "the reply never carries the credential"
        );
    }

    #[tokio::test]
    async fn req_032_the_connector_runner_records_what_it_was_asked_for() {
        let (test, _user_id, context, created) = seeded().await;
        let bot_id = created.bot.id.clone();

        boma_app::usecases::connections_install(test.app(), &context, "rss", None)
            .await
            .expect("the install lands");

        let call = boma_app::ports::ToolCallRequest {
            id: "call_1".to_owned(),
            name: "connector.rss.read".to_owned(),
            arguments: serde_json::json!({ "url": "https://example.test/feed" }),
        };
        let result =
            boma_app::usecases::dispatch_tool(test.app(), &context, bot_id.as_str(), &call)
                .await
                .expect("the dispatch answers");
        assert!(result.ok, "{:?}", result.body);
        assert_eq!(result.body["result"], serde_json::json!({}));
    }
}

#[cfg(test)]
mod marketplace_tests {
    // REQ-034 — plugins: install, no-local-code rule, omission naming.

    use std::sync::Arc;

    use super::TestApp;
    use crate::ScriptedPluginFetcher;

    async fn seeded(
        fetcher: ScriptedPluginFetcher,
    ) -> (
        TestApp,
        boma_domain::TenantContext,
        boma_app::usecases::BotCreated,
    ) {
        let test = TestApp::builder().plugin_fetcher(Arc::new(fetcher)).build();
        let user_id = test
            .seed_user("person@example.com", "correct horse battery")
            .await
            .expect("the person exists");
        let space = test
            .app()
            .stores
            .spaces
            .default_for(&user_id)
            .await
            .expect("the store answers")
            .expect("the person lands in a space");
        let context = TestApp::member_context(&space.id, &user_id);
        let created = boma_app::usecases::create_bot(test.app(), &context, "Assistant", None, None)
            .await
            .expect("the bot is created");
        (test, context, created)
    }

    fn plugin_files() -> Vec<boma_app::ports::PluginRepoFile> {
        vec![
            boma_app::ports::PluginRepoFile {
                path: "plugin.json".to_owned(),
                content: br#"{"name":"widget-pack","description":"A pack of widgets."}"#.to_vec(),
            },
            boma_app::ports::PluginRepoFile {
                path: "skills/alpha/SKILL.md".to_owned(),
                content: b"When asked about widgets, answer in bullet points.\n".to_vec(),
            },
            boma_app::ports::PluginRepoFile {
                path: "skills/alpha/script.sh".to_owned(),
                content: b"#!/bin/sh\necho nope\n".to_vec(),
            },
            boma_app::ports::PluginRepoFile {
                path: "skills/interrogate/SKILL.md".to_owned(),
                content: b"Shadow a built-in: must be refused.\n".to_vec(),
            },
            boma_app::ports::PluginRepoFile {
                path: "skills/broken/SKILL.md".to_owned(),
                content: b"   \n".to_vec(),
            },
            boma_app::ports::PluginRepoFile {
                path: ".mcp.json".to_owned(),
                content: br#"{"mcpServers":{"widgets":{"type":"streamable_http","url":"https://mcp.example.com/widgets"},"local":{"type":"stdio","command":"/opt/evil"}}}"#.to_vec(),
            },
            boma_app::ports::PluginRepoFile {
                path: "README.md".to_owned(),
                content: b"# widget-pack\n".to_vec(),
            },
            boma_app::ports::PluginRepoFile {
                path: "rules/ruleset.yaml".to_owned(),
                content: b"stuff: true\n".to_vec(),
            },
        ]
    }

    fn plugin_fixture() -> Vec<boma_app::ports::PluginRepoFile> {
        plugin_files()
    }

    #[tokio::test]
    async fn req_034_an_install_records_the_skills_and_names_the_omissions() {
        let (test, context, _created) = seeded(ScriptedPluginFetcher::with_responses(vec![Ok(
            boma_app::ports::FetchedPlugin {
                files: plugin_files(),
                resolved_sha: Some("deadbeefdeadbeefdeadbeefdeadbeefdeadbeef".to_owned()),
            },
        )]))
        .await;

        let outcome = boma_app::usecases::marketplace_install(
            test.app(),
            &context,
            "https://example.test/acme/widget-pack.git#main",
        )
        .await
        .expect("the install lands");

        assert_eq!(outcome.name, "widget-pack");
        assert_eq!(outcome.skill_count, 1, "exactly one skill was installed");
        assert_eq!(
            outcome.mcp_server_count, 1,
            "exactly one MCP server was registered"
        );

        let kinds: Vec<&str> = outcome
            .skipped
            .iter()
            .map(|entry| entry["kind"].as_str().expect("a kind"))
            .collect();
        for expected in [
            "skill_non_text",
            "name_taken",
            "skill_invalid",
            "mcp_stdio",
            "editor_only",
        ] {
            assert!(
                kinds.contains(&expected),
                "{expected} named in the omissions: {kinds:?}"
            );
        }

        // The installed skill loads, named with its plugin origin.
        let skill = boma_app::usecases::skill_get(test.app(), &context, "alpha")
            .await
            .expect("the store answers");
        assert_eq!(skill.origin, "plugin");
        assert!(skill.instructions.contains("widgets"));

        // The MCP server is registered without a credential and without private access.
        let servers = boma_app::usecases::mcp_servers_list(test.app(), &context)
            .await
            .expect("the list answers");
        let widgets = servers
            .iter()
            .find(|server| server.name == "widgets")
            .expect("the plugin's server is registered");
        assert!(!widgets.allow_private);

        // The install is listed.
        let (_connectors, installed) = boma_app::usecases::marketplace_list(test.app(), &context)
            .await
            .expect("the list answers");
        assert_eq!(installed.len(), 1);
        assert_eq!(installed[0].name, "widget-pack");
        assert_eq!(
            installed[0].resolved_sha.as_deref(),
            Some("deadbeefdeadbeefdeadbeefdeadbeefdeadbeef")
        );
        assert_eq!(installed[0].skipped.as_array().expect("an array").len(), 5);
    }

    #[tokio::test]
    async fn req_034_a_malformed_source_nothing_is_installed() {
        let fetcher = ScriptedPluginFetcher::new();
        let (test, context, _created) = seeded(fetcher).await;
        let outcome =
            boma_app::usecases::marketplace_install(test.app(), &context, "not a repository").await;
        assert!(outcome.is_err());
        let installed = test.database().plugin_installs().recorded();
        assert!(
            installed.is_empty(),
            "nothing is installed from a malformed source"
        );
    }

    #[tokio::test]
    async fn req_034_a_plugin_without_a_manifest_installs_nothing() {
        let fixture = vec![boma_app::ports::PluginRepoFile {
            path: "skills/alpha/SKILL.md".to_owned(),
            content: b"instructions\n".to_vec(),
        }];
        let (test, context, _created) = seeded(ScriptedPluginFetcher::with_responses(vec![Ok(
            boma_app::ports::FetchedPlugin {
                files: fixture,
                resolved_sha: None,
            },
        )]))
        .await;
        let outcome = boma_app::usecases::marketplace_install(
            test.app(),
            &context,
            "https://example.test/acme/pack.git#main",
        )
        .await;
        assert!(outcome.is_err());
        assert_eq!(test.database().plugin_installs().recorded().len(), 0);
        assert!(
            test.database()
                .skills()
                .recorded()
                .into_iter()
                .all(|skill| skill.origin != "plugin")
        );
    }

    #[tokio::test]
    async fn req_034_a_reinstall_replaces_what_the_plugin_owns() {
        let install = |sha: &str| {
            vec![Ok(boma_app::ports::FetchedPlugin {
                files: plugin_fixture(),
                resolved_sha: Some(sha.to_owned()),
            })]
        };
        let (test, context, _created) = seeded(ScriptedPluginFetcher::with_responses(
            install("aaaabbbbccccddddeeeeffff0000111122223333")
                .into_iter()
                .chain(install("ffffffffeeeeeeeedddddddccccccccbbbbbbbbbbbb"))
                .collect::<Vec<_>>(),
        ))
        .await;

        boma_app::usecases::marketplace_install(
            test.app(),
            &context,
            "https://example.test/acme/widget-pack.git#main",
        )
        .await
        .expect("the first install lands");
        boma_app::usecases::marketplace_install(
            test.app(),
            &context,
            "https://example.test/acme/widget-pack.git#main",
        )
        .await
        .expect("the reinstall lands");

        let installed = test.database().plugin_installs().recorded();
        assert_eq!(installed.len(), 1, "one row per (space, source)");
        assert_eq!(
            installed[0].resolved_sha.as_deref(),
            Some("ffffffffeeeeeeeedddddddccccccccbbbbbbbbbbbb"),
        );
        let alpha = test
            .database()
            .skills()
            .recorded()
            .into_iter()
            .filter(|skill| skill.name == "alpha" && skill.origin == "plugin")
            .count();
        assert_eq!(alpha, 1, "the reinstall replaces the plugin's own skills");
    }
}

#[cfg(test)]
mod oauth_tests {
    // REQ-033 — MCP OAuth: start, complete, state consumed exactly once, bearer sealed.

    use std::sync::Arc;

    use super::TestApp;
    use crate::{ScriptedMcpTransport, ScriptedOAuthClient};

    async fn seeded(
        transport: ScriptedMcpTransport,
        oauth: Arc<ScriptedOAuthClient>,
    ) -> (TestApp, boma_domain::TenantContext, String) {
        let test = TestApp::builder()
            .mcp_transport(Arc::new(transport))
            .oauth_client(oauth)
            .build();
        let user_id = test
            .seed_user("person@example.com", "correct horse battery")
            .await
            .expect("the person exists");
        let space = test
            .app()
            .stores
            .spaces
            .default_for(&user_id)
            .await
            .expect("the store answers")
            .expect("the person lands in a space");
        let context = TestApp::member_context(&space.id, &user_id);
        let summary = boma_app::usecases::mcp_server_add(
            test.app(),
            &context,
            "analytics",
            boma_domain::McpTransport::Http,
            Some("https://mcp.example.test/rpc".to_owned()),
            None,
            None,
            false,
            Some(boma_app::ports::McpOAuthConfig {
                authorization_url: "https://auth.example.test/authorize".to_owned(),
                token_url: "https://auth.example.test/token".to_owned(),
                client_id: "boma-cli".to_owned(),
                client_secret: Some(secrecy::SecretString::from("shh".to_owned())),
            }),
        )
        .await
        .expect("the server is added");
        (test, context, summary.id)
    }

    #[tokio::test]
    async fn req_033_start_returns_a_complete_authorization_url() {
        let oauth = Arc::new(ScriptedOAuthClient::new());
        let (test, context, server_id) =
            seeded(ScriptedMcpTransport::new(), Arc::clone(&oauth)).await;

        let started = boma_app::usecases::mcp_oauth_start(test.app(), &context, &server_id)
            .await
            .expect("the sign-in begins");
        assert!(
            started
                .authorization_url
                .starts_with("https://auth.example.test/authorize?"),
            "{}",
            started.authorization_url
        );
        for expected in [
            "response_type=code",
            "client_id=boma-cli",
            "redirect_uri=http%3A%2F%2F127.0.0.1%2Fboma-mcp%2Fcallback",
            "code_challenge=challenge-1",
            "code_challenge_method=S256",
        ] {
            assert!(
                started.authorization_url.contains(expected),
                "{expected} in {}",
                started.authorization_url
            );
        }
        assert!(
            started
                .authorization_url
                .contains(&format!("state={}", started.state))
        );
    }

    #[tokio::test]
    async fn req_033_complete_exchanges_seals_and_discovers_the_tools() {
        let tools = vec![boma_app::ports::McpTool {
            name: "query".to_owned(),
            description: "Query the warehouse.".to_owned(),
            input_schema: serde_json::json!({ "type": "object", "properties": {} }),
        }];
        let oauth = Arc::new(ScriptedOAuthClient::new().with_exchanges(vec![Ok(
            serde_json::json!({ "access_token": "token-1", "token_type": "bearer" }),
        )]));
        let (test, context, server_id) =
            seeded(ScriptedMcpTransport::with_tools(tools), Arc::clone(&oauth)).await;

        let started = boma_app::usecases::mcp_oauth_start(test.app(), &context, &server_id)
            .await
            .expect("the sign-in begins");
        let summary = boma_app::usecases::mcp_oauth_complete(
            test.app(),
            &context,
            &server_id,
            "code-1",
            &started.state,
        )
        .await
        .expect("the exchange succeeds");

        assert_eq!(summary.auth_state, "connected");
        assert_eq!(summary.tool_count, Some(1));

        // The bearer is sealed and readable through the reveal path.
        let bearer = test
            .app()
            .stores
            .mcp_servers
            .reveal_bearer(&context, &server_id)
            .await
            .expect("the reveal answers")
            .expect("a bearer is stored");
        assert_eq!(secrecy::ExposeSecret::expose_secret(&bearer), "token-1");

        // The tools the probe discovered are persisted for the catalogue.
        let catalogue = test
            .app()
            .stores
            .mcp_servers
            .catalogue_tools(&context)
            .await
            .expect("the catalogue answers");
        assert_eq!(catalogue.len(), 1);
        assert_eq!(catalogue[0].1.name, "query");

        // The exchange called the token endpoint with the right form.
        let calls = oauth.calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "https://auth.example.test/token");
        let form: std::collections::HashMap<&str, &str> = calls[0]
            .1
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
            .collect();
        assert_eq!(form["grant_type"], "authorization_code");
        assert_eq!(form["code"], "code-1");
        assert_eq!(form["code_verifier"], "verifier-1");
        assert_eq!(form["client_secret"], "shh");
        assert_eq!(form["client_id"], "boma-cli");
        assert_eq!(form["redirect_uri"], "http://127.0.0.1/boma-mcp/callback");
    }

    #[tokio::test]
    async fn req_033_a_replayed_state_is_refused_and_never_reaches_the_server() {
        let oauth = Arc::new(
            ScriptedOAuthClient::new()
                .with_exchanges(vec![Ok(serde_json::json!({ "access_token": "token-1" }))]),
        );
        let (test, context, server_id) =
            seeded(ScriptedMcpTransport::new(), Arc::clone(&oauth)).await;

        let started = boma_app::usecases::mcp_oauth_start(test.app(), &context, &server_id)
            .await
            .expect("the sign-in begins");
        boma_app::usecases::mcp_oauth_complete(
            test.app(),
            &context,
            &server_id,
            "code-1",
            &started.state,
        )
        .await
        .expect("the first exchange succeeds");
        let replayed = boma_app::usecases::mcp_oauth_complete(
            test.app(),
            &context,
            &server_id,
            "code-2",
            &started.state,
        )
        .await;
        assert!(replayed.is_err(), "a replayed state is refused");
        assert_eq!(oauth.calls().len(), 1, "the second exchange never happens");
    }

    #[tokio::test]
    async fn req_033_an_expired_session_is_refused() {
        let oauth = Arc::new(ScriptedOAuthClient::new());
        let (test, context, server_id) =
            seeded(ScriptedMcpTransport::new(), Arc::clone(&oauth)).await;

        test.app()
            .stores
            .mcp_servers
            .store_oauth_session(
                &context,
                &server_id,
                "state-x",
                secrecy::SecretString::from("verifier-x".to_owned()),
                "2020-01-01T00:00:00Z",
            )
            .await
            .expect("the session is stored");
        let expired = boma_app::usecases::mcp_oauth_complete(
            test.app(),
            &context,
            &server_id,
            "code-1",
            "state-x",
        )
        .await;
        assert!(expired.is_err(), "an expired session is refused");
        assert!(oauth.calls().is_empty(), "the exchange never happens");
    }

    #[tokio::test]
    async fn req_033_start_refuses_a_stdio_server_and_a_server_without_oauth() {
        let oauth = Arc::new(ScriptedOAuthClient::new());
        let (test, context, _server_id) =
            seeded(ScriptedMcpTransport::new(), Arc::clone(&oauth)).await;

        // An HTTP server without OAuth config answers the start with a refusal, not a panic.
        let plain = boma_app::usecases::mcp_server_add(
            test.app(),
            &context,
            "plain",
            boma_domain::McpTransport::Http,
            Some("https://mcp.example.test/plain".to_owned()),
            None,
            None,
            false,
            None,
        )
        .await
        .expect("the server is added");
        let refused = boma_app::usecases::mcp_oauth_start(test.app(), &context, &plain.id).await;
        assert!(refused.is_err(), "a server without OAuth config is refused");

        // A stdio server never signs in with OAuth either.
        let stdio = boma_app::usecases::mcp_server_add(
            test.app(),
            &context,
            "local-things",
            boma_domain::McpTransport::Stdio,
            None,
            Some("python3 /opt/server.py".to_owned()),
            None,
            false,
            None,
        )
        .await;
        assert!(stdio.is_err(), "an empty stdio allow-list refuses the add");
    }

    #[tokio::test]
    async fn req_033_an_exchange_refusal_marks_the_server_error() {
        let oauth = Arc::new(ScriptedOAuthClient::new().with_exchanges(vec![Err(
            boma_app::AppError::Rule(boma_domain::DomainError::InvalidName(
                "the authorization server refused the code".to_owned(),
            )),
        )]));
        let (test, context, server_id) =
            seeded(ScriptedMcpTransport::new(), Arc::clone(&oauth)).await;

        let started = boma_app::usecases::mcp_oauth_start(test.app(), &context, &server_id)
            .await
            .expect("the sign-in begins");
        let refused = boma_app::usecases::mcp_oauth_complete(
            test.app(),
            &context,
            &server_id,
            "bad-code",
            &started.state,
        )
        .await;
        assert!(refused.is_err());
        let servers = boma_app::usecases::mcp_servers_list(test.app(), &context)
            .await
            .expect("the list answers");
        let server = servers
            .iter()
            .find(|server| server.id == server_id)
            .expect("the server is listed");
        assert_eq!(server.auth_state, "error");
    }
}

#[cfg(test)]
mod artifact_tests {
    // REQ-036 — artifacts: versions, replay, blob reclamation, the approval-gated bot tool.

    use std::sync::Arc;

    use super::TestApp;
    use crate::ScriptedProvider;
    use boma_app::worker::Worker;

    fn sha256_hex(bytes: &[u8]) -> String {
        use sha2::{Digest, Sha256};
        let digest = Sha256::digest(bytes);
        let mut hex = String::with_capacity(64);
        for byte in digest {
            use std::fmt::Write as _;
            write!(&mut hex, "{byte:02x}").expect("a write to a String cannot fail");
        }
        format!("sha256:{hex}")
    }

    async fn seeded() -> (
        TestApp,
        boma_domain::TenantContext,
        boma_app::usecases::BotCreated,
    ) {
        let test = TestApp::builder().build();
        let user_id = test
            .seed_user("person@example.com", "correct horse battery")
            .await
            .expect("the person exists");
        let space = test
            .app()
            .stores
            .spaces
            .default_for(&user_id)
            .await
            .expect("the store answers")
            .expect("the person lands in a space");
        let context = TestApp::member_context(&space.id, &user_id);
        let created = boma_app::usecases::create_bot(test.app(), &context, "Assistant", None, None)
            .await
            .expect("the bot is created");
        (test, context, created)
    }

    fn png() -> Vec<u8> {
        b"\x89PNG\r\n\x1a\nfake-png-body".to_vec()
    }

    #[tokio::test]
    async fn req_036_a_create_is_stored_under_its_sniffed_type() {
        let (test, context, _created) = seeded().await;
        let bytes = png();
        let expected_hash = sha256_hex(&bytes);
        let _ = expected_hash;

        let created = boma_app::usecases::artifact_create(
            test.app(),
            &context,
            boma_app::usecases::ArtifactInput {
                bot_id: None,
                group_id: None,
                run_id: None,
                name: "chart.png".to_owned(),
                description: String::new(),
                declared_mime: "image/png".to_owned(),
                content: bytes.clone(),
                root_artifact_id: None,
                client_nonce: None,
            },
        )
        .await
        .expect("the artifact is stored");

        assert_eq!(created.version, 1);
        assert_eq!(created.root_artifact_id, created.id);
        assert_eq!(created.mime_type, "image/png");
        assert_eq!(created.declared_mime, "image/png");
        assert_eq!(created.size, bytes.len() as u64);
        assert_eq!(created.hash, expected_hash);

        let (fetched, stored) =
            boma_app::usecases::artifact_get(test.app(), &context, &created.id, None)
                .await
                .expect("the artifact reads back");
        assert_eq!(fetched.hash, expected_hash);
        assert_eq!(stored, bytes);
    }

    #[tokio::test]
    async fn req_036_an_update_versions_the_family() {
        let (test, context, _created) = seeded().await;
        let first = boma_app::usecases::artifact_create(
            test.app(),
            &context,
            boma_app::usecases::ArtifactInput {
                bot_id: None,
                group_id: None,
                run_id: None,
                name: "report.md".to_owned(),
                description: String::new(),
                declared_mime: "text/markdown".to_owned(),
                content: b"one".to_vec(),
                root_artifact_id: None,
                client_nonce: None,
            },
        )
        .await
        .expect("v1 lands");
        let second = boma_app::usecases::artifact_create(
            test.app(),
            &context,
            boma_app::usecases::ArtifactInput {
                bot_id: None,
                group_id: None,
                run_id: None,
                name: "report.md".to_owned(),
                description: String::new(),
                declared_mime: "text/markdown".to_owned(),
                content: b"two".to_vec(),
                root_artifact_id: Some(first.id.clone()),
                client_nonce: None,
            },
        )
        .await
        .expect("v2 lands");
        assert_eq!(second.version, 2);
        assert_eq!(second.root_artifact_id, first.id);

        let third = boma_app::usecases::artifact_create(
            test.app(),
            &context,
            boma_app::usecases::ArtifactInput {
                bot_id: None,
                group_id: None,
                run_id: None,
                name: "report.md".to_owned(),
                description: String::new(),
                declared_mime: "text/markdown".to_owned(),
                content: b"three".to_vec(),
                root_artifact_id: Some(first.id.clone()),
                client_nonce: None,
            },
        )
        .await
        .expect("v3 lands");
        assert_eq!(third.version, 3);

        let versions = boma_app::usecases::artifact_versions(test.app(), &context, &first.id)
            .await
            .expect("the versions answer");
        let numbers: Vec<i32> = versions.iter().map(|summary| summary.version).collect();
        assert_eq!(numbers, vec![3, 2, 1]);
        let roots: Vec<&str> = versions
            .iter()
            .map(|summary| summary.root_artifact_id.as_str())
            .collect();
        assert_eq!(roots, vec![first.id.as_str(); 3]);

        // The default read is the latest version.
        let (latest, bytes) =
            boma_app::usecases::artifact_get(test.app(), &context, &first.id, None)
                .await
                .expect("the latest reads back");
        assert_eq!(latest.version, 3);
        assert_eq!(bytes, b"three");
    }

    #[tokio::test]
    async fn req_036_a_client_nonce_replay_answers_the_same_row() {
        let (test, context, _created) = seeded().await;
        let input = |content: &[u8]| boma_app::usecases::ArtifactInput {
            bot_id: None,
            group_id: None,
            run_id: None,
            name: "notes.md".to_owned(),
            description: String::new(),
            declared_mime: "text/markdown".to_owned(),
            content: content.to_vec(),
            root_artifact_id: None,
            client_nonce: Some("nonce-artifact-0001".to_owned()),
        };
        let first = boma_app::usecases::artifact_create(test.app(), &context, input(b"one"))
            .await
            .expect("the first write lands");
        let second = boma_app::usecases::artifact_create(test.app(), &context, input(b"two"))
            .await
            .expect("the replay answers");
        assert_eq!(first.id, second.id);
        assert_eq!(first.hash, sha256_hex(b"one"));
        let rows = test.database().artifacts().recorded();
        assert_eq!(rows.len(), 1, "one row, not two");
    }

    #[tokio::test]
    async fn d03_an_html_upload_declared_as_png_is_refused() {
        let (test, context, _created) = seeded().await;
        let outcome = boma_app::usecases::artifact_create(
            test.app(),
            &context,
            boma_app::usecases::ArtifactInput {
                bot_id: None,
                group_id: None,
                run_id: None,
                name: "evil.png".to_owned(),
                description: String::new(),
                declared_mime: "image/png".to_owned(),
                content: b"<!DOCTYPE html><html><body>not a png</body></html>".to_vec(),
                root_artifact_id: None,
                client_nonce: None,
            },
        )
        .await;
        assert!(outcome.is_err(), "HTML declared as PNG is refused");
        assert_eq!(test.database().artifacts().recorded().len(), 0);
        assert_eq!(test.database().artifact_content().blob_count(), 0);
    }

    #[tokio::test]
    async fn req_036_removing_a_family_reclaims_the_unreferenced_blobs() {
        let (test, context, _created) = seeded().await;
        let make = |content: &[u8]| boma_app::usecases::ArtifactInput {
            bot_id: None,
            group_id: None,
            run_id: None,
            name: "data.bin".to_owned(),
            description: String::new(),
            declared_mime: "application/octet-stream".to_owned(),
            content: content.to_vec(),
            root_artifact_id: None,
            client_nonce: None,
        };
        let first = boma_app::usecases::artifact_create(test.app(), &context, make(b"one"))
            .await
            .expect("v1 lands");
        boma_app::usecases::artifact_create(
            test.app(),
            &context,
            boma_app::usecases::ArtifactInput {
                root_artifact_id: Some(first.id.clone()),
                ..make(b"two")
            },
        )
        .await
        .expect("v2 lands");

        let keys: Vec<String> = test
            .database()
            .artifacts()
            .recorded()
            .iter()
            .map(|row| row.storage_key.clone())
            .collect();
        for key in &keys {
            assert!(test.database().artifact_content().contains(key));
        }

        let removed = boma_app::usecases::artifact_remove(test.app(), &context, &first.id)
            .await
            .expect("the family removes");
        assert_eq!(removed, 2);
        assert_eq!(test.database().artifacts().recorded().len(), 0);
        for key in &keys {
            assert!(
                !test.database().artifact_content().contains(key),
                "the unreferenced blob {key} is reclaimed"
            );
        }

        // Every member of the space sees the artifact gone.
        let outcome = boma_app::usecases::artifact_get(test.app(), &context, &first.id, None).await;
        assert!(outcome.is_err());
    }

    #[tokio::test]
    async fn req_036_a_bot_creates_an_artifact_through_the_approval_gate() {
        let (test, context, created) = seeded().await;
        let bot_id = created.bot.id.clone();

        let outcome = boma_app::usecases::send_message(
            test.app(),
            &context,
            &boma_app::usecases::SendMessage {
                bot_id: bot_id.to_string(),
                text: "Write the release report.".to_owned(),
                client_nonce: None,
                reply_to_message_id: None,
                reply_quote: None,
            },
        )
        .await
        .expect("the message is sent");
        let _ = outcome;

        let provider = Arc::new(ScriptedProvider::new_turn(vec![
            boma_app::ports::ProviderTurn::ToolCalls(vec![boma_app::ports::ToolCallRequest {
                id: "call_1".to_owned(),
                name: "artifact.create".to_owned(),
                arguments: serde_json::json!({
                    "name": "release-report.md",
                    "description": "The release report.",
                    "mimeType": "text/markdown",
                    "contentBase64": base64::Engine::encode(&base64::engine::general_purpose::STANDARD, b"# Report\n"),
                }),
            }]),
            boma_app::ports::ProviderTurn::Chunks(vec!["Done.".to_owned()]),
        ]));
        let worker = Worker::new(
            Arc::new(test.app().clone()),
            provider,
            "worker-1".to_owned(),
        );
        assert!(worker.step().await.expect("the worker answers"));

        let messages = test
            .app()
            .stores
            .messages
            .page(&context, &created.thread.id, None, 20)
            .await
            .expect("the page answers")
            .messages;
        let card_id = messages
            .into_iter()
            .flat_map(|message| message.blocks.into_iter())
            .find_map(|block| match block {
                boma_domain::Block::Card { card_id, .. } => Some(card_id),
                _ => None,
            })
            .expect("an approval card is part of the thread");

        assert_eq!(
            test.database().artifacts().recorded().len(),
            0,
            "nothing exists before the approval"
        );

        let answered = boma_app::usecases::answer_card(test.app(), &context, &card_id, "approve")
            .await
            .expect("the card is answered");
        assert!(answered.effect_recorded);

        let artifacts = test.database().artifacts().recorded();
        assert_eq!(
            artifacts.len(),
            1,
            "the approval created exactly one artifact"
        );
        assert_eq!(artifacts[0].name, "release-report.md");
        assert_eq!(artifacts[0].bot_id.as_deref(), Some(bot_id.as_str()));
        assert_eq!(artifacts[0].mime_type, "text/markdown");
        assert_eq!(artifacts[0].size, 9);
        assert!(
            test.database()
                .artifact_content()
                .contains(&artifacts[0].storage_key)
        );
    }
}
