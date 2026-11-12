//! The row level security, against a real database (REQ-043).
//!
//! Two spaces, a bot in each, and the proof that the first one cannot see the second one. The
//! proof is only worth something when the connection is subject to the policies: PostgreSQL
//! never applies them to a superuser, so the tests act as a role that has no privilege.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

mod common;

use boma_app::ports::{BotStore, MessageStore};
use boma_domain::{SystemReason, TenantContext};
use boma_infra::{PostgresBotStore, PostgresMessageStore};

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
