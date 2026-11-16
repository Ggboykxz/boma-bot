//! Local ACP CLI executors (REQ-061) — the attachment record.
//!
//! One row per bots carrying the known executor id. The value never stores a credential; sign-in
//! stays with the CLI's own login state on this machine. Reads are tenant-scoped and never expose
//! another tenant's configuration (REQ-043 shape).

use async_trait::async_trait;
use boma_app::{AppError, ports};
use boma_domain::{BotId, acp::AcpAgent};
use sqlx::PgPool;

use crate::tenant;

/// The Postgres adapter for [`ports::AcpAgentStore`].
#[derive(Debug, Clone)]
pub struct PostgresAcpAgentStore {
    pool: PgPool,
}

impl PostgresAcpAgentStore {
    /// Builds the store on a pool.
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(sqlx::FromRow)]
struct AcpAgentRow {
    bot_id: String,
    space_id: String,
    executor: String,
    created_at: String,
    updated_at: String,
}

fn into_domain_row(row: AcpAgentRow) -> Result<AcpAgent, AppError> {
    Ok(AcpAgent {
        bot_id: BotId::new(row.bot_id).map_err(|_| AppError::Unavailable)?,
        space_id: boma_domain::SpaceId::new(row.space_id).map_err(|_| AppError::Unavailable)?,
        executor: row.executor,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

#[async_trait]
impl ports::AcpAgentStore for PostgresAcpAgentStore {
    async fn attach(&self, config: boma_domain::acp::NewAcpAgent) -> Result<AcpAgent, AppError> {
        let space_id = config.space_id.clone();
        tenant::in_space(&self.pool, &space_id, |conn| {
            Box::pin(async move {
                let row = sqlx::query_as::<_, AcpAgentRow>(
                    "INSERT INTO bot_acp_agents (bot_id, space_id, executor, created_at, updated_at)
                     VALUES ($1, $2, $3, $4::timestamptz, $4::timestamptz)
                     ON CONFLICT (bot_id) DO UPDATE
                     SET executor = EXCLUDED.executor,
                         updated_at = EXCLUDED.updated_at
                     RETURNING bot_id, space_id, executor,
                               boma.rfc3339(created_at) AS created_at,
                               boma.rfc3339(updated_at) AS updated_at",
                )
                .bind(config.bot_id.as_str())
                .bind(config.space_id.as_str())
                .bind(&config.executor)
                .bind(&config.saved_at)
                .fetch_one(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
                into_domain_row(row)
            })
        })
        .await
    }

    async fn detach(
        &self,
        context: &boma_domain::TenantContext,
        bot_id: &BotId,
    ) -> Result<(), AppError> {
        let bot_id_value = bot_id.as_str().to_owned();
        let space_id = context.space_id().cloned().ok_or(AppError::NotFound)?;
        let for_delete = space_id.clone();
        tenant::in_space(&self.pool, &space_id, |conn| {
            Box::pin(async move {
                sqlx::query("DELETE FROM bot_acp_agents WHERE bot_id = $1 AND space_id = $2")
                    .bind(bot_id_value.as_str())
                    .bind(for_delete.as_str())
                    .execute(&mut *conn)
                    .await
                    .map_err(tenant::unavailable)?;
                Ok(())
            })
        })
        .await
    }

    async fn find(
        &self,
        context: &boma_domain::TenantContext,
        bot_id: &BotId,
    ) -> Result<Option<AcpAgent>, AppError> {
        let bot_id_value = bot_id.as_str().to_owned();
        let space_id = context.space_id().cloned().ok_or(AppError::NotFound)?;
        let for_find = space_id.clone();
        tenant::in_space(&self.pool, &space_id, |conn| {
            Box::pin(async move {
                let row = sqlx::query_as::<_, AcpAgentRow>(
                    "SELECT bot_id, space_id, executor,
                            boma.rfc3339(created_at) AS created_at,
                            boma.rfc3339(updated_at) AS updated_at
                       FROM bot_acp_agents
                      WHERE bot_id = $1 AND space_id = $2",
                )
                .bind(bot_id_value.as_str())
                .bind(for_find.as_str())
                .fetch_optional(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
                row.map(into_domain_row).transpose()
            })
        })
        .await
    }
}
