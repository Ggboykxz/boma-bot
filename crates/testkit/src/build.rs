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
#[derive(Debug, Clone)]
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
            Arc::new(database.effects()),
            Arc::new(database.tools()),
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
        );
        let services = if self.computers {
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
            .send(vec!["Considering Friday.".to_owned()])
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

        // A bot whose grant is the echo tool: the catalogue carries exactly that, whatever the
        // space enabled.
        database.seed_bot_grant(&bot_id, vec!["mcp.echo".to_owned()]);

        let catalogue = boma_app::usecases::tool_catalogue(test.app(), &context, bot_id.as_str())
            .await
            .expect("the catalogue answers");
        assert_eq!(catalogue.len(), 1);
        assert_eq!(catalogue[0].key, "mcp.echo");
        assert_eq!(catalogue[0].kind, boma_domain::ToolKind::Mcp);

        // Empty grant: the bot sees every enabled install of the space, disabled ones never
        // (REQ-016).
        database.seed_bot_grant(&bot_id, Vec::new());
        let catalogue = boma_app::usecases::tool_catalogue(test.app(), &context, bot_id.as_str())
            .await
            .expect("the catalogue answers");
        let keys: Vec<&str> = catalogue.iter().map(|tool| tool.key.as_str()).collect();
        assert!(keys.contains(&"mcp.echo"));
        assert!(keys.contains(&"mcp.write"));
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
        assert_eq!(
            keys,
            vec!["mcp.write"],
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
