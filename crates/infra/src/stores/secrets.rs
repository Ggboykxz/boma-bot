//! Bot secrets: write-only, encrypted at rest, never in a response body (REQ-046).
//!
//! The tenant guard is the row level security on `space_keys` and `bot_secrets` (0025). The
//! master key never leaves this module: every call unwraps the space's data-encryption key,
//! sealed under the master key at row-creation time. The plaintext of a secret crosses the
//! process only on the way in (to be sealed) and on the way out (to the shell-out executor);
//! there is no other read path. List answers names and timestamps only.
//!
//! Master-key rotation is offline: [`rekey_space_keys`] unwraps every `space_keys` row under
//! the old master key and re-wraps it under the new one. Secret ciphertexts are not touched.

use boma_app::{
    AppError,
    ports::{NewBotSecret, SecretSummary},
};
use boma_domain::{BotId, TenantContext};
use secrecy::SecretString;
use sqlx::PgPool;

use crate::{secret_box::SecretBox, tenant};

/// The Postgres adapter for the secrets store.
#[derive(Debug, Clone)]
pub struct PostgresSecretStore {
    pool: PgPool,
    secrets: SecretBox,
}

impl PostgresSecretStore {
    /// Builds the store on a pool and the vault.
    #[must_use]
    pub fn new(pool: PgPool, secrets: SecretBox) -> Self {
        Self { pool, secrets }
    }
}

#[async_trait::async_trait]
impl boma_app::ports::SecretStore for PostgresSecretStore {
    async fn put(&self, context: &TenantContext, secret: NewBotSecret) -> Result<(), AppError> {
        let bot_id = secret.bot_id.as_str().to_owned();
        let name = secret.name.clone();
        let created_at = secret.created_at.clone();
        let id = secret.id.clone();
        let value = secret.value;
        let secrets = self.secrets.clone();
        tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                let space_id: Option<String> = sqlx::query_scalar(
                    "SELECT space_id FROM bots WHERE id = $1",
                )
                .bind(&bot_id)
                .fetch_optional(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
                let Some(space_id) = space_id else {
                    return Err(AppError::NotFound);
                };
                // The space's DEK: read the wrapped row, or mint a fresh one exactly once.
                // The insert is idempotent (ON CONFLICT DO NOTHING) so concurrent puts can
                // race harmlessly; the select afterwards always finds the surviving row.
                let wrapped: Option<(Vec<u8>, Vec<u8>)> = sqlx::query_as(
                    "SELECT wrapped_key, nonce FROM space_keys WHERE space_id = $1",
                )
                .bind(&space_id)
                .fetch_optional(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
                let space_key = if let Some((wrapped_key, nonce)) = wrapped {
                    secrets.unwrap_data_key(&boma_app::ports::SealedSecret {
                        ciphertext: wrapped_key,
                        nonce,
                    })?
                } else {
                        let dek = SecretBox::fresh_data_key();
                        let sealed_dek = secrets.wrap_data_key(&dek)?;
                        sqlx::query(
                            "INSERT INTO space_keys (space_id, wrapped_key, nonce)
                             VALUES ($1, $2, $3) ON CONFLICT (space_id) DO NOTHING",
                        )
                        .bind(&space_id)
                        .bind(&sealed_dek.ciphertext)
                        .bind(&sealed_dek.nonce)
                        .execute(&mut *conn)
                        .await
                        .map_err(tenant::unavailable)?;
                        let survivor: (Vec<u8>, Vec<u8>) = sqlx::query_as(
                            "SELECT wrapped_key, nonce FROM space_keys WHERE space_id = $1",
                        )
                        .bind(&space_id)
                        .fetch_one(&mut *conn)
                        .await
                        .map_err(tenant::unavailable)?;
                    secrets.unwrap_data_key(&boma_app::ports::SealedSecret {
                        ciphertext: survivor.0,
                        nonce: survivor.1,
                    })?
                };
                let sealed = SecretBox::seal_with_data_key(&space_key, &value)?;
                sqlx::query(
                    "INSERT INTO bot_secrets (id, space_id, bot_id, name, ciphertext, nonce, created_at)
                     VALUES ($1, $2, $3, $4, $5, $6, $7::timestamptz)
                     ON CONFLICT (space_id, bot_id, name) DO UPDATE
                         SET ciphertext = EXCLUDED.ciphertext,
                             nonce = EXCLUDED.nonce,
                             created_at = EXCLUDED.created_at",
                )
                .bind(id.as_str())
                .bind(space_id.as_str())
                .bind(bot_id.as_str())
                .bind(name.as_str())
                .bind(&sealed.ciphertext)
                .bind(&sealed.nonce)
                .bind(created_at.as_str())
                .execute(&mut *conn)
                .await
                .map_err(tenant::unavailable)
            })
        })
        .await?;
        Ok(())
    }

    async fn list(
        &self,
        context: &TenantContext,
        bot_id: &BotId,
    ) -> Result<Vec<SecretSummary>, AppError> {
        let bot_id = bot_id.as_str().to_owned();
        let rows = tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                sqlx::query_as::<_, (String, String)>(
                    "SELECT name, boma.rfc3339(created_at) AS created_at
                       FROM bot_secrets
                      WHERE space_id = (SELECT space_id FROM bots WHERE id = $1)
                        AND bot_id = $1
                      ORDER BY created_at DESC, name",
                )
                .bind(&bot_id)
                .fetch_all(&mut *conn)
                .await
                .map_err(tenant::unavailable)
            })
        })
        .await?;
        Ok(rows
            .into_iter()
            .map(|(name, created_at)| SecretSummary { name, created_at })
            .collect())
    }

    async fn delete(
        &self,
        context: &TenantContext,
        bot_id: &BotId,
        name: &str,
    ) -> Result<bool, AppError> {
        let bot_id = bot_id.as_str().to_owned();
        let name = name.to_owned();
        let deleted = tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                sqlx::query(
                    "DELETE FROM bot_secrets
                      WHERE bot_id = $1
                        AND name = $2
                        AND space_id = (SELECT space_id FROM bots WHERE id = $1)",
                )
                .bind(bot_id.as_str())
                .bind(name.as_str())
                .execute(&mut *conn)
                .await
                .map_err(tenant::unavailable)
            })
        })
        .await?;
        Ok(deleted.rows_affected() > 0)
    }

    async fn reveal(
        &self,
        context: &TenantContext,
        bot_id: &BotId,
        name: &str,
    ) -> Result<Option<SecretString>, AppError> {
        let bot_id = bot_id.as_str().to_owned();
        let name = name.to_owned();
        let secrets = self.secrets.clone();
        let row = tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                sqlx::query_as::<_, (Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>)>(
                    "SELECT sk.wrapped_key, sk.nonce, s.ciphertext, s.nonce
                       FROM space_keys sk
                       JOIN bot_secrets s ON s.space_id = sk.space_id
                      WHERE s.bot_id = $1 AND s.name = $2
                        AND s.space_id = (SELECT space_id FROM bots WHERE id = $1)",
                )
                .bind(bot_id.as_str())
                .bind(name.as_str())
                .fetch_optional(&mut *conn)
                .await
                .map_err(tenant::unavailable)
            })
        })
        .await?;
        let Some((wrapped_key, dek_nonce, ciphertext, nonce)) = row else {
            return Ok(None);
        };
        let space_key = secrets.unwrap_data_key(&boma_app::ports::SealedSecret {
            ciphertext: wrapped_key,
            nonce: dek_nonce,
        })?;
        let value = SecretBox::open_with_data_key(
            &space_key,
            &boma_app::ports::SealedSecret { ciphertext, nonce },
        )?;
        Ok(Some(value))
    }
}

