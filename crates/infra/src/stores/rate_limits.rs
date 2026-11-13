//! Shared rate-limit counters (D-01).
//!
//! The row is the limit: every replica increments the same bucket, and every hit returns the
//! count as it stands — after the increment, not before it, so refusing the over-limit caller
//! is the caller's choice and stays honest under races.

use boma_app::{
    AppError,
    ports::{RateLimitDecision, RateLimitStore},
};
use sqlx::PgPool;

/// The Postgres adapter for [`RateLimitStore`].
#[derive(Debug, Clone)]
pub struct PostgresRateLimitStore {
    pool: PgPool,
}

impl PostgresRateLimitStore {
    /// Builds the store on a pool.
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl RateLimitStore for PostgresRateLimitStore {
    async fn hit(
        &self,
        key: &str,
        now: &str,
        window_seconds: u64,
    ) -> Result<RateLimitDecision, AppError> {
        let at = time::OffsetDateTime::parse(now, &time::format_description::well_known::Rfc3339)
            .map_err(|_| AppError::Unavailable)?;
        let window = i64::try_from(window_seconds).map_err(|_| AppError::Unavailable)?;
        if window <= 0 {
            return Err(AppError::Unavailable);
        }
        let bucket = at.unix_timestamp() / window;
        let key = key.to_owned();
        // One transaction: the hit and the lazy cleanup of buckets older than 24 windows.
        // The cleanup keeps a quiet client's history from accumulating, and costs one delete
        // of an index-only scan every hit.
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| unavailable(&error))?;
        let count = sqlx::query_scalar::<_, i32>(
            "INSERT INTO rate_limits (key, bucket, hits)
             VALUES ($1, $2, 1)
             ON CONFLICT (key, bucket) DO UPDATE SET hits = rate_limits.hits + 1
             RETURNING hits",
        )
        .bind(&key)
        .bind(bucket)
        .fetch_one(&mut *transaction)
        .await
        .map_err(|error| unavailable(&error))?;
        sqlx::query("DELETE FROM rate_limits WHERE bucket < $1")
            .bind(bucket - 24)
            .execute(&mut *transaction)
            .await
            .map_err(|error| unavailable(&error))?;
        transaction
            .commit()
            .await
            .map_err(|error| unavailable(&error))?;
        Ok(RateLimitDecision {
            count: u64::try_from(i64::from(count)).unwrap_or(0),
            bucket,
        })
    }
}

/// The store is never tenant data: a rate limit applies to a caller before any tenant exists.
fn unavailable(error: &sqlx::Error) -> AppError {
    tracing::warn!(%error, "the rate-limit store could not answer");
    AppError::Unavailable
}
