//! Routines: cron definitions that wake a bot (REQ-023, REQ-024).
//!
//! One row per routine. `next_run_at` is the single wake the dispatcher claims; the store hands
//! the dispatcher what is due and nothing else.

use boma_app::{
    AppError,
    ports::{NewRoutine, Routine, RoutineOutcome, RoutineStore},
};
use boma_domain::{RoutineId, RunId, TenantContext};
use sqlx::PgPool;

use super::rows::{RoutineRow, routine as map_routine};
use crate::tenant;

/// The Postgres adapter for [`RoutineStore`].
#[derive(Debug, Clone)]
pub struct PostgresRoutineStore {
    pool: PgPool,
}

impl PostgresRoutineStore {
    /// Builds the store on a pool.
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl RoutineStore for PostgresRoutineStore {
    async fn create(
        &self,
        context: &TenantContext,
        routine: NewRoutine,
    ) -> Result<Routine, AppError> {
        let space_id = routine.space_id.clone();
        tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                let row = sqlx::query_as::<_, RoutineRow>(
                    "INSERT INTO routines (id, space_id, bot_id, thread_id, user_id, name,
                                            prompt, crons, timezone, active, notify, next_run_at,
                                            account_failures, created_at, updated_at)
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11,
                             $12::timestamptz, 0, $13::timestamptz, $13::timestamptz)
                     RETURNING id, space_id, bot_id, thread_id, user_id, name, prompt, crons,
                               timezone, active, notify,
                               boma.rfc3339(last_run_at) AS last_run_at,
                               boma.rfc3339(next_run_at) AS next_run_at,
                               account_failures,
                               boma.rfc3339(created_at) AS created_at,
                               boma.rfc3339(updated_at) AS updated_at",
                )
                .bind(routine.id.as_str())
                .bind(space_id.as_str())
                .bind(routine.bot_id.as_str())
                .bind(routine.thread_id.as_str())
                .bind(routine.user_id.as_str())
                .bind(routine.name.as_str())
                .bind(routine.prompt.as_str())
                .bind(&routine.crons)
                .bind(routine.timezone.as_str())
                .bind(routine.active)
                .bind(routine.notify)
                .bind(routine.next_run_at.as_deref())
                .bind(routine.created_at.as_str())
                .fetch_one(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
                map_routine(row)
            })
        })
        .await
    }

    async fn remove(&self, context: &TenantContext, id: &RoutineId) -> Result<bool, AppError> {
        let space_id = context.space_id().cloned().ok_or(AppError::NotFound)?;
        let id_key = id.as_str().to_owned();
        let space_key = space_id.as_str().to_owned();
        tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                let deleted = sqlx::query("DELETE FROM routines WHERE id = $1 AND space_id = $2")
                    .bind(id_key.as_str())
                    .bind(space_key.as_str())
                    .execute(&mut *conn)
                    .await
                    .map_err(tenant::unavailable)?;
                Ok(deleted.rows_affected() > 0)
            })
        })
        .await
    }

    async fn find(
        &self,
        context: &TenantContext,
        id: &RoutineId,
    ) -> Result<Option<Routine>, AppError> {
        let space_id = context.space_id().cloned().ok_or(AppError::NotFound)?;
        let id_key = id.as_str().to_owned();
        let space_key = space_id.as_str().to_owned();
        tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                let row = sqlx::query_as::<_, RoutineRow>(
                    "SELECT id, space_id, bot_id, thread_id, user_id, name, prompt, crons, timezone,
                            active, notify,
                            boma.rfc3339(last_run_at) AS last_run_at,
                            boma.rfc3339(next_run_at) AS next_run_at,
                            account_failures,
                            boma.rfc3339(created_at) AS created_at,
                            boma.rfc3339(updated_at) AS updated_at
                     FROM routines
                     WHERE id = $1 AND space_id = $2"
                )
                .bind(id_key.as_str())
                .bind(space_key.as_str())
                .fetch_optional(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
                row.map(map_routine).transpose()
            })
        })
        .await
    }

    async fn list(
        &self,
        context: &TenantContext,
        bot_id: &boma_domain::BotId,
    ) -> Result<Vec<Routine>, AppError> {
        let space_id = context.space_id().cloned().ok_or(AppError::NotFound)?;
        let bot_key = bot_id.as_str().to_owned();
        let space_key = space_id.as_str().to_owned();
        tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                let rows = sqlx::query_as::<_, RoutineRow>(
                    "SELECT id, space_id, bot_id, thread_id, user_id, name, prompt, crons, timezone,
                            active, notify,
                            boma.rfc3339(last_run_at) AS last_run_at,
                            boma.rfc3339(next_run_at) AS next_run_at,
                            account_failures,
                            boma.rfc3339(created_at) AS created_at,
                            boma.rfc3339(updated_at) AS updated_at
                     FROM routines
                     WHERE bot_id = $1 AND space_id = $2
                     ORDER BY created_at ASC, id ASC"
                )
                .bind(bot_key.as_str())
                .bind(space_key.as_str())
                .fetch_all(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
                rows.into_iter().map(map_routine).collect()
            })
        })
        .await
    }

    async fn due(&self, as_of: &str) -> Result<Vec<Routine>, AppError> {
        let as_of_key = as_of.to_owned();
        tenant::as_system(
            &self.pool,
            boma_domain::SystemReason::ScheduledJob,
            |conn| {
                Box::pin(async move {
                    let rows = sqlx::query_as::<_, RoutineRow>(
                    "SELECT id, space_id, bot_id, thread_id, user_id, name, prompt, crons, timezone,
                            active, notify,
                            boma.rfc3339(last_run_at) AS last_run_at,
                            boma.rfc3339(next_run_at) AS next_run_at,
                            account_failures,
                            boma.rfc3339(created_at) AS created_at,
                            boma.rfc3339(updated_at) AS updated_at
                     FROM routines
                     WHERE active AND next_run_at IS NOT NULL
                       AND next_run_at <= $1::timestamptz
                     ORDER BY next_run_at ASC, id ASC"
                )
                .bind(as_of_key.as_str())
                .fetch_all(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
                    rows.into_iter().map(map_routine).collect()
                })
            },
        )
        .await
    }

    async fn record_fired(
        &self,
        context: &TenantContext,
        id: &RoutineId,
        occurred_at: &str,
        run_id: Option<&RunId>,
        next_run_at: Option<&str>,
    ) -> Result<Routine, AppError> {
        let id_key = id.as_str().to_owned();
        let at = occurred_at.to_owned();
        let next = next_run_at.map(str::to_owned);
        let run_key = run_id.map(|run| run.as_str().to_owned());
        let space_id = context.space_id().cloned().ok_or(AppError::NotFound)?;
        let space_key = space_id.as_str().to_owned();
        tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                // `next_run_at: None` means `@once`: the routine fires and deactivates.
                let row = sqlx::query_as::<_, RoutineRow>(
                    "UPDATE routines
                     SET last_run_at = $2::timestamptz,
                         next_run_at = $3::timestamptz,
                         active = CASE WHEN $3::timestamptz IS NULL THEN false ELSE active END,
                         account_failures = 0,
                         updated_at = now()
                     WHERE id = $1 AND space_id = $4
                     RETURNING id, space_id, bot_id, thread_id, user_id, name, prompt, crons,
                               timezone, active, notify,
                               boma.rfc3339(last_run_at) AS last_run_at,
                               boma.rfc3339(next_run_at) AS next_run_at,
                               account_failures,
                               boma.rfc3339(created_at) AS created_at,
                               boma.rfc3339(updated_at) AS updated_at",
                )
                .bind(&id_key)
                .bind(&at)
                .bind(next.as_deref())
                .bind(&space_key)
                .fetch_optional(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
                let row = row.ok_or(AppError::NotFound)?;

                sqlx::query(
                    "INSERT INTO routine_runs (routine_id, space_id, run_id, outcome, occurred_at)
                     VALUES ($1, $2, $3, 'fired', $4::timestamptz)",
                )
                .bind(&id_key)
                .bind(&space_key)
                .bind(run_key.as_deref())
                .bind(&at)
                .execute(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;

                map_routine(row)
            })
        })
        .await
    }

    async fn record_failure(
        &self,
        context: &TenantContext,
        id: &RoutineId,
        occurred_at: &str,
        run_id: Option<&RunId>,
        threshold: u32,
    ) -> Result<Routine, AppError> {
        let id_key = id.as_str().to_owned();
        let at = occurred_at.to_owned();
        let run_key = run_id.map(|run| run.as_str().to_owned());
        let space_id = context.space_id().cloned().ok_or(AppError::NotFound)?;
        let space_key = space_id.as_str().to_owned();
        tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                let row = sqlx::query_as::<_, RoutineRow>(
                    "UPDATE routines
                     SET account_failures = account_failures + 1,
                         active = CASE WHEN account_failures + 1 >= $3 THEN false ELSE active END,
                         updated_at = now()
                     WHERE id = $1 AND space_id = $2
                     RETURNING id, space_id, bot_id, thread_id, user_id, name, prompt, crons,
                               timezone, active, notify,
                               boma.rfc3339(last_run_at) AS last_run_at,
                               boma.rfc3339(next_run_at) AS next_run_at,
                               account_failures,
                               boma.rfc3339(created_at) AS created_at,
                               boma.rfc3339(updated_at) AS updated_at",
                )
                .bind(&id_key)
                .bind(&space_key)
                .bind(i32::try_from(threshold).unwrap_or(i32::MAX))
                .fetch_optional(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
                let row = row.ok_or(AppError::NotFound)?;

                let outcome = if row.active { "failed" } else { "paused" };
                sqlx::query(
                    "INSERT INTO routine_runs (routine_id, space_id, run_id, outcome, occurred_at)
                     VALUES ($1, $2, $3, $4, $5::timestamptz)",
                )
                .bind(&id_key)
                .bind(&space_key)
                .bind(run_key.as_deref())
                .bind(outcome)
                .bind(&at)
                .execute(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;

                map_routine(row)
            })
        })
        .await
    }

    async fn record_completed(
        &self,
        context: &TenantContext,
        id: &RoutineId,
        occurred_at: &str,
        run_id: Option<&RunId>,
    ) -> Result<(), AppError> {
        let id_key = id.as_str().to_owned();
        let at = occurred_at.to_owned();
        let run_key = run_id.map(|run| run.as_str().to_owned());
        let space_id = context.space_id().cloned().ok_or(AppError::NotFound)?;
        let space_key = space_id.as_str().to_owned();
        tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                sqlx::query(
                    "UPDATE routines
                     SET account_failures = 0, updated_at = now()
                     WHERE id = $1 AND space_id = $2",
                )
                .bind(id_key.as_str())
                .bind(space_key.as_str())
                .execute(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
                sqlx::query(
                    "INSERT INTO routine_runs (routine_id, space_id, run_id, outcome, occurred_at)
                     VALUES ($1, $2, $3, 'fired', $4::timestamptz)",
                )
                .bind(id_key.as_str())
                .bind(space_key.as_str())
                .bind(run_key.as_deref())
                .bind(at.as_str())
                .execute(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
                Ok(())
            })
        })
        .await
    }

    async fn record_skip(
        &self,
        context: &TenantContext,
        id: &RoutineId,
        outcome: RoutineOutcome,
        occurred_at: &str,
        run_id: Option<&RunId>,
        next_run_at: Option<&str>,
        detail: Option<&str>,
    ) -> Result<(), AppError> {
        let id_key = id.as_str().to_owned();
        let at = occurred_at.to_owned();
        let run_key = run_id.map(|run| run.as_str().to_owned());
        let next = next_run_at.map(str::to_owned);
        let detail_key = detail.map(str::to_owned);
        let space_id = context.space_id().cloned().ok_or(AppError::NotFound)?;
        let space_key = space_id.as_str().to_owned();
        let outcome = match outcome {
            RoutineOutcome::Coalesced => "coalesced",
            RoutineOutcome::Skipped | RoutineOutcome::Once => "skipped",
            RoutineOutcome::Fired => "fired",
            RoutineOutcome::Failed => "failed",
            RoutineOutcome::Paused => "paused",
        };
        tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                sqlx::query(
                    "UPDATE routines
                     SET next_run_at = $3::timestamptz,
                         active = CASE WHEN $3::timestamptz IS NULL THEN false ELSE active END,
                         updated_at = now()
                     WHERE id = $1 AND space_id = $2",
                )
                .bind(&id_key)
                .bind(&space_key)
                .bind(next.as_deref())
                .execute(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
                sqlx::query(
                    "INSERT INTO routine_runs (routine_id, space_id, run_id, outcome, detail, occurred_at)
                     VALUES ($1, $2, $3, $4, $5, $6::timestamptz)",
                )
                .bind(&id_key)
                .bind(&space_key)
                .bind(run_key.as_deref())
                .bind(outcome)
                .bind(detail_key.as_deref())
                .bind(&at)
                .execute(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
                Ok(())
            })
        })
        .await
    }
}
