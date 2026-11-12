//! Runs: the executions of the agent, and the leases that keep two workers apart.
//!
//! Three rules shape this store:
//!
//! * **The nonce makes a start idempotent.** A retried send finds the run it already started and
//!   reports the *existing* run, so the caller creates nothing else (REQ-002).
//! * **The claim is atomic.** The oldest queued run moves to `leased` in a single statement that
//!   skips the runs another worker already holds, so two workers never claim the same run
//!   (REQ-012).
//! * **The fence is checked on every write.** A worker that was replaced, or that was slow past
//!   its lease expiry, presents a fence that no longer matches and is refused (REQ-012).

use boma_app::{
    AppError,
    ports::{ActivityPage, NewRun, RunActivity, RunStore},
};
use boma_domain::{BotId, Run, RunId, RunStatus, SystemReason, TenantContext, ThreadId};
use sqlx::PgPool;

use super::rows::{self, ActivityRow, RunRow};
use crate::tenant;

/// How long a claim holds a run before it may be taken back.
///
/// The lease is what a reconciler uses to recover a run whose worker died (REQ-014).
const LEASE_TTL_SECONDS: i64 = 3_600;

/// The Postgres adapter for [`RunStore`].
#[derive(Debug, Clone)]
pub struct PostgresRunStore {
    pool: PgPool,
}

impl PostgresRunStore {
    /// Builds the store on a pool.
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl RunStore for PostgresRunStore {
    async fn create(&self, run: NewRun) -> Result<Run, AppError> {
        let space_id = run.space_id.clone();
        let nonce = run.client_nonce.clone();
        let thread_id = run.thread_id.clone();
        let nonce_key = nonce.clone();
        let user_key = run.user_id.as_str().to_owned();

        let attempt = tenant::in_space(&self.pool, &space_id, |conn| {
            Box::pin(async move {
                let row = sqlx::query_as::<_, RunRow>(
                    "INSERT INTO runs (
                        id, space_id, bot_id, thread_id, user_id, status, trigger, client_nonce,
                        source_message_id, created_at, model_provider, model_id, model_pinned,
                        parent_run_id, routine_id
                     )
                     VALUES ($1, $2, $3, $4, $5, 'queued', $6, $7, $8, $9::timestamptz, $10, $11, $12, $13, $14)
                     RETURNING id, space_id, bot_id, thread_id, user_id, status, trigger,
                               client_nonce, source_message_id, lease_owner, lease_fence,
                               boma.rfc3339(lease_expires_at) AS lease_expires_at,
                               boma.rfc3339(started_at) AS started_at,
                               boma.rfc3339(completed_at) AS completed_at, error,
                               steering_from_seq, steering_consumed_seq, attempt,
                               model_provider, model_id, model_pinned,
                               parent_run_id, routine_id,
                               boma.rfc3339(not_before) AS not_before,
                               boma.rfc3339(created_at) AS created_at",
                )
                .bind(run.id.as_str())
                .bind(run.space_id.as_str())
                .bind(run.bot_id.as_str())
                .bind(run.thread_id.as_str())
                .bind(user_key.as_str())
                .bind(run.trigger.as_str())
                .bind(nonce_key.as_deref())
                .bind(
                    run.source_message_id
                        .as_ref()
                        .map(boma_domain::MessageId::as_str),
                )
                .bind(&run.created_at)
                .bind(run.model_provider.clone())
                .bind(run.model_id.clone())
                .bind(run.model_pinned)
                .bind(run.parent_run_id.as_ref().map(boma_domain::RunId::as_str))
                .bind(run.routine_id.as_ref().map(boma_domain::RoutineId::as_str))
                .fetch_one(&mut *conn)
                .await;

                match row {
                    Ok(row) => Ok(row),
                    Err(error) if tenant::is_unique_violation(&error) => Err(AppError::Conflict),
                    Err(error) => Err(tenant::unavailable(error)),
                }
            })
        })
        .await;

        match attempt {
            Ok(row) => rows::run(row),
            Err(AppError::Conflict) => {
                // The nonce already started a run: the caller is replaying a send.
                let Some(nonce) = nonce else {
                    tracing::error!("a run was refused for a reason its nonce cannot explain");
                    return Err(AppError::Unavailable);
                };
                let context = TenantContext::Member {
                    space_id: space_id.clone(),
                    user_id: run.user_id.clone(),
                };
                let Some(existing) = self.find_by_nonce(&context, &thread_id, &nonce).await? else {
                    tracing::error!("a run conflicted but no run carries that nonce");
                    return Err(AppError::Unavailable);
                };
                Ok(existing)
            }
            Err(error) => Err(error),
        }
    }

