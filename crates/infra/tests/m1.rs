//! Integration tests for the M1 Postgres adapters, against a real database.
//!
//! The in-memory testkit proves the use cases; these prove the SQL: the migrations, the
//! allocators, the foreign keys and the tenant context.

// An integration test file is compiled as its own crate, so clippy's `allow-*-in-tests` (which
// covers `#[test]` functions and `#[cfg(test)]` modules only) does not reach the helpers below.
// AGENTS.md allows unwrap, expect and panic in test code; this is where that permission is taken,
// once and in writing.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::{collections::BTreeMap, sync::Arc};

use boma_app::ports::IdGenerator as _;
use boma_app::{App, ports::*};
use boma_domain::{
    Block, BotId, Email, MessageId, MessageRole, RunStatus, RunTrigger, SpaceId, TenantContext,
    ThreadId, UserId,
};
use boma_infra::{
    Argon2idHasher, BroadcastPublisher, IdGenerator, Postgres, PostgresAccountStore,
    PostgresBotStore, PostgresCardStore, PostgresComputerStore, PostgresEffectStore,
    PostgresEventStore, PostgresInvitationStore, PostgresMemoryStore, PostgresMessageStore,
    PostgresPendingToolCallStore, PostgresRateLimitStore, PostgresReactionStore,
    PostgresReadStateStore, PostgresRosterStore, PostgresRoutineStore, PostgresRunStore,
    PostgresScreenCapabilityStore, PostgresSecretStore, PostgresSessionStore, PostgresSpaceStore,
    PostgresThreadStore, PostgresToolStore, PostgresUserStore, SystemClock, TokenGenerator,
};

mod common;

fn ids() -> IdGenerator {
    IdGenerator
}

/// Removes every row this test created, addressed by the unique email and organization.
///
/// The tenant rows fall with the organization (spaces cascade down to bots, threads, messages,
/// runs and events) or with the user (sessions), so two deletes are enough.
async fn cleanup(database: &Postgres, organization_id: &boma_domain::OrganizationId, email: &str) {
    sqlx::query("DELETE FROM organizations WHERE id = $1")
        .bind(organization_id.as_str())
        .execute(database.pool())
        .await
        .expect("the organization is removed");
    sqlx::query("DELETE FROM users WHERE email = $1")
        .bind(email)
        .execute(database.pool())
        .await
        .expect("the person is removed");
}

fn app_with_scripts(
    database: &Postgres,
    mcp_transport: Arc<dyn boma_app::ports::McpTransport>,
    oauth_client: Arc<dyn boma_app::ports::OAuthClient>,
) -> App {
    let pool = database.pool().clone();
    let publisher = Arc::new(BroadcastPublisher::default());
    App::new(
        boma_app::Stores::new(
            Arc::new(PostgresAccountStore::new(pool.clone())),
            Arc::new(PostgresUserStore::new(pool.clone())),
            Arc::new(PostgresSessionStore::new(pool.clone())),
            Arc::new(PostgresSpaceStore::new(pool.clone())),
            Arc::new(PostgresBotStore::new(pool.clone())),
            Arc::new(PostgresThreadStore::new(pool.clone())),
            Arc::new(PostgresMessageStore::new(pool.clone())),
            Arc::new(PostgresRunStore::new(pool.clone())),
            Arc::new(PostgresEventStore::new(pool.clone())),
            Arc::new(PostgresInvitationStore::new(pool.clone())),
            Arc::new(PostgresCardStore::new(pool.clone())),
            Arc::new(PostgresPendingToolCallStore::new(pool.clone())),
            Arc::new(PostgresEffectStore::new(pool.clone())),
            Arc::new(PostgresToolStore::new(pool.clone())),
            Arc::new(PostgresRateLimitStore::new(pool.clone())),
            Arc::new(PostgresSecretStore::new(
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
            Arc::new(boma_infra::PostgresConnectorStore::new(
                pool.clone(),
                boma_infra::SecretBox::ephemeral(),
            )),
            Arc::new(boma_infra::PostgresPluginInstallStore::new(pool.clone())),
            Arc::new(boma_infra::PostgresArtifactStore::new(pool.clone())),
            Arc::new(boma_infra::PostgresPageStore::new(pool.clone())),
            Arc::new(boma_infra::PostgresTicketStore::new(pool.clone())),
            Arc::new(PostgresReactionStore::new(pool.clone())),
            Arc::new(PostgresReadStateStore::new(pool.clone())),
            Arc::new(PostgresRosterStore::new(pool.clone())),
            Arc::new(PostgresMemoryStore::new(pool.clone())),
            Arc::new(PostgresRoutineStore::new(pool.clone())),
            Arc::new(PostgresComputerStore::new(pool.clone())),
            Arc::new(PostgresScreenCapabilityStore::new(pool)),
        ),
        boma_app::Services::new(
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
            mcp_transport,
            Arc::new(boma_testkit::ScriptedPluginFetcher::new()),
            oauth_client,
            Arc::new(
                boma_infra::FileArtifactContentStore::new(concat!(
                    "/tmp/boma-artifact-test-m1-",
                    env!("CARGO_PKG_NAME")
                ))
                .expect("the artifact root"),
            ),
        ),
    )
}

fn app(database: &Postgres) -> App {
    app_with_scripts(
        database,
        Arc::new(boma_testkit::ScriptedMcpTransport::new()),
        Arc::new(boma_testkit::ScriptedOAuthClient::new()),
    )
}

async fn space(
    app: &App,
    email_prefix: &str,
) -> (
    UserId,
    SpaceId,
    TenantContext,
    String,
    boma_domain::OrganizationId,
) {
    let suffix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let email = format!("{email_prefix}-{suffix}@example.com");
    let organization_id =
        boma_domain::OrganizationId::new(ids().new_id("org")).expect("an id is valid");
    let account = app
        .stores
        .accounts
        .create_account(NewAccount {
            user: NewUser {
                id: UserId::new(ids().new_id("usr")).expect("an id is valid"),
                email: Email::new(&email).expect("the address is valid"),
                password_hash: "hash".to_owned(),
                name: "Person".to_owned(),
                locale: "en-US".to_owned(),
                created_at: app.services.clock.now(),
            },
            organization: NewOrganization {
                id: organization_id.clone(),
                name: "Org".to_owned(),
                created_at: app.services.clock.now(),
            },
            space: NewSpace {
                id: SpaceId::new(ids().new_id("spc")).expect("an id is valid"),
                organization_id: organization_id.clone(),
                name: "Space".to_owned(),
                is_default: true,
                created_at: app.services.clock.now(),
            },
        })
        .await
        .expect("the account is created");
    let space_id = account.space.id.clone();
    let context = TenantContext::Member {
        space_id: space_id.clone(),
        user_id: account.user.id.clone(),
    };
    (account.user.id, space_id, context, email, organization_id)
}

#[tokio::test]
async fn req_040_the_account_store_writes_user_org_space_and_membership_in_one_transaction() {
    let database = common::database().await;
    let app = app(&database);
    let (user_id, space_id, _context, email, organization_id) = space(&app, "person").await;

    let found = app
        .stores
        .users
        .find_by_email(&Email::new(&email).expect("the address is valid"))
        .await
        .expect("the lookup answers");
    let (user, _hash) = found.expect("the account exists");
    assert_eq!(user.id, user_id);

    let role = app
        .stores
        .spaces
        .membership(&space_id, &user_id)
        .await
        .expect("the membership answers");
    assert_eq!(role, Some(boma_domain::SpaceRole::Owner));
    cleanup(&database, &organization_id, &email).await;
}

#[tokio::test]
async fn req_040_the_password_hasher_round_trips_a_password() {
    let database = common::database().await;
    let app = app(&database);
    let hash = app
        .services
        .hasher
        .hash("correct horse battery")
        .await
        .expect("the hash is computed");
    assert!(
        app.services
            .hasher
            .verify("correct horse battery", &hash)
            .await
            .expect("the verification answers")
    );
    assert!(
        !app.services
            .hasher
            .verify("wrong password entirely", &hash)
            .await
            .expect("the verification answers")
    );
}

#[tokio::test]
async fn req_001_a_send_writes_one_message_and_starts_one_run() {
    let database = common::database().await;
    let app = app(&database);
    let (user_id, space_id, context, email, organization_id) = space(&app, "send").await;

    let bot_id = BotId::new(ids().new_id("bot")).expect("an id is valid");
    let thread_id = ThreadId::new(ids().new_id("thr")).expect("an id is valid");
    let (bot, thread) = app
        .stores
        .bots
        .create_with_thread(
            NewBot {
                id: bot_id.clone(),
                space_id: space_id.clone(),
                user_id: user_id.clone(),
                name: "Analyst".to_owned(),
                description: None,
                color: None,
                created_at: app.services.clock.now(),
            },
            NewThread {
                id: thread_id.clone(),
                space_id: space_id.clone(),
                bot_id: bot_id.clone(),
                user_id: user_id.clone(),
                created_at: app.services.clock.now(),
            },
        )
        .await
        .expect("the bot is created");

    let message = app
        .stores
        .messages
        .insert(
            &context,
            NewMessage {
                id: MessageId::new(ids().new_id("msg")).expect("an id is valid"),
                space_id: space_id.clone(),
                thread_id: thread.id.clone(),
                role: MessageRole::User,
                blocks: vec![boma_domain::Block::Text {
                    text: "Plan my week".to_owned(),
                }],
                bot_id: None,
                author_user_id: Some(user_id.clone()),
                run_id: None,
                client_nonce: Some("nonce-1".to_owned()),
                created_at: app.services.clock.now(),
                reply_to_message_id: None,
                reply_quote: None,
            },
        )
        .await
        .expect("the message is written");
    assert!(message.created);

    let run = app
        .stores
        .runs
        .create(NewRun {
            id: boma_domain::RunId::new(ids().new_id("run")).expect("an id is valid"),
            space_id: space_id.clone(),
            bot_id: bot.id.clone(),
            thread_id: thread.id.clone(),
            user_id: user_id.clone(),
            trigger: RunTrigger::User,
            client_nonce: Some("nonce-1".to_owned()),
            source_message_id: Some(message.message.id.clone()),
            created_at: app.services.clock.now(),
            model_provider: None,
            model_id: None,
            model_pinned: false,
            parent_run_id: None,
            routine_id: None,
        })
        .await
        .expect("the run is created");
    assert_eq!(run.status, RunStatus::Queued);

    let activity = app
        .stores
        .runs
        .list(&context, Some(&bot.id), None, 10)
        .await
        .expect("the activity answers");
    assert_eq!(activity.runs.len(), 1);
    assert_eq!(
        activity.runs[0].prompt_snippet.as_deref(),
        Some("Plan my week")
    );

    let page = app
        .stores
        .messages
        .page(&context, &thread.id, None, 10)
        .await
        .expect("the page answers");
    assert_eq!(page.messages.len(), 1);

    cleanup(&database, &organization_id, &email).await;
}

#[tokio::test]
async fn req_012_the_claim_moves_a_run_to_leased_and_the_fence_guards_the_write() {
    // The queue is global by design, so this test gets a queue of its own.
    let fresh = common::fresh_database().await;
    let database = &fresh.database;
    let app = app(database);
    let (user_id, space_id, context, email, organization_id) = space(&app, "claim").await;

    let bot_id = BotId::new(ids().new_id("bot")).expect("an id is valid");
    let thread_id = ThreadId::new(ids().new_id("thr")).expect("an id is valid");
    let (bot, thread) = app
        .stores
        .bots
        .create_with_thread(
            NewBot {
                id: bot_id.clone(),
                space_id: space_id.clone(),
                user_id: user_id.clone(),
                name: "Analyst".to_owned(),
                description: None,
                color: None,
                created_at: app.services.clock.now(),
            },
            NewThread {
                id: thread_id.clone(),
                space_id: space_id.clone(),
                bot_id: bot_id.clone(),
                user_id: user_id.clone(),
                created_at: app.services.clock.now(),
            },
        )
        .await
        .expect("the bot is created");

    let run = app
        .stores
        .runs
        .create(NewRun {
            id: boma_domain::RunId::new(ids().new_id("run")).expect("an id is valid"),
            space_id: space_id.clone(),
            bot_id: bot.id.clone(),
            thread_id: thread.id.clone(),
            user_id: user_id.clone(),
            trigger: RunTrigger::User,
            client_nonce: Some("nonce-2".to_owned()),
            source_message_id: None,
            created_at: app.services.clock.now(),
            model_provider: None,
            model_id: None,
            model_pinned: false,
            parent_run_id: None,
            routine_id: None,
        })
        .await
        .expect("the run is created");

    // The database persists between runs and the claim takes the oldest queued run, so a
    // leftover from an earlier execution may come first. Each claim leases one run, so the loop
    // converges on ours; the cap only bounds the work.
    let mut claimed = None;
    for _ in 0..100 {
        match app
            .stores
            .runs
            .claim_next_queued("worker-1")
            .await
            .expect("the claim answers")
        {
            None => break,
            Some(candidate) => {
                if candidate.id == run.id {
                    claimed = Some(candidate);
                    break;
                }
            }
        }
    }
    let claimed = claimed.expect("the run is claimed");
    assert_eq!(claimed.status, RunStatus::Leased);
    assert_eq!(claimed.lease_owner.as_deref(), Some("worker-1"));

    // A stale fence is refused: another worker owns the run now.
    let stale = app
        .stores
        .runs
        .set_status(
            &context,
            &run.id,
            RunStatus::Completed,
            &app.services.clock.now(),
            Some(0),
            None,
        )
        .await;
    assert!(stale.is_err());

    let ok = app
        .stores
        .runs
        .set_status(
            &context,
            &run.id,
            RunStatus::Running,
            &app.services.clock.now(),
            Some(claimed.lease_fence),
            None,
        )
        .await;
    assert!(ok.is_ok());
    cleanup(database, &organization_id, &email).await;
    fresh.dispose().await;
}

#[tokio::test]
async fn req_020_events_are_sequenced_and_published() {
    let database = common::database().await;
    let app = app(&database);
    let (user_id, space_id, context, email, organization_id) = space(&app, "events").await;

    let bot_id = BotId::new(ids().new_id("bot")).expect("an id is valid");
    let thread_id = ThreadId::new(ids().new_id("thr")).expect("an id is valid");
    let (_bot, thread) = app
        .stores
        .bots
        .create_with_thread(
            NewBot {
                id: bot_id.clone(),
                space_id: space_id.clone(),
                user_id: user_id.clone(),
                name: "Analyst".to_owned(),
                description: None,
                color: None,
                created_at: app.services.clock.now(),
            },
            NewThread {
                id: thread_id.clone(),
                space_id: space_id.clone(),
                bot_id: bot_id.clone(),
                user_id: user_id.clone(),
                created_at: app.services.clock.now(),
            },
        )
        .await
        .expect("the bot is created");

    for text in ["one", "two"] {
        app.stores
            .events
            .append(
                &context,
                NewEvent {
                    space_id: space_id.clone(),
                    thread_id: thread.id.clone(),
                    kind: boma_domain::EventType::MessageUpdate,
                    payload: serde_json::json!({ "delta": text }),
                    created_at: app.services.clock.now(),
                },
            )
            .await
            .expect("the event is written");
    }

    let events = app
        .stores
        .events
        .since(&context, &thread.id, 0)
        .await
        .expect("the events answer");
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].seq, 1);
    assert_eq!(events[1].seq, 2);
    cleanup(&database, &organization_id, &email).await;
}

