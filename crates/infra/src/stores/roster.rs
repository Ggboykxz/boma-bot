//! The roster: one row per bot of the caller's space, with the member-specific state the rail
//! renders (REQ-009).
//!
//! Two rules shape this store:
//!
//! * **The snooze is a filter, never a hiding place.** A snoozed chat leaves the regular list
//!   while its deadline is in the future, and it is back the moment the deadline passes — no job,
//!   no flag to forget to clear.
//! * **The unread count never lies about background steps.** Background messages (`role: tool`)
//!   do not count towards the badge.

use boma_app::{
    AppError,
    ports::{BotSummary, RosterStore},
};
use boma_domain::{TenantContext, ThreadId};
use sqlx::PgPool;

use super::rows::{self, BotSummaryRow};
use crate::tenant;

/// The Postgres adapter for [`RosterStore`].
#[derive(Debug, Clone)]
pub struct PostgresRosterStore {
    pool: PgPool,
}

impl PostgresRosterStore {
    /// Builds the store on a pool.
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl RosterStore for PostgresRosterStore {
    async fn list_bots(&self, context: &TenantContext) -> Result<Vec<BotSummary>, AppError> {
        let user_id = context.user_id().cloned().ok_or(AppError::NotFound)?;
        let space_id = context.space_id().cloned().ok_or(AppError::NotFound)?;
        tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                // The space predicate is the defense in fact: the row level security
                // policy is defense in depth, and a superuser is not subject to it.
                let summaries = sqlx::query_as::<_, BotSummaryRow>(
                    "SELECT b.id, b.space_id, b.user_id, b.name, b.description, b.color,
                            b.thread_id, boma.rfc3339(b.archived_at) AS archived_at,
                            boma.rfc3339(b.created_at) AS created_at, b.enabled_tools,
                            b.model_provider, b.model_id, b.model_pinned,
                            boma.rfc3339(s.snoozed_until) AS snoozed_until,
                            (SELECT count(*) FROM messages m
                              WHERE m.thread_id = t.id AND m.role <> 'tool'
                                AND m.seq > COALESCE(p.last_seq, 0)) AS unread_count
                     FROM bots b
                     JOIN threads t ON t.bot_id = b.id AND t.space_id = $2
                     LEFT JOIN thread_snoozes s
                            ON s.thread_id = t.id AND s.user_id = $1 AND s.snoozed_until > now()
                     LEFT JOIN thread_read_states p
                            ON p.thread_id = t.id AND p.user_id = $1
                     WHERE b.space_id = $2
                     ORDER BY b.created_at ASC, b.id ASC",
                )
                .bind(user_id.as_str())
                .bind(space_id.as_str())
                .fetch_all(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;

                summaries.into_iter().map(rows::bot_summary).collect()
            })
        })
        .await
    }

    async fn set_snooze(
        &self,
        context: &TenantContext,
        thread_id: &ThreadId,
        until: Option<&str>,
    ) -> Result<Option<String>, AppError> {
        let user_id = context.user_id().cloned().ok_or(AppError::NotFound)?;
        let space_id = context.space_id().cloned().ok_or(AppError::NotFound)?;
        let thread_key = thread_id.as_str().to_owned();

        let Some(until) = until else {
            tenant::with_tenant(&self.pool, context, |conn| {
                Box::pin(async move {
                    sqlx::query("DELETE FROM thread_snoozes WHERE thread_id = $1 AND user_id = $2")
                        .bind(thread_key.as_str())
                        .bind(user_id.as_str())
                        .execute(&mut *conn)
                        .await
                        .map_err(tenant::unavailable)
                })
            })
            .await?;
            return Ok(None);
        };

        let until = until.to_owned();
        tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                // The row only lands when the thread belongs to the caller's space. The
                // predicate is the defense in fact; the row level security policy is the
                // defense in depth (and is what proves it under a non-privileged role).
                sqlx::query_scalar::<_, String>(
                    "INSERT INTO thread_snoozes (thread_id, space_id, user_id, snoozed_until, created_at, updated_at)
                     SELECT $1, $2, $3, $4::timestamptz, now(), now()
                     WHERE EXISTS (SELECT 1 FROM threads WHERE id = $1 AND space_id = $2)
                     ON CONFLICT (thread_id, user_id) DO UPDATE
                     SET snoozed_until = EXCLUDED.snoozed_until, updated_at = now()
                     RETURNING boma.rfc3339(snoozed_until)",
                )
                .bind(thread_key.as_str())
                .bind(space_id.as_str())
                .bind(user_id.as_str())
                .bind(until.as_str())
                .fetch_optional(&mut *conn)
                .await
                .map_err(tenant::unavailable)?
                .ok_or(AppError::NotFound)
                .map(Some)
            })
        })
        .await
    }
}
