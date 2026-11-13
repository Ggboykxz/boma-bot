//! Anonymous plugin submissions (REQ-034).
//!
//! A submission is not tenant data: the cookieless endpoint has no space. The table is outside
//! row-level security by design, like `rate_limits`. The review queue is an admin concern (M9);
//! this store records rows and nothing else.

use boma_app::{
    AppError,
    ports::{NewPluginSubmission, PluginSubmissionStore, PluginSubmissionSummary},
};
use sqlx::PgPool;

use crate::tenant;

/// The Postgres adapter for the plugin submissions store.
#[derive(Debug, Clone)]
pub struct PostgresPluginSubmissionStore {
    pool: PgPool,
}

impl PostgresPluginSubmissionStore {
    /// Builds the store on a pool.
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl PluginSubmissionStore for PostgresPluginSubmissionStore {
    async fn record(
        &self,
        submission: NewPluginSubmission,
    ) -> Result<PluginSubmissionSummary, AppError> {
        let submitter = submission
            .submitter_user_id
            .as_ref()
            .map(|user| user.as_str().to_owned());
        sqlx::query(
            "INSERT INTO plugin_submissions (id, source, name, contact, submitter_user_id, created_at)
             VALUES ($1, $2, $3, $4, $5, $6::timestamptz)",
        )
        .bind(&submission.id)
        .bind(&submission.source)
        .bind(&submission.name)
        .bind(&submission.contact)
        .bind(&submitter)
        .bind(&submission.created_at)
        .execute(&self.pool)
        .await
        .map_err(tenant::unavailable)?;
        Ok(PluginSubmissionSummary { id: submission.id })
    }
}