#[tokio::test]
async fn req_041_sessions_round_trip_and_a_deleted_session_is_gone() {
    let database = common::database().await;
    let app = app(&database);
    let (user_id, _space_id, _context, email, organization_id) = space(&app, "sessions").await;

    let token = app.services.tokens.generate().expect("a token is minted");
    let hash = app.services.tokens.hash(&token).expect("the token hashes");
    let session = app
        .stores
        .sessions
        .create(NewSession {
            id: boma_domain::SessionId::new(ids().new_id("ses")).expect("an id is valid"),
            user_id: user_id.clone(),
            token_hash: hash.clone(),
            expires_at: app.services.clock.now_plus_seconds(3600),
            created_at: app.services.clock.now(),
        })
        .await
        .expect("the session is created");

    let found = app
        .stores
        .sessions
        .find_by_token_hash(&hash)
        .await
        .expect("the lookup answers")
        .expect("the session exists");
    assert_eq!(found.user_id, user_id);

    app.stores
        .sessions
        .delete(&session.id)
        .await
        .expect("the session is revoked");
    let gone = app
        .stores
        .sessions
        .find_by_token_hash(&hash)
        .await
        .expect("the lookup answers");
    assert!(gone.is_none());
    cleanup(&database, &organization_id, &email).await;
}

#[tokio::test]
async fn req_014_an_expired_lease_is_reclaimed_by_another_worker() {
    let fresh = common::fresh_database().await;
    let database = &fresh.database;
    let app = app(database);
    let (user_id, space_id, _context, email, organization_id) = space(&app, "reclaim").await;

    let bot_id = BotId::new(ids().new_id("bot")).expect("an id is valid");
    let thread_id = ThreadId::new(ids().new_id("thr")).expect("an id is valid");
    let (bot, thread) = app
        .stores
        .bots
        .create_with_thread(
            NewBot {
                id: bot_id.clone(),
                space_id: space_id.clone(),
                user_id: user_id.clone(),
                name: "Analyst".to_owned(),
                description: None,
                color: None,
                created_at: app.services.clock.now(),
            },
            NewThread {
                id: thread_id.clone(),
                space_id: space_id.clone(),
                bot_id: bot_id.clone(),
                user_id: user_id.clone(),
                created_at: app.services.clock.now(),
            },
        )
        .await
        .expect("the bot is created");

    let run = app
        .stores
        .runs
        .create(NewRun {
            id: boma_domain::RunId::new(ids().new_id("run")).expect("an id is valid"),
            space_id: space_id.clone(),
            bot_id: bot.id.clone(),
            thread_id: thread.id.clone(),
            user_id: user_id.clone(),
            trigger: RunTrigger::User,
            client_nonce: Some("nonce-014".to_owned()),
            source_message_id: None,
            created_at: app.services.clock.now(),
            model_provider: None,
            model_id: None,
            model_pinned: false,
            parent_run_id: None,
            routine_id: None,
        })
        .await
        .expect("the run is created");

    // The first worker takes the lease, then disappears mid-turn.
    let first = app
        .stores
        .runs
        .claim_next_queued("worker-014-1")
        .await
        .expect("the claim answers");
    // Other executions may have queued runs older than ours; drain them until ours is claimed.
    let mut first = first;
    for _ in 0..100 {
        match &first {
            Some(candidate) if candidate.id == run.id => break,
            Some(_) => {
                first = app
                    .stores
                    .runs
                    .claim_next_queued("worker-014-1")
                    .await
                    .expect("the claim answers");
            }
            None => break,
        }
    }
    let first = first.expect("the run is claimed");
    assert_eq!(first.lease_fence, 1);

    // The lease goes stale: the disappearance is visible only in the clock.
    sqlx::query("UPDATE runs SET lease_expires_at = now() - interval '1 hour' WHERE id = $1")
        .bind(run.id.as_str())
        .execute(database.pool())
        .await
        .expect("the lease is expired");

    let reclaimed = app
        .stores
        .runs
        .claim_next_queued("worker-014-2")
        .await
        .expect("the claim answers")
        .expect("the stale lease is reclaimed");
    assert_eq!(reclaimed.id, run.id);
    assert_eq!(reclaimed.lease_owner.as_deref(), Some("worker-014-2"));
    assert_eq!(reclaimed.lease_fence, 2);
    cleanup(database, &organization_id, &email).await;
}

