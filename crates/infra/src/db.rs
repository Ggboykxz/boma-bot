//! The Postgres adapter.
//!
//! One pool per process, built from validated [`DatabaseSettings`]. Migrations are embedded in the
//! binary with `sqlx::migrate!`, so `boma migrate` is one forward-only step in a deploy and never
//! depends on a separate tool being installed.

use std::{fmt, sync::Arc, time::Duration};

use async_trait::async_trait;
use boma_app::ReadinessCheck;
use sqlx::{
    PgPool,
    migrate::{MigrateError, Migrator},
    postgres::PgPoolOptions,
};

use crate::config::DatabaseSettings;

/// Migrations embedded at compile time; the directory is relative to this crate's `Cargo.toml`.
static MIGRATOR: Migrator = sqlx::migrate!("../../migrations");

/// A pool of Postgres connections.
#[derive(Clone)]
pub struct Postgres {
    pool: PgPool,
}

impl Postgres {
    /// Connects eagerly and fails when the database is unreachable.
    ///
    /// # Errors
    ///
    /// Returns [`sqlx::Error`] when the pool cannot be created or the first connection fails.
    pub async fn connect(settings: &DatabaseSettings) -> Result<Self, sqlx::Error> {
        let pool = pool_options(settings).connect(settings.url()).await?;
        Ok(Self { pool })
    }

    /// Builds a pool without contacting the server.
    ///
    /// `boma serve` uses this: a database that is down must produce `503` on `/ready`, not a crash
    /// loop, because the database may come up after the application (REQ-053).
    ///
    /// # Errors
    ///
    /// Returns [`sqlx::Error`] when the connection string cannot be parsed.
    pub fn connect_lazy(settings: &DatabaseSettings) -> Result<Self, sqlx::Error> {
        let pool = pool_options(settings).connect_lazy(settings.url())?;
        Ok(Self { pool })
    }

    /// The underlying pool, for repositories.
    #[must_use]
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Applies every migration that is not yet recorded.
    ///
    /// # Errors
    ///
    /// Returns [`MigrateError`] when a migration fails or the migration table cannot be read.
    pub async fn migrate(&self) -> Result<(), MigrateError> {
        MIGRATOR.run(&self.pool).await
    }

    /// A readiness probe on this pool, bounded by `timeout`.
    #[must_use]
    pub fn readiness(&self, timeout: Duration) -> Arc<dyn ReadinessCheck> {
        Arc::new(PostgresReadiness {
            pool: self.pool.clone(),
            timeout,
        })
    }
}

fn pool_options(settings: &DatabaseSettings) -> PgPoolOptions {
    PgPoolOptions::new()
        .max_connections(settings.max_connections())
        .acquire_timeout(settings.acquire_timeout())
}

impl fmt::Debug for Postgres {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Postgres")
            .field("pool", &"<pg pool>")
            .finish_non_exhaustive()
    }
}

/// The `/ready` probe for the database.
struct PostgresReadiness {
    pool: PgPool,
    timeout: Duration,
}

impl fmt::Debug for PostgresReadiness {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PostgresReadiness")
            .field("timeout", &self.timeout)
            .finish_non_exhaustive()
    }
}

#[async_trait]
impl ReadinessCheck for PostgresReadiness {
    fn name(&self) -> &'static str {
        "database"
    }

    async fn check(&self) -> Result<(), &'static str> {
        let probe = sqlx::query("SELECT 1").execute(&self.pool);
        match tokio::time::timeout(self.timeout, probe).await {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(error)) => {
                // Driver errors can quote the connection target, so the detail stays at debug level
                // while the response only ever says that the dependency is unavailable (REQ-054).
                tracing::debug!(error = %error, "database readiness probe failed");
                Err("database rejected the probe")
            }
            Err(_elapsed) => {
                tracing::debug!("database readiness probe timed out");
                Err("database did not answer in time")
            }
        }
    }
}
