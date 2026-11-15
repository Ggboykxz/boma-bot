//! External agent executors (REQ-059) — the AG-UI delegation record.
//!
//! One row per bots carrying a reference to an external executor. The value never stores the
//! bearer; only the name of the space's bot-secret that holds it. Reads are tenant-scoped and
//! never expose another tenant's configuration (REQ-043 shape).

use async_trait::async_trait;
use boma_app::{AppError, ports};
use boma_domain::{BotId, SpaceId, external::ExternalAgent};
use sqlx::PgPool;

use crate::tenant;

/// The Postgres adapter for [`ports::ExternalAgentStore`].
#[derive(Debug, Clone)]
pub struct PostgresExternalAgentStore {
    pool: PgPool,
}

impl PostgresExternalAgentStore {
    /// Builds the store on a pool.
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(sqlx::FromRow)]
struct ExternalAgentRow {
    bot_id: String,
    space_id: String,
    base_url: String,
    agent_name: Option<String>,
    secret_name: Option<String>,
    created_at: String,
    updated_at: String,
}

fn into_domain_row(row: ExternalAgentRow) -> Result<ExternalAgent, AppError> {
    Ok(ExternalAgent {
        bot_id: BotId::new(row.bot_id).map_err(|_| AppError::Unavailable)?,
        space_id: SpaceId::new(row.space_id).map_err(|_| AppError::Unavailable)?,
        base_url: row.base_url,
        agent_name: row.agent_name,
        secret_name: row.secret_name,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

#[async_trait]
impl ports::ExternalAgentStore for PostgresExternalAgentStore {
    async fn connect(
        &self,
        config: boma_domain::external::NewExternalAgent,
    ) -> Result<ExternalAgent, AppError> {
        let space_id = config.space_id.clone();
        tenant::in_space(&self.pool, &space_id, |conn| {
            Box::pin(async move {
                let row = sqlx::query_as::<_, ExternalAgentRow>(
                    "INSERT INTO bot_external_agents (bot_id, space_id, base_url, agent_name, secret_name, created_at, updated_at)
                     VALUES ($1, $2, $3, $4, $5, $6::timestamptz, $6::timestamptz)
                     ON CONFLICT (bot_id) DO UPDATE
                     SET space_id = EXCLUDED.space_id,
                         base_url = EXCLUDED.base_url,
                         agent_name = EXCLUDED.agent_name,
                         secret_name = EXCLUDED.secret_name,
                         updated_at = EXCLUDED.updated_at
                     RETURNING bot_id, space_id, base_url, agent_name, secret_name,
                               boma.rfc3339(created_at) AS created_at,
                               boma.rfc3339(updated_at) AS updated_at",
                )
                .bind(config.bot_id.as_str())
                .bind(config.space_id.as_str())
                .bind(&config.base_url)
                .bind(config.agent_name.as_deref())
                .bind(config.secret_name.as_deref())
                .bind(&config.saved_at)
                .fetch_one(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
                into_domain_row(row)
            })
        })
        .await
    }

    async fn disconnect(
        &self,
        context: &boma_domain::TenantContext,
        bot_id: &BotId,
    ) -> Result<(), AppError> {
        let bot_id_value = bot_id.as_str().to_owned();
        let space_id = context.space_id().cloned().ok_or(AppError::NotFound)?;
        let for_delete = space_id.clone();
        tenant::in_space(&self.pool, &space_id, |conn| {
            Box::pin(async move {
                sqlx::query("DELETE FROM bot_external_agents WHERE bot_id = $1 AND space_id = $2")
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
    ) -> Result<Option<ExternalAgent>, AppError> {
        let bot_id_value = bot_id.as_str().to_owned();
        let space_id = context.space_id().cloned().ok_or(AppError::NotFound)?;
        let for_find = space_id.clone();
        tenant::in_space(&self.pool, &space_id, |conn| {
            Box::pin(async move {
                let row = sqlx::query_as::<_, ExternalAgentRow>(
                    "SELECT bot_id, space_id, base_url, agent_name, secret_name,
                            boma.rfc3339(created_at) AS created_at,
                            boma.rfc3339(updated_at) AS updated_at
                       FROM bot_external_agents
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
