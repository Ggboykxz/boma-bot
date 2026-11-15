//! The password recovery tokens (REQ-040).
//!
//! System-scoped like every credential store: the tenant of a recovery request is the
//! deployment, not a space, and the request is the proof of reachability, not a tenant at all.

use boma_app::{AppError, ports};
use sqlx::PgPool;

use crate::tenant;

/// The Postgres adapter for [`ports::PasswordResetStore`].
#[derive(Debug, Clone)]
pub struct PostgresPasswordResetStore {
    pool: PgPool,
}

impl PostgresPasswordResetStore {
    /// Builds the store on a pool.
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl ports::PasswordResetStore for PostgresPasswordResetStore {
    async fn create(&self, token: ports::NewPasswordResetToken) -> Result<(), AppError> {
        let token = token.clone();
        tenant::as_system(
            &self.pool,
            boma_domain::SystemReason::Authentication,
            |conn| {
                Box::pin(async move {
                    sqlx::query(
                        "INSERT INTO password_reset_tokens (id, user_id, token_hash, expires_at, created_at)
                         VALUES ($1, $2, $3, $4::timestamptz, $5::timestamptz)",
                    )
                    .bind(&token.id)
                    .bind(token.user_id.as_str())
                    .bind(&token.token_hash)
                    .bind(&token.expires_at)
                    .bind(&token.created_at)
                    .execute(&mut *conn)
                    .await
                    .map_err(tenant::unavailable)?;
                    Ok(())
                })
            },
        )
        .await
    }

    async fn find(&self, token_hash: &[u8]) -> Result<Option<ports::PasswordResetToken>, AppError> {
        let hash = token_hash.to_vec();
        tenant::as_system(
            &self.pool,
            boma_domain::SystemReason::Authentication,
            |conn| {
                Box::pin(async move {
                    sqlx::query_as::<_, (String, String, Vec<u8>, String, Option<String>)>(
                        "SELECT id, user_id, token_hash, boma.rfc3339(expires_at), boma.rfc3339(used_at)
                           FROM password_reset_tokens
                          WHERE token_hash = $1 AND used_at IS NULL",
                    )
                    .bind(&hash)
                    .fetch_optional(&mut *conn)
                    .await
                    .map(|row| {
                        row.map(|(id, user_id, token_hash, expires_at, used_at)| {
                            let user_id = boma_domain::UserId::new(user_id)
                                .map_err(|_| AppError::Unavailable)?;
                            Ok(ports::PasswordResetToken {
                                id,
                                user_id,
                                token_hash,
                                expires_at,
                                used_at,
                            })
                        })
                        .transpose()
                    })
                    .map_err(tenant::unavailable)?
                })
            },
        )
        .await
    }

    async fn mark_used(&self, id: &str, now: &str) -> Result<(), AppError> {
        let id = id.to_owned();
        let now = now.to_owned();
        tenant::as_system(
            &self.pool,
            boma_domain::SystemReason::Authentication,
            |conn| {
                Box::pin(async move {
                    sqlx::query(
                        "UPDATE password_reset_tokens SET used_at = $2::timestamptz WHERE id = $1",
                    )
                    .bind(&id)
                    .bind(&now)
                    .execute(&mut *conn)
                    .await
                    .map_err(tenant::unavailable)?;
                    Ok(())
                })
            },
        )
        .await
    }
}
