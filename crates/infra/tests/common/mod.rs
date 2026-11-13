//! The fixtures the integration tests share.
//!
//! The database comes from the environment, exactly like in CI (`BOMA_DATABASE_URL`). The suite
//! fails loudly when the variable is missing: a suite that skips itself when the database is
//! absent is a suite that proves nothing.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
// Each test binary uses a part of these helpers, so the ones it does not use are dead there.
#![allow(dead_code)]

use std::{str::FromStr, sync::Arc, time::Duration};

use boma_app::ports::{AccountStore, BotStore, MessageStore};
use boma_domain::{
    BotId, Email, OrganizationId, RunTrigger, SpaceId, SystemReason, TenantContext, ThreadId,
    UserId,
};
use boma_infra::{
    Argon2idHasher, BroadcastPublisher, DatabaseSettings, IdGenerator, Postgres,
    PostgresAccountStore, PostgresBotStore, PostgresMessageStore, PostgresRunStore, SystemClock,
    TokenGenerator,
};
use sqlx::{
    PgPool,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use uuid::Uuid;

/// The role the tests borrow to be subject to the row level security.
///
/// PostgreSQL never applies the policies to a superuser, nor to a role with `BYPASSRLS`, and the
/// account a development or CI database hands out is a superuser. The tests therefore act as a
/// role that has no privilege, which is the situation a deployment is in.
const PROBE_ROLE: &str = "boma_rls_probe";

/// The lock that serializes the setup of the probe role between the test binaries.
const PROBE_ROLE_LOCK: i64 = 7_418_529_631;

/// The instant the fixtures are written with. It is a constant so the fixtures are reproducible.
const CREATED_AT: &str = "2026-01-01T00:00:00Z";

/// The database under test, or a message that says how to start one.
pub fn database_url() -> String {
    std::env::var("BOMA_DATABASE_URL").unwrap_or_else(|_| {
        panic!(
            "BOMA_DATABASE_URL is not set: start a Postgres (docker compose up -d db, or a local \
             server) and point the variable at it before running the integration tests"
        )
    })
}

/// Settings for the database under test.
pub fn settings() -> DatabaseSettings {
    DatabaseSettings::new(database_url(), 8, Duration::from_secs(5))
        .expect("BOMA_DATABASE_URL is a usable Postgres URL")
}

/// The database under test, with the migrations applied.
pub async fn database() -> Postgres {
    let database = Postgres::connect(&settings())
        .await
        .expect("the database accepts a connection");
    database.migrate().await.expect("the migrations apply");
    database
}

/// A fresh, migrated database that only the test that created it ever sees, and the drop it
/// asks the server for at the end.
///
/// The queue a worker claims from crosses tenants by design, so a test that depends on what the
/// queue holds cannot share the database with the other tests running next to it. A name drawn
/// from an identifier decides which test is which; the template is the maintenance database.
#[derive(Debug)]
pub struct FreshDatabase {
    /// The database this test works on.
    pub database: Postgres,
    /// The connection that owns the right to create and drop databases.
    admin: PgPool,
    /// The database's name.
    name: String,
}

/// The URL of the database server, without the database name: `postgres://u@h:5433`.
fn database_url_prefix() -> String {
    let url = database_url();
    match url.rsplit_once('/') {
        Some((prefix, _)) => prefix.to_owned(),
        None => url,
    }
}

/// The maintenance database the test creates its fresh database in.
fn maintenance_url() -> String {
    format!("{}/postgres", database_url_prefix())
}

/// Creates a fresh, migrated database named after a random identifier.
pub async fn fresh_database() -> FreshDatabase {
    let name = format!("boma_test_{}", Uuid::new_v4().simple());
    let maintenance = PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&maintenance_url())
        .await
        .expect("the maintenance database accepts a connection");
    // The name is hex only, generated here: it cannot carry anything else.
    sqlx::query(sqlx::AssertSqlSafe(format!("CREATE DATABASE {name}")))
        .execute(&maintenance)
        .await
        .expect("a fresh database is created");
    // The tenant pools created by this test point at the fresh database, not the maintenance one.
    let fresh_settings = DatabaseSettings::new(
        format!("{}/{name}", database_url_prefix()),
        8,
        Duration::from_secs(5),
    )
    .expect("the fresh database is a usable Postgres URL");
    let database = Postgres::connect(&fresh_settings)
        .await
        .expect("the fresh database accepts a connection");
    database
        .migrate()
        .await
        .expect("the fresh database migrates");
    FreshDatabase {
        database,
        admin: maintenance,
        name,
    }
}

