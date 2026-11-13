//! The audit trail of the escape hatches (defect D-05).
//!
//! Append-only: this store exposes no read, no update and no delete. The export path is M9.

use boma_app::{
    AppError,
    ports::{AuditStore, NewAuditEvent},
};
use sqlx::PgPool;

use crate::tenant;

/// The Postgres adapter for the audit store.
#[derive(Debug, Clone)]
pub struct PostgresAuditStore {
    pool: PgPool,
}

impl PostgresAuditStore {
    /// Builds the store on a pool.
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl AuditStore for PostgresAuditStore {
    async fn append(&self, event: NewAuditEvent) -> Result<(), AppError> {
        let space_id = event.space_id.as_str().to_owned();
        let actor = event
            .actor_user_id
            .as_ref()
            .map(boma_domain::UserId::as_str)
            .map(str::to_owned);
        let kind = event.kind.as_str().to_owned();
        // Every writer today acts for a member; a future system path carries its enum reason.
        let context = match &event.actor_user_id {
            Some(user_id) => boma_domain::TenantContext::Member {
                space_id: event.space_id.clone(),
                user_id: user_id.clone(),
            },
            None => boma_domain::TenantContext::System {
                reason: boma_domain::SystemReason::ScheduledJob,
                user_id: None,
            },
        };
        tenant::with_tenant(&self.pool, &context, |conn| {
            Box::pin(async move {
                sqlx::query(
                    "INSERT INTO audit_events (id, space_id, actor_user_id, kind, target, metadata, created_at)
                     VALUES ($1, $2, $3, $4, $5, $6::jsonb, $7::timestamptz)",
                )
                .bind(&event.id)
                .bind(&space_id)
                .bind(&actor)
                .bind(&kind)
                .bind(&event.target)
                .bind(&event.metadata)
                .bind(&event.created_at)
                .execute(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
                Ok(())
            })
        })
        .await
    }
}
