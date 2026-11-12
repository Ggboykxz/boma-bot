//! The races the stores have to win, against a real database.
//!
//! Two sends at once, the same send twice, two workers claiming one run, and a worker that was
//! replaced trying to write anyway.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

mod common;

use boma_app::ports::{MessageStore, RunStore};
use boma_domain::RunStatus;
use boma_infra::PostgresMessageStore;

#[tokio::test]
async fn req_005_two_concurrent_sends_get_different_sequences() {
    let database = common::database().await;
    let pool = common::tenant_pool(&database).await;
    let prefix = common::unique_id();
    let fixture = common::fixture(&pool, &prefix).await;

    let store = PostgresMessageStore::new(pool.clone());
    let first = tokio::spawn({
        let store = store.clone();
        let context = fixture.context.clone();
        let thread_id = fixture.thread_id.clone();
        let space_id = fixture.space_id.clone();
        let user_id = fixture.user_id.clone();
        let prefix = prefix.clone();
        async move {
            store
                .insert(
                    &context,
                    boma_app::ports::NewMessage {
                        id: boma_domain::MessageId::new(format!("{prefix}msg1"))
                            .expect("the id is accepted"),
                        space_id,
                        thread_id,
                        role: boma_domain::MessageRole::User,
                        blocks: vec![boma_domain::Block::Text {
                            text: "first".to_owned(),
                        }],
                        bot_id: None,
                        author_user_id: Some(user_id),
                        run_id: None,
                        client_nonce: None,
                        created_at: "2026-01-01T00:00:00Z".to_owned(),
                    },
                )
                .await
        }
    });
    let second = tokio::spawn({
        let store = store.clone();
        let context = fixture.context.clone();
        let thread_id = fixture.thread_id.clone();
        let space_id = fixture.space_id.clone();
        let user_id = fixture.user_id.clone();
        let prefix = prefix.clone();
        async move {
            store
                .insert(
                    &context,
                    boma_app::ports::NewMessage {
                        id: boma_domain::MessageId::new(format!("{prefix}msg2"))
                            .expect("the id is accepted"),
                        space_id,
                        thread_id,
                        role: boma_domain::MessageRole::User,
                        blocks: vec![boma_domain::Block::Text {
                            text: "second".to_owned(),
                        }],
                        bot_id: None,
                        author_user_id: Some(user_id),
                        run_id: None,
                        client_nonce: None,
                        created_at: "2026-01-01T00:00:00Z".to_owned(),
                    },
                )
                .await
        }
    });

    let first = first
        .await
        .expect("the first send finishes")
        .expect("the first send is written");
    let second = second
        .await
        .expect("the second send finishes")
        .expect("the second send is written");

    assert_ne!(
        first.message.seq, second.message.seq,
        "two concurrent sends never claim the same sequence"
    );
    let mut sequences = [first.message.seq, second.message.seq];
    sequences.sort_unstable();
    assert_eq!(sequences, [1, 2], "the sequences are consecutive");

    common::cleanup(&pool, &prefix).await;
}

#[tokio::test]
async fn req_002_the_same_nonce_returns_the_same_message_and_run() {
    let database = common::database().await;
    let pool = common::tenant_pool(&database).await;
    let prefix = common::unique_id();
    let fixture = common::fixture(&pool, &prefix).await;

    let store = PostgresMessageStore::new(pool.clone());
    let message = |id: &str| boma_app::ports::NewMessage {
        id: boma_domain::MessageId::new(id.to_owned()).expect("the id is accepted"),
        space_id: fixture.space_id.clone(),
        thread_id: fixture.thread_id.clone(),
        role: boma_domain::MessageRole::User,
        blocks: vec![boma_domain::Block::Text {
            text: "hello".to_owned(),
        }],
        bot_id: None,
        author_user_id: Some(fixture.user_id.clone()),
        run_id: None,
        client_nonce: Some("the-same-nonce".to_owned()),
        created_at: "2026-01-01T00:00:00Z".to_owned(),
    };

    let first = store
        .insert(&fixture.context, message(&format!("{prefix}msg1")))
        .await
        .expect("the message is written");
    let replay = store
        .insert(&fixture.context, message(&format!("{prefix}msg2")))
        .await
        .expect("the replay is answered");

    assert!(first.created, "the first send creates a message");
    assert!(!replay.created, "the replay creates nothing");
    assert_eq!(
        first.message.id, replay.message.id,
        "the replay returns the message the nonce already produced"
    );

    let page = store
        .page(&fixture.context, &fixture.thread_id, None, 50)
        .await
        .expect("the page is read");
    assert_eq!(page.messages.len(), 1, "the replay writes no second row");

    let runs = common::runs(&pool);
    let first_run = runs
        .create(common::new_run(&fixture, "one", Some("the-same-nonce")))
        .await
        .expect("the run is written");
    let replayed_run = runs
        .create(common::new_run(&fixture, "two", Some("the-same-nonce")))
        .await
        .expect("the replay is answered");

    assert_eq!(
        first_run.id, replayed_run.id,
        "the replay returns the run the nonce already started"
    );

    let found = runs
        .find_by_nonce(&fixture.context, &fixture.thread_id, "the-same-nonce")
        .await
        .expect("the run is read");
    assert!(found.is_some(), "the run the nonce started is there");

    common::cleanup(&pool, &prefix).await;
}