impl FreshDatabase {
    /// Drops the fresh database, connections and all.
    ///
    /// Taking the value by move makes the disposal explicit: a test that forgets is a test that
    /// leaks a database, and the `unused_mut` warnings on callers say so.
    pub async fn dispose(self) {
        // Pools close their connections when they are dropped.
        drop(self.database);
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "DROP DATABASE IF EXISTS {} WITH (FORCE)",
            self.name
        )))
        .execute(&self.admin)
        .await
        .expect("the fresh database is dropped");
        self.admin.close().await;
    }
}

/// A pool that acts as a role the row level security applies to.
///
/// When the account the tests connect with is already unprivileged, its own pool is enough.
pub async fn tenant_pool(database: &Postgres) -> PgPool {
    let privileged = sqlx::query_scalar::<_, bool>(
        "SELECT rolsuper OR rolbypassrls FROM pg_roles WHERE rolname = current_user",
    )
    .fetch_one(database.pool())
    .await
    .expect("the roles of the database are readable");
    if !privileged {
        return database.pool().clone();
    }

    ensure_probe_role(database).await;

    // The probe role acts on the database the caller actually holds, never on a name it guessed.
    let database_name = database
        .pool()
        .connect_options()
        .get_database()
        .unwrap_or("postgres")
        .to_owned();
    let probe_url = format!("{}/{database_name}", database_url_prefix());
    let options = PgConnectOptions::from_str(&probe_url)
        .expect("the probe database is a usable Postgres URL")
        .options([("role", PROBE_ROLE)]);
    PgPoolOptions::new()
        .max_connections(8)
        .acquire_timeout(Duration::from_secs(5))
        .connect_with(options)
        .await
        .expect("the probe role can open connections")
}

/// Creates the role the tests borrow, and gives it the tables the product owns.
async fn ensure_probe_role(database: &Postgres) {
    // The advisory lock serializes the setup: two test binaries may run at the same time.
    sqlx::query("SELECT pg_advisory_lock($1)")
        .bind(PROBE_ROLE_LOCK)
        .execute(database.pool())
        .await
        .expect("the advisory lock is taken");

    sqlx::query(
        "DO $$
         BEGIN
             IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'boma_rls_probe') THEN
                 CREATE ROLE boma_rls_probe NOLOGIN NOSUPERUSER NOBYPASSRLS;
             END IF;
         END
         $$",
    )
    .execute(database.pool())
    .await
    .expect("the probe role exists");
    sqlx::query("GRANT USAGE ON SCHEMA public TO boma_rls_probe")
        .execute(database.pool())
        .await
        .expect("the probe role may use the schema");
    sqlx::query("GRANT USAGE ON SCHEMA boma TO boma_rls_probe")
        .execute(database.pool())
        .await
        .expect("the probe role may use the schema of the helpers");
    sqlx::query(
        "GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO boma_rls_probe",
    )
    .execute(database.pool())
    .await
    .expect("the probe role may read and write the tables");

    sqlx::query("SELECT pg_advisory_unlock($1)")
        .bind(PROBE_ROLE_LOCK)
        .execute(database.pool())
        .await
        .expect("the advisory lock is released");
}

/// A prefix that no other test run uses.
pub fn unique_id() -> String {
    format!("test_{}", Uuid::new_v4().simple())
}

/// The pattern that matches every row the fixtures of `prefix` wrote.
fn like_pattern(prefix: &str) -> String {
    format!("{}%", prefix.replace('_', r"\_"))
}

/// A person, a space, a bot and a thread: what a test needs to write something.
pub struct Fixture {
    /// The person who owns the space.
    pub user_id: UserId,
    /// The space.
    pub space_id: SpaceId,
    /// The bot.
    pub bot_id: BotId,
    /// The conversation of the bot.
    pub thread_id: ThreadId,
    /// The context that acts inside the space.
    pub context: TenantContext,
}

