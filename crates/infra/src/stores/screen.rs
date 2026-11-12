//! The Postgres adapter for screen capabilities (REQ-030).

use boma_app::ports::{NewScreenCapability, ScreenCapability, ScreenCapabilityStore};
use boma_domain::{ComputerId, SpaceId, TenantContext};

use boma_app::AppError;

use crate::tenant;

/// The screen capabilities of this deployment, in Postgres.
#[derive(Clone)]
pub struct PostgresScreenCapabilityStore {
    pool: sqlx::PgPool,
}

#[derive(sqlx::FromRow)]
pub(crate) struct ScreenCapabilityRow {
    pub(crate) computer_id: String,
    pub(crate) space_id: String,
    pub(crate) screen_generation: i32,
}

impl PostgresScreenCapabilityStore {
    /// Builds the store over the pool.
    #[must_use]
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl ScreenCapabilityStore for PostgresScreenCapabilityStore {
    async fn issue(
        &self,
        context: &TenantContext,
        capability: NewScreenCapability,
    ) -> Result<(), AppError> {
        let computer_key = capability.computer_id.as_str().to_owned();
        let space_key = capability.space_id.as_str().to_owned();
        let hash = capability.token_hash;
        let generation = i32::try_from(capability.screen_generation).unwrap_or(i32::MAX);
        let created_at = capability.created_at;
        tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                sqlx::query(
                    "INSERT INTO screen_capabilities (token_hash, space_id, computer_id,
                                                     screen_generation, created_at)
                     VALUES ($1, $2, $3, $4, $5::timestamptz)",
                )
                .bind(hash.as_slice())
                .bind(space_key.as_str())
                .bind(computer_key.as_str())
                .bind(generation)
                .bind(created_at.as_str())
                .execute(&mut *conn)
                .await
                .map_err(tenant::unavailable)
            })
        })
        .await?;
        Ok(())
    }

    async fn find_by_hash(&self, token_hash: &[u8]) -> Result<Option<ScreenCapability>, AppError> {
        let hash = token_hash.to_vec();
        tenant::as_system(
            &self.pool,
            boma_domain::SystemReason::Authentication,
            |conn| {
                Box::pin(async move {
                    let row = sqlx::query_as::<_, ScreenCapabilityRow>(
                        "SELECT computer_id, space_id, screen_generation
                         FROM screen_capabilities
                         WHERE token_hash = $1",
                    )
                    .bind(hash.as_slice())
                    .fetch_optional(&mut *conn)
                    .await
                    .map_err(tenant::unavailable)?;
                    row.map(|row| {
                        Ok(ScreenCapability {
                            computer_id: ComputerId::new(row.computer_id)?,
                            space_id: SpaceId::new(row.space_id)?,
                            screen_generation: u64::try_from(row.screen_generation)
                                .unwrap_or(u64::MAX),
                        })
                    })
                    .transpose()
                })
            },
        )
        .await
    }
}
