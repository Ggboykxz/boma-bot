//! Events: the stream behind a thread's subscription.
//!
//! The sequence is allocated the way the message sequence is: the store takes it from the
//! thread, in the same transaction as the row it belongs to, so two events never claim the same
//! number (REQ-005, REQ-020).

use boma_app::{
    AppError,
    ports::{EventStore, IdGenerator as _, NewEvent},
};
use boma_domain::{Event, TenantContext, ThreadId};
use sqlx::{PgPool, types::Json};

use super::rows::{self, EventRow};
use crate::tenant;

/// The Postgres adapter for [`EventStore`].
#[derive(Debug, Clone)]
pub struct PostgresEventStore {
    pool: PgPool,
    ids: crate::crypto::IdGenerator,
}

impl PostgresEventStore {
    /// Builds the store on a pool.
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self {
            pool,
            ids: crate::crypto::IdGenerator::new(),
        }
    }
}

#[async_trait::async_trait]
impl EventStore for PostgresEventStore {
    async fn append(&self, context: &TenantContext, event: NewEvent) -> Result<Event, AppError> {
        let event_id = self.ids.new_id("evt");
        let thread_id = event.thread_id.clone();

        let row = tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                // The sequence is taken from the thread, in this transaction.
                let seq = sqlx::query_scalar::<_, i64>(
                    "UPDATE threads SET next_event_seq = next_event_seq + 1 WHERE id = $1
                     RETURNING next_event_seq - 1",
                )
                .bind(thread_id.as_str())
                .fetch_one(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;

                sqlx::query_as::<_, EventRow>(
                    "INSERT INTO events (id, space_id, thread_id, seq, type, payload, created_at)
                     VALUES ($1, $2, $3, $4, $5, $6, $7::timestamptz)
                     RETURNING thread_id, seq, type, payload,
                               boma.rfc3339(created_at) AS created_at",
                )
                .bind(&event_id)
                .bind(event.space_id.as_str())
                .bind(thread_id.as_str())
                .bind(seq)
                .bind(event.kind.as_str())
                .bind(Json(event.payload))
                .bind(&event.created_at)
                .fetch_one(&mut *conn)
                .await
                .map_err(tenant::unavailable)
            })
        })
        .await?;

        rows::event(row)
    }

    async fn since(
        &self,
        context: &TenantContext,
        thread_id: &ThreadId,
        after_seq: i64,
    ) -> Result<Vec<Event>, AppError> {
        let thread_id = thread_id.as_str().to_owned();
        let row = tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                sqlx::query_as::<_, EventRow>(
                    "SELECT thread_id, seq, type, payload,
                            boma.rfc3339(created_at) AS created_at
                     FROM events
                     WHERE thread_id = $1 AND seq > $2
                     ORDER BY seq ASC",
                )
                .bind(thread_id.as_str())
                .bind(after_seq)
                .fetch_all(&mut *conn)
                .await
                .map_err(tenant::unavailable)
            })
        })
        .await?;

        row.into_iter().map(rows::event).collect()
    }
}