/// Writes a person, a space, a bot and a thread.
pub async fn fixture(pool: &PgPool, prefix: &str) -> Fixture {
    let user_id = UserId::new(format!("{prefix}usr")).expect("the fixture id is accepted");
    let organization_id =
        OrganizationId::new(format!("{prefix}org")).expect("the fixture id is accepted");
    let space_id = SpaceId::new(format!("{prefix}spc")).expect("the fixture id is accepted");
    let bot_id = BotId::new(format!("{prefix}bot")).expect("the fixture id is accepted");
    let thread_id = ThreadId::new(format!("{prefix}thr")).expect("the fixture id is accepted");
    let email = Email::new(format!("{prefix}@example.com")).expect("the fixture address is valid");

    let accounts = PostgresAccountStore::new(pool.clone());
    accounts
        .create_account(boma_app::ports::NewAccount {
            user: boma_app::ports::NewUser {
                id: user_id.clone(),
                email: email.clone(),
                password_hash: "not-a-real-hash".to_owned(),
                name: "Test Person".to_owned(),
                locale: "en-US".to_owned(),
                created_at: CREATED_AT.to_owned(),
            },
            organization: boma_app::ports::NewOrganization {
                id: organization_id.clone(),
                name: "Test Organization".to_owned(),
                created_at: CREATED_AT.to_owned(),
            },
            space: boma_app::ports::NewSpace {
                id: space_id.clone(),
                organization_id,
                name: "Test Space".to_owned(),
                is_default: true,
                created_at: CREATED_AT.to_owned(),
            },
        })
        .await
        .expect("the account is written");

    let bots = PostgresBotStore::new(pool.clone());
    bots.create_with_thread(
        boma_app::ports::NewBot {
            id: bot_id.clone(),
            space_id: space_id.clone(),
            user_id: user_id.clone(),
            name: "Test Bot".to_owned(),
            description: None,
            color: None,
            created_at: CREATED_AT.to_owned(),
        },
        boma_app::ports::NewThread {
            id: thread_id.clone(),
            space_id: space_id.clone(),
            bot_id: bot_id.clone(),
            user_id: user_id.clone(),
            created_at: CREATED_AT.to_owned(),
        },
    )
    .await
    .expect("the bot and its thread are written");

    Fixture {
        user_id: user_id.clone(),
        space_id: space_id.clone(),
        bot_id,
        thread_id,
        context: TenantContext::Member {
            space_id: space_id.clone(),
            user_id: user_id.clone(),
        },
    }
}

/// Writes a message in the thread of a fixture.
pub async fn message(
    pool: &PgPool,
    fixture: &Fixture,
    nonce: Option<&str>,
) -> boma_domain::Message {
    let store = PostgresMessageStore::new(pool.clone());
    let stored = store
        .insert(
            &fixture.context,
            boma_app::ports::NewMessage {
                id: boma_domain::MessageId::new(format!("{}msg", fixture.thread_id.as_str()))
                    .expect("the fixture id is accepted"),
                space_id: fixture.space_id.clone(),
                thread_id: fixture.thread_id.clone(),
                role: boma_domain::MessageRole::User,
                blocks: vec![boma_domain::Block::Text {
                    text: "hello".to_owned(),
                }],
                bot_id: None,
                author_user_id: Some(fixture.user_id.clone()),
                run_id: None,
                client_nonce: nonce.map(str::to_owned),
                created_at: CREATED_AT.to_owned(),
                reply_to_message_id: None,
                reply_quote: None,
            },
        )
        .await
        .expect("the message is written");
    stored.message
}

/// Removes every row the fixtures of `prefix` wrote.
pub async fn cleanup(pool: &PgPool, prefix: &str) {
    let pattern = like_pattern(prefix);
    boma_infra::tenant::as_system(pool, SystemReason::DataMigration, |conn| {
        Box::pin(async move {
            for statement in [
                "DELETE FROM events WHERE id LIKE $1 ESCAPE '\\'",
                "DELETE FROM runs WHERE id LIKE $1 ESCAPE '\\'",
                "DELETE FROM messages WHERE id LIKE $1 ESCAPE '\\'",
                "DELETE FROM threads WHERE id LIKE $1 ESCAPE '\\'",
                "DELETE FROM bots WHERE id LIKE $1 ESCAPE '\\'",
                "DELETE FROM invitations WHERE id LIKE $1 ESCAPE '\\'",
                "DELETE FROM space_members WHERE space_id LIKE $1 ESCAPE '\\'",
                "DELETE FROM spaces WHERE id LIKE $1 ESCAPE '\\'",
                "DELETE FROM sessions WHERE id LIKE $1 ESCAPE '\\'",
                "DELETE FROM organization_members
                 WHERE user_id LIKE $1 ESCAPE '\\' OR organization_id LIKE $1 ESCAPE '\\'",
                "DELETE FROM organizations WHERE id LIKE $1 ESCAPE '\\'",
                "DELETE FROM users WHERE id LIKE $1 ESCAPE '\\' OR email LIKE $1 ESCAPE '\\'",
            ] {
                sqlx::query(statement)
                    .bind(&pattern)
                    .execute(&mut *conn)
                    .await
                    .expect("the rows of the fixture are removed");
            }
            Ok(())
        })
    })
    .await
    .expect("the cleanup runs");
}