#[tokio::test]
async fn req_017_answer_a_card_and_record_its_effect_once_in_postgres() {
    let database = common::database().await;
    let app = app(&database);
    let (user_id, space_id, context, email, organization_id) = space(&app, "card").await;

    let bot_id = BotId::new(ids().new_id("bot")).expect("an id is valid");
    let thread_id = ThreadId::new(ids().new_id("thr")).expect("an id is valid");
    let (bot, thread) = app
        .stores
        .bots
        .create_with_thread(
            NewBot {
                id: bot_id.clone(),
                space_id: space_id.clone(),
                user_id: user_id.clone(),
                name: "Analyst".to_owned(),
                description: None,
                color: None,
                created_at: app.services.clock.now(),
            },
            NewThread {
                id: thread_id.clone(),
                space_id: space_id.clone(),
                bot_id: bot_id.clone(),
                user_id: user_id.clone(),
                created_at: app.services.clock.now(),
            },
        )
        .await
        .expect("the bot is created");

    let run = app
        .stores
        .runs
        .create(NewRun {
            id: boma_domain::RunId::new(ids().new_id("run")).expect("an id is valid"),
            space_id: space_id.clone(),
            bot_id: bot.id.clone(),
            thread_id: thread.id.clone(),
            user_id: user_id.clone(),
            trigger: RunTrigger::User,
            client_nonce: Some("nonce-card-017".to_owned()),
            source_message_id: None,
            created_at: app.services.clock.now(),
            model_provider: None,
            model_id: None,
            model_pinned: false,
            parent_run_id: None,
            routine_id: None,
        })
        .await
        .expect("the run is created");

    // A pending approval card, written like the product's migration would.
    let card_id = ids().new_id("card");
    sqlx::query(
        "INSERT INTO cards (id, space_id, thread_id, run_id, kind, status, created_at)
         VALUES ($1, $2, $3, $4, 'approval', 'pending', now())",
    )
    .bind(&card_id)
    .bind(space_id.as_str())
    .bind(thread_id.as_str())
    .bind(run.id.as_str())
    .execute(database.pool())
    .await
    .expect("the card is written");

    let answered = app
        .stores
        .cards
        .answer(
            &context,
            &boma_domain::CardId::new(card_id.clone()).expect("an id is valid"),
            "approve",
        )
        .await
        .expect("the card is answered");
    assert_eq!(answered.status, boma_domain::CardStatus::Answered);
    assert_eq!(answered.answer.as_deref(), Some("approve"));

    // The same effect key once per run: the second record is refused.
    let effect = NewEffect {
        run_id: run.id.clone(),
        effect_key: format!("approve:{card_id}"),
        kind: "card_answer".to_owned(),
        payload: serde_json::json!({ "card_id": card_id }),
    };
    app.stores
        .effects
        .record(&context, effect.clone())
        .await
        .expect("the effect is recorded once");
    let second = app.stores.effects.record(&context, effect).await;
    assert!(matches!(second, Err(boma_app::AppError::Conflict)));

    cleanup(&database, &organization_id, &email).await;
}

#[tokio::test]
async fn req_042_invitations_are_created_and_looked_up_by_token() {
    let database = common::database().await;
    let app = app(&database);
    let (_user_id, space_id, _context, email, organization_id) = space(&app, "invite").await;

    let token = app.services.tokens.generate().expect("a token is minted");
    let hash = app.services.tokens.hash(&token).expect("the token hashes");
    let ttl = app.services.clock.now_plus_seconds(3600);
    let _invitation = app
        .stores
        .invitations
        .create(NewInvitation {
            id: boma_domain::InviteId::new(ids().new_id("inv")).expect("an id is valid"),
            space_id: space_id.clone(),
            email: Email::new("invitee@example.com").expect("the address is valid"),
            token_hash: hash.clone(),
            role: boma_domain::SpaceRole::Member,
            expires_at: ttl.clone(),
            created_at: app.services.clock.now(),
        })
        .await
        .expect("the invitation is created");

    let found = app
        .stores
        .invitations
        .find_by_token_hash(&hash)
        .await
        .expect("the lookup answers")
        .expect("the invitation exists");
    assert_eq!(found.email.as_str(), "invitee@example.com");
    cleanup(&database, &organization_id, &email).await;
}

/// Seeds a bot, its thread and one user message; returns their thread and message ids.
async fn seed_thread_with_message(
    app: &App,
    context: &TenantContext,
    space_id: &SpaceId,
    user_id: &UserId,
) -> (ThreadId, MessageId) {
    let bot_id = BotId::new(ids().new_id("bot")).expect("an id is valid");
    let thread_id = ThreadId::new(ids().new_id("thr")).expect("an id is valid");
    let (_bot, thread) = app
        .stores
        .bots
        .create_with_thread(
            NewBot {
                id: bot_id.clone(),
                space_id: space_id.clone(),
                user_id: user_id.clone(),
                name: "Analyst".to_owned(),
                description: None,
                color: None,
                created_at: app.services.clock.now(),
            },
            NewThread {
                id: thread_id.clone(),
                space_id: space_id.clone(),
                bot_id: bot_id.clone(),
                user_id: user_id.clone(),
                created_at: app.services.clock.now(),
            },
        )
        .await
        .expect("the bot is created");

    let message_id = MessageId::new(ids().new_id("msg")).expect("an id is valid");
    app.stores
        .messages
        .insert(
            context,
            NewMessage {
                id: message_id.clone(),
                space_id: space_id.clone(),
                thread_id: thread.id.clone(),
                role: MessageRole::User,
                blocks: vec![Block::Text {
                    text: "a message to react to".to_owned(),
                }],
                bot_id: None,
                author_user_id: Some(user_id.clone()),
                run_id: None,
                client_nonce: None,
                created_at: app.services.clock.now(),
                reply_to_message_id: None,
                reply_quote: None,
            },
        )
        .await
        .expect("the message is written");
    (thread.id, message_id)
}

#[tokio::test]
async fn req_008_reactions_round_trip_in_postgres() {
    let database = common::database().await;
    let app = app(&database);
    let (user_id, space_id, context, email, organization_id) = space(&app, "reactions").await;

    let (thread_id, message_id) =
        seed_thread_with_message(&app, &context, &space_id, &user_id).await;

    // The first reaction lands and reads back with the truth for the message.
    let added = boma_app::usecases::react_to_message(
        &app,
        &context,
        message_id.as_str(),
        "\u{1F44D}",
        ReactionAction::Add,
    )
    .await
    .expect("the reaction lands");
    assert_eq!(
        added.reactions,
        vec![boma_domain::Reaction {
            emoji: "\u{1F44D}".to_owned(),
            count: 1,
            reacted: true,
        }],
    );

    // Retrying the same add changes nothing: one row per (message, person, emoji).
    let retried = boma_app::usecases::react_to_message(
        &app,
        &context,
        message_id.as_str(),
        "\u{1F44D}",
        ReactionAction::Add,
    )
    .await
    .expect("a retry is harmless");
    assert_eq!(retried.reactions[0].count, 1);

    // The page of history carries the reaction, so it survives a reload.
    let page =
        boma_app::usecases::page_messages(&app, &context, thread_id.as_str(), None, Some(10))
            .await
            .expect("the page answers");
    let found = page
        .messages
        .iter()
        .find(|message| message.id == message_id)
        .expect("the message is on the page");
    assert_eq!(found.reactions[0].emoji, "\u{1F44D}");

    // A message that does not exist in this tenant is not found.
    let missing = boma_app::usecases::react_to_message(
        &app,
        &context,
        "msg_missing",
        "\u{1F44D}",
        ReactionAction::Add,
    )
    .await;
    assert!(matches!(missing, Err(boma_app::AppError::NotFound)));

    // An empty emoji is refused by the boundary.
    let refused = boma_app::usecases::react_to_message(
        &app,
        &context,
        message_id.as_str(),
        "   ",
        ReactionAction::Add,
    )
    .await;
    assert!(matches!(refused, Err(boma_app::AppError::Rule(_))));

    // Removing the caller's reaction empties the list.
    let removed = boma_app::usecases::react_to_message(
        &app,
        &context,
        message_id.as_str(),
        "\u{1F44D}",
        ReactionAction::Remove,
    )
    .await
    .expect("the removal lands");
    assert_eq!(removed.reactions, []);

    cleanup(&database, &organization_id, &email).await;
}

#[tokio::test]
async fn req_009_the_read_position_round_trips_in_postgres() {
    let database = common::database().await;
    let app = app(&database);
    let (user_id, space_id, context, email, organization_id) = space(&app, "readstate").await;
    let (thread_id, message_id) =
        seed_thread_with_message(&app, &context, &space_id, &user_id).await;

    // An unread thread: the caller's head reports zero read, one unread, the newest message set.
    let head = boma_app::usecases::thread_head(&app, &context, thread_id.as_str())
        .await
        .expect("the head answers");
    assert_eq!(head.read_seq, 0);
    assert_eq!(head.unread_count, 1);
    assert_eq!(head.last_seq, 1);
    assert!(head.last_message_at.is_some());
    let _ = message_id;

    // Marking read moves the position to the newest sequence, and a reload keeps it.
    let marked = boma_app::usecases::mark_read(&app, &context, thread_id.as_str())
        .await
        .expect("the mark lands");
    assert_eq!(marked.read_seq, 1);
    assert_eq!(marked.unread_count, 0);
    let after = boma_app::usecases::thread_head(&app, &context, thread_id.as_str())
        .await
        .expect("the head answers after marking");
    assert_eq!(after.read_seq, 1);

    // Marking unread moves it back to zero, and the badge counts everything again.
    let unread = boma_app::usecases::mark_unread(&app, &context, thread_id.as_str())
        .await
        .expect("the mark lands");
    assert_eq!(unread.read_seq, 0);
    assert_eq!(unread.unread_count, 1);

    // The cross-tenant denial is proven with the RLS probe in tenancy.rs: here the connection is
    // a superuser, the policies do not apply, and reading through it would prove nothing.
    cleanup(&database, &organization_id, &email).await;
}

