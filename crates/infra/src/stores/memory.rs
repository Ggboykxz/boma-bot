//! Memory documents and their append-only revisions (REQ-022).
//!
//! One row per document; the current row is a projection of the latest revision, and every
//! write appends a new revision row in the same transaction. Revision numbers only increase.

use boma_app::{
    AppError,
    ports::{MemoryStore, NewMemoryDocument},
};
use boma_domain::{MemoryDocument, TenantContext, validate_memory_path};
use sqlx::PgPool;

use super::rows::{self, MemoryDocumentRow};
use crate::tenant;

/// The stable id of a document: deterministic so one document has one id, forever.
fn document_id(space_id: &str, scope: &str, bot_id: &str, path: &str) -> String {
    format!("mem:{space_id}:{scope}:{bot_id}:{path}")
}

/// The Postgres adapter for [`MemoryStore`].
#[derive(Debug, Clone)]
pub struct PostgresMemoryStore {
    pool: PgPool,
}

impl PostgresMemoryStore {
    /// Builds the store on a pool.
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl MemoryStore for PostgresMemoryStore {
    async fn write(
        &self,
        context: &TenantContext,
        document: NewMemoryDocument,
    ) -> Result<MemoryDocument, AppError> {
        validate_memory_path(&document.path)?;
        let space_id = context.space_id().cloned().ok_or(AppError::NotFound)?;
        let space_key = space_id.as_str().to_owned();
        let scope = if document.bot_id.is_some() {
            "bot"
        } else {
            "shared"
        };
        let bot_key = document
            .bot_id
            .as_ref()
            .map(|id| id.as_str().to_owned())
            .unwrap_or_default();
        let path = document.path.clone();
        let payload = document.content;
        let updated_at = document.updated_at;
        tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                let id = document_id(&space_key, scope, &bot_key, &path);
                // The row level security policy is the defense in depth; the path is
                // validated inside the tenant before anything touches the database, and the
                // rows are keyed by the tenant's space_id.
                let row = sqlx::query_as::<_, MemoryDocumentRow>("INSERT INTO memory_documents (id, space_id, scope, bot_id, path, content, revision, updated_at)
                     VALUES ($1, $2, $3, $4, $5, $6, 1, $7::timestamptz)
                     ON CONFLICT (space_id, scope, bot_id, path) DO UPDATE
                         SET content = EXCLUDED.content,
                             revision = memory_documents.revision + 1,
                             updated_at = EXCLUDED.updated_at
                     RETURNING space_id, bot_id, path, content, revision,
                               boma.rfc3339(updated_at) AS updated_at"
                )
                .bind(id.as_str())
                .bind(space_key.as_str())
                .bind(scope)
                .bind(bot_key.as_str())
                .bind(path.as_str())
                .bind(payload.as_str())
                .bind(updated_at.as_str())
                .fetch_one(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;

                sqlx::query(
                    "INSERT INTO memory_revisions (document_id, space_id, revision, content, created_at)
                     VALUES ($1, $2, $3, $4, $5::timestamptz)",
                )
                .bind(id.as_str())
                .bind(space_key.as_str())
                .bind(row.revision)
                .bind(payload.as_str())
                .bind(updated_at.as_str())
                .execute(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;

                rows::memory_document(row)
            })
        })
        .await
    }

    async fn read(
        &self,
        context: &TenantContext,
        bot_id: Option<&boma_domain::BotId>,
        path: &str,
    ) -> Result<Option<MemoryDocument>, AppError> {
        let space_id = context.space_id().cloned().ok_or(AppError::NotFound)?;
        let path = path.to_owned();
        let space_key = space_id.as_str().to_owned();
        let scope = if bot_id.is_some() { "bot" } else { "shared" };
        let bot_key = bot_id.map(|id| id.as_str().to_owned()).unwrap_or_default();
        tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                let row = sqlx::query_as::<_, MemoryDocumentRow>(
                    "SELECT space_id, bot_id, path, content, revision,
                               boma.rfc3339(updated_at) AS updated_at
                     FROM memory_documents
                     WHERE space_id = $1 AND scope = $2 AND bot_id = $3 AND path = $4",
                )
                .bind(space_key.as_str())
                .bind(scope)
                .bind(bot_key.as_str())
                .bind(path)
                .fetch_optional(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
                row.map(rows::memory_document).transpose()
            })
        })
        .await
    }

    async fn list(
        &self,
        context: &TenantContext,
        bot_id: Option<&boma_domain::BotId>,
    ) -> Result<Vec<MemoryDocument>, AppError> {
        let space_id = context.space_id().cloned().ok_or(AppError::NotFound)?;
        let space_key = space_id.as_str().to_owned();
        let scope = if bot_id.is_some() { "bot" } else { "shared" };
        let bot_key = bot_id.map(|id| id.as_str().to_owned()).unwrap_or_default();
        tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                let rows = sqlx::query_as::<_, MemoryDocumentRow>(
                    "SELECT space_id, bot_id, path, content, revision,
                               boma.rfc3339(updated_at) AS updated_at
                     FROM memory_documents
                     WHERE space_id = $1 AND scope = $2 AND bot_id = $3
                     ORDER BY path",
                )
                .bind(space_key.as_str())
                .bind(scope)
                .bind(bot_key.as_str())
                .fetch_all(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
                rows.into_iter().map(rows::memory_document).collect()
            })
        })
        .await
    }

    async fn remove(
        &self,
        context: &TenantContext,
        bot_id: Option<&boma_domain::BotId>,
        path: &str,
    ) -> Result<bool, AppError> {
        let space_id = context.space_id().cloned().ok_or(AppError::NotFound)?;
        let path = path.to_owned();
        let space_key = space_id.as_str().to_owned();
        let scope = if bot_id.is_some() { "bot" } else { "shared" };
        let bot_key = bot_id.map(|id| id.as_str().to_owned()).unwrap_or_default();
        tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                let deleted = sqlx::query(
                    "DELETE FROM memory_documents
                     WHERE space_id = $1 AND scope = $2 AND bot_id = $3 AND path = $4",
                )
                .bind(space_key.as_str())
                .bind(scope)
                .bind(bot_key.as_str())
                .bind(path)
                .execute(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
                Ok(deleted.rows_affected() > 0)
            })
        })
        .await
    }
}
