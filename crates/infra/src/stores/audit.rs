//! The audit trail of the escape hatches (defect D-05).
//!
//! Append-only: this store exposes no read, no update and no delete. The export path is M9.

use boma_app::{
    AppError,
    ports::{AuditStore, NewAuditEvent},
};
use boma_domain::TenantContext;
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
    async fn list_for_space(
        &self,
        context: &TenantContext,
        _space_id: &boma_domain::SpaceId,
        kinds_prefix: Option<&str>,
        limit: i64,
    ) -> Result<Vec<boma_app::ports::AuditEventView>, AppError> {
        let prefix = kinds_prefix.map(|text| format!("{text}%"));
        let rows = tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                let rows = sqlx::query_as::<_, (String, Option<String>, Option<String>, String, String, serde_json::Value, String)>(
                    "SELECT id, space_id, actor_user_id, kind, target, metadata, boma.rfc3339(created_at)
                       FROM audit_events
                      WHERE ($1::text IS NULL OR kind LIKE $1)
                      ORDER BY created_at DESC, id DESC
                      LIMIT $2",
                )
                .bind(prefix.as_deref())
                .bind(limit)
                .fetch_all(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
                Ok(rows)
            })
        })
        .await?;
        Ok(rows
            .into_iter()
            .map(
                |(id, space_id, actor_user_id, kind, target, metadata, created_at)| {
                    boma_app::ports::AuditEventView {
                        id,
                        space_id,
                        actor_user_id,
                        kind,
                        target,
                        metadata,
                        created_at,
                    }
                },
            )
            .collect())
    }

    async fn append(&self, event: NewAuditEvent) -> Result<(), AppError> {
        let space_id = event
            .space_id
            .as_ref()
            .map(boma_domain::SpaceId::as_str)
            .map(str::to_owned);
        let actor = event
            .actor_user_id
            .as_ref()
            .map(boma_domain::UserId::as_str)
            .map(str::to_owned);
        let kind = event.kind.as_str().to_owned();
        // A member-shaped event keeps the member context; a spaceless or actorless event is a
        // system path, so the is_system write-check above is the one it uses.
        let context = match (&event.space_id, &event.actor_user_id) {
            (Some(space_id), Some(user_id)) => boma_domain::TenantContext::Member {
                space_id: space_id.clone(),
                user_id: user_id.clone(),
            },
            (Some(_), None) => boma_domain::TenantContext::System {
                reason: boma_domain::SystemReason::ScheduledJob,
                user_id: None,
            },
            (None, user_id) => boma_domain::TenantContext::System {
                reason: boma_domain::SystemReason::ScheduledJob,
                user_id: user_id.clone(),
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