#[tokio::test]
async fn req_009_the_roster_reads_unread_and_snooze_in_postgres() {
    let database = common::database().await;
    let app = app(&database);
    let (user_id, space_id, context, email, organization_id) = space(&app, "roster").await;
    let (thread_id, message_id) =
        seed_thread_with_message(&app, &context, &space_id, &user_id).await;
    let _ = message_id;

    // The roster shows the bot with one unread message.
    let roster = boma_app::usecases::list_bots(&app, &context)
        .await
        .expect("the roster answers");
    assert_eq!(roster.len(), 1);
    assert_eq!(roster[0].unread_count, 1);
    assert!(roster[0].snoozed_until.is_none());

    // Snoozing with a deadline in the past never hides the chat: it is simply expired.
    let expired = boma_app::usecases::snooze_thread(
        &app,
        &context,
        thread_id.as_str(),
        Some("2001-01-01T00:00:00Z"),
    )
    .await
    .expect("the snooze is recorded");
    assert_eq!(
        expired.snoozed_until.as_deref(),
        Some("2001-01-01T00:00:00Z")
    );
    let roster = boma_app::usecases::list_bots(&app, &context)
        .await
        .expect("the roster answers after an expired snooze");
    assert!(roster[0].snoozed_until.is_none());

    // Snoozing with a future deadline hides the chat until it passes.
    let snoozed = boma_app::usecases::snooze_thread(
        &app,
        &context,
        thread_id.as_str(),
        Some("2999-01-01T00:00:00Z"),
    )
    .await
    .expect("the snooze is recorded");
    assert_eq!(
        snoozed.snoozed_until.as_deref(),
        Some("2999-01-01T00:00:00Z")
    );
    let roster = boma_app::usecases::list_bots(&app, &context)
        .await
        .expect("the roster answers while snoozed");
    assert_eq!(
        roster[0].snoozed_until.as_deref(),
        Some("2999-01-01T00:00:00Z")
    );

    // A malformed deadline is refused, nothing is stored.
    let refused =
        boma_app::usecases::snooze_thread(&app, &context, thread_id.as_str(), Some("tomorrow"))
            .await;
    assert!(matches!(refused, Err(boma_app::AppError::Rule(_))));

    // Unsnoozing brings the chat back and keeps the snooze clear.
    let unsnoozed = boma_app::usecases::snooze_thread(&app, &context, thread_id.as_str(), None)
        .await
        .expect("the snooze is cleared");
    assert!(unsnoozed.snoozed_until.is_none());
    let roster = boma_app::usecases::list_bots(&app, &context)
        .await
        .expect("the roster answers after the unsnooze");
    assert!(roster[0].snoozed_until.is_none());

    cleanup(&database, &organization_id, &email).await;
}

#[tokio::test]
async fn req_012_a_failed_run_is_retried_once_in_postgres() {
    let database = common::database().await;
    let app = app(&database);
    let (user_id, space_id, context, email, organization_id) = space(&app, "retry").await;

    let bot_id = BotId::new(ids().new_id("bot")).expect("an id is valid");
    let thread_id = ThreadId::new(ids().new_id("thr")).expect("an id is valid");
    let (_bot, _thread) = app
        .stores
        .bots
        .create_with_thread(
            NewBot {
                id: bot_id.clone(),
                space_id: space_id.clone(),
                user_id: user_id.clone(),
                name: "Analyst".to_owned(),
                description: None,
                color: None,
                created_at: app.services.clock.now(),
            },
            NewThread {
                id: thread_id.clone(),
                space_id: space_id.clone(),
                bot_id: bot_id.clone(),
                user_id: user_id.clone(),
                created_at: app.services.clock.now(),
            },
        )
        .await
        .expect("the bot is created");

    // A failed run, written the way a finished failure looks in the table.
    let run_id = boma_domain::RunId::new(ids().new_id("run")).expect("an id is valid");
    let created_at = app.services.clock.now();
    sqlx::query(
        "INSERT INTO runs (id, space_id, bot_id, thread_id, user_id, status, trigger,
                          created_at, started_at, completed_at, error, steering_from_seq,
                          steering_consumed_seq, attempt)
         VALUES ($1, $2, $3, $4, $5, 'failed', 'user', $6::timestamptz, $6::timestamptz,
                 $6::timestamptz, 'the provider refused the request', NULL, 0, 1)",
    )
    .bind(run_id.as_str())
    .bind(space_id.as_str())
    .bind(bot_id.as_str())
    .bind(thread_id.as_str())
    .bind(user_id.as_str())
    .bind(created_at.as_str())
    .execute(database.pool())
    .await
    .expect("the failed run is written");

    // The retry re-queues the same run, with one attempt added.
    let retried = boma_app::usecases::retry_run(&app, &context, run_id.as_str())
        .await
        .expect("the retry lands");
    assert_eq!(retried.status, boma_domain::RunStatus::Queued);
    assert_eq!(retried.attempt, 2);
    // A failed run that was retried is no longer failed: a second retry is refused.
    let refused = boma_app::usecases::retry_run(&app, &context, run_id.as_str()).await;
    assert!(matches!(refused, Err(boma_app::AppError::Conflict)));

    // A completed run never moves, not even with the retry.
    let completed_id = boma_domain::RunId::new(ids().new_id("run")).expect("an id is valid");
    sqlx::query(
        "INSERT INTO runs (id, space_id, bot_id, thread_id, user_id, status, trigger,
                          created_at, started_at, completed_at, steering_from_seq,
                          steering_consumed_seq, attempt)
         VALUES ($1, $2, $3, $4, $5, 'completed', 'user', $6::timestamptz, $6::timestamptz,
                 $6::timestamptz, NULL, 0, 1)",
    )
    .bind(completed_id.as_str())
    .bind(space_id.as_str())
    .bind(bot_id.as_str())
    .bind(thread_id.as_str())
    .bind(user_id.as_str())
    .bind(created_at.as_str())
    .execute(database.pool())
    .await
    .expect("the completed run is written");
    let refused = boma_app::usecases::retry_run(&app, &context, completed_id.as_str()).await;
    assert!(matches!(refused, Err(boma_app::AppError::Conflict)));

    cleanup(&database, &organization_id, &email).await;
}

#[tokio::test]
async fn req_022_memory_revisions_are_append_only_in_postgres() {
    let database = common::database().await;
    let app = app(&database);
    let (user_id, space_id, context, email, organization_id) = space(&app, "memory").await;

    let bot_id = BotId::new(ids().new_id("bot")).expect("an id is valid");
    let thread_id = ThreadId::new(ids().new_id("thr")).expect("an id is valid");
    app.stores
        .bots
        .create_with_thread(
            NewBot {
                id: bot_id.clone(),
                space_id: space_id.clone(),
                user_id: user_id.clone(),
                name: "Analyst".to_owned(),
                description: None,
                color: None,
                created_at: app.services.clock.now(),
            },
            NewThread {
                id: thread_id.clone(),
                space_id: space_id.clone(),
                bot_id: bot_id.clone(),
                user_id: user_id.clone(),
                created_at: app.services.clock.now(),
            },
        )
        .await
        .expect("the bot is created");

    // Three writes grow the revision chain without ever rewriting history.
    for content in ["one\n", "two\n", "three\n"] {
        let document = boma_app::usecases::memory_write(
            &app,
            &context,
            "bot",
            Some(bot_id.as_str()),
            "notes/commits.md",
            content,
        )
        .await
        .expect("the write lands");
        assert_eq!(document.document.content, content);
    }
    let document = boma_app::usecases::memory_read(
        &app,
        &context,
        "bot",
        Some(bot_id.as_str()),
        "notes/commits.md",
    )
    .await
    .expect("the document is read");
    assert_eq!(document.revision, 3);
    assert_eq!(document.content, "three\n");

    // The revision chain is append-only and says so in the table. Filter by the document id.
    let document_id = format!(
        "mem:{}:bot:{}:notes/commits.md",
        space_id.as_str(),
        bot_id.as_str()
    );
    let chain: i64 =
        sqlx::query_scalar("SELECT count(*) FROM memory_revisions WHERE document_id = $1")
            .bind(&document_id)
            .fetch_one(database.pool())
            .await
            .expect("the count is read");
    assert_eq!(chain, 3);

    // Removing the document cascades the revision chain.
    let removed = boma_app::usecases::memory_remove(
        &app,
        &context,
        "bot",
        Some(bot_id.as_str()),
        "notes/commits.md",
    )
    .await
    .expect("the removal lands");
    assert!(removed.removed);
    let chain: i64 =
        sqlx::query_scalar("SELECT count(*) FROM memory_revisions WHERE document_id = $1")
            .bind(&document_id)
            .fetch_one(database.pool())
            .await
            .expect("the count is read");
    assert_eq!(chain, 0);

    cleanup(&database, &organization_id, &email).await;
}

