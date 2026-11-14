//! Pages and their immutable versions (REQ-037). Only token hashes are stored; the token itself
//! is handed out once.

use boma_app::{
    AppError,
    ports::{NewPageVersion, PageStore, PageSummary, PageVersion},
};
use boma_domain::{SystemReason, TenantContext};
use sqlx::PgPool;

use crate::tenant;

/// The Postgres adapter for the page store.
#[derive(Debug, Clone)]
pub struct PostgresPageStore {
    pool: PgPool,
}

impl PostgresPageStore {
    /// Builds the store on a pool.
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

/// The tuple a page summary row reads.
type PageSummaryRow = (
    String,
    Option<String>,
    String,
    String,
    String,
    Option<String>,
    bool,
    Option<String>,
    String,
    String,
);

fn map_page_row(row: PageSummaryRow) -> PageSummary {
    let (
        id,
        bot_id,
        created_by,
        name,
        visibility,
        current_version_id,
        shared,
        client_nonce,
        created_at,
        updated_at,
    ) = row;
    PageSummary {
        id,
        bot_id,
        created_by,
        name,
        visibility,
        current_version_id,
        shared,
        client_nonce,
        created_at,
        updated_at,
    }
}

#[allow(clippy::too_many_arguments)]
fn version_from(row: (String, String, i32, serde_json::Value, String)) -> PageVersion {
    let (id, page_id, version, source, compiled_at) = row;
    PageVersion {
        id,
        page_id,
        version,
        source,
        compiled_at,
    }
}

#[async_trait::async_trait]
impl PageStore for PostgresPageStore {
    async fn find_by_nonce(
        &self,
        context: &TenantContext,
        client_nonce: &str,
    ) -> Result<Option<PageSummary>, AppError> {
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
                        String,
                        String,
                        String,
                        Option<String>,
                        bool,
                        Option<String>,
                        String,
                        String,
                    ),
                >(concat!(
                    "SELECT ",
                    "id, bot_id, created_by, name, visibility, current_version_id,",
                    " (token_hash IS NOT NULL) AS shared, client_nonce,",
                    " boma.rfc3339(created_at), boma.rfc3339(updated_at)",
                    " FROM pages WHERE created_by = $1 AND client_nonce = $2",
                ))
                .bind(&user_id)
                .bind(&client_nonce)
                .fetch_optional(&mut *conn)
                .await
                .map_err(tenant::unavailable)
            })
        })
        .await?;
        Ok(row.map(map_page_row))
    }

    async fn put_version(
        &self,
        context: &TenantContext,
        version: NewPageVersion,
    ) -> Result<PageSummary, AppError> {
        let (space_id, created_by) = match context {
            TenantContext::Member { space_id, user_id } => {
                (space_id.as_str().to_owned(), user_id.as_str().to_owned())
            }
            TenantContext::System { .. } => return Err(AppError::NotFound),
        };
        tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                let page_id = version.page_id.clone().unwrap_or_else(|| version.new_id.clone());
                if version.page_id.is_some() {
                    let exists: Option<String> = sqlx::query_scalar(
                        "SELECT id FROM pages WHERE id = $1",
                    )
                    .bind(&page_id)
                    .fetch_optional(&mut *conn)
                    .await
                    .map_err(tenant::unavailable)?;
                    if exists.is_none() {
                        return Err(AppError::NotFound);
                    }
                }
                let next: i32 = sqlx::query_scalar(
                    "SELECT coalesce(max(version), 0) + 1 FROM page_versions WHERE page_id = $1",
                )
                .bind(&page_id)
                .fetch_one(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
                let name = version
                    .name
                    .clone()
                    .unwrap_or_else(|| "Page".to_owned());
                if version.page_id.is_none() {
                    sqlx::query(
                        "INSERT INTO pages (id, space_id, bot_id, created_by, name, visibility, client_nonce, created_at, updated_at)
                         VALUES ($1, $2, $3, $4, $5, 'private', $6, $7::timestamptz, $7::timestamptz)",
                    )
                    .bind(&page_id)
                    .bind(&space_id)
                    .bind(&version.bot_id)
                    .bind(&created_by)
                    .bind(&name)
                    .bind(&version.client_nonce)
                    .bind(&version.created_at)
                    .execute(&mut *conn)
                    .await
                    .map_err(tenant::unavailable)?;
                } else {
                    sqlx::query(
                        "UPDATE pages SET updated_at = $2::timestamptz WHERE id = $1",
                    )
                    .bind(&page_id)
                    .bind(&version.created_at)
                    .execute(&mut *conn)
                    .await
                    .map_err(tenant::unavailable)?;
                }
                sqlx::query(
                    "INSERT INTO page_versions (id, page_id, version, source, compiled_at)
                     VALUES ($1, $2, $3, $4::jsonb, $5::timestamptz)",
                )
                .bind(&version.new_version_id)
                .bind(&page_id)
                .bind(next)
                .bind(&version.source)
                .bind(&version.created_at)
                .execute(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
                sqlx::query(
                    "UPDATE pages SET current_version_id = $1, updated_at = now() WHERE id = $2",
                )
                .bind(&version.new_version_id)
                .bind(&page_id)
                .execute(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
                let row = sqlx::query_as::<_, (
                    String,
                    Option<String>,
                    String,
                    String,
                    String,
                    Option<String>,
                    bool,
                    Option<String>,
                    String,
                    String,
                )>(concat!("SELECT ", "id, bot_id, created_by, name, visibility, current_version_id,", " (token_hash IS NOT NULL) AS shared, client_nonce,", " boma.rfc3339(created_at), boma.rfc3339(updated_at)", " FROM pages WHERE id = $1"))
                .bind(&page_id)
                .fetch_one(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
                Ok(map_page_row(row))
            })
        })
        .await
    }

    async fn get(
        &self,
        context: &TenantContext,
        page_id: &str,
        version: Option<i32>,
    ) -> Result<Option<(PageSummary, PageVersion)>, AppError> {
        let page_id = page_id.to_owned();
        tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                let page_row = sqlx::query_as::<
                    _,
                    (
                        String,
                        Option<String>,
                        String,
                        String,
                        String,
                        Option<String>,
                        bool,
                        Option<String>,
                        String,
                        String,
                    ),
                >(concat!(
                    "SELECT ",
                    "id, bot_id, created_by, name, visibility, current_version_id,",
                    " (token_hash IS NOT NULL) AS shared, client_nonce,",
                    " boma.rfc3339(created_at), boma.rfc3339(updated_at)",
                    " FROM pages WHERE id = $1"
                ))
                .bind(&page_id)
                .fetch_optional(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
                let Some(page_row) = page_row else {
                    return Ok(None);
                };
                let version = match version {
                    Some(version) => version,
                    None => sqlx::query_scalar(
                        "SELECT max(version) FROM page_versions WHERE page_id = $1",
                    )
                    .bind(&page_id)
                    .fetch_one(&mut *conn)
                    .await
                    .map_err(tenant::unavailable)?,
                };
                let version_row =
                    sqlx::query_as::<_, (String, String, i32, serde_json::Value, String)>(
                        "SELECT id, page_id, version, source, boma.rfc3339(compiled_at)
                       FROM page_versions
                      WHERE page_id = $1 AND version = $2",
                    )
                    .bind(&page_id)
                    .bind(version)
                    .fetch_optional(&mut *conn)
                    .await
                    .map_err(tenant::unavailable)?;
                Ok(version_row
                    .map(|version_row| (map_page_row(page_row), version_from(version_row))))
            })
        })
        .await
    }

    async fn share(
        &self,
        context: &TenantContext,
        page_id: &str,
        token_hash: &[u8],
    ) -> Result<bool, AppError> {
        let page_id = page_id.to_owned();
        let token_hash = token_hash.to_owned();
        let result = tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                sqlx::query(
                    "UPDATE pages SET visibility = 'link', token_hash = $2, updated_at = now()
                      WHERE id = $1",
                )
                .bind(&page_id)
                .bind(&token_hash)
                .execute(&mut *conn)
                .await
                .map_err(tenant::unavailable)
            })
        })
        .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn revoke_share(&self, context: &TenantContext, page_id: &str) -> Result<bool, AppError> {
        let page_id = page_id.to_owned();
        let result = tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                sqlx::query(
                    "UPDATE pages SET visibility = 'private', token_hash = NULL, updated_at = now()
                      WHERE id = $1",
                )
                .bind(&page_id)
                .execute(&mut *conn)
                .await
                .map_err(tenant::unavailable)
            })
        })
        .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn shared_by_token_hash(
        &self,
        token_hash: &[u8],
    ) -> Result<Option<PageVersion>, AppError> {
        let token_hash = token_hash.to_owned();
        tenant::as_system(&self.pool, SystemReason::SharedPageRead, |conn| {
            Box::pin(async move {
                let row = sqlx::query_as::<_, (String, String, i32, serde_json::Value, String)>(
                    "SELECT v.id, v.page_id, v.version, v.source, boma.rfc3339(v.compiled_at)
                       FROM page_versions v
                       JOIN pages p ON p.current_version_id = v.id
                      WHERE p.visibility = 'link' AND p.token_hash = $1",
                )
                .bind(&token_hash)
                .fetch_optional(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
                Ok(row.map(version_from))
            })
        })
        .await
    }
}
