//! Inbound mail for bots (REQ-047): the sender rules and the held queue.

use boma_app::{
    AppError,
    ports::{HeldMail, MailRule, MailStore, NewHeldMail},
};
use boma_domain::{SpaceId, TenantContext};
use sqlx::PgPool;

use crate::tenant;

/// The Postgres adapter for the mail store.
#[derive(Debug, Clone)]
pub struct PostgresMailStore {
    pool: PgPool,
}

impl PostgresMailStore {
    /// Builds the store on a pool.
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl MailStore for PostgresMailStore {
    async fn rule(
        &self,
        context: &TenantContext,
        bot_id: &str,
    ) -> Result<Option<MailRule>, AppError> {
        let bot_id = bot_id.to_owned();
        let row = tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                sqlx::query_as::<_, (String, String, serde_json::Value, String)>(
                    "SELECT bot_id, policy, allowed_senders, boma.rfc3339(updated_at)
                       FROM bot_mail_rules WHERE bot_id = $1",
                )
                .bind(&bot_id)
                .fetch_optional(&mut *conn)
                .await
                .map_err(tenant::unavailable)
            })
        })
        .await?;
        Ok(
            row.map(|(bot_id, policy, allowed_senders, updated_at)| MailRule {
                bot_id,
                policy,
                allowed_senders: allowed_senders
                    .as_array()
                    .map(|list| {
                        list.iter()
                            .filter_map(|entry| entry.as_str().map(str::to_owned))
                            .collect()
                    })
                    .unwrap_or_default(),
                updated_at,
            }),
        )
    }

    async fn set_rule(
        &self,
        context: &TenantContext,
        bot_id: &str,
        policy: &str,
        allowed_senders: &[String],
    ) -> Result<MailRule, AppError> {
        let space_id = match context {
            TenantContext::Member { space_id, .. } => space_id.as_str().to_owned(),
            TenantContext::System { .. } => return Err(AppError::NotFound),
        };
        let bot_id_owned = bot_id.to_owned();
        let policy = policy.to_owned();
        let senders = serde_json::Value::Array(
            allowed_senders
                .iter()
                .cloned()
                .map(serde_json::Value::String)
                .collect(),
        );
        tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                sqlx::query(
                    "INSERT INTO bot_mail_rules (bot_id, space_id, policy, allowed_senders, updated_at)
                     VALUES ($1, $2, $3, $4::jsonb, now())
                     ON CONFLICT (bot_id) DO UPDATE
                       SET policy = EXCLUDED.policy,
                           allowed_senders = EXCLUDED.allowed_senders,
                           updated_at = now()",
                )
                .bind(&bot_id_owned)
                .bind(&space_id)
                .bind(&policy)
                .bind(&senders)
                .execute(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
                Ok(())
            })
        })
        .await?;
        self.rule(context, bot_id)
            .await?
            .ok_or(AppError::Unavailable)
    }

    async fn hold(&self, context: &TenantContext, held: NewHeldMail) -> Result<(), AppError> {
        let space_id = match context {
            TenantContext::Member { space_id, .. } => space_id.as_str().to_owned(),
            TenantContext::System { .. } => {
                // The inbound relay acts for the space, not for a member.
                held.space_id.clone()
            }
        };
        tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                sqlx::query(
                    "INSERT INTO held_mail (id, space_id, bot_id, sender, subject, body, reason, created_at)
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8::timestamptz)",
                )
                .bind(&held.id)
                .bind(&space_id)
                .bind(&held.bot_id)
                .bind(&held.sender)
                .bind(&held.subject)
                .bind(&held.body)
                .bind(&held.reason)
                .bind(&held.created_at)
                .execute(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
                Ok(())
            })
        })
        .await
    }

    async fn held_list(
        &self,
        context: &TenantContext,
        bot_id: &str,
    ) -> Result<Vec<HeldMail>, AppError> {
        let bot_id = bot_id.to_owned();
        let rows = tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                sqlx::query_as::<_, (String, String, String, String, String, String, String)>(
                    "SELECT id, bot_id, sender, subject, body, reason, boma.rfc3339(created_at)
                       FROM held_mail
                      WHERE bot_id = $1
                      ORDER BY created_at DESC
                      LIMIT 50",
                )
                .bind(&bot_id)
                .fetch_all(&mut *conn)
                .await
                .map_err(tenant::unavailable)
            })
        })
        .await?;
        Ok(rows
            .into_iter()
            .map(
                |(id, bot_id, sender, subject, body, reason, created_at)| HeldMail {
                    id,
                    bot_id,
                    sender,
                    subject,
                    body,
                    reason,
                    created_at,
                },
            )
            .collect())
    }

    async fn take_held(
        &self,
        context: &TenantContext,
        held_id: &str,
    ) -> Result<Option<HeldMail>, AppError> {
        let held_id = held_id.to_owned();
        let row = tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                sqlx::query_as::<_, (String, String, String, String, String, String, String)>(
                    "DELETE FROM held_mail WHERE id = $1
                  RETURNING id, bot_id, sender, subject, body, reason, boma.rfc3339(created_at)",
                )
                .bind(&held_id)
                .fetch_optional(&mut *conn)
                .await
                .map_err(tenant::unavailable)
            })
        })
        .await?;
        Ok(row.map(
            |(id, bot_id, sender, subject, body, reason, created_at)| HeldMail {
                id,
                bot_id,
                sender,
                subject,
                body,
                reason,
                created_at,
            },
        ))
    }

    async fn sender_is_member(
        &self,
        context: &TenantContext,
        space_id: &SpaceId,
        sender: &str,
    ) -> Result<bool, AppError> {
        let space_id = space_id.as_str().to_owned();
        let sender = sender.to_ascii_lowercase();
        let found: Option<bool> = tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                sqlx::query_scalar(
                    "SELECT EXISTS (
                        SELECT 1 FROM memberships m
                          JOIN users u ON u.id = m.user_id
                         WHERE m.space_id = $1 AND lower(u.email) = $2
                     )",
                )
                .bind(&space_id)
                .bind(&sender)
                .fetch_one(&mut *conn)
                .await
                .map_err(tenant::unavailable)
            })
        })
        .await?;
        Ok(found.unwrap_or(false))
    }
}
