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
    BotId, Email, MessageId, MessageRole, RunStatus, RunTrigger, SpaceId, TenantContext, ThreadId,
    UserId,
};
use boma_infra::{
    Argon2idHasher, BroadcastPublisher, IdGenerator, Postgres, PostgresAccountStore,
    PostgresBotStore, PostgresCardStore, PostgresEffectStore, PostgresEventStore,
    PostgresInvitationStore, PostgresMessageStore, PostgresRunStore, PostgresSessionStore,
    PostgresSpaceStore, PostgresThreadStore, PostgresToolStore, PostgresUserStore, SystemClock,
    TokenGenerator,
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
            Arc::new(PostgresEffectStore::new(pool.clone())),
            Arc::new(PostgresToolStore::new(pool)),
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
