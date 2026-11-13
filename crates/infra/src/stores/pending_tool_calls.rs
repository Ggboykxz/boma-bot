//! Pending tool calls (REQ-017): the tool calls a run is waiting to apply.
//!
//! The worker registers a call here when the model asked for a write-class tool and the run
//! parks on an approval card. The person answers the card; on `approve` the call is applied
//! exactly once and its result is written as a tool message. `(run_id, call_id)` is the primary
//! key, so a replayed pass replays the same registration instead of creating a second row.

use serde_json::Value;

use boma_app::{
    AppError,
    ports::{NewPendingToolCall, PendingToolCall, PendingToolCallStore},
};
use boma_domain::{PendingToolCallStatus, RunId, SpaceId, TenantContext, ThreadId};
use sqlx::{PgPool, types::Json};

use crate::tenant;

/// A row of `pending_tool_calls`.
#[derive(Debug, sqlx::FromRow)]
struct PendingToolCallRow {
    run_id: String,
    call_id: String,
    space_id: String,
    thread_id: String,
    name: String,
    arguments: Json<Value>,
    status: String,
    result: Option<Json<Value>>,
    created_at: String,
    answered_at: Option<String>,
}

/// Turns a stored row into the pending tool call it describes.
fn into_pending_tool_call(row: PendingToolCallRow) -> Result<PendingToolCall, AppError> {
    let status = PendingToolCallStatus::parse(&row.status).map_err(AppError::Rule)?;
    Ok(PendingToolCall {
        run_id: RunId::new(row.run_id)?,
        call_id: row.call_id,
        space_id: SpaceId::new(row.space_id)?,
        thread_id: ThreadId::new(row.thread_id)?,
        name: row.name,
        arguments: row.arguments.0,
        status,
        result: row.result.map(|result| result.0),
        created_at: row.created_at,
        answered_at: row.answered_at,
    })
}

/// The Postgres adapter for [`PendingToolCallStore`].
#[derive(Debug, Clone)]
pub struct PostgresPendingToolCallStore {
    pool: PgPool,
}

