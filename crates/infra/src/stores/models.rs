//! Model credentials and the person's model preference (REQ-052).

use boma_app::{
    AppError,
    ports::{ModelCredential, ModelStore, NewModelCredential},
};
use boma_domain::TenantContext;
use sqlx::PgPool;

use crate::{secret_box::SecretBox, tenant};

/// The Postgres adapter for [`ModelStore`].
#[derive(Debug, Clone)]
pub struct PostgresModelStore {
    pool: PgPool,
    secrets: SecretBox,
}

impl PostgresModelStore {
    /// Builds the store on a pool and the vault.
    #[must_use]
    pub const fn new(pool: PgPool, secrets: SecretBox) -> Self {
        Self { pool, secrets }
    }

    async fn space_key(
        secrets: &SecretBox,
        conn: &mut sqlx::PgConnection,
        space_id: &str,
    ) -> Result<[u8; 32], AppError> {
        let wrapped: Option<(Vec<u8>, Vec<u8>)> =
            sqlx::query_as("SELECT wrapped_key, nonce FROM space_keys WHERE space_id = $1")
                .bind(space_id)
                .fetch_optional(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
        if let Some((wrapped_key, nonce)) = wrapped {
            return secrets.unwrap_data_key(&boma_app::ports::SealedSecret {
                ciphertext: wrapped_key,
                nonce,
            });
        }
        let fresh = SecretBox::fresh_data_key();
        let sealed = secrets.wrap_data_key(&fresh)?;
        sqlx::query(
            "INSERT INTO space_keys (space_id, wrapped_key, nonce)
             VALUES ($1, $2, $3) ON CONFLICT (space_id) DO NOTHING",
        )
        .bind(space_id)
        .bind(&sealed.ciphertext)
        .bind(&sealed.nonce)
        .execute(&mut *conn)
        .await
        .map_err(tenant::unavailable)?;
        let survivor: Option<(Vec<u8>, Vec<u8>)> =
            sqlx::query_as("SELECT wrapped_key, nonce FROM space_keys WHERE space_id = $1")
                .bind(space_id)
                .fetch_optional(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
        let Some((wrapped_key, nonce)) = survivor else {
            return Err(AppError::Unavailable);
        };
        secrets.unwrap_data_key(&boma_app::ports::SealedSecret {
            ciphertext: wrapped_key,
            nonce,
        })
    }
}

#[async_trait::async_trait]
impl ModelStore for PostgresModelStore {
    async fn put_credential(
        &self,
        context: &TenantContext,
        credential: NewModelCredential,
    ) -> Result<(), AppError> {
        let space_id = match context {
            TenantContext::Member { space_id, .. } => space_id.as_str().to_owned(),
            TenantContext::System { .. } => return Err(AppError::NotFound),
        };
        let created_at = credential.created_at.clone();
        let provider = credential.provider.clone();
        let secrets = self.secrets.clone();
        let api_key = credential.api_key;
        tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                let dek = Self::space_key(&secrets, conn, &space_id).await?;
                let sealed = SecretBox::seal_with_data_key(&dek, &api_key)?;
                sqlx::query(
                    "INSERT INTO model_credentials (space_id, provider, status, ciphertext, nonce, created_at, updated_at)
                     VALUES ($1, $2, 'connected', $3, $4, $5::timestamptz, $5::timestamptz)
                     ON CONFLICT (space_id, provider) DO UPDATE
                       SET ciphertext = EXCLUDED.ciphertext,
                           nonce = EXCLUDED.nonce,
                           updated_at = now()",
                )
                .bind(&space_id)
                .bind(&provider)
                .bind(&sealed.ciphertext)
                .bind(&sealed.nonce)
                .bind(&created_at)
                .execute(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
                Ok(())
            })
        })
        .await
    }

    async fn forget_credential(
        &self,
        context: &TenantContext,
        provider: &str,
    ) -> Result<bool, AppError> {
        let provider = provider.to_owned();
        let removed = tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                sqlx::query("DELETE FROM model_credentials WHERE provider = $1")
                    .bind(&provider)
                    .execute(&mut *conn)
                    .await
                    .map_err(tenant::unavailable)
            })
        })
        .await?;
        Ok(removed.rows_affected() > 0)
    }

    async fn list_credentials(
        &self,
        context: &TenantContext,
    ) -> Result<Vec<ModelCredential>, AppError> {
        let rows = tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                sqlx::query_as::<_, (String, String, String)>(
                    "SELECT provider, status, boma.rfc3339(created_at)
                       FROM model_credentials
                      ORDER BY provider",
                )
                .fetch_all(&mut *conn)
                .await
                .map_err(tenant::unavailable)
            })
        })
        .await?;
        Ok(rows
            .into_iter()
            .map(|(provider, status, created_at)| ModelCredential {
                provider,
                status,
                created_at,
            })
            .collect())
    }

    async fn has_credential(
        &self,
        context: &TenantContext,
        provider: &str,
    ) -> Result<bool, AppError> {
        let provider = provider.to_owned();
        let found: Option<bool> = tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                sqlx::query_scalar(
                    "SELECT EXISTS(SELECT 1 FROM model_credentials WHERE provider = $1 AND status = 'connected')",
                )
                .bind(&provider)
                .fetch_one(&mut *conn)
                .await
                .map_err(tenant::unavailable)
            })
        })
        .await?;
        Ok(found.unwrap_or(false))
    }

    async fn preference(&self, context: &TenantContext) -> Result<Option<String>, AppError> {
        let user_id = match context {
            TenantContext::Member { user_id, .. } => user_id.as_str().to_owned(),
            TenantContext::System { .. } => return Ok(None),
        };
        let model_key: Option<String> = tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                sqlx::query_scalar(
                    "SELECT model_key FROM user_model_preferences WHERE user_id = $1",
                )
                .bind(&user_id)
                .fetch_optional(&mut *conn)
                .await
                .map_err(tenant::unavailable)
            })
        })
        .await?;
        Ok(model_key)
    }

    async fn set_preference(
        &self,
        context: &TenantContext,
        model_key: Option<&str>,
    ) -> Result<(), AppError> {
        let user_id = match context {
            TenantContext::Member { user_id, .. } => user_id.as_str().to_owned(),
            TenantContext::System { .. } => return Err(AppError::NotFound),
        };
        let model_key = model_key.map(str::to_owned);
        tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                match &model_key {
                    Some(key) => {
                        sqlx::query(
                            "INSERT INTO user_model_preferences (user_id, model_key, updated_at)
                             VALUES ($1, $2, now())
                             ON CONFLICT (user_id) DO UPDATE
                               SET model_key = EXCLUDED.model_key, updated_at = now()",
                        )
                        .bind(&user_id)
                        .bind(key)
                        .execute(&mut *conn)
                        .await
                        .map_err(tenant::unavailable)?;
                    }
                    None => {
                        sqlx::query("DELETE FROM user_model_preferences WHERE user_id = $1")
                            .bind(&user_id)
                            .execute(&mut *conn)
                            .await
                            .map_err(tenant::unavailable)?;
                    }
                }
                Ok(())
            })
        })
        .await
    }
}