    async fn find(&self, context: &TenantContext, id: &RunId) -> Result<Option<Run>, AppError> {
        let id = id.as_str().to_owned();
        let row = tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                sqlx::query_as::<_, RunRow>(
                    "SELECT id, space_id, bot_id, thread_id, user_id, status, trigger, client_nonce,
                            source_message_id, lease_owner, lease_fence,
                            steering_from_seq, steering_consumed_seq, attempt,
                            model_provider, model_id, model_pinned,
                            parent_run_id, routine_id,
                            boma.rfc3339(not_before) AS not_before,
                            boma.rfc3339(lease_expires_at) AS lease_expires_at,
                            boma.rfc3339(started_at) AS started_at,
                            boma.rfc3339(completed_at) AS completed_at, error,
                            boma.rfc3339(created_at) AS created_at
                     FROM runs
                     WHERE id = $1",
                )
                .bind(id.as_str())
                .fetch_optional(&mut *conn)
                .await
                .map_err(tenant::unavailable)
            })
        })
        .await?;

        row.map(rows::run).transpose()
    }

    async fn find_by_nonce(
        &self,
        context: &TenantContext,
        thread_id: &ThreadId,
        client_nonce: &str,
    ) -> Result<Option<Run>, AppError> {
        let thread_id = thread_id.as_str().to_owned();
        let client_nonce = client_nonce.to_owned();
        let row = tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                sqlx::query_as::<_, RunRow>(
                    "SELECT id, space_id, bot_id, thread_id, user_id, status, trigger, client_nonce,
                            source_message_id, lease_owner, lease_fence,
                            steering_from_seq, steering_consumed_seq, attempt,
                            model_provider, model_id, model_pinned,
                            parent_run_id, routine_id,
                            boma.rfc3339(not_before) AS not_before,
                            boma.rfc3339(lease_expires_at) AS lease_expires_at,
                            boma.rfc3339(started_at) AS started_at,
                            boma.rfc3339(completed_at) AS completed_at, error,
                            boma.rfc3339(created_at) AS created_at
                     FROM runs
                     WHERE thread_id = $1 AND client_nonce = $2",
                )
                .bind(thread_id.as_str())
                .bind(client_nonce)
                .fetch_optional(&mut *conn)
                .await
                .map_err(tenant::unavailable)
            })
        })
        .await?;

        row.map(rows::run).transpose()
    }

    async fn find_active_for_thread(
        &self,
        context: &TenantContext,
        thread_id: &ThreadId,
    ) -> Result<Option<Run>, AppError> {
        let thread_id = thread_id.as_str().to_owned();
        let row = tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                sqlx::query_as::<_, RunRow>(
                    "SELECT id, space_id, bot_id, thread_id, user_id, status, trigger, client_nonce,
                            source_message_id, lease_owner, lease_fence,
                            steering_from_seq, steering_consumed_seq, attempt,
                            model_provider, model_id, model_pinned,
                            parent_run_id, routine_id,
                            boma.rfc3339(not_before) AS not_before,
                            boma.rfc3339(lease_expires_at) AS lease_expires_at,
                            boma.rfc3339(started_at) AS started_at,
                            boma.rfc3339(completed_at) AS completed_at, error,
                            boma.rfc3339(created_at) AS created_at
                     FROM runs
                     WHERE thread_id = $1
                       AND status IN ('leased', 'running')
                       -- A routine or a webhook turn is not conversational (REQ-003).
                       AND trigger IN ('user', 'helper_result', 'retry')
                     ORDER BY created_at DESC, id DESC
                     LIMIT 1",
                )
                .bind(thread_id.as_str())
                .fetch_optional(&mut *conn)
                .await
                .map_err(tenant::unavailable)
            })
        })
        .await?;

        row.map(rows::run).transpose()
    }

    async fn set_steering_from(
        &self,
        context: &TenantContext,
        id: &RunId,
        seq: i64,
    ) -> Result<(), AppError> {
        let id = id.as_str().to_owned();
        tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                // The first pending message decides: later ones never move the window.
                sqlx::query(
                    "UPDATE runs
                     SET steering_from_seq = CASE
                             WHEN steering_from_seq IS NULL OR steering_from_seq > $2 THEN $2
                             ELSE steering_from_seq
                         END
                     WHERE id = $1 AND status NOT IN ('completed', 'failed', 'cancelled')",
                )
                .bind(id.as_str())
                .bind(seq)
                .execute(&mut *conn)
                .await
                .map_err(tenant::unavailable)
            })
        })
        .await?;
        Ok(())
    }

    async fn set_steering_consumed(
        &self,
        context: &TenantContext,
        id: &RunId,
        seq: i64,
        fence: Option<i64>,
    ) -> Result<(), AppError> {
        let id = id.as_str().to_owned();
        let updated = tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                sqlx::query(
                    "UPDATE runs
                     SET steering_consumed_seq = GREATEST(steering_consumed_seq, $2)
                     WHERE id = $1
                       AND status NOT IN ('completed', 'failed', 'cancelled')
                       AND ($3::bigint IS NULL OR lease_fence = $3)",
                )
                .bind(id.as_str())
                .bind(seq)
                .bind(fence)
                .execute(&mut *conn)
                .await
                .map_err(tenant::unavailable)
            })
        })
        .await?;
        if updated.rows_affected() == 0 {
            return Err(AppError::Conflict);
        }
        Ok(())
    }

    async fn find_open_routine(
        &self,
        context: &TenantContext,
        routine_id: &boma_domain::RoutineId,
    ) -> Result<Option<Run>, AppError> {
        let routine_key = routine_id.as_str().to_owned();
        tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                let row = sqlx::query_as::<_, RunRow>(
                    "SELECT id, space_id, bot_id, thread_id, user_id, status, trigger,
                            client_nonce, source_message_id, lease_owner, lease_fence,
                            steering_from_seq, steering_consumed_seq, attempt,
                            model_provider, model_id, model_pinned,
                            parent_run_id, routine_id,
                            boma.rfc3339(not_before) AS not_before,
                            boma.rfc3339(lease_expires_at) AS lease_expires_at,
                            boma.rfc3339(started_at) AS started_at,
                            boma.rfc3339(completed_at) AS completed_at, error,
                            boma.rfc3339(created_at) AS created_at
                     FROM runs
                     WHERE routine_id = $1
                       AND status NOT IN ('completed', 'failed', 'cancelled')
                     ORDER BY created_at ASC
                     LIMIT 1",
                )
                .bind(routine_key.as_str())
                .fetch_optional(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
                row.map(rows::run).transpose()
            })
        })
        .await
    }

    async fn claim_next_queued(&self, owner: &str) -> Result<Option<Run>, AppError> {
        let owner_key = owner.to_owned();
        // The claim crosses tenants: a worker takes the oldest queued run wherever it is. It is
        // a system path, and the reason says so.
        let row = tenant::as_system(&self.pool, SystemReason::ScheduledJob, |conn| {
            Box::pin(async move {
                // One statement: the row is locked, skipped when another worker holds it, and
                // updated in place. Two workers therefore never claim the same run. A run the
                // automatic retry scheduled (REQ-024) is not visible until its not_before passes.
                //
                // An expired lease is no longer held: its worker disappeared mid-turn and the
                // run is handed out again with the fence grown (REQ-013, REQ-014).
                sqlx::query_as::<_, RunRow>(
                    "WITH candidate AS (
                        SELECT id
                        FROM runs
                        WHERE (status = 'queued'
                               AND (not_before IS NULL OR not_before <= now()))
                           OR (status IN ('leased', 'running')
                               AND lease_expires_at IS NOT NULL
                               AND lease_expires_at <= now())
                        ORDER BY created_at ASC, id ASC
                        FOR UPDATE SKIP LOCKED
                        LIMIT 1
                     )
                     UPDATE runs r
                     SET status = 'leased',
                         lease_owner = $1,
                         lease_fence = r.lease_fence + 1,
                         lease_expires_at = now() + ($2::bigint * interval '1 second'),
                         started_at = COALESCE(r.started_at, now())
                     FROM candidate
                     WHERE r.id = candidate.id
                     RETURNING r.id, r.space_id, r.bot_id, r.thread_id, r.user_id, r.status, r.trigger,
                               r.client_nonce, r.source_message_id, r.lease_owner, r.lease_fence,
                               r.steering_from_seq, r.steering_consumed_seq, r.attempt,
                               r.model_provider, r.model_id, r.model_pinned,
                               r.parent_run_id, r.routine_id,
                               boma.rfc3339(r.not_before) AS not_before,
                               boma.rfc3339(r.lease_expires_at) AS lease_expires_at,
                               boma.rfc3339(r.started_at) AS started_at,
                               boma.rfc3339(r.completed_at) AS completed_at, r.error,
                               boma.rfc3339(r.created_at) AS created_at",
                )
                .bind(owner_key.as_str())
                .bind(LEASE_TTL_SECONDS)
                .fetch_optional(&mut *conn)
                .await
                .map_err(tenant::unavailable)
            })
        })
        .await?;

        row.map(rows::run).transpose()
    }

    async fn set_status(
        &self,
        context: &TenantContext,
        id: &RunId,
        status: RunStatus,
        at: &str,
        fence: Option<i64>,
        error: Option<String>,
    ) -> Result<(), AppError> {
        let id = id.as_str().to_owned();
        let at = at.to_owned();
        let updated = tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                sqlx::query(
                    "UPDATE runs
                     SET status = $2,
                         completed_at = CASE
                             WHEN $2 IN ('completed', 'failed', 'cancelled') THEN $3::timestamptz
                             ELSE completed_at
                         END,
                         -- A transition that carries no reason never erases one: the last
                         -- legible failure stays visible (REQ-021).
                         error = COALESCE($5, error)
                     WHERE id = $1 AND ($4::bigint IS NULL OR lease_fence = $4)",
                )
                .bind(id.as_str())
                .bind(status.as_str())
                .bind(at)
                .bind(fence)
                .bind(error)
                .execute(&mut *conn)
                .await
                .map_err(tenant::unavailable)
            })
        })
        .await?;

        if updated.rows_affected() == 0 {
            // The fence no longer matches: another worker took the run over, and this one must
            // not write (REQ-012).
            return Err(AppError::Conflict);
        }
        Ok(())
    }

    async fn retry(
        &self,
        context: &TenantContext,
        id: &RunId,
        not_before: Option<&str>,
    ) -> Result<Run, AppError> {
        let id = id.as_str().to_owned();
        let space_id = context.space_id().cloned().ok_or(AppError::NotFound)?;
        let space_id_key = space_id.as_str().to_owned();
        let not_before = not_before.map(str::to_owned);
        tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                // The only transition out of a terminal state, atomically: failed → queued,
                // one attempt added. A concurrent retry sees `queued` and gets zero rows.
                let updated = sqlx::query_as::<_, rows::RunRow>(
                    "UPDATE runs
                     SET status = 'queued',
                         attempt = attempt + 1,
                         started_at = NULL,
                         completed_at = NULL,
                         lease_owner = NULL,
                         lease_fence = 0,
                         lease_expires_at = NULL,
                         steering_from_seq = NULL,
                         steering_consumed_seq = 0,
                         not_before = $3::timestamptz
                     WHERE id = $1 AND status = 'failed' AND space_id = $2
                     RETURNING id, space_id, bot_id, thread_id, user_id, status, trigger,
                               client_nonce, source_message_id, lease_owner, lease_fence,
                               boma.rfc3339(lease_expires_at) AS lease_expires_at,
                               boma.rfc3339(started_at) AS started_at,
                               boma.rfc3339(completed_at) AS completed_at, error,
                               steering_from_seq, steering_consumed_seq, attempt,
                               model_provider, model_id, model_pinned,
                               parent_run_id, routine_id,
                               boma.rfc3339(not_before) AS not_before,
                               boma.rfc3339(created_at) AS created_at",
                )
                .bind(id.as_str())
                .bind(space_id_key.as_str())
                .bind(not_before.as_deref())
                .fetch_one(&mut *conn)
                .await
                .map_err(tenant::unavailable);

                if let Ok(row) = updated {
                    return rows::run(row);
                }
                // Not retried: find out why, so the caller hears the honest reason.
                {
                    let found = sqlx::query_as::<_, rows::RunRow>(
                        "SELECT id, space_id, bot_id, thread_id, user_id, status, trigger,
                                    client_nonce, source_message_id, lease_owner, lease_fence,
                                    boma.rfc3339(lease_expires_at) AS lease_expires_at,
                                    boma.rfc3339(started_at) AS started_at,
                                    boma.rfc3339(completed_at) AS completed_at, error,
                                    steering_from_seq, steering_consumed_seq, attempt,
                                    model_provider, model_id, model_pinned,
                                    parent_run_id, routine_id,
                                    boma.rfc3339(not_before) AS not_before,
                                    boma.rfc3339(created_at) AS created_at
                             FROM runs WHERE id = $1 AND space_id = $2",
                    )
                    .bind(id.as_str())
                    .bind(space_id_key.as_str())
                    .fetch_optional(&mut *conn)
                    .await
                    .map_err(tenant::unavailable)?;
                    match found {
                        Some(_) => Err(AppError::Conflict),
                        None => Err(AppError::NotFound),
                    }
                }
            })
        })
        .await
    }

    async fn list(
        &self,
        context: &TenantContext,
        bot_id: Option<&BotId>,
        before: Option<&str>,
        limit: usize,
    ) -> Result<ActivityPage, AppError> {
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        let cursor = before.map(parse_run_cursor).transpose()?;
        let bot = bot_id.map(|id| id.as_str().to_owned());

        let row = tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                sqlx::query_as::<_, ActivityRow>(
                    "SELECT r.id, r.space_id, r.bot_id, r.thread_id, r.user_id, r.status, r.trigger,
                            r.client_nonce, r.source_message_id, r.lease_owner, r.lease_fence,
                            r.steering_from_seq, r.steering_consumed_seq, r.attempt,
                            r.model_provider, r.model_id, r.model_pinned,
                            r.parent_run_id, r.routine_id,
                            boma.rfc3339(r.not_before) AS not_before,
                            boma.rfc3339(r.lease_expires_at) AS lease_expires_at,
                            boma.rfc3339(r.started_at) AS started_at,
                            boma.rfc3339(r.completed_at) AS completed_at, r.error,
                            boma.rfc3339(r.created_at) AS created_at,
                            (SELECT m.blocks FROM messages m WHERE m.id = r.source_message_id)
                                AS prompt_blocks
                     FROM runs r
                     WHERE ($1::text IS NULL OR r.bot_id = $1)
                       AND ($2::timestamptz IS NULL OR (r.created_at, r.id) < ($2::timestamptz, $3::text))
                     ORDER BY r.created_at DESC, r.id DESC
                     LIMIT $4",
                )
                .bind(bot)
                .bind(cursor.as_ref().map(|(_, at)| at.as_str()))
                .bind(cursor.as_ref().map(|(_, id)| id.as_str()))
                .bind(limit)
                .fetch_all(&mut *conn)
                .await
                .map_err(tenant::unavailable)
            })
        })
        .await?;

        let mut runs = Vec::with_capacity(row.len());
        for activity in row {
            let snippet = activity
                .prompt_blocks
                .as_ref()
                .and_then(|blocks| rows::prompt_snippet(&blocks.0));
            runs.push(RunActivity {
                run: rows::run(activity.run)?,
                prompt_snippet: snippet,
            });
        }
        let next_cursor = if i64::try_from(runs.len()).unwrap_or(i64::MAX) == limit {
            runs.last().map(|activity| {
                format!(
                    "run:{}|{}",
                    activity.run.created_at,
                    activity.run.id.as_str()
                )
            })
        } else {
            None
        };

        Ok(ActivityPage { runs, next_cursor })
    }
}

/// The cursor of the activity list: the instant and the id of the oldest row of a page.
///
/// The format is opaque to the client, like every other cursor of the product.
fn parse_run_cursor(cursor: &str) -> Result<(String, String), AppError> {
    let rest = cursor.strip_prefix("run:").ok_or_else(|| {
        tracing::debug!("a run cursor does not carry the shape this product writes");
        AppError::Rule(boma_domain::DomainError::InvalidCursor(cursor.to_owned()))
    })?;
    let (at, id) = rest.split_once('|').ok_or_else(|| {
        tracing::debug!("a run cursor does not carry the shape this product writes");
        AppError::Rule(boma_domain::DomainError::InvalidCursor(cursor.to_owned()))
    })?;
    Ok((at.to_owned(), id.to_owned()))
}