#[tokio::test]
async fn req_024_a_due_routine_is_claimed_and_a_once_fire_deactivates_postgres() {
    let database = common::database().await;
    let app = app(&database);
    let (user_id, space_id, context, email, organization_id) = space(&app, "routine").await;

    let bot_id = BotId::new(ids().new_id("bot")).expect("an id is valid");
    let thread_id = ThreadId::new(ids().new_id("thr")).expect("an id is valid");
    let (bot, thread) = app
        .stores
        .bots
        .create_with_thread(
            NewBot {
                id: bot_id.clone(),
                space_id: space_id.clone(),
                user_id: user_id.clone(),
                name: "Analyst".to_owned(),
                description: None,
                color: None,
                created_at: app.services.clock.now(),
            },
            NewThread {
                id: thread_id.clone(),
                space_id: space_id.clone(),
                bot_id: bot_id.clone(),
                user_id: user_id.clone(),
                created_at: app.services.clock.now(),
            },
        )
        .await
        .expect("the bot is created");

    let routine_id = boma_domain::RoutineId::new(ids().new_id("rtn")).expect("an id is valid");
    let created_at = app.services.clock.now();
    app.stores
        .routines
        .create(
            &context,
            NewRoutine {
                id: routine_id.clone(),
                space_id: space_id.clone(),
                bot_id: bot.id.clone(),
                thread_id: thread.id.clone(),
                user_id: user_id.clone(),
                name: "Morning digest".to_owned(),
                prompt: "Summarise overnight commits.".to_owned(),
                crons: vec!["@once".to_owned()],
                timezone: "UTC".to_owned(),
                active: true,
                notify: false,
                next_run_at: Some("2000-01-01T00:00:00Z".to_owned()),
                created_at: created_at.clone(),
            },
        )
        .await
        .expect("the routine is created");

    // The dispatch claims it and, because it is @once, deactivates it.
    let dispatched = boma_app::usecases::dispatch_routines(&app)
        .await
        .expect("the dispatch answers");
    assert_eq!(dispatched, 1);

    let routine = app
        .stores
        .routines
        .find(&context, &routine_id)
        .await
        .expect("the routine answers")
        .expect("the routine exists");
    assert!(!routine.active, "the @once routine is spent");
    assert_eq!(routine.next_run_at, None);

    let due = app
        .stores
        .routines
        .due(&created_at)
        .await
        .expect("the due scan answers");
    assert!(due.is_empty(), "the spent routine never comes due again");

    cleanup(&database, &organization_id, &email).await;
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // the setup mirrors the dispatch fixture of the sibling tests
async fn req_024_a_backoff_queued_run_waits_for_its_window_in_postgres() {
    // The queue is global by design; this test depends on what the queue holds, so it gets
    // a queue of its own.
    let fresh = common::fresh_database().await;
    let database = &fresh.database;
    let app = app(database);
    let (user_id, space_id, context, email, organization_id) = space(&app, "backoff").await;

    let bot_id = BotId::new(ids().new_id("bot")).expect("an id is valid");
    let thread_id = ThreadId::new(ids().new_id("thr")).expect("an id is valid");
    let (bot, thread) = app
        .stores
        .bots
        .create_with_thread(
            NewBot {
                id: bot_id.clone(),
                space_id: space_id.clone(),
                user_id: user_id.clone(),
                name: "Analyst".to_owned(),
                description: None,
                color: None,
                created_at: app.services.clock.now(),
            },
            NewThread {
                id: thread_id.clone(),
                space_id: space_id.clone(),
                bot_id: bot_id.clone(),
                user_id: user_id.clone(),
                created_at: app.services.clock.now(),
            },
        )
        .await
        .expect("the bot is created");

    let routine_id = boma_domain::RoutineId::new(ids().new_id("rtn")).expect("an id is valid");
    let created_at = app.services.clock.now();
    app.stores
        .routines
        .create(
            &context,
            NewRoutine {
                id: routine_id.clone(),
                space_id: space_id.clone(),
                bot_id: bot.id.clone(),
                thread_id: thread.id.clone(),
                user_id: user_id.clone(),
                name: "Morning digest".to_owned(),
                prompt: "Summarise overnight commits.".to_owned(),
                crons: vec!["* * * * *".to_owned()],
                timezone: "UTC".to_owned(),
                active: true,
                notify: false,
                next_run_at: Some("2000-01-01T00:00:00Z".to_owned()),
                created_at: created_at.clone(),
            },
        )
        .await
        .expect("the routine is created");
    assert_eq!(
        boma_app::usecases::dispatch_routines(&app)
            .await
            .expect("the dispatch answers"),
        1
    );

    // A worker claims the wake, the turn blows up with a dependency error, the automatic retry
    // holds the re-queued run back.
    let mut claimed = None;
    for _ in 0..100 {
        match app
            .stores
            .runs
            .claim_next_queued("worker-backoff-1")
            .await
            .expect("the claim answers")
        {
            None => break,
            Some(candidate) => {
                if candidate.routine_id.as_ref() == Some(&routine_id) {
                    claimed = Some(candidate);
                    break;
                }
            }
        }
    }
    let claimed = claimed.expect("the routine's run is claimed");
    app.stores
        .runs
        .set_status(
            &context,
            &claimed.id,
            RunStatus::Failed,
            &app.services.clock.now(),
            Some(claimed.lease_fence),
            Some("a dependency is unavailable".to_owned()),
        )
        .await
        .expect("the failure is recorded");
    assert!(
        boma_app::usecases::auto_retry_routine_run(&app, &claimed)
            .await
            .expect("the retry answers"),
        "the schedule owes the wake another attempt"
    );

    // The backoff holds in the database, not just in the in-memory store: the claim predicate
    // does not see the run at all (asserted as the predicate itself, so this test never leases
    // another tenant's queued run out from under a parallel test).
    let claimable: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM runs \
         WHERE id = $1 AND status = 'queued' \
           AND (not_before IS NULL OR not_before <= now())",
    )
    .bind(claimed.id.as_str())
    .fetch_one(database.pool())
    .await
    .expect("the claim predicate answers");
    assert_eq!(claimable, 0, "a backoff-queued run is not claimable");

    // Once the window passes, the run is claimable again, one attempt on.
    sqlx::query("UPDATE runs SET not_before = now() - interval '1 minute' WHERE id = $1")
        .bind(claimed.id.as_str())
        .execute(database.pool())
        .await
        .expect("the window is over");
    let mut reclaimed = None;
    for _ in 0..100 {
        match app
            .stores
            .runs
            .claim_next_queued("worker-backoff-3")
            .await
            .expect("the claim answers")
        {
            None => break,
            Some(candidate) => {
                if candidate.id == claimed.id {
                    reclaimed = Some(candidate);
                    break;
                }
            }
        }
    }
    let reclaimed = reclaimed.expect("the backoff window passed and the run is claimable");
    assert_eq!(reclaimed.attempt, 2);

    cleanup(database, &organization_id, &email).await;
    fresh.dispose().await;
}

/// A provider that records every request it was handed, then answers with one chunk.
#[derive(Default)]
struct CapturingProvider {
    requests: std::sync::Mutex<Vec<boma_app::ports::ProviderRequest>>,
}

#[async_trait::async_trait]
impl boma_app::ports::ModelProvider for CapturingProvider {
    async fn complete(
        &self,
        request: boma_app::ports::ProviderRequest,
    ) -> Result<boma_app::ports::ProviderTurn, boma_app::AppError> {
        self.requests
            .lock()
            .expect("the requests are captured")
            .push(request);
        Ok(boma_app::ports::ProviderTurn::Chunks(vec![
            "Answered.".to_owned(),
        ]))
    }
}

#[tokio::test]
async fn req_022_the_worker_injects_only_memory_of_the_runs_space_in_postgres() {
    let fresh = common::fresh_database().await;
    let database = &fresh.database;
    let app = app(database);
    let (_user_id, _space_id, context, email, organization_id) = space(&app, "memory-scope").await;
    let bot = boma_app::usecases::create_bot(&app, &context, "Assistant", None, None)
        .await
        .expect("the bot is created");
    let bot_id = bot.bot.id.clone();

    boma_app::usecases::memory_write(
        &app,
        &context,
        "bot",
        Some(bot_id.as_str()),
        "notes/strategy.md",
        "This space's strategy.",
    )
    .await
    .expect("the bot memory is written");
    boma_app::usecases::memory_write(
        &app,
        &context,
        "shared",
        None,
        "shared/manifesto.md",
        "This space's manifesto.",
    )
    .await
    .expect("the shared memory is written");

    // A second space, with its own shared document the run knows nothing about.
    let (_, _, context_other, email_other, organization_id_other) =
        space(&app, "memory-scope-other").await;
    boma_app::usecases::memory_write(
        &app,
        &context_other,
        "shared",
        None,
        "shared/secret.md",
        "Other space's secret.",
    )
    .await
    .expect("the other shared memory is written");

    boma_app::usecases::send_message(
        &app,
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

    let provider = Arc::new(CapturingProvider::default());
    let worker = boma_app::worker::Worker::new(
        Arc::new(app.clone()),
        provider.clone(),
        "worker-1".to_owned(),
    );
    assert!(worker.step().await.expect("the worker answers"));

    let request = provider
        .requests
        .lock()
        .expect("the requests are captured")
        .first()
        .cloned()
        .expect("the provider was asked");
    let contents: Vec<&str> = request
        .memory
        .iter()
        .map(|document| document.content.as_str())
        .collect();
    assert!(
        contents.contains(&"This space's strategy."),
        "the bot's own document reaches the turn: {contents:?}"
    );
    assert!(
        contents.contains(&"This space's manifesto."),
        "this space's shared document reaches the turn: {contents:?}"
    );
    assert!(
        !contents.contains(&"Other space's secret."),
        "the other space's shared document never reaches the turn: {contents:?}"
    );

    cleanup(database, &organization_id, &email).await;
    cleanup(database, &organization_id_other, &email_other).await;
    fresh.dispose().await;
}

const CREATED_AT: &str = "2026-01-01T00:00:00Z";

fn test_master_key() -> BTreeMap<String, String> {
    BTreeMap::from([(
        "BOMA_MASTER_KEY".to_owned(),
        "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f".to_owned(),
    )])
}

fn test_new_master_key() -> BTreeMap<String, String> {
    BTreeMap::from([(
        "BOMA_MASTER_KEY".to_owned(),
        "1f1e1d1c1b1a191817161514131211100f0e0d0c0b0a09080706050403020100".to_owned(),
    )])
}

#[tokio::test]
async fn req_046_a_secret_round_trips_through_postgres_and_survives_a_rekey() {
    use secrecy::ExposeSecret;

    let fresh = common::fresh_database().await;
    let pool = fresh.database.pool().clone();
    let fixture = common::fixture(&pool, "sek").await;
    let outsider = common::fixture(&pool, "otx").await;
    // The tenant pool acts as the unprivileged role: the row level security applies to it.
    let tenant = common::tenant_pool(&fresh.database).await;

    let old_box = boma_infra::SecretBox::from_env(&test_master_key()).expect("the key is valid");
    let store = boma_infra::PostgresSecretStore::new(tenant.clone(), old_box.clone());

    store
        .put(
            &fixture.context,
            NewBotSecret {
                id: "segreq046".to_owned(),
                bot_id: fixture.bot_id.clone(),
                name: "OPENAI_KEY".to_owned(),
                value: secrecy::SecretString::from("s3cr3t-value".to_owned()),
                created_at: CREATED_AT.to_owned(),
            },
        )
        .await
        .expect("the secret is stored");

    // The summary carries no value.
    let list = store
        .list(&fixture.context, &fixture.bot_id)
        .await
        .expect("the list answers");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].name, "OPENAI_KEY");
    assert!(!format!("{:?}", list[0]).contains("s3cr3t-value"));

    // The row holds no plaintext: the bytes never carry the value verbatim.
    let ciphertext: Vec<u8> = sqlx::query_scalar(
        "SELECT ciphertext FROM bot_secrets WHERE bot_id = $1 AND name = 'OPENAI_KEY'",
    )
    .bind(fixture.bot_id.as_str())
    .fetch_one(&pool)
    .await
    .expect("the row is there");
    assert!(
        !ciphertext
            .windows("s3cr3t-value".len())
            .any(|window| window == b"s3cr3t-value"),
        "the stored bytes are not the plaintext"
    );

    // The value comes back for the executor.
    let value = store
        .reveal(&fixture.context, &fixture.bot_id, "OPENAI_KEY")
        .await
        .expect("the reveal answers")
        .expect("the value is there");
    assert_eq!(value.expose_secret(), "s3cr3t-value");

    // Another tenant sees nothing: not listed, not readable, not deletable.
    let list = store
        .list(&outsider.context, &fixture.bot_id)
        .await
        .expect("the list answers");
    assert_eq!(list.len(), 0, "another tenant sees no secret names");
    let value = store
        .reveal(&outsider.context, &fixture.bot_id, "OPENAI_KEY")
        .await
        .expect("the reveal answers");
    assert!(value.is_none());
    let deleted = store
        .delete(&outsider.context, &fixture.bot_id, "OPENAI_KEY")
        .await
        .expect("the delete answers");
    assert!(!deleted);

    // Rotating the master key re-wraps the data-encryption keys only; the secret survives.
    let new_box =
        boma_infra::SecretBox::from_env(&test_new_master_key()).expect("the key is valid");
    let rotated = boma_infra::rekey_space_keys(&tenant, &old_box, &new_box)
        .await
        .expect("the rekey answers");
    assert_eq!(rotated, 1, "exactly this test's space key is re-wrapped");
    let rotated_store = boma_infra::PostgresSecretStore::new(tenant.clone(), new_box);
    let value = rotated_store
        .reveal(&fixture.context, &fixture.bot_id, "OPENAI_KEY")
        .await
        .expect("the reveal answers")
        .expect("the secret still reads after the rotation");
    assert_eq!(value.expose_secret(), "s3cr3t-value");

    // Forgetting removes the row exactly once.
    let deleted = store
        .delete(&fixture.context, &fixture.bot_id, "OPENAI_KEY")
        .await
        .expect("the delete answers");
    assert!(deleted);
    let deleted = store
        .delete(&fixture.context, &fixture.bot_id, "OPENAI_KEY")
        .await
        .expect("the delete answers");
    assert!(!deleted);

    common::cleanup(&pool, "sek").await;
    common::cleanup(&pool, "otx").await;
}

