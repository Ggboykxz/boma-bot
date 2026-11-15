//! Integration tests for the Postgres Web Push adapters (REQ-064).
//!
//! The in-memory testkit proves the use cases; these prove the SQL: the migration, the
//! unique key on `(space_id, user_id, endpoint)`, the RLS of the rows, and the UTC-day cap query.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use boma_app::ports::NotificationStore;
use boma_domain::notification::{
    NewNotificationSubscription, NotificationKind, SetNotificationPolicy,
};

mod common;

#[tokio::test]
async fn req_064_subscribe_list_unsubscribe_roundtrip_and_fail_count_reset() {
    let fresh = common::fresh_database().await;
    let fixture = common::fixture(fresh.database.pool(), "notifx").await;
    let store = boma_infra::PostgresNotificationStore::new(fresh.database.pool().clone());

    let subscription = store
        .subscribe(NewNotificationSubscription {
            id: "notifxsub1".to_owned(),
            space_id: fixture.space_id.clone(),
            user_id: fixture.user_id.clone(),
            endpoint: "https://updates.push.services.mozilla.com/a".to_owned(),
            p256dh: "p-key".to_owned(),
            auth: "an-auth".to_owned(),
            user_agent: Some("Chrome".to_owned()),
            created_at: "2026-11-15T00:00:00Z".to_owned(),
        })
        .await
        .expect("the subscription is stored");
    assert_eq!(subscription.fail_count, 0);

    // Re-subscribing the same endpoint replaces the row instead of duplicating it.
    let resubscribed = store
        .subscribe(NewNotificationSubscription {
            id: "notifxsub2".to_owned(),
            space_id: fixture.space_id.clone(),
            user_id: fixture.user_id.clone(),
            endpoint: "https://updates.push.services.mozilla.com/a".to_owned(),
            p256dh: "p-key-2".to_owned(),
            auth: "an-auth".to_owned(),
            user_agent: None,
            created_at: "2026-11-15T00:01:00Z".to_owned(),
        })
        .await
        .expect("the subscription is replaced");
    assert_eq!(resubscribed.p256dh, "p-key-2");
    let listed = store
        .list(&fixture.context)
        .await
        .expect("the list answers");
    assert_eq!(listed.len(), 1, "re-subscribing replaces, never dupes");

    assert!(
        store
            .unsubscribe(
                &fixture.context,
                "https://updates.push.services.mozilla.com/a"
            )
            .await
            .expect("the unsubscribe answers"),
    );
    assert!(
        !store
            .unsubscribe(
                &fixture.context,
                "https://updates.push.services.mozilla.com/a"
            )
            .await
            .expect("a missing endpoint unsubscribes to false"),
    );
    let listed = store
        .list(&fixture.context)
        .await
        .expect("the list answers");
    assert_eq!(listed.len(), 0, "the subscription was removed");

    fresh.dispose().await;
}

#[tokio::test]
async fn req_064_policy_round_trip_and_the_missing_row_defaults() {
    let fresh = common::fresh_database().await;
    let fixture = common::fixture(fresh.database.pool(), "req064p").await;
    let store = boma_infra::PostgresNotificationStore::new(fresh.database.pool().clone());

    let missing = store
        .get_policy(&fixture.context, &fixture.bot_id)
        .await
        .expect("the default answers");
    assert!(missing.notify_waiting);
    assert!(!missing.notify_completed);
    assert_eq!(missing.daily_cap, 20);

    let patched = store
        .set_policy(
            &fixture.context,
            SetNotificationPolicy {
                bot_id: fixture.bot_id.clone(),
                notify_waiting: Some(false),
                notify_completed: Some(true),
                daily_cap: Some(3),
            },
        )
        .await
        .expect("the policy is stored");
    assert!(!patched.notify_waiting);
    assert!(patched.notify_completed);
    assert_eq!(patched.daily_cap, 3);

    // An empty patch keeps the previous values.
    let unchanged = store
        .set_policy(
            &fixture.context,
            SetNotificationPolicy {
                bot_id: fixture.bot_id.clone(),
                notify_waiting: None,
                notify_completed: None,
                daily_cap: None,
            },
        )
        .await
        .expect("the policy is stored");
    assert!(!unchanged.notify_waiting);
    assert!(unchanged.notify_completed);
    assert_eq!(unchanged.daily_cap, 3);

    fresh.dispose().await;
}

