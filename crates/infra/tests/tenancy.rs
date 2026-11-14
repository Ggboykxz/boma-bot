//! The row level security, against a real database (REQ-043).
//!
//! Two spaces, a bot in each, and the proof that the first one cannot see the second one. The
//! proof is only worth something when the connection is subject to the policies: PostgreSQL
//! never applies them to a superuser, so the tests act as a role that has no privilege.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

mod common;

use boma_app::ports::{BotStore, MessageStore, ReadStateStore, RosterStore, TicketStore};
use boma_domain::{SystemReason, TenantContext};
use boma_infra::{PostgresBotStore, PostgresMessageStore, PostgresReadStateStore};

#[tokio::test]
async fn req_043_rls_hides_another_tenants_rows() {
    let database = common::database().await;
    let pool = common::tenant_pool(&database).await;
    let prefix = common::unique_id();

    let first = common::fixture(&pool, &format!("{prefix}a")).await;
    let second = common::fixture(&pool, &format!("{prefix}b")).await;
    common::message(&pool, &first, None).await;
    common::message(&pool, &second, None).await;

    let bots = PostgresBotStore::new(pool.clone());
    let messages = PostgresMessageStore::new(pool.clone());

    // The tenant sees its own bot.
    assert!(
        bots.find(&first.context, &first.bot_id)
            .await
            .expect("the bot is read")
            .is_some(),
        "the tenant sees its own bot"
    );

    // The tenant does not see the bot of another tenant: it is *not found*, never forbidden.
    assert!(
        bots.find(&first.context, &second.bot_id)
            .await
            .expect("the bot is read")
            .is_none(),
        "the tenant does not see the bot of another tenant"
    );

    // The tenant does not see the messages of another tenant.
    let page = messages
        .page(&first.context, &second.thread_id, None, 50)
        .await
        .expect("the page is read");
    assert!(
        page.messages.is_empty(),
        "the tenant does not see the messages of another tenant"
    );

    // A query without a tenant context sees nothing at all: that is the intended failure mode.
    let visible: i64 = sqlx::query_scalar("SELECT count(*) FROM bots")
        .fetch_one(&pool)
        .await
        .expect("the count is read");
    assert_eq!(visible, 0, "a query without a tenant context sees nothing");

    let visible: i64 = sqlx::query_scalar("SELECT count(*) FROM messages")
        .fetch_one(&pool)
        .await
        .expect("the count is read");
    assert_eq!(visible, 0, "a query without a tenant context sees nothing");

    common::cleanup(&pool, &prefix).await;
}

#[tokio::test]
async fn req_043_a_system_context_reaches_every_tenant() {
    let database = common::database().await;
    let pool = common::tenant_pool(&database).await;
    let prefix = common::unique_id();

    let first = common::fixture(&pool, &format!("{prefix}a")).await;
    let second = common::fixture(&pool, &format!("{prefix}b")).await;
    common::message(&pool, &first, None).await;
    common::message(&pool, &second, None).await;

    // A system path crosses tenants, for a reason that comes from a closed enum.
    let context = TenantContext::System {
        reason: SystemReason::ScheduledJob,
        user_id: None,
    };
    let bots = PostgresBotStore::new(pool.clone());
    assert!(
        bots.find(&context, &first.bot_id)
            .await
            .expect("the bot is read")
            .is_some(),
        "the system path reaches the first tenant"
    );
    assert!(
        bots.find(&context, &second.bot_id)
            .await
            .expect("the bot is read")
            .is_some(),
        "the system path reaches the second tenant"
    );

    let messages = PostgresMessageStore::new(pool.clone());
    let page = messages
        .page(&context, &second.thread_id, None, 50)
        .await
        .expect("the page is read");
    assert_eq!(
        page.messages.len(),
        1,
        "the system path reaches the messages of the second tenant"
    );

    common::cleanup(&pool, &prefix).await;
}