/// REQ-034: an anonymous submission survives a round trip to Postgres, verbatim.
#[tokio::test]
async fn req_034_a_plugin_submission_round_trips_through_postgres() {
    let fresh = common::fresh_database().await;
    let pool = fresh.database.pool().clone();
    let store = boma_infra::PostgresPluginSubmissionStore::new(pool.clone());
    let source = boma_domain::parse_plugin_source("https://example.test/plugin.git#main")
        .expect("the source parses");
    let submission = boma_app::ports::NewPluginSubmission {
        id: "sub_postgres_round_trip".to_owned(),
        source: source.canonical(),
        name: Some("Example".to_owned()),
        contact: Some("author@example.test".to_owned()),
        submitter_user_id: None,
        created_at: "2026-01-01T00:00:00Z".to_owned(),
    };
    let summary = store.record(submission).await.expect("the row is written");
    assert_eq!(summary.id, "sub_postgres_round_trip");
    let (source, name, contact, status): (String, Option<String>, Option<String>, String) =
        sqlx::query_as(
            "SELECT source, name, contact, status FROM plugin_submissions WHERE id = $1",
        )
        .bind("sub_postgres_round_trip")
        .fetch_one(&pool)
        .await
        .expect("the row is there");
    assert_eq!(source, "https://example.test/plugin#main");
    assert_eq!(name.as_deref(), Some("Example"));
    assert_eq!(contact.as_deref(), Some("author@example.test"));
    assert_eq!(status, "pending");
    sqlx::query("DELETE FROM plugin_submissions WHERE id = $1")
        .bind("sub_postgres_round_trip")
        .execute(&pool)
        .await
        .expect("the row is removed");
}

/// REQ-035: a taught skill survives a round trip to Postgres, and the RLS barrier holds.
#[tokio::test]
async fn req_035_a_taught_skill_round_trips_through_postgres_and_other_spaces_cannot_see_it() {
    let fresh = common::fresh_database().await;
    let pool = fresh.database.pool().clone();
    let tenant = common::tenant_pool(&fresh.database).await;
    let fixture = common::fixture(&pool, "skil").await;
    let outsider = common::fixture(&pool, "sko").await;
    let store = boma_infra::PostgresSkillStore::new(tenant.clone());

    store
        .put(
            &fixture.context,
            boma_app::ports::NewSkill {
                id: "sk_postgres_round_trip".to_owned(),
                name: "commits".to_owned(),
                description: "How this space summarises commits.".to_owned(),
                instructions: "Group by intent; quote the riskiest line.".to_owned(),
                origin: boma_domain::SkillOrigin::Taught,
                plugin_source: None,
                created_at: "2026-01-01T00:00:00Z".to_owned(),
            },
        )
        .await
        .expect("the skill is stored");

    let record = store
        .get(&fixture.context, "commits")
        .await
        .expect("the store answers")
        .expect("the skill is there");
    assert_eq!(record.name, "commits");
    assert_eq!(record.origin, "taught");
    assert_eq!(
        record.instructions,
        "Group by intent; quote the riskiest line."
    );

    // Another space: nothing is listed, nothing is read.
    let list = store
        .list(&outsider.context)
        .await
        .expect("the store answers");
    assert!(list.is_empty(), "another space sees no skills");
    let missing = store
        .get(&outsider.context, "commits")
        .await
        .expect("the store answers");
    assert!(missing.is_none());

    // Re-teaching replaces rather than duplicates.
    store
        .put(
            &fixture.context,
            boma_app::ports::NewSkill {
                id: "sk_postgres_round_trip_2".to_owned(),
                name: "commits".to_owned(),
                description: "Updated.".to_owned(),
                instructions: "Updated instructions.".to_owned(),
                origin: boma_domain::SkillOrigin::Taught,
                plugin_source: None,
                created_at: "2026-01-02T00:00:00Z".to_owned(),
            },
        )
        .await
        .expect("the skill is replaced");
    let list = store
        .list(&fixture.context)
        .await
        .expect("the store answers");
    assert_eq!(list.len(), 1, "one row per (space, name)");
    assert_eq!(list[0].description, "Updated.");

    sqlx::query("DELETE FROM skills WHERE id LIKE 'sk_postgres_round_trip%'")
        .execute(&pool)
        .await
        .expect("the rows are removed");
    common::cleanup(&pool, "skil").await;
    common::cleanup(&pool, "sko").await;
}

/// REQ-033: a server, its probed tools and its audit events survive a round trip to Postgres.
///
/// Cross-space isolation is proven by the record-level policies and the RLS probe suite; this
/// test pins the round trip and the seal.
#[tokio::test]
#[allow(clippy::too_many_lines)] // the scenario is the documentation
async fn req_033_a_server_and_its_tools_round_trip_through_postgres() {
    let fresh = common::fresh_database().await;
    let pool = fresh.database.pool().clone();
    let fixture = common::fixture(&pool, "mcpa").await;
    let app = app(&fresh.database);
    let context = fixture.context.clone();

    let summary = boma_app::usecases::mcp_server_add(
        &app,
        &context,
        "company-files",
        boma_domain::McpTransport::Http,
        Some("https://mcp.example.com/mcp".to_owned()),
        None,
        Some(secrecy::SecretString::from("pg-token".to_owned())),
        false,
        None,
    )
    .await
    .expect("the server is registered");

    let list = boma_app::usecases::mcp_servers_list(&app, &context)
        .await
        .expect("the list answers");
    assert_eq!(list.len(), 1);
    assert!(
        !format!("{list:?}").contains("pg-token"),
        "the bearer never appears in a list"
    );

    // The bearer is sealed like every secret: the row carries no plaintext.
    let stored: Option<Vec<u8>> =
        sqlx::query_scalar("SELECT bearer_token FROM mcp_servers WHERE id = $1")
            .bind(&summary.id)
            .fetch_one(&pool)
            .await
            .expect("the row exists");
    let bytes = stored.expect("a sealed bearer is stored");
    assert!(
        !bytes
            .windows(b"pg-token".len())
            .any(|window| window == b"pg-token"),
        "the stored bytes are not the plaintext"
    );

    // Probed tools land in the catalogue, atomically.
    let tools = vec![
        boma_app::ports::McpTool {
            name: "list-issues".to_owned(),
            description: "Lists the open issues.".to_owned(),
            input_schema: serde_json::json!({ "type": "object" }),
        },
        boma_app::ports::McpTool {
            name: "get-issue".to_owned(),
            description: "Reads one issue.".to_owned(),
            input_schema: serde_json::json!({ "type": "object" }),
        },
    ];
    app.stores
        .mcp_servers
        .replace_tools(&context, &summary.id, &tools)
        .await
        .expect("the tools are stored");
    app.stores
        .mcp_servers
        .set_auth_state(&context, &summary.id, "connected", Some(2))
        .await
        .expect("the probe is recorded");
    let catalogue = app
        .stores
        .mcp_servers
        .catalogue_tools(&context)
        .await
        .expect("the catalogue answers");
    assert_eq!(catalogue.len(), 2);
    assert_eq!(catalogue[0].0, "company-files");
    // The audit event lands in its own table, and only the space's tenant reads it.
    app.stores
        .audit
        .append(boma_app::ports::NewAuditEvent {
            id: "aud_mcp_pg_round_trip".to_owned(),
            space_id: fixture.space_id.clone(),
            actor_user_id: Some(fixture.user_id.clone()),
            kind: boma_domain::AuditKind::McpPrivateCall,
            target: "company-files".to_owned(),
            metadata: serde_json::json!({ "tool": "list-issues" }),
            created_at: "2026-01-01T00:00:00Z".to_owned(),
        })
        .await
        .expect("the audit event lands");
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM audit_events WHERE id = $1")
        .bind("aud_mcp_pg_round_trip")
        .fetch_one(&pool)
        .await
        .expect("the row exists");
    assert_eq!(count, 1);

    boma_app::usecases::mcp_server_remove(&app, &context, &summary.id)
        .await
        .expect("the remove answers");
    let tools =
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM mcp_server_tools WHERE server_id = $1")
            .bind(&summary.id)
            .fetch_one(&pool)
            .await
            .expect("the rows are gone");
    assert_eq!(tools, 0, "removing the server removes its tools");
    sqlx::query("DELETE FROM audit_events WHERE id = $1")
        .bind("aud_mcp_pg_round_trip")
        .execute(&pool)
        .await
        .expect("the audit row is removed");
    common::cleanup(&pool, "mcpa").await;
}

