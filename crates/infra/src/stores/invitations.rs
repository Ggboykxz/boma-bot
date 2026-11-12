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
    async fn create(
        &self,
        invitation: NewInvitation,
    ) -> Result<boma_app::ports::Invitation, AppError> {
        let space_id = invitation.space_id.clone();

        tenant::in_space(&self.pool, &space_id, |conn| {
            Box::pin(async move {
                let row = sqlx::query_as::<_, InvitationRow>(
                    "INSERT INTO invitations (id, space_id, email, token_hash, role, expires_at, status, created_at)
                     VALUES ($1, $2, $3, $4, $5, $6::timestamptz, 'pending', $7::timestamptz)
                     RETURNING id, space_id, email, role,
                               boma.rfc3339(expires_at) AS expires_at, status",
                )
                .bind(invitation.id.as_str())
                .bind(invitation.space_id.as_str())
                .bind(invitation.email.as_str())
                .bind(invitation.token_hash)
                .bind(invitation.role.as_str())
                .bind(&invitation.expires_at)
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
                            boma.rfc3339(expires_at) AS expires_at, status
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
