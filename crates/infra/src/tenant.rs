//! The tenant context, and the transaction that carries it.
//!
//! Every query that reads or writes tenant data runs inside a transaction that has told the
//! database which tenant it acts in. The tenant is a session setting, `boma.space_id`, and the
//! row level security policies compare it with the `space_id` of every row (REQ-043).
//!
//! Three rules hold here, and they are the whole point of this module:
//!
//! * The setting is **transaction scoped**: `set_config(..., true)` reverts it when the
//!   transaction ends, so a pooled connection never carries a tenant to the next borrower.
//! * The setting is set in **one place**. A grep for `boma.space_id` outside this module finds
//!   nothing, which is the static half of ADR-002's check.
//! * A **system** context sets the space to the empty string and sets a reason from the closed
//!   enum `boma_domain::SystemReason`. Free text would let any caller claim any exception
//!   (defect D-09).

use std::{future::Future, pin::Pin};

use boma_app::AppError;
use boma_domain::{SpaceId, SystemReason, TenantContext};
use sqlx::{PgConnection, PgPool};

/// The session setting that carries the tenant.
///
/// The name is part of the contract: ADR-002's CI check greps for it, and the policies in
/// `migrations/0002_m1.sql` read it.
pub const SPACE_ID_SETTING: &str = "boma.space_id";

/// The session setting that carries the reason a system path crosses tenants.
///
/// The value comes from [`SystemReason`], never from free text.
pub const SYSTEM_REASON_SETTING: &str = "boma.system_reason";

/// The future a tenant closure returns.
///
/// The closure receives a connection borrowed from the transaction, so the future it returns
/// borrows that connection too; boxing it is what makes the higher-ranked closure signature
/// below expressible.
pub type TenantFuture<'c, T> = Pin<Box<dyn Future<Output = Result<T, AppError>> + Send + 'c>>;

/// Runs `body` in a transaction that acts inside the tenant of `context`.
///
/// A member context limits the transaction to one space. A system context lifts the limit and
/// records why, in the setting the policies read.
///
/// # Errors
///
/// Returns [`AppError::Unavailable`] when the database refuses the transaction or the closure
/// fails. The transaction is rolled back when the closure fails, so a half-written multi-row
/// change never survives.
pub async fn with_tenant<'a, T, F>(
    pool: &'a PgPool,
    context: &TenantContext,
    body: F,
) -> Result<T, AppError>
where
    F: for<'c> FnOnce(&'c mut PgConnection) -> TenantFuture<'c, T>,
{
    match context {
        TenantContext::Member { space_id, .. } => in_space(pool, space_id, body).await,
        TenantContext::System { reason, .. } => as_system(pool, *reason, body).await,
    }
}

/// Runs `body` in a transaction that acts inside `space_id`.
///
/// This is the tenant scope without a person: the policies only ever compare the space, so a
/// write that the product owns (creating a space and its first membership, for example) can
/// name the tenant it creates instead of the one it comes from.
///
/// # Errors
///
/// Returns [`AppError::Unavailable`] when the database refuses the transaction or the closure
/// fails.
pub async fn in_space<'a, T, F>(
    pool: &'a PgPool,
    space_id: &SpaceId,
    body: F,
) -> Result<T, AppError>
where
    F: for<'c> FnOnce(&'c mut PgConnection) -> TenantFuture<'c, T>,
{
    scope(pool, space_id.as_str(), "", body).await
}

/// Runs `body` in a transaction that may cross tenants, for a reason the product owns.
///
/// `reason` is a member of a closed enum: a new system path is a code change that does not
/// compile until it says why it crosses tenants.
///
/// # Errors
///
/// Returns [`AppError::Unavailable`] when the database refuses the transaction or the closure
/// fails.
pub async fn as_system<'a, T, F>(
    pool: &'a PgPool,
    reason: SystemReason,
    body: F,
) -> Result<T, AppError>
where
    F: for<'c> FnOnce(&'c mut PgConnection) -> TenantFuture<'c, T>,
{
    scope(pool, "", reason.as_str(), body).await
}

/// Opens the transaction, tells the database who it is, and runs the body inside it.
async fn scope<'a, T, F>(
    pool: &'a PgPool,
    space_id: &str,
    system_reason: &str,
    body: F,
) -> Result<T, AppError>
where
    F: for<'c> FnOnce(&'c mut PgConnection) -> TenantFuture<'c, T>,
{
    let mut transaction = pool.begin().await.map_err(unavailable)?;

    // `is_local = true`: the setting belongs to this transaction only, so a pooled connection
    // never carries a tenant to the next borrower.
    sqlx::query("SELECT set_config('boma.space_id', $1, true)")
        .bind(space_id)
        .execute(&mut *transaction)
        .await
        .map_err(unavailable)?;
    sqlx::query("SELECT set_config('boma.system_reason', $1, true)")
        .bind(system_reason)
        .execute(&mut *transaction)
        .await
        .map_err(unavailable)?;

    let outcome = body(&mut transaction).await;
    match outcome {
        Ok(value) => {
            transaction.commit().await.map_err(unavailable)?;
            Ok(value)
        }
        Err(error) => {
            // The transaction is rolled back: the body may have failed halfway through a write.
            let _ = transaction.rollback().await;
            Err(error)
        }
    }
}

/// Turns a driver error into the error the application layer understands.
///
/// The detail stays at debug level: a driver error can quote the connection target, and the
/// response must never carry it (REQ-054).
///
/// The error is taken by value because this is a `map_err` adapter: the driver hands it over and
/// the caller never looks at it again.
#[allow(clippy::needless_pass_by_value)]
pub(crate) fn unavailable(error: sqlx::Error) -> AppError {
    tracing::debug!(error = %error, "the database refused a query");
    AppError::Unavailable
}

/// Whether a driver error is a unique violation.
pub(crate) fn is_unique_violation(error: &sqlx::Error) -> bool {
    match error {
        sqlx::Error::Database(database) => database.is_unique_violation(),
        _ => false,
    }
}