impl PostgresPendingToolCallStore {
    /// Builds the store on a pool.
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl PendingToolCallStore for PostgresPendingToolCallStore {
    async fn create(
        &self,
        context: &TenantContext,
        call: NewPendingToolCall,
    ) -> Result<PendingToolCall, AppError> {
        let space_id = call.space_id.as_str().to_owned();
        let row = tenant::with_tenant(&self.pool, context, |conn| {
            let space_id = space_id.clone();
            Box::pin(async move {
                sqlx::query_as::<_, PendingToolCallRow>(
                    "INSERT INTO pending_tool_calls
                         (run_id, call_id, space_id, thread_id, name, arguments, created_at)
                     VALUES ($1, $2, $3, $4, $5, $6, $7)
                     ON CONFLICT (run_id, call_id) DO NOTHING
                     RETURNING run_id, call_id, space_id, thread_id, name, arguments, status,
                               result, boma.rfc3339(created_at) AS created_at,
                               boma.rfc3339(answered_at) AS answered_at",
                )
                .bind(call.run_id.as_str())
                .bind(call.call_id.as_str())
                .bind(&space_id)
                .bind(call.thread_id.as_str())
                .bind(call.name.as_str())
                .bind(Json(call.arguments))
                .bind(call.created_at.as_str())
                .fetch_optional(&mut *conn)
                .await
                .map_err(tenant::unavailable)
            })
        })
        .await?;
        match row {
            Some(row) => into_pending_tool_call(row),
            // A replayed pass re-registers the same call: the caller decides what a conflict
            // means, it is not an error out of this use case.
            None => Err(AppError::Conflict),
        }
    }

    async fn find_pending(
        &self,
        context: &TenantContext,
        run_id: &RunId,
    ) -> Result<Vec<PendingToolCall>, AppError> {
        let run_id = run_id.as_str().to_owned();
        let rows = tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                sqlx::query_as::<_, PendingToolCallRow>(
                    "SELECT run_id, call_id, space_id, thread_id, name, arguments, status,
                            result, boma.rfc3339(created_at) AS created_at,
                            boma.rfc3339(answered_at) AS answered_at
                       FROM pending_tool_calls
                      WHERE run_id = $1 AND status = 'pending'
                      ORDER BY created_at, call_id",
                )
                .bind(&run_id)
                .fetch_all(&mut *conn)
                .await
                .map_err(tenant::unavailable)
            })
        })
        .await?;
        rows.into_iter().map(into_pending_tool_call).collect()
    }

    async fn mark_applied(
        &self,
        context: &TenantContext,
        run_id: &RunId,
        call_id: &str,
        result: Value,
        answered_at: &str,
    ) -> Result<PendingToolCall, AppError> {
        let answer = answered_at.to_owned();
        let run_id_owned = run_id.as_str().to_owned();
        let call_id_owned = call_id.to_owned();
        let outcome = tenant::with_tenant(&self.pool, context, |conn| {
            let result = result.clone();
            Box::pin(async move {
                sqlx::query_as::<_, PendingToolCallRow>(
                    "UPDATE pending_tool_calls
                        SET status = 'applied', result = $3, answered_at = $4
                      WHERE run_id = $1 AND call_id = $2 AND status = 'pending'
                      RETURNING run_id, call_id, space_id, thread_id, name, arguments, status,
                                result, boma.rfc3339(created_at) AS created_at,
                                boma.rfc3339(answered_at) AS answered_at",
                )
                .bind(&run_id_owned)
                .bind(&call_id_owned)
                .bind(Json(result))
                .bind(answer.as_str())
                .fetch_optional(&mut *conn)
                .await
                .map_err(tenant::unavailable)
            })
        })
        .await?;
        match outcome {
            Some(row) => into_pending_tool_call(row),
            None => pending_row_state(&self.pool, context, run_id, call_id).await,
        }
    }

    async fn mark_refused(
        &self,
        context: &TenantContext,
        run_id: &RunId,
        call_id: &str,
        answered_at: &str,
    ) -> Result<PendingToolCall, AppError> {
        let answer = answered_at.to_owned();
        let run_id_owned = run_id.as_str().to_owned();
        let call_id_owned = call_id.to_owned();
        let outcome = tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                sqlx::query_as::<_, PendingToolCallRow>(
                    "UPDATE pending_tool_calls
                        SET status = 'refused', answered_at = $3
                      WHERE run_id = $1 AND call_id = $2 AND status = 'pending'
                      RETURNING run_id, call_id, space_id, thread_id, name, arguments, status,
                                result, boma.rfc3339(created_at) AS created_at,
                                boma.rfc3339(answered_at) AS answered_at",
                )
                .bind(&run_id_owned)
                .bind(&call_id_owned)
                .bind(answer.as_str())
                .fetch_optional(&mut *conn)
                .await
                .map_err(tenant::unavailable)
            })
        })
        .await?;
        match outcome {
            Some(row) => into_pending_tool_call(row),
            None => pending_row_state(&self.pool, context, run_id, call_id).await,
        }
    }
}

/// Tells why a pending-row update found nothing: does the row exist (already answered), or is
/// it absent (a stranger to this tenant, or a call that never registered)?
async fn pending_row_state(
    pool: &PgPool,
    context: &TenantContext,
    run_id: &RunId,
    call_id: &str,
) -> Result<PendingToolCall, AppError> {
    let run_id = run_id.as_str().to_owned();
    let call_id = call_id.to_owned();
    let exists = tenant::with_tenant(pool, context, |conn| {
        Box::pin(async move {
            sqlx::query_scalar::<_, String>(
                "SELECT status FROM pending_tool_calls WHERE run_id = $1 AND call_id = $2",
            )
            .bind(&run_id)
            .bind(&call_id)
            .fetch_optional(&mut *conn)
            .await
            .map_err(tenant::unavailable)
        })
    })
    .await?;
    match exists {
        Some(_) => Err(AppError::Conflict),
        None => Err(AppError::NotFound),
    }
}
