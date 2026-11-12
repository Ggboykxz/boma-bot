//! The Postgres adapter for computers.

use sqlx::PgPool;

use boma_app::{
    AppError,
    ports::{ComputerStore, NewComputer},
};
use boma_domain::{BotId, Computer, ComputerId, TenantContext};

use crate::tenant;

use super::computer_row::{COMPUTER_COLUMNS, ComputerRow, computer};

/// The computers of this deployment, in Postgres.
#[derive(Clone)]
pub struct PostgresComputerStore {
    pool: PgPool,
}

impl PostgresComputerStore {
    /// Builds the store over the pool.
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl ComputerStore for PostgresComputerStore {
    async fn find(
        &self,
        context: &TenantContext,
        bot_id: &BotId,
    ) -> Result<Option<Computer>, AppError> {
        let bot_key = bot_id.as_str().to_owned();
        let row = tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                sqlx::query_as::<_, ComputerRow>(sqlx::AssertSqlSafe(format!(
                    "SELECT {COMPUTER_COLUMNS} FROM computers WHERE bot_id = $1"
                )))
                .bind(bot_key.as_str())
                .fetch_optional(&mut *conn)
                .await
                .map_err(tenant::unavailable)
            })
        })
        .await?;
        row.map(computer).transpose()
    }

    async fn find_by_id(
        &self,
        context: &TenantContext,
        id: &ComputerId,
    ) -> Result<Option<Computer>, AppError> {
        let id_key = id.as_str().to_owned();
        let row = tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                sqlx::query_as::<_, ComputerRow>(sqlx::AssertSqlSafe(format!(
                    "SELECT {COMPUTER_COLUMNS} FROM computers WHERE id = $1"
                )))
                .bind(id_key.as_str())
                .fetch_optional(&mut *conn)
                .await
                .map_err(tenant::unavailable)
            })
        })
        .await?;
        row.map(computer).transpose()
    }

    async fn create(
        &self,
        context: &TenantContext,
        computer_row: NewComputer,
    ) -> Result<Computer, AppError> {
        let id = computer_row.id.as_str().to_owned();
        let space_key = computer_row.space_id.as_str().to_owned();
        let bot_key = computer_row.bot_id.as_str().to_owned();
        let row = tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                sqlx::query_as::<_, ComputerRow>(sqlx::AssertSqlSafe(format!(
                    "INSERT INTO computers (
                        id, space_id, bot_id, provider, status, screen_generation,
                        private, memory_limit_mb, cpu_limit, idle_minutes,
                        created_at, updated_at
                     )
                     VALUES ($1, $2, $3, $4, 'stopped', 0, $5, $6, $7, $8, $9::timestamptz, $9::timestamptz)
                     ON CONFLICT (id) DO NOTHING
                     RETURNING {COMPUTER_COLUMNS}"
                )))
                .bind(id.as_str())
                .bind(space_key.as_str())
                .bind(bot_key.as_str())
                .bind(computer_row.provider.as_str())
                .bind(computer_row.private)
                .bind(i32::try_from(computer_row.memory_limit_mb).unwrap_or(i32::MAX))
                .bind(computer_row.cpu_limit)
                .bind(i32::try_from(computer_row.idle_minutes).unwrap_or(i32::MAX))
                .bind(computer_row.created_at.as_str())
                .fetch_one(&mut *conn)
                .await
                .map_err(tenant::unavailable)
            })
        })
        .await?;
        computer(row)
    }

    async fn update(
        &self,
        context: &TenantContext,
        computer_in: &Computer,
    ) -> Result<(), AppError> {
        let id = computer_in.id.as_str().to_owned();
        let provider = computer_in.provider.clone();
        let external_id = computer_in.external_id.clone();
        let status = computer_in.status.as_str().to_owned();
        let generation = i32::try_from(computer_in.screen_generation).unwrap_or(i32::MAX);
        let home_id = computer_in.home_id.clone();
        let private = computer_in.private;
        let memory_limit_mb = i32::try_from(computer_in.memory_limit_mb).unwrap_or(i32::MAX);
        let cpu_limit = computer_in.cpu_limit;
        let idle_minutes = i32::try_from(computer_in.idle_minutes).unwrap_or(i32::MAX);
        let started_at = computer_in.started_at.clone();
        let last_used_at = computer_in.last_used_at.clone();
        let stopped_at = computer_in.stopped_at.clone();
        let updated = computer_in.updated_at.clone();
        let takeover_by = computer_in
            .takeover_by_user
            .as_ref()
            .map(boma_domain::UserId::as_str)
            .map(str::to_owned);
        let takeover_at = computer_in.takeover_at.clone();
        tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                sqlx::query(
                    "UPDATE computers
                     SET provider = $2,
                         external_id = $3,
                         status = $4,
                         screen_generation = $5,
                         home_id = $6,
                         private = $7,
                         memory_limit_mb = $8,
                         cpu_limit = $9,
                         idle_minutes = $10,
                         started_at = $11::timestamptz,
                         last_used_at = $12::timestamptz,
                         stopped_at = $13::timestamptz,
                         takeover_by = $15,
                         takeover_at = $16::timestamptz,
                         updated_at = $14::timestamptz
                     WHERE id = $1",
                )
                .bind(id.as_str())
                .bind(provider.as_str())
                .bind(external_id.as_deref())
                .bind(status.as_str())
                .bind(generation)
                .bind(home_id.as_deref())
                .bind(private)
                .bind(memory_limit_mb)
                .bind(cpu_limit)
                .bind(idle_minutes)
                .bind(started_at.as_deref())
                .bind(last_used_at.as_deref())
                .bind(stopped_at.as_deref())
                .bind(updated.as_str())
                .bind(takeover_by.as_deref())
                .bind(takeover_at.as_deref())
                .execute(&mut *conn)
                .await
                .map_err(tenant::unavailable)
            })
        })
        .await?;
        Ok(())
    }

    async fn running_idle(&self, as_of: &str) -> Result<Vec<Computer>, AppError> {
        let as_of_key = as_of.to_owned();
        tenant::as_system(
            &self.pool,
            boma_domain::SystemReason::ScheduledJob,
            |conn| {
                Box::pin(async move {
                    let rows = sqlx::query_as::<_, ComputerRow>(sqlx::AssertSqlSafe(format!(
                        "SELECT {COMPUTER_COLUMNS} FROM computers
                         WHERE status = 'running'
                           AND last_used_at IS NOT NULL
                           AND last_used_at <= $1::timestamptz - (idle_minutes || ' minutes')::interval"
                    )))
                    .bind(as_of_key.as_str())
                    .fetch_all(&mut *conn)
                    .await
                    .map_err(tenant::unavailable)?;
                    rows.into_iter().map(computer).collect()
                })
            },
        )
        .await
    }
}