#[tokio::test]
async fn req_009_a_read_position_cannot_cross_tenants() {
    let database = common::database().await;
    let pool = common::tenant_pool(&database).await;
    let prefix = common::unique_id();

    let first = common::fixture(&pool, &format!("{prefix}a")).await;
    let second = common::fixture(&pool, &format!("{prefix}b")).await;

    let reads = PostgresReadStateStore::new(pool.clone());

    // A write against another tenant's thread is refused: nothing is stored, and nothing is
    // silently attributed to the wrong space.
    let refused = reads.set(&first.context, &second.thread_id, 7).await;
    assert!(matches!(refused, Err(boma_app::AppError::NotFound)));

    // A read for that thread defaults to zero: the other tenant's position never leaks.
    let position = reads
        .position(&first.context, &second.thread_id)
        .await
        .expect("the position answers");
    assert_eq!(position, 0);

    // The write inside the caller's tenant works and is remembered.
    let stored = reads
        .set(&first.context, &first.thread_id, 7)
        .await
        .expect("the write lands");
    assert_eq!(stored, 7);
    let position = reads
        .position(&first.context, &first.thread_id)
        .await
        .expect("the position answers");
    assert_eq!(position, 7);

    common::cleanup(&pool, &prefix).await;
}

#[tokio::test]
async fn req_009_a_snooze_cannot_cross_tenants() {
    let database = common::database().await;
    let pool = common::tenant_pool(&database).await;
    let prefix = common::unique_id();

    let first = common::fixture(&pool, &format!("{prefix}a")).await;
    let second = common::fixture(&pool, &format!("{prefix}b")).await;

    let roster = boma_infra::PostgresRosterStore::new(pool.clone());

    // A snooze against another tenant's thread is refused: nothing is stored.
    let refused = roster
        .set_snooze(
            &first.context,
            &second.thread_id,
            Some("2999-01-01T00:00:00Z"),
        )
        .await;
    assert!(matches!(refused, Err(boma_app::AppError::NotFound)));

    // The roster of the first tenant shows its own bot only.
    let bots = roster
        .list_bots(&first.context)
        .await
        .expect("the roster answers");
    assert_eq!(bots.len(), 1);
    assert_eq!(bots[0].thread_id, first.thread_id);

    // A snooze inside the tenant works and is remembered.
    let stored = roster
        .set_snooze(
            &first.context,
            &first.thread_id,
            Some("2999-01-01T00:00:00Z"),
        )
        .await
        .expect("the snooze is recorded");
    assert_eq!(stored.as_deref(), Some("2999-01-01T00:00:00Z"));
    let bots = roster
        .list_bots(&first.context)
        .await
        .expect("the roster answers");
    assert_eq!(
        bots[0].snoozed_until.as_deref(),
        Some("2999-01-01T00:00:00Z")
    );

    // Clearing it works as well.
    let cleared = roster
        .set_snooze(&first.context, &first.thread_id, None)
        .await
        .expect("the snooze is cleared");
    assert!(cleared.is_none());

    common::cleanup(&pool, &prefix).await;
}

#[tokio::test]
async fn req_043_a_ticket_cannot_cross_tenants() {
    let database = common::database().await;
    let pool = common::tenant_pool(&database).await;
    let prefix = common::unique_id();

    let first = common::fixture(&pool, &format!("{prefix}a")).await;
    let second = common::fixture(&pool, &format!("{prefix}b")).await;

    let tickets = boma_infra::PostgresTicketStore::new(pool.clone());
    let created = tickets
        .create(
            &first.context,
            boma_app::ports::NewTicket {
                id: format!("{prefix}t1"),
                state: "backlog".to_owned(),
                assignee_bot_id: None,
                title: "First space only".to_owned(),
                description: String::new(),
                created_at: "2026-01-01T00:00:00Z".to_owned(),
            },
        )
        .await
        .expect("the ticket is created");
    assert_eq!(created.key, "BOT-1");

    // The other tenant's board does not contain it.
    let board = tickets
        .list(&second.context)
        .await
        .expect("the board is read");
    assert!(
        board.is_empty(),
        "the tenant does not see the tickets of another tenant"
    );

    // A move from the other tenant does nothing.
    let moved = tickets
        .move_to(&second.context, &created.id, "done")
        .await
        .expect("the move is read");
    assert!(!moved, "the other tenant cannot move the ticket");

    // A comment from the other tenant does nothing.
    let commented = tickets
        .comment(
            &second.context,
            boma_app::ports::NewTicketComment {
                id: format!("{prefix}c1"),
                ticket_id: created.id.clone(),
                author_kind: "member".to_owned(),
                author_id: "someone-else".to_owned(),
                body: "not allowed".to_owned(),
                created_at: "2026-01-01T00:00:00Z".to_owned(),
            },
        )
        .await
        .expect("the comment path answers");
    assert!(
        commented.is_none(),
        "the other tenant cannot comment on the ticket"
    );

    common::cleanup(&pool, &prefix).await;
}
