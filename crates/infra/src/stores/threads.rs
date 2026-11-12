//! Threads: one conversation per bot, and the sequences that order it.

use boma_app::{AppError, ports::ThreadStore};
use boma_domain::{BotId, TenantContext, Thread, ThreadId};
use sqlx::PgPool;

use super::rows::{self, ThreadRow};
use crate::tenant;

/// The Postgres adapter for [`ThreadStore`].
#[derive(Debug, Clone)]
pub struct PostgresThreadStore {
    pool: PgPool,
}

impl PostgresThreadStore {
    /// Builds the store on a pool.
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl ThreadStore for PostgresThreadStore {
    async fn find(
        &self,
        context: &TenantContext,
        id: &ThreadId,
    ) -> Result<Option<Thread>, AppError> {
        let id = id.as_str().to_owned();
        let row = tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                sqlx::query_as::<_, ThreadRow>(
                    "SELECT id, space_id, bot_id, user_id, next_message_seq, next_event_seq,
                            boma.rfc3339(created_at) AS created_at
                     FROM threads
                     WHERE id = $1",
                )
                .bind(id.as_str())
                .fetch_optional(&mut *conn)
                .await
                .map_err(tenant::unavailable)
            })
        })
        .await?;

        row.map(rows::thread).transpose()
    }

    async fn for_bot(
        &self,
        context: &TenantContext,
        bot_id: &BotId,
    ) -> Result<Option<Thread>, AppError> {
        let bot_id = bot_id.as_str().to_owned();
        let row = tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                sqlx::query_as::<_, ThreadRow>(
                    "SELECT id, space_id, bot_id, user_id, next_message_seq, next_event_seq,
                            boma.rfc3339(created_at) AS created_at
                     FROM threads
                     WHERE bot_id = $1
                     ORDER BY created_at ASC
                     LIMIT 1",
                )
                .bind(bot_id.as_str())
                .fetch_optional(&mut *conn)
                .await
                .map_err(tenant::unavailable)
            })
        })
        .await?;

        row.map(rows::thread).transpose()
    }
}