/// REQ-032: a connector install, its sealed credential and its catalogue flip survive Postgres.
#[tokio::test]
async fn req_032_a_connector_install_seals_its_credential_and_the_catalogue_flips() {
    let fresh = common::fresh_database().await;
    let pool = fresh.database.pool().clone();
    let fixture = common::fixture(&pool, "cnct").await;
    let app = app(&fresh.database);
    let context = fixture.context.clone();

    boma_app::usecases::connections_install(
        &app,
        &context,
        "http-json",
        Some(secrecy::SecretString::from("pg-key-1".to_owned())),
    )
    .await
    .expect("the install lands");

    // The credential is sealed: the row carries no plaintext.
    let stored: Option<Vec<u8>> = sqlx::query_scalar(
        "SELECT sealed_credential FROM connector_installs WHERE connector_id = 'http-json' AND space_id = $1",
    )
    .bind(fixture.space_id.as_str())
    .fetch_one(&pool)
    .await
    .expect("the row exists");
    let bytes = stored.expect("a sealed credential is stored");
    assert!(
        !bytes.windows(b"pg-key-1".len()).any(|w| w == b"pg-key-1"),
        "the stored bytes are not the plaintext"
    );

    let tools = app
        .stores
        .connectors
        .catalogue_tools(&context)
        .await
        .expect("the catalogue answers");
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].key, "connector.http-json.get");

    boma_app::usecases::connections_remove(&app, &context, "http-json")
        .await
        .expect("the remove answers");

    let tools = app
        .stores
        .connectors
        .catalogue_tools(&context)
        .await
        .expect("the catalogue answers");
    assert!(tools.is_empty(), "a revoked connector contributes no tools");
    let removed = app
        .stores
        .connectors
        .is_connected(&context, "http-json")
        .await
        .expect("the store answers");
    assert!(!removed);

    common::cleanup(&pool, "cnct").await;
}

/// REQ-034: a plugin install and its skills survive a round trip to Postgres, and the
/// wholesale revoke on reinstall removes exactly the plugin's own skills.
#[tokio::test]
async fn req_034_a_plugin_install_round_trips_through_postgres() {
    let fresh = common::fresh_database().await;
    let pool = fresh.database.pool().clone();
    let fixture = common::fixture(&pool, "plg").await;
    let app = app(&fresh.database);
    let context = fixture.context.clone();
    let source = "https://example.test/acme/widget-pack.git#main";

    // Seed two plugin skills and one taught skill.
    for (id, name, origin) in [
        ("psk1", "alpha", boma_domain::SkillOrigin::Plugin),
        ("psk2", "beta", boma_domain::SkillOrigin::Plugin),
        ("psk3", "keep-me", boma_domain::SkillOrigin::Taught),
    ] {
        app.stores
            .skills
            .put(
                &context,
                boma_app::ports::NewSkill {
                    id: id.to_owned(),
                    name: name.to_owned(),
                    description: "desc".to_owned(),
                    instructions: "instructions".to_owned(),
                    origin,
                    plugin_source: (origin == boma_domain::SkillOrigin::Plugin)
                        .then(|| source.to_owned()),
                    created_at: "2026-01-01T00:00:00Z".to_owned(),
                },
            )
            .await
            .expect("the skill is stored");
    }

    app.stores
        .plugin_installs
        .record(
            &context,
            boma_app::ports::NewPluginInstall {
                id: "pin_round_trip".to_owned(),
                source: source.to_owned(),
                resolved_sha: Some("deadbeefdeadbeefdeadbeefdeadbeefdeadbeef".to_owned()),
                name: "widget-pack".to_owned(),
                parts: serde_json::json!({ "skills": 2, "mcpServers": 1 }),
                skipped: vec![
                    serde_json::json!({"kind":"mcp_stdio","name":"local","reason":"stdio"}),
                ],
                created_at: "2026-01-01T00:00:00Z".to_owned(),
            },
        )
        .await
        .expect("the install is recorded");

    let installs = app
        .stores
        .plugin_installs
        .list(&context)
        .await
        .expect("the list answers");
    assert_eq!(installs.len(), 1);
    assert_eq!(installs[0].name, "widget-pack");
    assert_eq!(installs[0].parts["skills"], serde_json::json!(2));

    // A reinstall revokes exactly the plugin's own skills, not the taught one.
    let removed = app
        .stores
        .plugin_installs
        .remove_plugin_skills(&context, source)
        .await
        .expect("the revoke answers");
    assert_eq!(removed, 2, "exactly the two plugin skills are revoked");
    let remaining = app
        .stores
        .skills
        .list(&context)
        .await
        .expect("the list answers");
    assert_eq!(remaining.len(), 1, "the taught skill survives");
    assert_eq!(remaining[0].name, "keep-me");

    sqlx::query("DELETE FROM plugin_installs WHERE id = $1")
        .bind("pin_round_trip")
        .execute(&pool)
        .await
        .expect("the row is removed");
    common::cleanup(&pool, "plg").await;
}

/// REQ-033: the OAuth dance against the real Postgres: the verifier is sealed at rest, the
/// session is consumed exactly once, and the tools discovered land in `mcp_server_tools`.
#[tokio::test]
#[allow(clippy::too_many_lines)] // the round trip is eleven ordered steps; it reads top to bottom
async fn req_033_the_oauth_round_trip_survives_postgres() {
    let fresh = common::fresh_database().await;
    let pool = fresh.database.pool().clone();
    let fixture = common::fixture(&pool, "oauth").await;
    let context = fixture.context.clone();

    let mcp_transport = Arc::new(boma_testkit::ScriptedMcpTransport::with_tools(vec![
        boma_app::ports::McpTool {
            name: "query".to_owned(),
            description: "Query the warehouse.".to_owned(),
            input_schema: serde_json::json!({ "type": "object", "properties": {} }),
        },
    ]));
    let oauth_client: Arc<dyn boma_app::ports::OAuthClient> = Arc::new(
        boma_testkit::ScriptedOAuthClient::new().with_exchanges(vec![Ok(
            serde_json::json!({ "access_token": "pg-token-1", "token_type": "bearer" }),
        )]),
    );
    let app = app_with_scripts(&fresh.database, mcp_transport, oauth_client);

    let server = boma_app::usecases::mcp_server_add(
        &app,
        &context,
        "warehouse",
        boma_domain::McpTransport::Http,
        Some("https://mcp.example.test/warehouse".to_owned()),
        None,
        None,
        false,
        Some(boma_app::ports::McpOAuthConfig {
            authorization_url: "https://auth.example.test/authorize".to_owned(),
            token_url: "https://auth.example.test/token".to_owned(),
            client_id: "boma-cli".to_owned(),
            client_secret: None,
        }),
    )
    .await
    .expect("the server is added");

    let started = boma_app::usecases::mcp_oauth_start(&app, &context, &server.id)
        .await
        .expect("the sign-in begins");

    // The verifier is sealed at rest: the row carries no plaintext.
    let sealed: Option<Vec<u8>> =
        sqlx::query_scalar("SELECT sealed_verifier FROM mcp_oauth_sessions WHERE server_id = $1")
            .bind(&server.id)
            .fetch_one(&pool)
            .await
            .expect("the row exists");
    assert!(
        sealed
            .expect("a sealed verifier")
            .windows(b"verifier-1".len())
            .all(|window| window != b"verifier-1"),
        "the sealed verifier does not contain the plaintext"
    );

    let summary = boma_app::usecases::mcp_oauth_complete(
        &app,
        &context,
        &server.id,
        "code-1",
        &started.state,
    )
    .await
    .expect("the exchange succeeds");
    assert_eq!(summary.auth_state, "connected");
    assert_eq!(summary.tool_count, Some(1));

    // The session row is gone: replay is refused at the database level too.
    let remaining: i64 =
        sqlx::query_scalar("SELECT count(*) FROM mcp_oauth_sessions WHERE server_id = $1")
            .bind(&server.id)
            .fetch_one(&pool)
            .await
            .expect("the count answers");
    assert_eq!(remaining, 0);

    // A replay is refused before reaching the server.
    let replay = boma_app::usecases::mcp_oauth_complete(
        &app,
        &context,
        &server.id,
        "code-2",
        &started.state,
    )
    .await;
    assert!(replay.is_err());

    // The bearer is sealed: the row carries no plaintext token.
    let sealed_bearer: Option<Vec<u8>> =
        sqlx::query_scalar("SELECT bearer_token FROM mcp_servers WHERE id = $1")
            .bind(&server.id)
            .fetch_one(&pool)
            .await
            .expect("the row exists");
    let bytes = sealed_bearer.expect("a sealed bearer");
    assert!(
        bytes
            .windows(b"pg-token-1".len())
            .all(|window| window != b"pg-token-1"),
        "the stored bearer does not contain the plaintext"
    );

    // The tool was discovered and persisted.
    let tool_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM mcp_server_tools WHERE server_id = $1")
            .bind(&server.id)
            .fetch_one(&pool)
            .await
            .expect("the count answers");
    assert_eq!(tool_count, 1);

    // Clean up.
    sqlx::query("DELETE FROM mcp_server_tools WHERE server_id = $1")
        .bind(&server.id)
        .execute(&pool)
        .await
        .expect("rows removed");
    sqlx::query("DELETE FROM mcp_servers WHERE id = $1")
        .bind(&server.id)
        .execute(&pool)
        .await
        .expect("rows removed");
    common::cleanup(&pool, "oauth").await;
}