#[tokio::test]
async fn req_012_two_workers_cannot_claim_the_same_run() {
    // The queue is global by design, so this test gets a queue of its own.
    let fresh = common::fresh_database().await;
    let pool = common::tenant_pool(&fresh.database).await;
    let prefix = common::unique_id();
    let fixture = common::fixture(&pool, &prefix).await;

    let runs = common::runs(&pool);
    let first = runs
        .create(common::new_run(&fixture, "one", Some("claim-one")))
        .await
        .expect("the run is written");
    let second = runs
        .create(common::new_run(&fixture, "two", Some("claim-two")))
        .await
        .expect("the run is written");

    let (claimed_by_one, claimed_by_two) = tokio::join!(
        tokio::spawn({
            let runs = runs.clone();
            async move { runs.claim_next_queued("worker-one").await }
        }),
        tokio::spawn({
            let runs = runs.clone();
            async move { runs.claim_next_queued("worker-two").await }
        }),
    );
    let claimed_by_one = claimed_by_one
        .expect("the first worker finishes")
        .expect("the claim is answered")
        .expect("the first worker claims a run");
    let claimed_by_two = claimed_by_two
        .expect("the second worker finishes")
        .expect("the claim is answered")
        .expect("the second worker claims a run");

    assert_ne!(
        claimed_by_one.id, claimed_by_two.id,
        "two workers never claim the same run"
    );
    assert!(
        [first.id.clone(), second.id.clone()].contains(&claimed_by_one.id),
        "the first worker claims one of the queued runs"
    );
    assert!(
        [first.id, second.id].contains(&claimed_by_two.id),
        "the second worker claims the other queued run"
    );

    let nothing = runs
        .claim_next_queued("worker-three")
        .await
        .expect("the claim is answered");
    assert!(
        nothing.is_none(),
        "a third worker finds nothing: both runs are leased"
    );

    common::cleanup(&pool, &prefix).await;
    fresh.dispose().await;
}

#[tokio::test]
async fn req_012_a_stale_fence_cannot_write() {
    // Same reason: the claim is global, so this test holds a private queue.
    let fresh = common::fresh_database().await;
    let pool = common::tenant_pool(&fresh.database).await;
    let prefix = common::unique_id();
    let fixture = common::fixture(&pool, &prefix).await;

    let runs = common::runs(&pool);
    runs.create(common::new_run(&fixture, "one", Some("fenced")))
        .await
        .expect("the run is written");

    let claimed = runs
        .claim_next_queued("worker-one")
        .await
        .expect("the claim is answered")
        .expect("the run is claimed");
    assert_eq!(claimed.lease_fence, 1, "the first claim fences at one");

    // The holder of the lease writes with the fence it was given.
    runs.set_status(
        &fixture.context,
        &claimed.id,
        RunStatus::Running,
        "2026-01-01T00:00:01Z",
        Some(claimed.lease_fence),
        None,
    )
    .await
    .expect("the holder of the lease writes");

    // An outdated fence is refused: the row was not updated.
    let refused = runs
        .set_status(
            &fixture.context,
            &claimed.id,
            RunStatus::Completed,
            "2026-01-01T00:00:02Z",
            Some(claimed.lease_fence - 1),
            None,
        )
        .await;
    assert!(
        refused.is_err(),
        "a worker whose fence is outdated cannot write"
    );

    // The run that was replaced: the run goes back to the queue, another worker takes it, and
    // the first one is refused from then on.
    runs.set_status(
        &fixture.context,
        &claimed.id,
        RunStatus::Queued,
        "2026-01-01T00:00:03Z",
        Some(claimed.lease_fence),
        None,
    )
    .await
    .expect("the holder of the lease releases the run");
    let taken_again = runs
        .claim_next_queued("worker-two")
        .await
        .expect("the claim is answered")
        .expect("the run is claimed again");
    assert_eq!(taken_again.lease_fence, 2, "the second claim fences at two");

    let refused = runs
        .set_status(
            &fixture.context,
            &claimed.id,
            RunStatus::Completed,
            "2026-01-01T00:00:04Z",
            Some(claimed.lease_fence),
            None,
        )
        .await;
    assert!(refused.is_err(), "a worker that was replaced cannot write");

    common::cleanup(&pool, &prefix).await;
    fresh.dispose().await;
}