/// Re-wraps every data-encryption key under a new master key (REQ-046, spec DEC-10).
///
/// Runs under the system tenant; secret ciphertexts are never re-encrypted, only the wrapping
/// rows change. Returns the number of keys re-wrapped.
///
/// # Errors
///
/// Returns an error when the old key does not unwrap a row (wrong `--old-key`) or the database
/// cannot answer.
pub async fn rekey_space_keys(
    pool: &PgPool,
    old: &SecretBox,
    new: &SecretBox,
) -> Result<u64, AppError> {
    let old = old.clone();
    let new = new.clone();
    tenant::with_tenant(
        pool,
        &TenantContext::System {
            reason: boma_domain::SystemReason::DataMigration,
            user_id: None,
        },
        |conn| {
            Box::pin(async move {
                let rows: Vec<(String, Vec<u8>, Vec<u8>)> = sqlx::query_as(
                    "SELECT space_id, wrapped_key, nonce FROM space_keys FOR UPDATE",
                )
                .fetch_all(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
                let mut updated = 0_u64;
                for (space_id, wrapped_key, nonce) in rows {
                    let dek = old.unwrap_data_key(&boma_app::ports::SealedSecret {
                        ciphertext: wrapped_key,
                        nonce,
                    })?;
                    let rewrapped = new.wrap_data_key(&dek)?;
                    sqlx::query(
                        "UPDATE space_keys
                        SET wrapped_key = $2, nonce = $3, rotated_at = now()
                      WHERE space_id = $1",
                    )
                    .bind(&space_id)
                    .bind(&rewrapped.ciphertext)
                    .bind(&rewrapped.nonce)
                    .execute(&mut *conn)
                    .await
                    .map_err(tenant::unavailable)?;
                    updated += 1;
                }
                Ok(updated)
            })
        },
    )
    .await
}