/// A run in the thread of a fixture.
///
/// `suffix` names the run, so two runs of the same thread differ by their id and not only by
/// their nonce.
pub fn new_run(fixture: &Fixture, suffix: &str, nonce: Option<&str>) -> boma_app::ports::NewRun {
    boma_app::ports::NewRun {
        id: boma_domain::RunId::new(format!("{}run{suffix}", fixture.thread_id.as_str()))
            .expect("the fixture id is accepted"),
        space_id: fixture.space_id.clone(),
        bot_id: fixture.bot_id.clone(),
        thread_id: fixture.thread_id.clone(),
        user_id: fixture.user_id.clone(),
        trigger: RunTrigger::User,
        client_nonce: nonce.map(str::to_owned),
        source_message_id: None,
        created_at: CREATED_AT.to_owned(),
        model_provider: None,
        model_id: None,
        model_pinned: false,
        parent_run_id: None,
        routine_id: None,
    }
}

/// The store of the runs, on a pool.
pub fn runs(pool: &PgPool) -> PostgresRunStore {
    PostgresRunStore::new(pool.clone())
}

/// A computer the conformance battery runs against, with its runtime attached.
pub struct ComputerFixture {
    /// The application under test.
    pub app: boma_app::App,
    /// The member context the tests act in.
    pub context: boma_domain::TenantContext,
    /// The bot that owns the computer.
    pub bot_id: boma_domain::BotId,
    /// The organisation that owns the space.
    pub organization_id: boma_domain::OrganizationId,
    /// The sign-in e-mail, for cleanup.
    pub email: String,
    /// The fresh database, kept alive for the test.
    pub fresh: FreshDatabase,
}

