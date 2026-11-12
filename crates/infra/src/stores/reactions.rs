//! Emoji reactions on messages (REQ-008).
//!
//! One row per (message, person, emoji). The store answers one RPC: add or remove the caller's
//! reaction, then return the truth for the message so the client does not count.

use boma_app::{
    AppError,
    ports::{ReactionAction, ReactionStore},
};
use boma_domain::{MessageId, Reaction, TenantContext, UserId};
use sqlx::PgPool;

use super::rows::{ReactionAttributionRow, ReactionRow};
use crate::tenant;

/// The Postgres adapter for [`ReactionStore`].
#[derive(Debug, Clone)]
pub struct PostgresReactionStore {
    pool: PgPool,
}

impl PostgresReactionStore {
    /// Builds the store on a pool.
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl ReactionStore for PostgresReactionStore {
    async fn react(
        &self,
        context: &TenantContext,
        message_id: &MessageId,
        user_id: &UserId,
        emoji: &str,
        action: ReactionAction,
    ) -> Result<Vec<Reaction>, AppError> {
        if !valid_emoji(emoji) {
            return Err(AppError::Rule(boma_domain::DomainError::InvalidName(
                "the emoji is not valid".to_owned(),
            )));
        }
        let message_id_key = message_id.as_str().to_owned();
        let user_id_key = user_id.as_str().to_owned();
        let emoji = emoji.trim().to_owned();
        tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                // The message must exist inside this tenant. RLS hides other tenants' rows, so a
                // miss is simply "no such message for this caller".
                let exists = sqlx::query("SELECT 1 FROM messages WHERE id = $1")
                    .bind(&message_id_key)
                    .fetch_optional(&mut *conn)
                    .await
                    .map_err(tenant::unavailable)?;
                if exists.is_none() {
                    return Err(AppError::NotFound);
                }

                match action {
                    ReactionAction::Add => {
                        // The row is either created or left alone: the primary key keeps the
                        // same person from stacking two of the same emoji on one message.
                        sqlx::query(
                            "INSERT INTO message_reactions (thread_id, message_id, space_id, user_id, emoji, created_at)
                             SELECT thread_id, id, space_id, $1, $2, now()
                             FROM messages
                             WHERE id = $3
                             ON CONFLICT (message_id, user_id, emoji) DO NOTHING",
                        )
                        .bind(&user_id_key)
                        .bind(&emoji)
                        .bind(&message_id_key)
                        .execute(&mut *conn)
                        .await
                        .map_err(tenant::unavailable)?;
                    }
                    ReactionAction::Remove => {
                        sqlx::query(
                            "DELETE FROM message_reactions
                             WHERE message_id = $1 AND user_id = $2 AND emoji = $3",
                        )
                        .bind(&message_id_key)
                        .bind(&user_id_key)
                        .bind(&emoji)
                        .execute(&mut *conn)
                        .await
                        .map_err(tenant::unavailable)?;
                    }
                }

                // The full truth for the message: one row per emoji with the count and whether
                // the caller reacted with it.
                let rows = sqlx::query_as::<_, ReactionRow>(
                    "SELECT emoji, count(*) AS count,
                            bool_or(user_id = $2) AS reacted
                     FROM message_reactions
                     WHERE message_id = $1
                     GROUP BY emoji
                     ORDER BY emoji",
                )
                .bind(message_id_key.as_str())
                .bind(user_id_key.as_str())
                .fetch_all(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;

                rows.into_iter()
                    .map(|row| {
                        Ok(Reaction {
                            emoji: row.emoji,
                            count: u32::try_from(row.count).unwrap_or(u32::MAX),
                            reacted: row.reacted,
                        })
                    })
                    .collect()
            })
        })
        .await
    }

    async fn list_for_messages(
        &self,
        context: &TenantContext,
        message_ids: &[MessageId],
    ) -> Result<std::collections::HashMap<MessageId, Vec<Reaction>>, AppError> {
        if message_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }
        let caller = context.user_id().map(|id| id.as_str().to_owned());
        let keys: Vec<String> = message_ids
            .iter()
            .map(|id| id.as_str().to_owned())
            .collect();
        tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                // One pass: the reactions of a page never outgrow a page-full of rows.
                let rows = sqlx::query_as::<_, ReactionAttributionRow>(
                    "SELECT message_id, emoji, user_id FROM message_reactions
                     WHERE message_id = ANY($1)",
                )
                .bind(&keys)
                .fetch_all(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;

                let mut grouped: std::collections::HashMap<
                    String,
                    std::collections::BTreeMap<String, (u32, bool)>,
                > = std::collections::HashMap::new();
                for row in rows {
                    let entry = grouped
                        .entry(row.message_id)
                        .or_default()
                        .entry(row.emoji)
                        .or_insert((0, false));
                    entry.0 += 1;
                    if caller.as_deref().is_some_and(|id| id == row.user_id) {
                        entry.1 = true;
                    }
                }
                grouped
                    .into_iter()
                    .map(|(message_id, emoji_counts)| {
                        let reaction_of =
                            |(emoji, (count, reacted)): (String, (u32, bool))| Reaction {
                                emoji,
                                count,
                                reacted,
                            };
                        Ok::<_, AppError>((
                            MessageId::new(message_id)?,
                            emoji_counts
                                .into_iter()
                                .map(reaction_of)
                                .collect::<Vec<_>>(),
                        ))
                    })
                    .collect()
            })
        })
        .await
    }
}

/// Whether the string is a plausible emoji for this product.
fn valid_emoji(emoji: &str) -> bool {
    let trimmed = emoji.trim();
    if trimmed.is_empty() || trimmed.chars().count() > 16 {
        return false;
    }
    trimmed.chars().all(|c| !c.is_control())
}