#[tokio::test]
async fn req_064_three_permanent_failures_delete_the_row_in_postgres() {
    let fresh = common::fresh_database().await;
    let fixture = common::fixture(fresh.database.pool(), "req064f").await;
    let store = boma_infra::PostgresNotificationStore::new(fresh.database.pool().clone());

    store
        .subscribe(NewNotificationSubscription {
            id: "req064fsub".to_owned(),
            space_id: fixture.space_id.clone(),
            user_id: fixture.user_id.clone(),
            endpoint: "https://fcm.googleapis.com/x".to_owned(),
            p256dh: "p".to_owned(),
            auth: "a".to_owned(),
            user_agent: None,
            created_at: "2026-11-15T00:00:00Z".to_owned(),
        })
        .await
        .expect("the subscription is stored");

    // Transient failures bump and survive.
    store
        .note_failure(&fixture.context, "req064fsub", false)
        .await
        .expect("a transient failure is recorded");
    store
        .note_success(&fixture.context, "req064fsub")
        .await
        .expect("success resets the counter");

    for _ in 0..2 {
        store
            .note_failure(&fixture.context, "req064fsub", true)
            .await
            .expect("a permanent failure is recorded");
    }
    assert_eq!(
        store
            .list(&fixture.context)
            .await
            .expect("the list answers")
            .len(),
        1,
        "two permanent failures do not delete the row"
    );
    store
        .note_failure(&fixture.context, "req064fsub", true)
        .await
        .expect("the third permanent failure is recorded");
    assert!(
        store
            .list(&fixture.context)
            .await
            .expect("the list answers")
            .is_empty(),
        "the third permanent failure deletes the row"
    );

    fresh.dispose().await;
}

#[tokio::test]
async fn req_064_count_today_ignores_rows_from_yesterday() {
    let fresh = common::fresh_database().await;
    let fixture = common::fixture(fresh.database.pool(), "req064c").await;
    let store = boma_infra::PostgresNotificationStore::new(fresh.database.pool().clone());

    for index in 0..2 {
        store
            .record_send(
                &fixture.context,
                &format!("req064clog{index}"),
                &fixture.bot_id,
                NotificationKind::Waiting,
            )
            .await
            .expect("the send is recorded");
    }
    assert_eq!(
        store
            .count_today(&fixture.context, &fixture.bot_id)
            .await
            .expect("the count answers"),
        2,
    );

    // Push yesterday's row hour back by one day; it falls out of the cap window.
    sqlx::query(
        "UPDATE notification_log SET sent_at = sent_at - interval '1 day' WHERE id = 'req064clog0'",
    )
    .execute(fresh.database.pool())
    .await
    .expect("the log row is backdated");
    assert_eq!(
        store
            .count_today(&fixture.context, &fixture.bot_id)
            .await
            .expect("the count answers"),
        1,
    );

    fresh.dispose().await;
}

#[tokio::test]
async fn req_064_a_foreign_tenant_cannot_see_another_spaces_subscriptions() {
    let fresh = common::fresh_database().await;
    let alpha = common::fixture(fresh.database.pool(), "req064rla").await;
    let beta = common::fixture(fresh.database.pool(), "req064rlb").await;
    let pool = fresh.database.pool().clone();
    let store = boma_infra::PostgresNotificationStore::new(pool);

    store
        .subscribe(NewNotificationSubscription {
            id: "req064rls1".to_owned(),
            space_id: alpha.space_id.clone(),
            user_id: alpha.user_id.clone(),
            endpoint: "https://updates.push.services.mozilla.com/shared".to_owned(),
            p256dh: "p".to_owned(),
            auth: "a".to_owned(),
            user_agent: None,
            created_at: "2026-11-15T00:00:00Z".to_owned(),
        })
        .await
        .expect("the subscription is stored");

    let seen_from_beta = store.list(&beta.context).await.expect("the list answers");
    assert!(
        seen_from_beta.is_empty(),
        "a member of another space cannot see subscriptions it does not own"
    );

    assert!(
        !store
            .unsubscribe(
                &beta.context,
                "https://updates.push.services.mozilla.com/shared"
            )
            .await
            .expect("the unsubscribe answers"),
        "another tenant cannot delete rows it does not own"
    );

    fresh.dispose().await;
}
