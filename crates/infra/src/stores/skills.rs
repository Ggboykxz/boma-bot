//! Taught and plugin skills (REQ-035).
//!
//! Built-in skills are code, not rows; this store carries what a space owns. A skill is
//! instructions, never code, and the instructions are served to the model through the tool
//! loop, not through a query a person can read.

use boma_app::{
    AppError,
    ports::{NewSkill, SkillRecord, SkillStore, SkillSummary},
};
use boma_domain::TenantContext;
use sqlx::PgPool;

use crate::tenant;

/// The Postgres adapter for the skills store.
#[derive(Debug, Clone)]
pub struct PostgresSkillStore {
    pool: PgPool,
}

impl PostgresSkillStore {
    /// Builds the store on a pool.
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl SkillStore for PostgresSkillStore {
    async fn list(&self, context: &TenantContext) -> Result<Vec<SkillSummary>, AppError> {
        tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                let rows = sqlx::query_as::<_, (String, String, String, Option<String>, String)>(
                    "SELECT name, description, origin, plugin_source, boma.rfc3339(created_at) AS created_at
                       FROM skills
                      ORDER BY name ASC",
                )
                .fetch_all(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
                Ok(rows
                    .into_iter()
                    .map(|(name, description, origin, plugin_source, created_at)| {
                        SkillSummary {
                            name,
                            description,
                            origin,
                            plugin_source,
                            created_at,
                        }
                    })
                    .collect())
            })
        })
        .await
    }

    async fn get(
        &self,
        context: &TenantContext,
        name: &str,
    ) -> Result<Option<SkillRecord>, AppError> {
        let name = name.to_owned();
        tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                let row = sqlx::query_as::<_, (String, String, String, String, Option<String>, String)>(
                    "SELECT name, description, instructions, origin, plugin_source, boma.rfc3339(created_at) AS created_at
                       FROM skills
                      WHERE name = $1",
                )
                .bind(&name)
                .fetch_optional(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
                Ok(row.map(|(name, description, instructions, origin, plugin_source, created_at)| {
                    SkillRecord {
                        name,
                        description,
                        instructions,
                        origin,
                        plugin_source,
                        created_at,
                    }
                }))
            })
        })
        .await
    }

    async fn put(&self, context: &TenantContext, skill: NewSkill) -> Result<(), AppError> {
        let space_id = match context {
            TenantContext::Member { space_id, .. } => space_id.as_str().to_owned(),
            // A system write is not in scope for skills; the teaching path is member-driven.
            TenantContext::System { .. } => return Err(AppError::NotFound),
        };
        let origin = skill.origin.as_str().to_owned();
        tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                sqlx::query(
                    "INSERT INTO skills (id, space_id, name, description, instructions, origin, plugin_source, created_at, updated_at)
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8::timestamptz, $8::timestamptz)
                     ON CONFLICT (space_id, name) DO UPDATE
                       SET description = EXCLUDED.description,
                           instructions = EXCLUDED.instructions,
                           origin = EXCLUDED.origin,
                           plugin_source = EXCLUDED.plugin_source,
                           updated_at = now()",
                )
                .bind(&skill.id)
                .bind(&space_id)
                .bind(&skill.name)
                .bind(&skill.description)
                .bind(&skill.instructions)
                .bind(&origin)
                .bind(&skill.plugin_source)
                .bind(&skill.created_at)
                .execute(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
                Ok(())
            })
        })
        .await
    }

    async fn remove(&self, context: &TenantContext, name: &str) -> Result<bool, AppError> {
        let name = name.to_owned();
        tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                let result = sqlx::query("DELETE FROM skills WHERE name = $1")
                    .bind(&name)
                    .execute(&mut *conn)
                    .await
                    .map_err(tenant::unavailable)?;
                Ok(result.rows_affected() > 0)
            })
        })
        .await
    }
}
