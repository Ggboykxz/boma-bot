//! People, read by address and by id.
//!
//! A person is not tenant data: the row belongs to the person, not to a space, so no row level
//! security applies and the queries run without a tenant context.

use boma_app::{AppError, ports::UserStore};
use boma_domain::{Email, User, UserId};
use sqlx::PgPool;

use super::rows::{self, UserRow};
use crate::tenant;

/// The Postgres adapter for [`UserStore`].
#[derive(Debug, Clone)]
pub struct PostgresUserStore {
    pool: PgPool,
}

impl PostgresUserStore {
    /// Builds the store on a pool.
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl UserStore for PostgresUserStore {
    async fn find_by_email(&self, email: &Email) -> Result<Option<(User, String)>, AppError> {
        let row = sqlx::query_as::<_, UserRow>(
            "SELECT id, email, name, email_verified, locale, password_hash,
                    boma.rfc3339(deactivated_at) AS deactivated_at
             FROM users
             WHERE email = $1",
        )
        .bind(email.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(tenant::unavailable)?;

        row.map(|row| {
            // The hash travels with the person so that a sign-in never has to fetch it separately.
            let password_hash = row.password_hash.clone();
            rows::user(row).map(|user| (user, password_hash))
        })
        .transpose()
    }

    async fn find(&self, id: &UserId) -> Result<Option<User>, AppError> {
        let row = sqlx::query_as::<_, UserRow>(
            "SELECT id, email, name, email_verified, locale, password_hash,
                    boma.rfc3339(deactivated_at) AS deactivated_at
             FROM users
             WHERE id = $1",
        )
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(tenant::unavailable)?;

        row.map(rows::user).transpose()
    }
}
