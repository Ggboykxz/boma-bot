//! Invitations: the way somebody joins a space.
//!
//! The token never leaves the application: it is hashed, and only the hash is stored (REQ-042).
//!
//! Reading an invitation by its token hash crosses tenants: the hash is all the caller has, and
//! the space it belongs to is what the answer contains. The lookup therefore runs on a system
//! path, with the reason recorded
//! ([`boma_domain::SystemReason::Authentication`], "look a person up before a tenant exists").

use boma_app::{
    AppError,
    ports::{InvitationStore, NewInvitation},
};
use boma_domain::SystemReason;
use sqlx::PgPool;

use super::rows::{self, InvitationRow};
use crate::tenant;

/// The Postgres adapter for [`InvitationStore`].
#[derive(Debug, Clone)]
pub struct PostgresInvitationStore {
    pool: PgPool,
}

impl PostgresInvitationStore {
    /// Builds the store on a pool.
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl InvitationStore for PostgresInvitationStore {
    async fn accept(&self, id: &boma_domain::InviteId, _now: &str) -> Result<(), AppError> {
        let id = id.as_str().to_owned();
        tenant::as_system(
            &self.pool,
            boma_domain::SystemReason::Authentication,
            |conn| {
                Box::pin(async move {
                    sqlx::query("UPDATE invitations SET status = 'accepted' WHERE id = $1")
                        .bind(&id)
                        .execute(&mut *conn)
                        .await
                        .map_err(tenant::unavailable)?;
                    Ok(())
                })
            },
        )
        .await
    }
    async fn create(
        &self,
        invitation: NewInvitation,
    ) -> Result<boma_app::ports::Invitation, AppError> {
        let space_id = invitation.space_id.clone();

        tenant::in_space(&self.pool, &space_id, |conn| {
            Box::pin(async move {
                let row = sqlx::query_as::<_, InvitationRow>(
                    "INSERT INTO invitations (id, space_id, email, token_hash, role, expires_at, status, inviter_user_id, created_at)
                     VALUES ($1, $2, $3, $4, $5, $6::timestamptz, 'pending', $7, $8::timestamptz)
                     RETURNING id, space_id, email, role,
                               boma.rfc3339(expires_at) AS expires_at, status, inviter_user_id",
                )
                .bind(invitation.id.as_str())
                .bind(invitation.space_id.as_str())
                .bind(invitation.email.as_str())
                .bind(invitation.token_hash)
                .bind(invitation.role.as_str())
                .bind(&invitation.expires_at)
                .bind(invitation.inviter_user_id.as_ref().map(boma_domain::UserId::as_str))
                .bind(&invitation.created_at)
                .fetch_one(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;

                rows::invitation(row)
            })
        })
        .await
    }

    async fn find_by_token_hash(
        &self,
        token_hash: &[u8],
    ) -> Result<Option<boma_app::ports::Invitation>, AppError> {
        let token_hash = token_hash.to_vec();
        let row = tenant::as_system(&self.pool, SystemReason::Authentication, |conn| {
            Box::pin(async move {
                sqlx::query_as::<_, InvitationRow>(
                    "SELECT id, space_id, email, role,
                            boma.rfc3339(expires_at) AS expires_at, status, inviter_user_id
                     FROM invitations
                     WHERE token_hash = $1
                     ORDER BY created_at DESC
                     LIMIT 1",
                )
                .bind(token_hash)
                .fetch_optional(&mut *conn)
                .await
                .map_err(tenant::unavailable)
            })
        })
        .await?;

        row.map(rows::invitation).transpose()
    }
}
