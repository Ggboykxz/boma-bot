//! Sessions: the sign-in cookies.
//!
//! A session is not tenant data: it belongs to a person. The row holds the hash of the token,
//! never the token itself.

use boma_app::{AppError, ports::SessionStore};
use boma_domain::Session;
use sqlx::PgPool;

use super::rows::{self, SessionRow};
use crate::tenant;

/// The Postgres adapter for [`SessionStore`].
#[derive(Debug, Clone)]
pub struct PostgresSessionStore {
    pool: PgPool,
}

impl PostgresSessionStore {
    /// Builds the store on a pool.
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl SessionStore for PostgresSessionStore {
    async fn create(&self, session: boma_app::ports::NewSession) -> Result<Session, AppError> {
        let row = sqlx::query_as::<_, SessionRow>(
            "INSERT INTO sessions (id, user_id, token_hash, expires_at, created_at)
             VALUES ($1, $2, $3, $4::timestamptz, $5::timestamptz)
             RETURNING id, user_id, token_hash, boma.rfc3339(expires_at) AS expires_at",
        )
        .bind(session.id.as_str())
        .bind(session.user_id.as_str())
        .bind(session.token_hash)
        .bind(session.expires_at)
        .bind(session.created_at)
        .fetch_one(&self.pool)
        .await
        .map_err(tenant::unavailable)?;

        rows::session(row)
    }

    async fn find_by_token_hash(&self, token_hash: &[u8]) -> Result<Option<Session>, AppError> {
        let row = sqlx::query_as::<_, SessionRow>(
            "SELECT id, user_id, token_hash, boma.rfc3339(expires_at) AS expires_at
             FROM sessions
             WHERE token_hash = $1",
        )
        .bind(token_hash)
        .fetch_optional(&self.pool)
        .await
        .map_err(tenant::unavailable)?;

        // The hash is compared once more, in constant time. The lookup is already exact, but the
        // comparison must never say how much of the hash was guessed right.
        let row = row.filter(|row| crate::crypto::constant_time_eq(&row.token_hash, token_hash));
        row.map(rows::session).transpose()
    }

    async fn delete(&self, id: &boma_domain::SessionId) -> Result<(), AppError> {
        sqlx::query("DELETE FROM sessions WHERE id = $1")
            .bind(id.as_str())
            .execute(&self.pool)
            .await
            .map_err(tenant::unavailable)?;
        Ok(())
    }
}