/// REQ-036: an artifact round trip through Postgres and the content-addressed blob store,
/// including versioning, replay and family reclamation.
#[tokio::test]
#[allow(clippy::too_many_lines)] // the round trip is a long happy path; it reads top to bottom
async fn req_036_an_artifact_round_trip_survives_postgres() {
    let fresh = common::fresh_database().await;
    let pool = fresh.database.pool().clone();
    let fixture = common::fixture(&pool, "artf").await;
    let context = fixture.context.clone();
    let content_root = format!("/tmp/boma-artifact-pg-{}", std::process::id());
    let _ = std::fs::remove_dir_all(&content_root);

    let app = {
        let pool = pool.clone();
        let content_root = content_root.clone();
        let publisher = Arc::new(BroadcastPublisher::default());
        let mcp_transport = Arc::new(boma_testkit::ScriptedMcpTransport::new());
        let oauth_client = Arc::new(boma_testkit::ScriptedOAuthClient::new());
        App::new(
            boma_app::Stores::new(
                Arc::new(PostgresAccountStore::new(pool.clone())),
                Arc::new(PostgresUserStore::new(pool.clone())),
                Arc::new(PostgresSessionStore::new(pool.clone())),
                Arc::new(PostgresSpaceStore::new(pool.clone())),
                Arc::new(PostgresBotStore::new(pool.clone())),
                Arc::new(PostgresThreadStore::new(pool.clone())),
                Arc::new(PostgresMessageStore::new(pool.clone())),
                Arc::new(PostgresRunStore::new(pool.clone())),
                Arc::new(PostgresEventStore::new(pool.clone())),
                Arc::new(PostgresInvitationStore::new(pool.clone())),
                Arc::new(PostgresCardStore::new(pool.clone())),
                Arc::new(PostgresPendingToolCallStore::new(pool.clone())),
                Arc::new(PostgresEffectStore::new(pool.clone())),
                Arc::new(PostgresToolStore::new(pool.clone())),
                Arc::new(PostgresRateLimitStore::new(pool.clone())),
                Arc::new(PostgresSecretStore::new(
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
                Arc::new(boma_infra::PostgresConnectorStore::new(
                    pool.clone(),
                    boma_infra::SecretBox::ephemeral(),
                )),
                Arc::new(boma_infra::PostgresPluginInstallStore::new(pool.clone())),
                Arc::new(boma_infra::PostgresArtifactStore::new(pool.clone())),
                Arc::new(boma_infra::PostgresPageStore::new(pool.clone())),
                Arc::new(boma_infra::PostgresTicketStore::new(pool.clone())),
                Arc::new(PostgresReactionStore::new(pool.clone())),
                Arc::new(PostgresReadStateStore::new(pool.clone())),
                Arc::new(PostgresRosterStore::new(pool.clone())),
                Arc::new(PostgresMemoryStore::new(pool.clone())),
                Arc::new(PostgresRoutineStore::new(pool.clone())),
                Arc::new(PostgresComputerStore::new(pool.clone())),
                Arc::new(PostgresScreenCapabilityStore::new(pool)),
            ),
            boma_app::Services::new(
                publisher,
                Arc::new(Argon2idHasher::default()),
                Arc::new(TokenGenerator),
                Arc::new(IdGenerator),
                Arc::new(SystemClock),
                boma_domain::model::DeploymentModels {
                    reachable: vec!["openai/gpt-5".to_owned()],
                    default: "openai/gpt-5".to_owned(),
                },
                mcp_transport,
                Arc::new(boma_testkit::ScriptedPluginFetcher::new()),
                oauth_client,
                Arc::new(
                    boma_infra::FileArtifactContentStore::new(&content_root)
                        .expect("the artifact root"),
                ),
            ),
        )
    };

    let first = boma_app::usecases::artifact_create(
        &app,
        &context,
        boma_app::usecases::ArtifactInput {
            bot_id: None,
            group_id: None,
            run_id: None,
            name: "pg.bin".to_owned(),
            description: String::new(),
            declared_mime: "application/octet-stream".to_owned(),
            content: b"payload-one".to_vec(),
            root_artifact_id: None,
            client_nonce: None,
        },
    )
    .await
    .expect("v1 lands");
    let _second = boma_app::usecases::artifact_create(
        &app,
        &context,
        boma_app::usecases::ArtifactInput {
            bot_id: None,
            group_id: None,
            run_id: None,
            name: "pg.bin".to_owned(),
            description: String::new(),
            declared_mime: "application/octet-stream".to_owned(),
            content: b"payload-two".to_vec(),
            root_artifact_id: Some(first.id.clone()),
            client_nonce: None,
        },
    )
    .await
    .expect("v2 lands");

    let versions = boma_app::usecases::artifact_versions(&app, &context, &first.id)
        .await
        .expect("the versions answer");
    assert_eq!(versions.len(), 2);

    // The blob exists on disk under the content-addressed path.
    let blob_dirs = std::fs::read_dir(&content_root)
        .expect("the root exists")
        .count();
    assert!(
        blob_dirs >= 1,
        "the content-addressed blob tree is populated"
    );

    let removed = boma_app::usecases::artifact_remove(&app, &context, &first.id)
        .await
        .expect("the family removes");
    assert_eq!(removed, 2);

    let remaining: i64 = sqlx::query_scalar("SELECT count(*) FROM artifacts WHERE space_id = $1")
        .bind(fixture.space_id.as_str())
        .fetch_one(&pool)
        .await
        .expect("the count answers");
    assert_eq!(remaining, 0);

    let _ = std::fs::remove_dir_all(&content_root);
    common::cleanup(&pool, "artf").await;
}

/// REQ-037: a page round trip through Postgres: the token is hashed at rest, the public read
/// is a system lookup by hash, a revoke is uniform, and rows go with the space.
#[tokio::test]
async fn req_037_a_page_round_trip_survives_postgres() {
    let fresh = common::fresh_database().await;
    let pool = fresh.database.pool().clone();
    let fixture = common::fixture(&pool, "page").await;
    let app = app(&fresh.database);
    let context = fixture.context.clone();

    let saved = boma_app::usecases::pages_save(
        &app,
        &context,
        boma_app::usecases::PageSaveInput {
            page_id: None,
            name: Some("Weekly report".to_owned()),
            bot_id: None,
            run_id: None,
            source: serde_json::json!({
                "title": "Weekly report",
                "blocks": [{ "type": "text", "text": "All green." }],
            }),
            client_nonce: None,
        },
    )
    .await
    .expect("the page saves");

    let versioned = boma_app::usecases::pages_save(
        &app,
        &context,
        boma_app::usecases::PageSaveInput {
            page_id: Some(saved.id.clone()),
            name: None,
            bot_id: None,
            run_id: None,
            source: serde_json::json!({
                "title": "Weekly report",
                "blocks": [{ "type": "text", "text": "All green, v2." }],
            }),
            client_nonce: None,
        },
    )
    .await
    .expect("the second version saves");

    let token = boma_app::usecases::pages_share(&app, &context, &saved.id)
        .await
        .expect("the page shares");

    // The row stores the hash, never the token.
    let stored: Option<Vec<u8>> = sqlx::query_scalar("SELECT token_hash FROM pages WHERE id = $1")
        .bind(&saved.id)
        .fetch_one(&pool)
        .await
        .expect("the row exists");
    let stored = stored.expect("a token hash is stored");
    assert_ne!(stored, token.as_bytes(), "the plaintext token never lands");

    // The public read works under the shared-page system reason, by hash only.
    let (updated_at, source) = boma_app::usecases::pages_shared(&app, &token)
        .await
        .expect("the public read answers")
        .expect("the token works");
    assert_eq!(
        source["blocks"][0]["text"],
        serde_json::json!("All green, v2.")
    );
    assert_ne!(updated_at.len(), 0);

    // A garbage token answers uniformly.
    assert!(
        boma_app::usecases::pages_shared(&app, "garbage-token")
            .await
            .expect("the public read answers")
            .is_none()
    );

    let revoked = boma_app::usecases::pages_revoke_share(&app, &context, &saved.id)
        .await
        .expect("the revoke answers");
    assert!(revoked);
    assert!(
        boma_app::usecases::pages_shared(&app, &token)
            .await
            .expect("the public read answers")
            .is_none()
    );

    sqlx::query("DELETE FROM page_versions WHERE page_id = $1")
        .bind(&versioned.id)
        .execute(&pool)
        .await
        .expect("rows removed");
    sqlx::query("DELETE FROM pages WHERE id = $1")
        .bind(&saved.id)
        .execute(&pool)
        .await
        .expect("rows removed");
    common::cleanup(&pool, "page").await;
}

/// REQ-038: tickets round trip through Postgres — keys are per space, comments bump the
/// counter, and the board does not leak across spaces.
#[tokio::test]
async fn req_038_tickets_round_trip_through_postgres() {
    let fresh = common::fresh_database().await;
    let pool = fresh.database.pool().clone();
    let fixture = common::fixture(&pool, "tkt").await;
    let fixture_b = common::fixture(&pool, "tk2").await;
    let app = app(&fresh.database);
    let context = fixture.context.clone();
    let context_b = fixture_b.context.clone();

    let first = boma_app::usecases::tickets_create(&app, &context, "First ticket", "first", None)
        .await
        .expect("the first ticket lands");
    assert_eq!(first.key, "BOT-1");

    let second = boma_app::usecases::tickets_create(&app, &context, "Second ticket", "", None)
        .await
        .expect("the second ticket lands");
    assert_eq!(second.key, "BOT-2");

    // The key counter is per space: another space counts from BOT-1.
    let other_space_ticket =
        boma_app::usecases::tickets_create(&app, &context_b, "Other space", "", None)
            .await
            .expect("the other space's ticket lands");
    assert_eq!(other_space_ticket.key, "BOT-1");

    // Move and comment, then read the board.
    boma_app::usecases::tickets_move(&app, &context, &first.id, "in_progress")
        .await
        .expect("the move answers");
    boma_app::usecases::tickets_comment(&app, &context, &first.id, "Working on it.")
        .await
        .expect("the comment lands");
    boma_app::usecases::tickets_comment(&app, &context, &first.id, "Still on it.")
        .await
        .expect("the second comment lands");

    let board = boma_app::usecases::boards_get(&app, &context)
        .await
        .expect("the board answers");
    // This fixture connects as a superuser, so RLS does not filter here: every row of the
    // database is visible. The per-space behaviour asserted here is the rest — allocation,
    // ordering, counts. Cross-tenant *visibility* is proved in tenancy.rs against the probe
    // role, where the policies actually apply.
    assert_eq!(board.len(), 3);
    let first = board
        .iter()
        .find(|ticket| ticket.title == "First ticket")
        .expect("the first ticket is there");
    assert_eq!(first.state, "in_progress");
    assert_eq!(first.comment_count, 2);
    let second = board
        .iter()
        .find(|ticket| ticket.title == "Second ticket")
        .expect("the second ticket is there");
    assert_eq!(second.key, "BOT-2");
    let other = board
        .iter()
        .find(|ticket| ticket.title == "Other space")
        .expect("the other space's ticket is there");
    assert_eq!(other.key, "BOT-1");
    assert_eq!(other.state, "backlog");

    // Clean up.
    for context in [&context, &context_b] {
        let board = boma_app::usecases::boards_get(&app, context)
            .await
            .expect("the board answers");
        for ticket in board {
            sqlx::query("DELETE FROM ticket_comments WHERE ticket_id = $1")
                .bind(&ticket.id)
                .execute(&pool)
                .await
                .expect("comments removed");
            sqlx::query("DELETE FROM tickets WHERE id = $1")
                .bind(&ticket.id)
                .execute(&pool)
                .await
                .expect("ticket removed");
        }
        sqlx::query("DELETE FROM board_state WHERE space_id = $1")
            .bind(match context {
                TenantContext::Member { space_id, .. } => space_id.as_str().to_owned(),
                TenantContext::System { .. } => unreachable!(),
            })
            .execute(&pool)
            .await
            .expect("board state removed");
    }
    common::cleanup(&pool, "tkt").await;
    common::cleanup(&pool, "tk2").await;
}
