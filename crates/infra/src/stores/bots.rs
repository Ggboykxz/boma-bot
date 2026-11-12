//! Bots: the agents a member talks to.
//!
//! A bot and its thread belong together: a bot without a conversation is a half-created bot, so
//! the two rows are written in one transaction (REQ-004, REQ-042).

use boma_app::{AppError, ports::BotStore};
use boma_domain::{Bot, Thread};
use sqlx::PgPool;

use super::rows::{self, BotRow};
use crate::tenant;

/// The Postgres adapter for [`BotStore`].
#[derive(Debug, Clone)]
pub struct PostgresBotStore {
    pool: PgPool,
}

impl PostgresBotStore {
    /// Builds the store on a pool.
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl BotStore for PostgresBotStore {
    async fn create_with_thread(
        &self,
        bot: boma_app::ports::NewBot,
        thread: boma_app::ports::NewThread,
    ) -> Result<(Bot, Thread), AppError> {
        let space_id = bot.space_id.clone();

        tenant::in_space(&self.pool, &space_id, |conn| {
            Box::pin(async move {
                // The bot first: the thread points at it.
                sqlx::query(
                    "INSERT INTO bots (id, space_id, user_id, name, description, color, thread_id, created_at)
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8::timestamptz)",
                )
                .bind(bot.id.as_str())
                .bind(bot.space_id.as_str())
                .bind(bot.user_id.as_str())
                .bind(&bot.name)
                .bind(&bot.description)
                .bind(&bot.color)
                .bind(thread.id.as_str())
                .bind(&bot.created_at)
                .execute(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;

                sqlx::query(
                    "INSERT INTO threads (id, space_id, bot_id, user_id, created_at)
                     VALUES ($1, $2, $3, $4, $5::timestamptz)",
                )
                .bind(thread.id.as_str())
                .bind(thread.space_id.as_str())
                .bind(thread.bot_id.as_str())
                .bind(thread.user_id.as_str())
                .bind(&thread.created_at)
                .execute(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;

                Ok((
                    Bot {
                        id: bot.id,
                        space_id: bot.space_id,
                        user_id: bot.user_id,
                        name: bot.name,
                        description: bot.description,
                        color: bot.color,
                        thread_id: thread.id.clone(),
                        archived_at: None,
                        created_at: bot.created_at.clone(),
                    },
                    Thread {
                        id: thread.id,
                        space_id: thread.space_id,
                        bot_id: thread.bot_id,
                        user_id: thread.user_id,
                        next_message_seq: 1,
                        next_event_seq: 1,
                        created_at: thread.created_at,
                    },
                ))
            })
        })
        .await
    }

    async fn find(
        &self,
        context: &boma_domain::TenantContext,
        id: &boma_domain::BotId,
    ) -> Result<Option<Bot>, AppError> {
        let id = id.as_str().to_owned();
        let row = tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                sqlx::query_as::<_, BotRow>(
                    "SELECT id, space_id, user_id, name, description, color, thread_id,
                            boma.rfc3339(archived_at) AS archived_at,
                            boma.rfc3339(created_at) AS created_at
                     FROM bots
                     WHERE id = $1",
                )
                .bind(id.as_str())
                .fetch_optional(&mut *conn)
                .await
                .map_err(tenant::unavailable)
            })
        })
        .await?;

        row.map(rows::bot).transpose()
    }
}
