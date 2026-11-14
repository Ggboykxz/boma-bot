//! Artifact metadata rows (REQ-036). Bytes live out of band, addressed by their hash.

use boma_app::{
    AppError,
    ports::{ArtifactStore, ArtifactSummary, NewArtifact},
};
use boma_domain::TenantContext;
use sqlx::PgPool;

use crate::tenant;

/// The Postgres adapter for the artifact metadata store.
#[derive(Debug, Clone)]
pub struct PostgresArtifactStore {
    pool: PgPool,
}

impl PostgresArtifactStore {
    /// Builds the store on a pool.
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

/// The tuple a summary query returns: id, bot_id, group_id, user_id, run_id, name,
/// description, declared_mime, mime_type, hash, size, storage_key, root_artifact_id, version, created_at.
type ArtifactSummaryRow = (
    String,
    Option<String>,
    Option<String>,
    String,
    Option<String>,
    String,
    String,
    String,
    String,
    String,
    i64,
    String,
    String,
    i32,
    String,
);

fn summary_from(row: ArtifactSummaryRow) -> ArtifactSummary {
    let (
        id,
        bot_id,
        group_id,
        user_id,
        run_id,
        name,
        description,
        declared_mime,
        mime_type,
        hash,
        size,
        storage_key,
        root_artifact_id,
        version,
        created_at,
    ) = row;
    ArtifactSummary {
        id,
        bot_id,
        group_id,
        user_id,
        run_id,
        name,
        description,
        declared_mime,
        mime_type,
        size: u64::try_from(size).unwrap_or(u64::MAX),
        hash,
        storage_key,
        root_artifact_id,
        version,
        created_at,
    }
}

const SQL_FIND_BY_NONCE: &str = concat!(
    "SELECT ",
    "id, bot_id, group_id, user_id, run_id, name, description, declared_mime,",
    " mime_type, hash, size, storage_key, root_artifact_id, version,",
    " boma.rfc3339(created_at) FROM artifacts WHERE user_id = $1 AND client_nonce = $2"
);
const SQL_GET_BY_ID: &str = concat!(
    "SELECT ",
    "id, bot_id, group_id, user_id, run_id, name, description, declared_mime,",
    " mime_type, hash, size, storage_key, root_artifact_id, version,",
    " boma.rfc3339(created_at) FROM artifacts WHERE id = $1"
);
const SQL_GET_EXACT: &str = concat!(
    "SELECT ",
    "id, bot_id, group_id, user_id, run_id, name, description, declared_mime,",
    " mime_type, hash, size, storage_key, root_artifact_id, version,",
    " boma.rfc3339(created_at) FROM artifacts WHERE id = $1"
);
const SQL_GET_PINNED: &str = concat!(
    "SELECT ",
    "id, bot_id, group_id, user_id, run_id, name, description, declared_mime,",
    " mime_type, hash, size, storage_key, root_artifact_id, version,",
    " boma.rfc3339(created_at) FROM artifacts",
    " WHERE (id = $1 OR root_artifact_id = $1) AND version = $2"
);
const SQL_GET_LATEST: &str = concat!(
    "SELECT ",
    "id, bot_id, group_id, user_id, run_id, name, description, declared_mime,",
    " mime_type, hash, size, storage_key, root_artifact_id, version,",
    " boma.rfc3339(created_at) FROM artifacts",
    " WHERE id = $1 OR root_artifact_id = $1 ORDER BY version DESC LIMIT 1"
);
const SQL_LIST_VERSIONS: &str = concat!(
    "SELECT ",
    "id, bot_id, group_id, user_id, run_id, name, description, declared_mime,",
    " mime_type, hash, size, storage_key, root_artifact_id, version,",
    " boma.rfc3339(created_at) FROM artifacts",
    " WHERE root_artifact_id = (SELECT root_artifact_id FROM artifacts WHERE id = $1)",
    " ORDER BY version DESC"
);
const SQL_LIST_SPACE: &str = concat!(
    "SELECT ",
    "id, bot_id, group_id, user_id, run_id, name, description, declared_mime,",
    " mime_type, hash, size, storage_key, root_artifact_id, version,",
    " boma.rfc3339(created_at) FROM artifacts",
    " WHERE ($1::text IS NULL OR bot_id = $1) AND ($2::text IS NULL OR group_id = $2)",
    "   AND version = (SELECT max(version) FROM artifacts a2",
    "                   WHERE a2.root_artifact_id = artifacts.root_artifact_id)",
    " ORDER BY created_at DESC LIMIT $3"
);

#[async_trait::async_trait]
impl ArtifactStore for PostgresArtifactStore {
    async fn find_by_nonce(
        &self,
        context: &TenantContext,
        client_nonce: &str,
    ) -> Result<Option<ArtifactSummary>, AppError> {
        let user_id = match context {
            TenantContext::Member { user_id, .. } => user_id.as_str().to_owned(),
            TenantContext::System { .. } => return Ok(None),
        };
        let client_nonce = client_nonce.to_owned();
        let row = tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                sqlx::query_as::<
                    _,
                    (
                        String,
                        Option<String>,
                        Option<String>,
                        String,
                        Option<String>,
                        String,
                        String,
                        String,
                        String,
                        String,
                        i64,
                        String,
                        String,
                        i32,
                        String,
                    ),
                >(SQL_FIND_BY_NONCE)
                .bind(&user_id)
                .bind(&client_nonce)
                .fetch_optional(&mut *conn)
                .await
                .map_err(tenant::unavailable)
            })
        })
        .await?;
        Ok(row.map(summary_from))
    }

    async fn put(
        &self,
        context: &TenantContext,
        artifact: NewArtifact,
    ) -> Result<ArtifactSummary, AppError> {
        let space_id = match context {
            TenantContext::Member { space_id, .. } => space_id.as_str().to_owned(),
            TenantContext::System { .. } => return Err(AppError::NotFound),
        };
        let user_id = match context {
            TenantContext::Member { user_id, .. } => user_id.as_str().to_owned(),
            TenantContext::System { .. } => return Err(AppError::NotFound),
        };
        tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                let root = artifact
                    .root_artifact_id
                    .clone()
                    .unwrap_or_else(|| artifact.id.clone());
                // The root must belong to this space; guessing roots is not a read primitive.
                if root != artifact.id {
                    let exists: Option<String> = sqlx::query_scalar(
                        "SELECT id FROM artifacts WHERE root_artifact_id = $1 OR id = $1 LIMIT 1",
                    )
                    .bind(&root)
                    .fetch_optional(&mut *conn)
                    .await
                    .map_err(tenant::unavailable)?;
                    if exists.is_none() {
                        return Err(AppError::NotFound);
                    }
                }
                let version: i32 = sqlx::query_scalar(
                    "SELECT coalesce(max(version), 0) + 1 FROM artifacts WHERE root_artifact_id = $1",
                )
                .bind(&root)
                .fetch_one(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
                sqlx::query(
                    "INSERT INTO artifacts (id, space_id, bot_id, group_id, user_id, run_id, name, description,
                                      declared_mime, mime_type, size, hash, storage_key, root_artifact_id,
                                      version, standalone, client_nonce, created_at)
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, true, $16, $17::timestamptz)",
                )
                .bind(&artifact.id)
                .bind(&space_id)
                .bind(&artifact.bot_id)
                .bind(&artifact.group_id)
                .bind(&user_id)
                .bind(&artifact.run_id)
                .bind(&artifact.name)
                .bind(&artifact.description)
                .bind(&artifact.declared_mime)
                .bind(&artifact.mime_type)
                .bind(i64::try_from(artifact.size).map_err(|_| AppError::Unavailable)?)
                .bind(&artifact.hash)
                .bind(&artifact.hash)
                .bind(&root)
                .bind(version)
                .bind(&artifact.client_nonce)
                .bind(&artifact.created_at)
                .execute(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
                let row = sqlx::query_as::<_, (
                    String,
                    Option<String>,
                    Option<String>,
                    String,
                    Option<String>,
                    String,
                    String,
                    String,
                    String,
                    String,
                    i64,
                    String,
                    String,
                    i32,
                    String,
                )>(SQL_GET_BY_ID)
                .bind(&artifact.id)
                .fetch_one(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
                Ok(summary_from(row))
            })
        })
        .await
    }

    async fn get(
        &self,
        context: &TenantContext,
        artifact_id: &str,
        version: Option<i32>,
    ) -> Result<Option<ArtifactSummary>, AppError> {
        let artifact_id = artifact_id.to_owned();
        let row = tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                let query = if let Some(version) = version {
                    sqlx::query_as::<
                        _,
                        (
                            String,
                            Option<String>,
                            Option<String>,
                            String,
                            Option<String>,
                            String,
                            String,
                            String,
                            String,
                            String,
                            i64,
                            String,
                            String,
                            i32,
                            String,
                        ),
                    >(SQL_GET_PINNED)
                    .bind(&artifact_id)
                    .bind(version)
                } else {
                    sqlx::query_as::<
                        _,
                        (
                            String,
                            Option<String>,
                            Option<String>,
                            String,
                            Option<String>,
                            String,
                            String,
                            String,
                            String,
                            String,
                            i64,
                            String,
                            String,
                            i32,
                            String,
                        ),
                    >(SQL_GET_LATEST)
                    .bind(&artifact_id)
                };
                query
                    .fetch_optional(&mut *conn)
                    .await
                    .map_err(tenant::unavailable)
            })
        })
        .await?;
        Ok(row.map(summary_from))
    }

    async fn get_exact(
        &self,
        context: &TenantContext,
        artifact_id: &str,
    ) -> Result<Option<ArtifactSummary>, AppError> {
        let artifact_id = artifact_id.to_owned();
        let row = tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                sqlx::query_as::<
                    _,
                    (
                        String,
                        Option<String>,
                        Option<String>,
                        String,
                        Option<String>,
                        String,
                        String,
                        String,
                        String,
                        String,
                        i64,
                        String,
                        String,
                        i32,
                        String,
                    ),
                >(SQL_GET_EXACT)
                .bind(&artifact_id)
                .fetch_optional(&mut *conn)
                .await
                .map_err(tenant::unavailable)
            })
        })
        .await?;
        Ok(row.map(summary_from))
    }

    async fn list_versions(
        &self,
        context: &TenantContext,
        artifact_id: &str,
    ) -> Result<Vec<ArtifactSummary>, AppError> {
        let artifact_id = artifact_id.to_owned();
        let rows = tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                sqlx::query_as::<
                    _,
                    (
                        String,
                        Option<String>,
                        Option<String>,
                        String,
                        Option<String>,
                        String,
                        String,
                        String,
                        String,
                        String,
                        i64,
                        String,
                        String,
                        i32,
                        String,
                    ),
                >(SQL_LIST_VERSIONS)
                .bind(&artifact_id)
                .fetch_all(&mut *conn)
                .await
                .map_err(tenant::unavailable)
            })
        })
        .await?;
        Ok(rows.into_iter().map(summary_from).collect())
    }

    async fn list_space(
        &self,
        context: &TenantContext,
        bot_id: Option<&str>,
        group_id: Option<&str>,
        limit: i64,
    ) -> Result<Vec<ArtifactSummary>, AppError> {
        let bot_id = bot_id.map(str::to_owned);
        let group_id = group_id.map(str::to_owned);
        let limit = limit.clamp(1, 100);
        let rows = tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                sqlx::query_as::<
                    _,
                    (
                        String,
                        Option<String>,
                        Option<String>,
                        String,
                        Option<String>,
                        String,
                        String,
                        String,
                        String,
                        String,
                        i64,
                        String,
                        String,
                        i32,
                        String,
                    ),
                >(SQL_LIST_SPACE)
                .bind(&bot_id)
                .bind(&group_id)
                .bind(limit)
                .fetch_all(&mut *conn)
                .await
                .map_err(tenant::unavailable)
            })
        })
        .await?;
        Ok(rows.into_iter().map(summary_from).collect())
    }

    async fn family_keys(
        &self,
        context: &TenantContext,
        artifact_id: &str,
    ) -> Result<Vec<String>, AppError> {
        let artifact_id = artifact_id.to_owned();
        tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                sqlx::query_scalar(
                    "SELECT DISTINCT storage_key FROM artifacts
                      WHERE root_artifact_id = (SELECT root_artifact_id FROM artifacts WHERE id = $1)",
                )
                .bind(&artifact_id)
                .fetch_all(&mut *conn)
                .await
                .map_err(tenant::unavailable)
            })
        })
        .await
    }

    async fn remove_family(
        &self,
        context: &TenantContext,
        artifact_id: &str,
    ) -> Result<u64, AppError> {
        let artifact_id = artifact_id.to_owned();
        let result = tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                sqlx::query(
                    "DELETE FROM artifacts
                      WHERE root_artifact_id = (SELECT root_artifact_id FROM artifacts WHERE id = $1)",
                )
                .bind(&artifact_id)
                .execute(&mut *conn)
                .await
                .map_err(tenant::unavailable)
            })
        })
        .await?;
        Ok(result.rows_affected())
    }

    async fn storage_key_refs(
        &self,
        context: &TenantContext,
        storage_key: &str,
    ) -> Result<i64, AppError> {
        let storage_key = storage_key.to_owned();
        tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                sqlx::query_scalar("SELECT count(*)::bigint FROM artifacts WHERE storage_key = $1")
                    .bind(&storage_key)
                    .fetch_one(&mut *conn)
                    .await
                    .map_err(tenant::unavailable)
            })
        })
        .await
    }
}
