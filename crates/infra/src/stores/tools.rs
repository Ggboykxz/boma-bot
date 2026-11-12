//! The per-run tool catalogue (REQ-016).
//!
//! A run receives only the tools its bot may use: built-ins from the runtime, and the installs
//! the space shares with the bot. A bot with an empty grant sees every enabled install; a grant
//! is the list of keys it may use instead. A tool the bot may not use never appears here.

use boma_app::{AppError, ports::ToolStore};
use boma_domain::{BotId, TenantContext, ToolSpec};
use sqlx::PgPool;

use super::rows::{self, ToolInstallRow};
use crate::tenant;

/// The Postgres adapter for [`ToolStore`].
#[derive(Debug, Clone)]
pub struct PostgresToolStore {
    pool: PgPool,
}

impl PostgresToolStore {
    /// Builds the store on a pool.
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl ToolStore for PostgresToolStore {
    async fn catalogue_for(
        &self,
        context: &TenantContext,
        bot_id: &BotId,
    ) -> Result<Vec<ToolSpec>, AppError> {
        let bot_id = bot_id.as_str().to_owned();

        // The bot anchors the catalogue: no caller space required (a system context may ask,
        // the worker is one), and the tenant guard is the row-level security on the rows below.
        let bot = tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                sqlx::query_as::<_, (Vec<String>, String)>(
                    "SELECT enabled_tools, space_id FROM bots WHERE id = $1",
                )
                .bind(bot_id.as_str())
                .fetch_optional(&mut *conn)
                .await
                .map_err(tenant::unavailable)
            })
        })
        .await?;
        let Some((enabled_tools, space_id)) = bot else {
            return Ok(Vec::new());
        };
        let grant = enabled_tools;

        let installed = tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                sqlx::query_as::<_, ToolInstallRow>(
                    "SELECT tool_key, description, kind
                     FROM tool_installs
                     WHERE space_id = $1 AND enabled
                     ORDER BY tool_key ASC",
                )
                .bind(space_id.as_str())
                .fetch_all(&mut *conn)
                .await
                .map_err(tenant::unavailable)
            })
        })
        .await?;

        installed
            .into_iter()
            .filter(|row| grant.is_empty() || grant.iter().any(|key| key == &row.tool_key))
            .map(rows::tool_spec)
            .collect()
    }
}
