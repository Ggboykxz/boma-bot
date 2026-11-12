//! Messages: the thread's history.
//!
//! Two rules shape this store, and both are about races:
//!
//! * **The sequence belongs to the store.** The caller never computes it: the store takes it from
//!   the thread, in the same transaction as the row it belongs to, so two concurrent sends can
//!   never claim the same number (REQ-005).
//! * **The nonce makes a send idempotent.** A retried send finds the message it already wrote and
//!   reports that it did not create a new one (REQ-002).

use boma_app::{
    AppError,
    ports::{MessageStore, NewMessage, StoredMessage},
};
use boma_domain::{Message, MessagePage, TenantContext, ThreadId, cursor_from_seq};
use sqlx::{PgPool, types::Json};

use super::rows::{self, MessageRow};
use crate::tenant;

/// The Postgres adapter for [`MessageStore`].
#[derive(Debug, Clone)]
pub struct PostgresMessageStore {
    pool: PgPool,
}

impl PostgresMessageStore {
    /// Builds the store on a pool.
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl MessageStore for PostgresMessageStore {
    async fn insert(
        &self,
        context: &TenantContext,
        message: NewMessage,
    ) -> Result<StoredMessage, AppError> {
        let thread_id = message.thread_id.clone();
        let nonce = message.client_nonce.clone();
        let thread_key = thread_id.as_str().to_owned();
        let nonce_key = nonce.clone();

        let attempt = tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                // The sequence is taken from the thread, in this transaction: the row and the
                // number it carries are written together or not at all.
                let seq = sqlx::query_scalar::<_, i64>(
                    "UPDATE threads SET next_message_seq = next_message_seq + 1 WHERE id = $1
                     RETURNING next_message_seq - 1",
                )
                .bind(thread_key.as_str())
                .fetch_one(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;

                let blocks = serde_json::to_value(&message.blocks).map_err(|error| {
                    tracing::debug!(error = %error, "the blocks of a message cannot be written");
                    AppError::Unavailable
                })?;

                let row = sqlx::query_as::<_, MessageRow>(
                    "INSERT INTO messages (
                        id, space_id, thread_id, seq, role, blocks, bot_id, author_user_id,
                        run_id, client_nonce, created_at
                     )
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11::timestamptz)
                     RETURNING id, thread_id, seq, role, blocks, bot_id, author_user_id, run_id,
                               client_nonce, boma.rfc3339(created_at) AS created_at",
                )
                .bind(message.id.as_str())
                .bind(message.space_id.as_str())
                .bind(thread_key.as_str())
                .bind(seq)
                .bind(message.role.as_str())
                .bind(Json(blocks))
                .bind(message.bot_id.as_ref().map(boma_domain::BotId::as_str))
                .bind(
                    message
                        .author_user_id
                        .as_ref()
                        .map(boma_domain::UserId::as_str),
                )
                .bind(message.run_id.as_ref().map(boma_domain::RunId::as_str))
                .bind(nonce_key.as_deref())
                .bind(&message.created_at)
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
            Ok(row) => Ok(StoredMessage {
                message: rows::message(row)?,
                created: true,
            }),
            Err(AppError::Conflict) => {
                // The nonce already produced a message: the send is a replay, not a new message.
                let Some(nonce) = nonce else {
                    tracing::error!("a message was refused for a reason its nonce cannot explain");
                    return Err(AppError::Unavailable);
                };
                let Some(existing) = self.find_by_nonce(context, &thread_id, &nonce).await? else {
                    tracing::error!("a message conflicted but no message carries that nonce");
                    return Err(AppError::Unavailable);
                };
                Ok(StoredMessage {
                    message: existing,
                    created: false,
                })
            }
            Err(error) => Err(error),
        }
    }

    async fn find_by_nonce(
        &self,
        context: &TenantContext,
        thread_id: &ThreadId,
        client_nonce: &str,
    ) -> Result<Option<Message>, AppError> {
        let thread_id = thread_id.as_str().to_owned();
        let client_nonce = client_nonce.to_owned();
        let row = tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                sqlx::query_as::<_, MessageRow>(
                    "SELECT id, thread_id, seq, role, blocks, bot_id, author_user_id, run_id,
                            client_nonce, boma.rfc3339(created_at) AS created_at
                     FROM messages
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

        row.map(rows::message).transpose()
    }

    async fn page(
        &self,
        context: &TenantContext,
        thread_id: &ThreadId,
        before: Option<i64>,
        limit: usize,
    ) -> Result<MessagePage, AppError> {
        let thread_id = thread_id.as_str().to_owned();
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        let row = tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                sqlx::query_as::<_, MessageRow>(
                    "SELECT id, thread_id, seq, role, blocks, bot_id, author_user_id, run_id,
                            client_nonce, boma.rfc3339(created_at) AS created_at
                     FROM messages
                     WHERE thread_id = $1 AND ($2::bigint IS NULL OR seq < $2)
                     ORDER BY seq DESC
                     LIMIT $3",
                )
                .bind(thread_id.as_str())
                .bind(before)
                .bind(limit)
                .fetch_all(&mut *conn)
                .await
                .map_err(tenant::unavailable)
            })
        })
        .await?;

        let messages = row
            .into_iter()
            .map(rows::message)
            .collect::<Result<Vec<Message>, AppError>>()?;

        // The cursor points at the oldest message of this page: the next page starts before it.
        let next_cursor = if i64::try_from(messages.len()).unwrap_or(i64::MAX) == limit {
            messages.last().map(|message| cursor_from_seq(message.seq))
        } else {
            None
        };

        Ok(MessagePage {
            messages,
            next_cursor,
        })
    }

    async fn set_text(
        &self,
        context: &TenantContext,
        message_id: &boma_domain::MessageId,
        text: &str,
    ) -> Result<(), AppError> {
        let message_id = message_id.as_str().to_owned();
        let text = text.to_owned();
        let blocks = serde_json::json!([{ "type": "text", "text": text }]);
        tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                sqlx::query("UPDATE messages SET blocks = $2 WHERE id = $1")
                    .bind(message_id.as_str())
                    .bind(Json(blocks))
                    .execute(&mut *conn)
                    .await
                    .map_err(tenant::unavailable)?;
                Ok(())
            })
        })
        .await
    }
}
