//! Sign-in requests (REQ-060) — pending/granted/refused sign-in grants for bots.

use async_trait::async_trait;
use boma_app::{AppError, ports};
use boma_domain::{BotId, SpaceId, UserId};
use sqlx::PgPool;

use crate::tenant;

/// The Postgres adapter for [`ports::SignInStore`].
#[derive(Debug, Clone)]
pub struct PostgresSignInStore {
    pool: PgPool,
}

impl PostgresSignInStore {
    /// Builds the store on a pool.
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(sqlx::FromRow)]
struct SignInRequestRow {
    id: String,
    space_id: String,
    bot_id: String,
    requested_by: String,
    thread_id: Option<String>,
    origin: String,
    display_name: String,
    status: String,
    granted_by: Option<String>,
    created_at: String,
    updated_at: String,
}

fn into_domain(row: SignInRequestRow) -> Result<boma_domain::signin::SignInRequest, AppError> {
    let status = match row.status.as_str() {
        "pending" => boma_domain::signin::SignInRequestStatus::Pending,
        "granted" => boma_domain::signin::SignInRequestStatus::Granted,
        "refused" => boma_domain::signin::SignInRequestStatus::Refused,
        _ => return Err(AppError::Unavailable),
    };
    Ok(boma_domain::signin::SignInRequest {
        id: row.id,
        space_id: SpaceId::new(row.space_id).map_err(|_| AppError::Unavailable)?,
        bot_id: BotId::new(row.bot_id).map_err(|_| AppError::Unavailable)?,
        requested_by: UserId::new(row.requested_by).map_err(|_| AppError::Unavailable)?,
        thread_id: row.thread_id,
        origin: row.origin,
        display_name: row.display_name,
        status,
        granted_by: row
            .granted_by
            .map(UserId::new)
            .transpose()
            .map_err(|_| AppError::Unavailable)?,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

#[async_trait]
impl ports::SignInStore for PostgresSignInStore {
    async fn record(
        &self,
        request: boma_domain::signin::NewSignInRequest,
    ) -> Result<boma_domain::signin::SignInRequest, AppError> {
        let space_id = request.space_id.clone();
        tenant::in_space(&self.pool, &space_id, |conn| {
            Box::pin(async move {
                sqlx::query_as::<_, SignInRequestRow>(
                    "INSERT INTO signin_requests (id, space_id, bot_id, requested_by, thread_id, origin, display_name, status, created_at, updated_at)
                     VALUES ($1, $2, $3, $4, $5, $6, $7, 'pending', $8::timestamptz, $8::timestamptz)
                     ON CONFLICT (id) DO UPDATE
                     SET space_id = EXCLUDED.space_id,
                         bot_id = EXCLUDED.bot_id,
                         requested_by = EXCLUDED.requested_by,
                         thread_id = EXCLUDED.thread_id,
                         origin = EXCLUDED.origin,
                         display_name = EXCLUDED.display_name,
                         updated_at = EXCLUDED.updated_at
                     RETURNING id, space_id, bot_id, requested_by, thread_id, origin,
                               display_name, status, granted_by,
                               boma.rfc3339(created_at) AS created_at,
                               boma.rfc3339(updated_at) AS updated_at",
                )
                .bind(&request.id)
                .bind(request.space_id.as_str())
                .bind(request.bot_id.as_str())
                .bind(request.requested_by.as_str())
                .bind(request.thread_id.as_deref())
                .bind(&request.origin)
                .bind(&request.display_name)
                .bind(&request.created_at)
                .fetch_one(&mut *conn)
                .await
                .map_err(tenant::unavailable)
                .and_then(into_domain)
            })
        })
        .await
    }

    async fn list(
        &self,
        context: &boma_domain::TenantContext,
        limit: Option<usize>,
    ) -> Result<Vec<boma_domain::signin::SignInRequest>, AppError> {
        let space_id = context.space_id().cloned().ok_or(AppError::NotFound)?;
        let for_find = space_id.clone();
        tenant::in_space(&self.pool, &space_id, |conn| {
            Box::pin(async move {
                let rows = sqlx::query_as::<_, SignInRequestRow>(
                    "SELECT id, space_id, bot_id, requested_by, thread_id, origin,
                            display_name, status, granted_by,
                            boma.rfc3339(created_at) AS created_at,
                            boma.rfc3339(updated_at) AS updated_at
                       FROM signin_requests
                      WHERE space_id = $1
                      ORDER BY created_at DESC
                      LIMIT $2",
                )
                .bind(for_find.as_str())
                .bind(i64::try_from(limit.unwrap_or(50)).unwrap_or(i64::MAX))
                .fetch_all(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
                rows.into_iter().map(into_domain).collect()
            })
        })
        .await
    }

    async fn resolve(
        &self,
        context: &boma_domain::TenantContext,
        id: &str,
        granted: bool,
        resolved_by: &UserId,
    ) -> Result<boma_domain::signin::SignInRequest, AppError> {
        let space_id = context.space_id().cloned().ok_or(AppError::NotFound)?;
        let for_resolve = space_id.clone();
        let resolved_by_value = resolved_by.as_str().to_owned();
        let id_value = id.to_owned();
        tenant::in_space(&self.pool, &space_id, |conn| {
            Box::pin(async move {
                let new_status = if granted { "granted" } else { "refused" };
                let row = sqlx::query_as::<_, SignInRequestRow>(
                    "UPDATE signin_requests
                        SET status = $2,
                            granted_by = $3,
                            updated_at = now()
                      WHERE id = $1 AND space_id = $4 AND status = 'pending'
                    RETURNING id, space_id, bot_id, requested_by, thread_id, origin,
                              display_name, status, granted_by,
                              boma.rfc3339(created_at) AS created_at,
                              boma.rfc3339(updated_at) AS updated_at",
                )
                .bind(id_value.as_str())
                .bind(new_status)
                .bind(resolved_by_value.as_str())
                .bind(for_resolve.as_str())
                .fetch_optional(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
                let row = row.ok_or(AppError::Conflict)?;
                into_domain(row)
            })
        })
        .await
    }
}