/// Builds an application over a fresh database with the requested computer runtime attached.
///
/// This is the harness every computer-provider conformance test shares: one call, one seeded
/// space with one bot and an owned context, and the runtime handed over by the factory.
#[allow(clippy::too_many_lines)] // one seed per test keeps the story explicit instead of a fixture maze
pub async fn computer_app(
    runtime: Arc<dyn boma_app::ports::ComputerRuntime>,
    prefix: &str,
) -> ComputerFixture {
    let fresh = fresh_database().await;
    let pool = fresh.database.pool().clone();
    let email = format!("{prefix}@example.com");
    let organization_id = boma_domain::OrganizationId::new(format!("{prefix}org"))
        .expect("the fixture id is accepted");
    let user_id =
        boma_domain::UserId::new(format!("{prefix}usr")).expect("the fixture id is accepted");
    let accounts = boma_infra::PostgresAccountStore::new(pool.clone());
    accounts
        .create_account(boma_app::ports::NewAccount {
            user: boma_app::ports::NewUser {
                id: user_id.clone(),
                email: boma_domain::Email::new(email.clone())
                    .expect("the fixture address is valid"),
                password_hash: "not-a-real-hash".to_owned(),
                name: "Test Person".to_owned(),
                locale: "en-US".to_owned(),
                created_at: CREATED_AT.to_owned(),
            },
            organization: boma_app::ports::NewOrganization {
                id: organization_id.clone(),
                name: "Test Organization".to_owned(),
                created_at: CREATED_AT.to_owned(),
            },
            space: boma_app::ports::NewSpace {
                id: boma_domain::SpaceId::new(format!("{prefix}spc"))
                    .expect("the fixture id is accepted"),
                organization_id: organization_id.clone(),
                name: "Test Space".to_owned(),
                is_default: true,
                created_at: CREATED_AT.to_owned(),
            },
        })
        .await
        .expect("the account is written");
    let space_id =
        boma_domain::SpaceId::new(format!("{prefix}spc")).expect("the fixture id is accepted");
    let bot_id =
        boma_domain::BotId::new(format!("{prefix}bot")).expect("the fixture id is accepted");
    let thread_id =
        boma_domain::ThreadId::new(format!("{prefix}thr")).expect("the fixture id is accepted");
    let bots = boma_infra::PostgresBotStore::new(pool.clone());
    bots.create_with_thread(
        boma_app::ports::NewBot {
            id: bot_id.clone(),
            space_id: space_id.clone(),
            user_id: user_id.clone(),
            name: "Test Bot".to_owned(),
            description: None,
            color: None,
            created_at: CREATED_AT.to_owned(),
        },
        boma_app::ports::NewThread {
            id: thread_id,
            space_id: space_id.clone(),
            bot_id: bot_id.clone(),
            user_id: user_id.clone(),
            created_at: CREATED_AT.to_owned(),
        },
    )
    .await
    .expect("the bot and its thread are written");
    let publisher = Arc::new(BroadcastPublisher::default());
    let services = boma_app::Services::new(
        publisher,
        Arc::new(Argon2idHasher::default()),
        Arc::new(TokenGenerator),
        Arc::new(IdGenerator),
        Arc::new(SystemClock),
        boma_domain::model::DeploymentModels {
            reachable: vec![
                "openai/gpt-5".to_owned(),
                "anthropic/claude-sonnet".to_owned(),
            ],
            default: "anthropic/claude-sonnet".to_owned(),
        },
        Arc::new(boma_testkit::ScriptedMcpTransport::new()),
    )
    .with_computers(runtime);
    let app = boma_app::App::new(
        boma_app::Stores::new(
            Arc::new(boma_infra::PostgresAccountStore::new(pool.clone())),
            Arc::new(boma_infra::PostgresUserStore::new(pool.clone())),
            Arc::new(boma_infra::PostgresSessionStore::new(pool.clone())),
            Arc::new(boma_infra::PostgresSpaceStore::new(pool.clone())),
            Arc::new(PostgresBotStore::new(pool.clone())),
            Arc::new(boma_infra::PostgresThreadStore::new(pool.clone())),
            Arc::new(boma_infra::PostgresMessageStore::new(pool.clone())),
            Arc::new(boma_infra::PostgresRunStore::new(pool.clone())),
            Arc::new(boma_infra::PostgresEventStore::new(pool.clone())),
            Arc::new(boma_infra::PostgresInvitationStore::new(pool.clone())),
            Arc::new(boma_infra::PostgresCardStore::new(pool.clone())),
            Arc::new(boma_infra::PostgresPendingToolCallStore::new(pool.clone())),
            Arc::new(boma_infra::PostgresEffectStore::new(pool.clone())),
            Arc::new(boma_infra::PostgresToolStore::new(pool.clone())),
            Arc::new(boma_infra::PostgresRateLimitStore::new(pool.clone())),
            Arc::new(boma_infra::PostgresSecretStore::new(
                pool.clone(),
                boma_infra::SecretBox::ephemeral(),
            )),
            Arc::new(boma_infra::PostgresPluginSubmissionStore::new(pool.clone())),
            Arc::new(boma_infra::PostgresSkillStore::new(pool.clone())),
            Arc::new(boma_infra::PostgresMcpServerStore::new(
                pool.clone(),
                boma_infra::SecretBox::ephemeral(),
            )),
            Arc::new(boma_infra::PostgresAuditStore::new(pool.clone())),
            Arc::new(boma_infra::PostgresReactionStore::new(pool.clone())),
            Arc::new(boma_infra::PostgresReadStateStore::new(pool.clone())),
            Arc::new(boma_infra::PostgresRosterStore::new(pool.clone())),
            Arc::new(boma_infra::PostgresMemoryStore::new(pool.clone())),
            Arc::new(boma_infra::PostgresRoutineStore::new(pool.clone())),
            Arc::new(boma_infra::PostgresComputerStore::new(pool.clone())),
            Arc::new(boma_infra::PostgresScreenCapabilityStore::new(pool)),
        ),
        services,
    );
    let context = TenantContext::Member { space_id, user_id };
    ComputerFixture {
        app,
        context,
        bot_id,
        organization_id,
        email,
        fresh,
    }
}
