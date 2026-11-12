//! Read positions: where each member last read a conversation (REQ-009).
//!
//! A position only moves through the store; there is no notion of a thread-wide position, and
//! there never will be: two readers of the same thread have two rows.

use boma_app::{AppError, ports::ReadStateStore};
use boma_domain::{TenantContext, ThreadId};
use sqlx::PgPool;

use crate::tenant;

/// The Postgres adapter for [`ReadStateStore`].
#[derive(Debug, Clone)]
pub struct PostgresReadStateStore {
    pool: PgPool,
}

impl PostgresReadStateStore {
    /// Builds the store on a pool.
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl ReadStateStore for PostgresReadStateStore {
    async fn position(
        &self,
        context: &TenantContext,
        thread_id: &ThreadId,
    ) -> Result<i64, AppError> {
        let user_id = context.user_id().cloned().ok_or(AppError::NotFound)?;
        let thread_key = thread_id.as_str().to_owned();
        let value = tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                sqlx::query_scalar::<_, i64>(
                    "SELECT last_seq FROM thread_read_states
                     WHERE thread_id = $1 AND user_id = $2",
                )
                .bind(thread_key.as_str())
                .bind(user_id.as_str())
                .fetch_optional(&mut *conn)
                .await
                .map_err(tenant::unavailable)
            })
        })
        .await?;
        Ok(value.unwrap_or(0))
    }

    async fn set(
        &self,
        context: &TenantContext,
        thread_id: &ThreadId,
        last_seq: i64,
    ) -> Result<i64, AppError> {
        if last_seq < 0 {
            return Err(AppError::Rule(boma_domain::DomainError::InvalidName(
                "the read position cannot be negative".to_owned(),
            )));
        }
        let user_id = context.user_id().cloned().ok_or(AppError::NotFound)?;
        let space_id = context.space_id().cloned().ok_or(AppError::NotFound)?;
        let thread_key = thread_id.as_str().to_owned();
        tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                // The write only lands when the thread belongs to the caller's space. The
                // row level security policy is the defense in depth; this predicate is the
                // defense in fact, and it holds even where the policies do not apply
                // (a superuser connection).
                sqlx::query_scalar::<_, i64>(
                    "INSERT INTO thread_read_states (thread_id, space_id, user_id, last_seq, updated_at)
                     SELECT $1, $2, $3, $4, now()
                     WHERE EXISTS (SELECT 1 FROM threads WHERE id = $1 AND space_id = $2)
                     ON CONFLICT (thread_id, user_id) DO UPDATE
                     SET last_seq = EXCLUDED.last_seq, updated_at = now()
                     RETURNING last_seq",
                )
                .bind(thread_key.as_str())
                .bind(space_id.as_str())
                .bind(user_id.as_str())
                .bind(last_seq)
                .fetch_optional(&mut *conn)
                .await
                .map_err(tenant::unavailable)?
                .ok_or(AppError::NotFound)
            })
        })
        .await
    }
}
