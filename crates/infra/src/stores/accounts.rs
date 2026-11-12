//! Accounts: the person, their organization, their first space and their membership in it.
//!
//! The four rows belong together: a person with no space cannot do anything, and a space with no
//! member is invisible (REQ-042). They are written in one transaction, so an account is never
//! half-created.

use boma_app::{
    AppError,
    ports::{Account, AccountStore, NewAccount},
};
use boma_domain::{Space, User};
use sqlx::PgPool;

use crate::tenant;

/// The Postgres adapter for [`AccountStore`].
#[derive(Debug, Clone)]
pub struct PostgresAccountStore {
    pool: PgPool,
}

impl PostgresAccountStore {
    /// Builds the store on a pool.
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl AccountStore for PostgresAccountStore {
    async fn create_account(&self, account: NewAccount) -> Result<Account, AppError> {
        let space_id = account.space.id.clone();

        // The transaction acts inside the space it creates: that is the only tenant the new rows
        // can belong to.
        tenant::in_space(&self.pool, &space_id, |conn| {
            Box::pin(async move {
                let user_row = sqlx::query(
                    "INSERT INTO users (id, email, name, email_verified, locale, password_hash, created_at)
                     VALUES ($1, $2, $3, false, $4, $5, $6::timestamptz)",
                )
                .bind(account.user.id.as_str())
                .bind(account.user.email.as_str())
                .bind(&account.user.name)
                .bind(&account.user.locale)
                .bind(&account.user.password_hash)
                .bind(&account.user.created_at)
                .execute(&mut *conn)
                .await;

                match user_row {
                    Ok(_) => {}
                    // The address is taken: the caller is told so, and nothing else is written.
                    Err(error) if tenant::is_unique_violation(&error) => {
                        return Err(AppError::Conflict);
                    }
                    Err(error) => return Err(tenant::unavailable(error)),
                }

                sqlx::query(
                    "INSERT INTO organizations (id, name, created_at)
                     VALUES ($1, $2, $3::timestamptz)",
                )
                .bind(account.organization.id.as_str())
                .bind(&account.organization.name)
                .bind(&account.organization.created_at)
                .execute(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;

                sqlx::query(
                    "INSERT INTO organization_members (organization_id, user_id, role, created_at)
                     VALUES ($1, $2, 'owner', $3::timestamptz)",
                )
                .bind(account.organization.id.as_str())
                .bind(account.user.id.as_str())
                .bind(&account.user.created_at)
                .execute(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;

                sqlx::query(
                    "INSERT INTO spaces (id, organization_id, name, is_default, created_at)
                     VALUES ($1, $2, $3, $4, $5::timestamptz)",
                )
                .bind(account.space.id.as_str())
                .bind(account.space.organization_id.as_str())
                .bind(&account.space.name)
                .bind(account.space.is_default)
                .bind(&account.space.created_at)
                .execute(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;

                sqlx::query(
                    "INSERT INTO space_members (space_id, user_id, role, created_at)
                     VALUES ($1, $2, 'owner', $3::timestamptz)",
                )
                .bind(account.space.id.as_str())
                .bind(account.user.id.as_str())
                .bind(&account.space.created_at)
                .execute(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;

                Ok(Account {
                    user: User {
                        id: account.user.id,
                        email: account.user.email,
                        name: account.user.name,
                        email_verified: false,
                        locale: account.user.locale,
                        deactivated_at: None,
                    },
                    space: Space {
                        id: account.space.id,
                        organization_id: account.space.organization_id,
                        name: account.space.name,
                        is_default: account.space.is_default,
                    },
                })
            })
        })
        .await
    }
}
