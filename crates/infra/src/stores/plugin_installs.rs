//! Plugin installs (REQ-034).
//!
//! One row per (space, source); reinstalling a source replaces its plugin skills wholesale —
//! the install step revokes the plugin's own skills before writing the fresh set, so a reinstall
//! never leaves a half-old, half-new bundle behind.

use boma_app::{
    AppError,
    ports::{NewPluginInstall, PluginInstallStore, PluginInstallSummary},
};
use boma_domain::TenantContext;
use sqlx::PgPool;

use crate::tenant;

/// The Postgres adapter for the plugin install store.
#[derive(Debug, Clone)]
pub struct PostgresPluginInstallStore {
    pool: PgPool,
}

impl PostgresPluginInstallStore {
    /// Builds the store on a pool.
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl PluginInstallStore for PostgresPluginInstallStore {
    async fn list(&self, context: &TenantContext) -> Result<Vec<PluginInstallSummary>, AppError> {
        tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                let rows = sqlx::query_as::<
                    _,
                    (String, String, String, Option<String>, serde_json::Value, serde_json::Value, String),
                >(
                    "SELECT id, name, source, resolved_sha, parts, skipped, boma.rfc3339(created_at) AS created_at
                       FROM plugin_installs
                      ORDER BY created_at DESC, id ASC",
                )
                .fetch_all(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
                Ok(rows
                    .into_iter()
                    .map(|(id, name, source, resolved_sha, parts, skipped, created_at)| {
                        PluginInstallSummary {
                            id,
                            name,
                            source,
                            resolved_sha,
                            parts,
                            skipped,
                            created_at,
                        }
                    })
                    .collect())
            })
        })
        .await
    }

    async fn record(
        &self,
        context: &TenantContext,
        install: NewPluginInstall,
    ) -> Result<(), AppError> {
        let space_id = match context {
            TenantContext::Member { space_id, .. } => space_id.as_str().to_owned(),
            TenantContext::System { .. } => return Err(AppError::NotFound),
        };
        tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                sqlx::query(
                    "INSERT INTO plugin_installs (id, space_id, source, resolved_sha, name, parts, skipped, created_at, updated_at)
                     VALUES ($1, $2, $3, $4, $5, $6::jsonb, $7::jsonb, $8::timestamptz, now())
                     ON CONFLICT (space_id, source) DO UPDATE
                       SET resolved_sha = EXCLUDED.resolved_sha,
                           name = EXCLUDED.name,
                           parts = EXCLUDED.parts,
                           skipped = EXCLUDED.skipped,
                           created_at = now(),
                           updated_at = now()",
                )
                .bind(&install.id)
                .bind(&space_id)
                .bind(&install.source)
                .bind(&install.resolved_sha)
                .bind(&install.name)
                .bind(&install.parts)
                .bind(serde_json::Value::Array(install.skipped))
                .bind(&install.created_at)
                .execute(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
                Ok(())
            })
        })
        .await
    }

    async fn remove_plugin_skills(
        &self,
        context: &TenantContext,
        source: &str,
    ) -> Result<u32, AppError> {
        let source = source.to_owned();
        let result = tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                sqlx::query(
                    "DELETE FROM skills
                      WHERE origin = 'plugin'
                        AND plugin_source = $1",
                )
                .bind(&source)
                .execute(&mut *conn)
                .await
                .map_err(tenant::unavailable)
            })
        })
        .await?;
        Ok(u32::try_from(result.rows_affected()).unwrap_or(u32::MAX))
    }
}
