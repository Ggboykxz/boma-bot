//! Integration tests for the Postgres adapter, against a real database.
//!
//! AGENTS.md asks for "integration tests in `tests/` against a real database": a pool that only
//! ever sees a mock proves nothing about SQL, migrations or the readiness probe. The database
//! comes from the environment, exactly like in CI (`BOMA_DATABASE_URL`) and in Compose
//! (`postgres:17.11-alpine`).
//!
//! The test fails loudly when the variable is missing. A suite that skips itself when the
//! database is absent is a suite that proves nothing, so it says what to do instead.

// An integration test file is compiled as its own crate, so clippy's `allow-*-in-tests` (which
// covers `#[test]` functions and `#[cfg(test)]` modules only) does not reach the helpers below.
// AGENTS.md allows unwrap, expect and panic in test code; this is where that permission is taken,
// once and in writing.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::time::Duration;

use boma_infra::{DatabaseSettings, Postgres};

/// The database under test, or a message that says how to start one.
fn database_url() -> String {
    std::env::var("BOMA_DATABASE_URL").unwrap_or_else(|_| {
        panic!(
            "BOMA_DATABASE_URL is not set: start a Postgres (docker compose up -d db, or a local \
             server) and point the variable at it before running the integration tests"
        )
    })
}

/// Settings for the database under test.
fn settings() -> DatabaseSettings {
    DatabaseSettings::new(database_url(), 4, Duration::from_secs(5))
        .expect("BOMA_DATABASE_URL is a usable Postgres URL")
}

#[tokio::test]
async fn req_053_the_migrations_apply_and_are_recorded() {
    let database = Postgres::connect(&settings())
        .await
        .expect("the database accepts a connection");
    database.migrate().await.expect("the migrations apply");

    // Applying them twice must be a no-op, not an error: a deploy that restarts mid-migration
    // has to be able to run the step again.
    database
        .migrate()
        .await
        .expect("the migrations are idempotent");

    let applied: i64 =
        sqlx::query_scalar("SELECT count(*) FROM _sqlx_migrations WHERE success = true")
            .fetch_one(database.pool())
            .await
            .expect("the migration table is readable");
    assert!(applied > 0, "the migrations must be recorded");
}

#[tokio::test]
async fn req_053_the_database_answers_its_readiness_probe() {
    let database = Postgres::connect(&settings())
        .await
        .expect("the database accepts a connection");
    let probe = database.readiness(Duration::from_secs(5));
    assert_eq!(probe.name(), "database");
    assert_eq!(probe.check().await, Ok(()));
}

#[tokio::test]
async fn req_053_a_lazy_pool_reports_an_unreachable_database_as_unavailable() {
    // A pool that never connected still answers the probe: it fails, with a reason that does not
    // quote the connection string (REQ-054).
    let settings = DatabaseSettings::new(
        "postgres://boma@127.0.0.1:1/does-not-exist",
        1,
        Duration::from_millis(200),
    )
    .expect("the fixture URL is well formed");
    let database = Postgres::connect_lazy(&settings).expect("a lazy pool never fails to build");
    let probe = database.readiness(Duration::from_millis(200));
    let reason = probe
        .check()
        .await
        .expect_err("an unreachable database must fail the probe");
    assert_eq!(reason, "database rejected the probe");
    assert!(
        !reason.contains("127.0.0.1"),
        "the reason must not leak the target"
    );
}
