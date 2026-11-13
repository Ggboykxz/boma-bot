//! Integration tests for the M1 Postgres adapters, against a real database.
//!
//! The in-memory testkit proves the use cases; these prove the SQL: the migrations, the
//! allocators, the foreign keys and the tenant context.

// An integration test file is compiled as its own crate, so clippy's `allow-*-in-tests` (which
// covers `#[test]` functions and `#[cfg(test)]` modules only) does not reach the helpers below.
// AGENTS.md allows unwrap, expect and panic in test code; this is where that permission is taken,
// once and in writing.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::sync::Arc;

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
    PostgresScreenCapabilityStore, PostgresSessionStore, PostgresSpaceStore, PostgresThreadStore,
    PostgresToolStore, PostgresUserStore, SystemClock, TokenGenerator,
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

fn app(database: &Postgres) -> App {
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
        ),
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
