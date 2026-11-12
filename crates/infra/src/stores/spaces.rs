//! Spaces: the tenants, and who belongs to them.
//!
//! A space is tenant data, so every query here runs inside a tenant context. Two of them need
//! more than the tenant they are given:
//!
//! * `create` writes the space and its first membership in one transaction. The transaction acts
//!   inside the space it creates: that is the only tenant the new rows can belong to.
//! * `default_for` finds the space a person lands in. A person may belong to several spaces, so
//!   the question crosses tenants: it is asked on a system path, with the reason recorded
//!   ([`boma_domain::SystemReason::Authentication`], "look a person up before a tenant exists").

use boma_app::{AppError, ports::SpaceStore};
use boma_domain::{Space, SpaceId, SystemReason, UserId};
use sqlx::PgPool;

use super::rows::{self, MembershipRow, SpaceRow};
use crate::tenant;

/// The Postgres adapter for [`SpaceStore`].
#[derive(Debug, Clone)]
pub struct PostgresSpaceStore {
    pool: PgPool,
}

impl PostgresSpaceStore {
    /// Builds the store on a pool.
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl SpaceStore for PostgresSpaceStore {
    async fn create(
        &self,
        space: boma_app::ports::NewSpace,
        owner: &UserId,
    ) -> Result<Space, AppError> {
        let space_id = space.id.clone();
        let owner_id = owner.clone();

        tenant::in_space(&self.pool, &space_id, |conn| {
            Box::pin(async move {
                sqlx::query(
                    "INSERT INTO spaces (id, organization_id, name, is_default, created_at)
                     VALUES ($1, $2, $3, $4, $5::timestamptz)",
                )
                .bind(space.id.as_str())
                .bind(space.organization_id.as_str())
                .bind(&space.name)
                .bind(space.is_default)
                .bind(&space.created_at)
                .execute(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;

                // The membership belongs to the same transaction: a space nobody belongs to is
                // invisible (REQ-042).
                sqlx::query(
                    "INSERT INTO space_members (space_id, user_id, role, created_at)
                     VALUES ($1, $2, 'owner', $3::timestamptz)",
                )
                .bind(space.id.as_str())
                .bind(owner_id.as_str())
                .bind(&space.created_at)
                .execute(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;

                Ok(Space {
                    id: space.id,
                    organization_id: space.organization_id,
                    name: space.name,
                    is_default: space.is_default,
                })
            })
        })
        .await
    }

    async fn find(&self, space_id: &SpaceId, user_id: &UserId) -> Result<Option<Space>, AppError> {
        let space_key = space_id.as_str().to_owned();
        let user_key = user_id.as_str().to_owned();
        let row = tenant::in_space(&self.pool, space_id, |conn| {
            Box::pin(async move {
                sqlx::query_as::<_, SpaceRow>(
                    "SELECT s.id, s.organization_id, s.name, s.is_default
                     FROM spaces s
                     JOIN space_members m ON m.space_id = s.id AND m.user_id = $2
                     WHERE s.id = $1",
                )
                .bind(space_key.as_str())
                .bind(user_key.as_str())
                .fetch_optional(&mut *conn)
                .await
                .map_err(tenant::unavailable)
            })
        })
        .await?;

        row.map(rows::space).transpose()
    }

    async fn membership(
        &self,
        space_id: &SpaceId,
        user_id: &UserId,
    ) -> Result<Option<boma_domain::SpaceRole>, AppError> {
        let space_key = space_id.as_str().to_owned();
        let user_key = user_id.as_str().to_owned();
        let row = tenant::in_space(&self.pool, space_id, |conn| {
            Box::pin(async move {
                sqlx::query_as::<_, MembershipRow>(
                    "SELECT role FROM space_members WHERE space_id = $1 AND user_id = $2",
                )
                .bind(space_key.as_str())
                .bind(user_key.as_str())
                .fetch_optional(&mut *conn)
                .await
                .map_err(tenant::unavailable)
            })
        })
        .await?;

        row.map(|row| rows::role(&row.role)).transpose()
    }

    async fn default_for(&self, user_id: &UserId) -> Result<Option<Space>, AppError> {
        let user_key = user_id.as_str().to_owned();
        let row = tenant::as_system(&self.pool, SystemReason::Authentication, |conn| {
            Box::pin(async move {
                sqlx::query_as::<_, SpaceRow>(
                    "SELECT s.id, s.organization_id, s.name, s.is_default
                     FROM spaces s
                     JOIN space_members m ON m.space_id = s.id AND m.user_id = $1
                     ORDER BY s.is_default DESC, s.created_at ASC, s.id ASC
                     LIMIT 1",
                )
                .bind(user_key.as_str())
                .fetch_optional(&mut *conn)
                .await
                .map_err(tenant::unavailable)
            })
        })
        .await?;

        row.map(rows::space).transpose()
    }
}
