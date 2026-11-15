//! The admin store: members and invitations of the caller's space, grants of the deployment,
//! and the plugin-submission review queue (REQ-058).
//!
//! Grants and submissions are not tenant data: they belong to the whole deployment and only
//! the operator's own channels can see them. Members and invitations are space-scoped like
//! everything else; the use case gates by role before calling.

use boma_app::{
    AppError,
    ports::{AdminGrant, AdminInvitation, AdminMember, AdminPluginSubmission, AdminStore},
};
use boma_domain::{Email, SpaceId, TenantContext, UserId};
use sqlx::PgPool;

use crate::tenant;

/// The Postgres adapter for [`AdminStore`].
#[derive(Debug, Clone)]
pub struct PostgresAdminStore {
    pool: PgPool,
}

impl PostgresAdminStore {
    /// Builds the store on a pool.
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl AdminStore for PostgresAdminStore {
    async fn members(
        &self,
        context: &TenantContext,
        space_id: &SpaceId,
    ) -> Result<Vec<AdminMember>, AppError> {
        let space_id = space_id.as_str().to_owned();
        let rows = tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                sqlx::query_as::<_, (String, String, String, Option<String>, String)>(
                    "SELECT u.id, u.email, u.name, boma.rfc3339(u.deactivated_at), sm.role
                       FROM users u
                       JOIN space_members sm ON sm.user_id = u.id
                      WHERE sm.space_id = $1
                      ORDER BY u.name ASC, u.email ASC",
                )
                .bind(&space_id)
                .fetch_all(&mut *conn)
                .await
                .map_err(tenant::unavailable)
            })
        })
        .await?;
        Ok(rows
            .into_iter()
            .map(
                |(id, email, name, _deactivated_at, role)| -> Result<AdminMember, AppError> {
                    Ok(AdminMember {
                        user_id: UserId::new(id).map_err(|_| AppError::Unavailable)?,
                        email: Email::new(email).map_err(|_| AppError::Unavailable)?,
                        name,
                        role: crate::stores::rows::role(&role)?,
                        last_seen_at: None,
                    })
                },
            )
            .collect::<Result<Vec<_>, _>>()?)
    }

    async fn invitations(
        &self,
        context: &TenantContext,
        space_id: &SpaceId,
    ) -> Result<Vec<AdminInvitation>, AppError> {
        let space_id = space_id.as_str().to_owned();
        let rows = tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                sqlx::query_as::<_, (String, String, String, String)>(
                    "SELECT id, email, role, boma.rfc3339(expires_at)
                       FROM invitations
                      WHERE space_id = $1
                        AND status = 'pending'
                      ORDER BY created_at DESC",
                )
                .bind(&space_id)
                .fetch_all(&mut *conn)
                .await
                .map_err(tenant::unavailable)
            })
        })
        .await?;
        Ok(rows
            .into_iter()
            .map(
                |(id, email, role, expires_at)| -> Result<AdminInvitation, AppError> {
                    Ok(AdminInvitation {
                        id,
                        email,
                        role: crate::stores::rows::role(&role)?,
                        expires_at,
                    })
                },
            )
            .collect::<Result<Vec<_>, _>>()?)
    }

    async fn revoke_invitation(
        &self,
        context: &TenantContext,
        space_id: &SpaceId,
        invitation_id: &str,
    ) -> Result<bool, AppError> {
        let space_id = space_id.as_str().to_owned();
        let invitation_id = invitation_id.to_owned();
        tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                let result = sqlx::query(
                    "UPDATE invitations SET status = 'revoked'
                      WHERE id = $1 AND space_id = $2 AND status = 'pending'",
                )
                .bind(&invitation_id)
                .bind(&space_id)
                .execute(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
                Ok(result.rows_affected() > 0)
            })
        })
        .await
    }

    async fn grants(&self) -> Result<Vec<AdminGrant>, AppError> {
        let rows = tenant::as_system(
            &self.pool,
            boma_domain::SystemReason::ScheduledJob,
            |conn| {
                Box::pin(async move {
                    sqlx::query_as::<_, (String, String, String, String)>(
                        "SELECT email, reason, granted_by, boma.rfc3339(created_at)
                           FROM access_grants
                          ORDER BY email ASC",
                    )
                    .fetch_all(&mut *conn)
                    .await
                    .map_err(tenant::unavailable)
                })
            },
        )
        .await?;
        Ok(rows
            .into_iter()
            .map(|(email, reason, granted_by, created_at)| AdminGrant {
                email,
                reason,
                granted_by,
                created_at,
            })
            .collect())
    }

    async fn grant_access(
        &self,
        email: &boma_domain::Email,
        reason: &str,
        granted_by: &UserId,
        created_at: &str,
    ) -> Result<AdminGrant, AppError> {
        let email = email.as_str().to_owned();
        let reason = reason.to_owned();
        let granted_by = granted_by.as_str().to_owned();
        let created_at = created_at.to_owned();
        tenant::as_system(
            &self.pool,
            boma_domain::SystemReason::ScheduledJob,
            |conn| {
                Box::pin(async move {
                    let result = sqlx::query(
                        "INSERT INTO access_grants (email, reason, granted_by, created_at)
                         VALUES ($1, $2, $3, $4::timestamptz)
                         ON CONFLICT (email) DO NOTHING",
                    )
                    .bind(&email)
                    .bind(&reason)
                    .bind(&granted_by)
                    .bind(&created_at)
                    .execute(&mut *conn)
                    .await
                    .map_err(tenant::unavailable)?;
                    if result.rows_affected() == 0 {
                        return Err(AppError::Conflict);
                    }
                    let row = sqlx::query_as::<_, (String, String, String, String)>(
                        "SELECT email, reason, granted_by, boma.rfc3339(created_at)
                           FROM access_grants WHERE email = $1",
                    )
                    .bind(&email)
                    .fetch_one(&mut *conn)
                    .await
                    .map_err(tenant::unavailable)?;
                    Ok(AdminGrant {
                        email: row.0,
                        reason: row.1,
                        granted_by: row.2,
                        created_at: row.3,
                    })
                })
            },
        )
        .await
    }

    async fn revoke_grant(&self, email: &boma_domain::Email) -> Result<bool, AppError> {
        let email = email.as_str().to_owned();
        tenant::as_system(
            &self.pool,
            boma_domain::SystemReason::ScheduledJob,
            |conn| {
                Box::pin(async move {
                    let result = sqlx::query("DELETE FROM access_grants WHERE email = $1")
                        .bind(&email)
                        .execute(&mut *conn)
                        .await
                        .map_err(tenant::unavailable)?;
                    Ok(result.rows_affected() > 0)
                })
            },
        )
        .await
    }

    async fn plugin_submissions(
        &self,
        status: Option<&str>,
        limit: i64,
    ) -> Result<Vec<AdminPluginSubmission>, AppError> {
        let status = status.map(str::to_owned);
        tenant::as_system(
            &self.pool,
            boma_domain::SystemReason::ScheduledJob,
            |conn| {
                Box::pin(async move {
                    let rows =
                        sqlx::query_as::<_, (String, String, Option<String>, String, String)>(
                            "SELECT id, source, name, status, boma.rfc3339(created_at)
                           FROM plugin_submissions
                          WHERE ($1::text IS NULL OR status = $1)
                          ORDER BY created_at DESC
                          LIMIT $2",
                        )
                        .bind(status.as_deref())
                        .bind(limit)
                        .fetch_all(&mut *conn)
                        .await
                        .map_err(tenant::unavailable)?;
                    Ok(rows
                        .into_iter()
                        .map(
                            |(id, source, name, status, created_at)| AdminPluginSubmission {
                                id,
                                source,
                                name,
                                status,
                                created_at,
                            },
                        )
                        .collect::<Vec<_>>())
                })
            },
        )
        .await
    }

    async fn decide_submission(
        &self,
        id: &str,
        decision: &str,
        _reviewer: &UserId,
    ) -> Result<(), AppError> {
        let id = id.to_owned();
        let decision = decision.to_owned();
        tenant::as_system(
            &self.pool,
            boma_domain::SystemReason::ScheduledJob,
            |conn| {
                Box::pin(async move {
                    let result = sqlx::query(
                        "UPDATE plugin_submissions SET status = $2
                          WHERE id = $1 AND status = 'pending'",
                    )
                    .bind(&id)
                    .bind(&decision)
                    .execute(&mut *conn)
                    .await
                    .map_err(tenant::unavailable)?;
                    if result.rows_affected() == 0 {
                        return Err(AppError::Conflict);
                    }
                    Ok(())
                })
            },
        )
        .await
    }
}
