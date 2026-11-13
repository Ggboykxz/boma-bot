//! REQ-053 / D-01: the auth rate limit is one shared limit, not one limit per replica.
//!
//! The defect was counters living in one process' memory. Here, two pools play two replicas
//! and share exactly one database: the first replica exhausts the allowance, the second must
//! see the same count — this is the assertion that fails on a per-process implementation.

#![allow(clippy::expect_used, clippy::panic)]

use boma_app::ports::RateLimitStore;
use boma_infra::PostgresRateLimitStore;

mod common;

#[tokio::test]
async fn d_01_two_instances_enforce_one_shared_limit() {
    let fresh = common::fresh_database().await;
    let first_pool = fresh.database.pool().clone();
    let second_pool = fresh.database.pool().clone();

    let first = PostgresRateLimitStore::new(first_pool);
    let second = PostgresRateLimitStore::new(second_pool);
    let key = "auth:sign-in:203.0.113.7";
    let now = "2026-01-01T00:00:00Z";

    // Ten attempts from the first replica: all served.
    for expected in 1..=10 {
        let decision = first.hit(key, now, 300).await.expect("the hit is recorded");
        assert_eq!(decision.count, expected);
    }
    // The eleventh — through the other replica — must see the shared count.
    let decision = second
        .hit(key, now, 300)
        .await
        .expect("the hit is recorded");
    assert_eq!(
        decision.count, 11,
        "the second replica sees the shared counter, not its own"
    );

    // The window, not a rolling count: the next bucket starts at one.
    let next_bucket = first
        .hit(key, "2026-01-01T00:05:00Z", 300)
        .await
        .expect("the hit is recorded");
    assert_eq!(next_bucket.count, 1);

    fresh.dispose().await;
}
