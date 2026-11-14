//! Tickets and their comments, per space (REQ-038).

use boma_app::{
    AppError,
    ports::{NewTicket, NewTicketComment, TicketComment, TicketStore, TicketSummary},
};
use boma_domain::TenantContext;
use sqlx::PgPool;

use crate::tenant;

/// The Postgres adapter for the ticket board.
#[derive(Debug, Clone)]
pub struct PostgresTicketStore {
    pool: PgPool,
}

impl PostgresTicketStore {
    /// Builds the store on a pool.
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

type TicketRow = (
    String,
    String,
    String,
    String,
    String,
    Option<String>,
    String,
    i32,
    String,
    String,
);

fn summary_from(row: TicketRow) -> TicketSummary {
    let (
        id,
        key,
        title,
        description,
        state,
        assignee_bot_id,
        created_by,
        comment_count,
        created_at,
        updated_at,
    ) = row;
    TicketSummary {
        id,
        key,
        title,
        description,
        state,
        assignee_bot_id,
        created_by,
        comment_count,
        created_at,
        updated_at,
    }
}

#[async_trait::async_trait]
impl TicketStore for PostgresTicketStore {
    async fn list(&self, context: &TenantContext) -> Result<Vec<TicketSummary>, AppError> {
        let rows = tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                sqlx::query_as::<_, TicketRow>(concat!(
                    "SELECT ",
                    "id, key, title, description, state, assignee_bot_id, created_by,",
                    " comment_count, boma.rfc3339(created_at), boma.rfc3339(updated_at)",
                    " FROM tickets ORDER BY key ASC",
                ))
                .fetch_all(&mut *conn)
                .await
                .map_err(tenant::unavailable)
            })
        })
        .await?;
        Ok(rows.into_iter().map(summary_from).collect())
    }

    async fn create(
        &self,
        context: &TenantContext,
        ticket: NewTicket,
    ) -> Result<TicketSummary, AppError> {
        let space_id = match context {
            TenantContext::Member { space_id, .. } => space_id.as_str().to_owned(),
            TenantContext::System { .. } => return Err(AppError::NotFound),
        };
        let created_by = match context {
            TenantContext::Member { user_id, .. } => user_id.as_str().to_owned(),
            TenantContext::System { .. } => return Err(AppError::NotFound),
        };
        tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                // The key counter is one row per space, allocated under the same transaction.
                let seq: i32 = sqlx::query_scalar(
                    "INSERT INTO board_state (space_id, next_seq)
                     VALUES ($1, 2)
                     ON CONFLICT (space_id) DO UPDATE SET next_seq = board_state.next_seq + 1
                     RETURNING next_seq - 1",
                )
                .bind(&space_id)
                .fetch_one(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
                let key = format!("BOT-{seq}");
                sqlx::query(
                    "INSERT INTO tickets (id, space_id, key, title, description, state,
                                   assignee_bot_id, created_by, comment_count, created_at, updated_at)
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 0, $9::timestamptz, $9::timestamptz)",
                )
                .bind(&ticket.id)
                .bind(&space_id)
                .bind(&key)
                .bind(&ticket.title)
                .bind(&ticket.description)
                .bind(&ticket.state)
                .bind(&ticket.assignee_bot_id)
                .bind(&created_by)
                .bind(&ticket.created_at)
                .execute(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
                let row = sqlx::query_as::<_, TicketRow>(concat!(
                    "SELECT ",
                    "id, key, title, description, state, assignee_bot_id, created_by,",
                    " comment_count, boma.rfc3339(created_at), boma.rfc3339(updated_at)",
                    " FROM tickets WHERE id = $1",
                ))
                .bind(&ticket.id)
                .fetch_one(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
                Ok(summary_from(row))
            })
        })
        .await
    }

    async fn move_to(
        &self,
        context: &TenantContext,
        ticket_id: &str,
        state: &str,
    ) -> Result<bool, AppError> {
        let ticket_id = ticket_id.to_owned();
        let state = state.to_owned();
        let result = tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                sqlx::query("UPDATE tickets SET state = $2, updated_at = now() WHERE id = $1")
                    .bind(&ticket_id)
                    .bind(&state)
                    .execute(&mut *conn)
                    .await
                    .map_err(tenant::unavailable)
            })
        })
        .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn comment(
        &self,
        context: &TenantContext,
        comment: NewTicketComment,
    ) -> Result<Option<TicketComment>, AppError> {
        let ticket_id = comment.ticket_id.clone();
        tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                let exists: Option<String> = sqlx::query_scalar(
                    "SELECT id FROM tickets WHERE id = $1",
                )
                .bind(&ticket_id)
                .fetch_optional(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
                if exists.is_none() {
                    return Ok(None);
                }
                sqlx::query(
                    "INSERT INTO ticket_comments (id, ticket_id, author_kind, author_id, body, created_at)
                     VALUES ($1, $2, $3, $4, $5, $6::timestamptz)",
                )
                .bind(&comment.id)
                .bind(&ticket_id)
                .bind(&comment.author_kind)
                .bind(&comment.author_id)
                .bind(&comment.body)
                .bind(&comment.created_at)
                .execute(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
                sqlx::query(
                    "UPDATE tickets SET comment_count = comment_count + 1, updated_at = now() WHERE id = $1",
                )
                .bind(&ticket_id)
                .execute(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
                Ok(Some(TicketComment {
                    id: comment.id,
                    ticket_id,
                    author_kind: comment.author_kind,
                    author_id: comment.author_id,
                    body: comment.body,
                    created_at: comment.created_at,
                }))
            })
        })
        .await
    }

    async fn get(
        &self,
        context: &TenantContext,
        ticket_id: &str,
    ) -> Result<Option<TicketSummary>, AppError> {
        let ticket_id = ticket_id.to_owned();
        let row = tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                sqlx::query_as::<_, TicketRow>(concat!(
                    "SELECT ",
                    "id, key, title, description, state, assignee_bot_id, created_by,",
                    " comment_count, boma.rfc3339(created_at), boma.rfc3339(updated_at)",
                    " FROM tickets WHERE id = $1",
                ))
                .bind(&ticket_id)
                .fetch_optional(&mut *conn)
                .await
                .map_err(tenant::unavailable)
            })
        })
        .await?;
        Ok(row.map(summary_from))
    }
}
