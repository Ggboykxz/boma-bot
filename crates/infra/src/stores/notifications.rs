//! Web Push subscriptions and the per-bot policy (REQ-064).

use async_trait::async_trait;
use boma_app::{AppError, ports};
use boma_domain::{BotId, SpaceId, UserId};
use sqlx::PgPool;

use crate::tenant;

/// The Postgres adapter for [`ports::NotificationStore`].
#[derive(Debug, Clone)]
pub struct PostgresNotificationStore {
    pool: PgPool,
}

impl PostgresNotificationStore {
    /// Builds the store on a pool.
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(sqlx::FromRow)]
struct SubscriptionRow {
    id: String,
    space_id: String,
    user_id: String,
    endpoint: String,
    p256dh: String,
    auth_secret: String,
    user_agent: Option<String>,
    fail_count: i32,
    created_at: String,
    updated_at: String,
}

fn subscription_into_domain(
    row: SubscriptionRow,
) -> Result<boma_domain::notification::NotificationSubscription, AppError> {
    Ok(boma_domain::notification::NotificationSubscription {
        id: row.id,
        space_id: SpaceId::new(row.space_id).map_err(|_| AppError::Unavailable)?,
        user_id: UserId::new(row.user_id).map_err(|_| AppError::Unavailable)?,
        endpoint: row.endpoint,
        p256dh: row.p256dh,
        auth: row.auth_secret,
        user_agent: row.user_agent,
        fail_count: u32::try_from(row.fail_count).unwrap_or(0),
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

#[derive(sqlx::FromRow)]
#[allow(clippy::struct_excessive_bools)] // four independent wiring switches; read them separately
struct PolicyRow {
    bot_id: String,
    notify_waiting: bool,
    notify_completed: bool,
    daily_cap: i32,
    destination_slack_webhook: bool,
    destination_teams_webhook: bool,
    created_at: String,
    updated_at: String,
}

fn policy_into_domain(
    row: PolicyRow,
) -> Result<boma_domain::notification::NotificationPolicy, AppError> {
    Ok(boma_domain::notification::NotificationPolicy {
        bot_id: BotId::new(row.bot_id).map_err(|_| AppError::Unavailable)?,
        notify_waiting: row.notify_waiting,
        notify_completed: row.notify_completed,
        daily_cap: u32::try_from(row.daily_cap).unwrap_or(0),
        destination_slack_webhook: row.destination_slack_webhook,
        destination_teams_webhook: row.destination_teams_webhook,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

#[async_trait]
impl ports::NotificationStore for PostgresNotificationStore {
    async fn subscribe(
        &self,
        subscription: boma_domain::notification::NewNotificationSubscription,
    ) -> Result<boma_domain::notification::NotificationSubscription, AppError> {
        let space_id = subscription.space_id.clone();
        tenant::in_space(&self.pool, &space_id, |conn| {
            Box::pin(async move {
                sqlx::query_as::<_, SubscriptionRow>(
                    "INSERT INTO notification_subscriptions (id, space_id, user_id, endpoint, p256dh, auth_secret, user_agent, created_at, updated_at)
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8::timestamptz, $8::timestamptz)
                     ON CONFLICT (space_id, user_id, endpoint) DO UPDATE
                     SET user_agent = EXCLUDED.user_agent,
                         p256dh = EXCLUDED.p256dh,
                         auth_secret = EXCLUDED.auth_secret,
                         fail_count = 0,
                         updated_at = EXCLUDED.updated_at
                     RETURNING id, space_id, user_id, endpoint, p256dh, auth_secret, user_agent, fail_count,
                               boma.rfc3339(created_at) AS created_at,
                               boma.rfc3339(updated_at) AS updated_at",
                )
                .bind(&subscription.id)
                .bind(subscription.space_id.as_str())
                .bind(subscription.user_id.as_str())
                .bind(&subscription.endpoint)
                .bind(&subscription.p256dh)
                .bind(&subscription.auth)
                .bind(subscription.user_agent.as_deref())
                .bind(&subscription.created_at)
                .fetch_one(&mut *conn)
                .await
                .map_err(tenant::unavailable)
                .and_then(subscription_into_domain)
            })
        })
        .await
    }

    async fn unsubscribe(
        &self,
        context: &boma_domain::TenantContext,
        endpoint: &str,
    ) -> Result<bool, AppError> {
        let space_id = context.space_id().cloned().ok_or(AppError::NotFound)?;
        let user_id = context.user_id().cloned().ok_or(AppError::Forbidden)?;
        let endpoint = endpoint.to_owned();
        let for_del = space_id.clone();
        let for_del_user = user_id.clone();
        tenant::in_space(&self.pool, &space_id, |conn| {
            Box::pin(async move {
                sqlx::query(
                    "DELETE FROM notification_subscriptions
                      WHERE space_id = $1 AND user_id = $2 AND endpoint = $3",
                )
                .bind(for_del.as_str())
                .bind(for_del_user.as_str())
                .bind(endpoint.as_str())
                .execute(&mut *conn)
                .await
                .map(|result| result.rows_affected() > 0)
                .map_err(tenant::unavailable)
            })
        })
        .await
    }

    async fn list(
        &self,
        context: &boma_domain::TenantContext,
    ) -> Result<Vec<boma_domain::notification::NotificationSubscription>, AppError> {
        let space_id = context.space_id().cloned().ok_or(AppError::NotFound)?;
        let user_id = context.user_id().cloned().ok_or(AppError::Forbidden)?;
        let for_list = space_id.clone();
        let for_list_user = user_id.as_str().to_owned();
        tenant::in_space(&self.pool, &space_id, |conn| {
            Box::pin(async move {
                let rows = sqlx::query_as::<_, SubscriptionRow>(
                    "SELECT id, space_id, user_id, endpoint, p256dh, auth_secret, user_agent, fail_count,
                            boma.rfc3339(created_at) AS created_at,
                            boma.rfc3339(updated_at) AS updated_at
                       FROM notification_subscriptions
                      WHERE space_id = $1 AND user_id = $2
                      ORDER BY updated_at DESC",
                )
                .bind(for_list.as_str())
                .bind(for_list_user.clone())
                .fetch_all(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
                rows.into_iter().map(subscription_into_domain).collect()
            })
        })
        .await
    }

    async fn get_policy(
        &self,
        context: &boma_domain::TenantContext,
        bot_id: &BotId,
    ) -> Result<boma_domain::notification::NotificationPolicy, AppError> {
        let space_id = context.space_id().cloned().ok_or(AppError::NotFound)?;
        let for_policy = space_id.clone();
        let for_policy_bot = bot_id.clone();
        let user_id = context.user_id().cloned().ok_or(AppError::Forbidden)?;
        tenant::in_space(&self.pool, &space_id, |conn| {
            Box::pin(async move {
                let row = sqlx::query_as::<_, PolicyRow>(
                    "SELECT bot_id, notify_waiting, notify_completed, daily_cap,
                            destination_slack_webhook, destination_teams_webhook,
                            boma.rfc3339(created_at) AS created_at,
                            boma.rfc3339(updated_at) AS updated_at
                       FROM notification_policies
                      WHERE space_id = $1 AND user_id = $2 AND bot_id = $3",
                )
                .bind(for_policy.as_str())
                .bind(user_id.as_str())
                .bind(for_policy_bot.as_str())
                .fetch_optional(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
                if let Some(row) = row {
                    policy_into_domain(row)
                } else {
                    let stamp: String = sqlx::query_scalar("SELECT boma.rfc3339(now())")
                        .fetch_one(&mut *conn)
                        .await
                        .map_err(tenant::unavailable)?;
                    Ok(boma_domain::notification::NotificationPolicy::defaults(
                        for_policy_bot.clone(),
                        &stamp,
                    ))
                }
            })
        })
        .await
    }

    async fn set_policy(
        &self,
        context: &boma_domain::TenantContext,
        patch: boma_domain::notification::SetNotificationPolicy,
    ) -> Result<boma_domain::notification::NotificationPolicy, AppError> {
        let space_id = context.space_id().cloned().ok_or(AppError::NotFound)?;
        let for_upsert = space_id.clone();
        let user_id = context.user_id().cloned().ok_or(AppError::Forbidden)?;
        tenant::in_space(&self.pool, &space_id, |conn| {
            Box::pin(async move {
                let row = sqlx::query_as::<_, PolicyRow>(
                    "INSERT INTO notification_policies (space_id, user_id, bot_id, notify_waiting, notify_completed, daily_cap, destination_slack_webhook, destination_teams_webhook, created_at, updated_at)
                     VALUES ($1, $2, $3,
                             COALESCE($4, true), COALESCE($5, false), COALESCE($6, 20),
                             COALESCE($7, false), COALESCE($8, false),
                             now(), now())
                     ON CONFLICT (space_id, user_id, bot_id) DO UPDATE
                     SET notify_waiting = COALESCE($4, notification_policies.notify_waiting),
                         notify_completed = COALESCE($5, notification_policies.notify_completed),
                         daily_cap = COALESCE($6, notification_policies.daily_cap),
                         destination_slack_webhook = COALESCE($7, notification_policies.destination_slack_webhook),
                         destination_teams_webhook = COALESCE($8, notification_policies.destination_teams_webhook),
                         updated_at = now()
                     RETURNING bot_id, notify_waiting, notify_completed, daily_cap, destination_slack_webhook, destination_teams_webhook,
                               boma.rfc3339(created_at) AS created_at,
                               boma.rfc3339(updated_at) AS updated_at",
                )
                .bind(for_upsert.as_str())
                .bind(user_id.as_str())
                .bind(patch.bot_id.as_str())
                .bind(patch.notify_waiting)
                .bind(patch.notify_completed)
                .bind(patch.daily_cap.map(|value| i32::try_from(value).unwrap_or(i32::MAX)))
                .bind(patch.destination_slack_webhook)
                .bind(patch.destination_teams_webhook)
                .fetch_one(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
                policy_into_domain(row)
            })
        })
        .await
    }

    async fn record_send(
        &self,
        context: &boma_domain::TenantContext,
        id: &str,
        bot_id: &BotId,
        kind: boma_domain::notification::NotificationKind,
    ) -> Result<(), AppError> {
        let space_id = context.space_id().cloned().ok_or(AppError::NotFound)?;
        let user_id = context.user_id().cloned().ok_or(AppError::Forbidden)?;
        let id = id.to_owned();
        let for_log = space_id.clone();
        let for_log_user = user_id.as_str().to_owned();
        let for_log_bot = bot_id.as_str().to_owned();
        tenant::in_space(&self.pool, &space_id, |conn| {
            Box::pin(async move {
                sqlx::query(
                    "INSERT INTO notification_log (id, space_id, user_id, bot_id, kind)
                     VALUES ($1, $2, $3, $4, $5)",
                )
                .bind(id.as_str())
                .bind(for_log.as_str())
                .bind(for_log_user.clone())
                .bind(for_log_bot.clone())
                .bind(kind.as_str())
                .execute(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
                Ok(())
            })
        })
        .await
    }

    async fn count_today(
        &self,
        context: &boma_domain::TenantContext,
        bot_id: &BotId,
    ) -> Result<u64, AppError> {
        let space_id = context.space_id().cloned().ok_or(AppError::NotFound)?;
        let user_id = context.user_id().cloned().ok_or(AppError::Forbidden)?;
        let for_count = space_id.clone();
        let for_count_user = user_id.as_str().to_owned();
        let for_count_bot = bot_id.as_str().to_owned();
        tenant::in_space(&self.pool, &space_id, |conn| {
            Box::pin(async move {
                let row: (i64,) = sqlx::query_as(
                    "SELECT COUNT(*) FROM notification_log
                      WHERE space_id = $1 AND user_id = $2 AND bot_id = $3
                        AND sent_at >= date_trunc('day', now() AT TIME ZONE 'utc') AT TIME ZONE 'utc'",
                )
                .bind(for_count.as_str())
                .bind(for_count_user.clone())
                .bind(for_count_bot.clone())
                .fetch_one(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
                Ok(u64::try_from(row.0).unwrap_or(0))
            })
        })
        .await
    }

    async fn note_failure(
        &self,
        context: &boma_domain::TenantContext,
        id: &str,
        permanent: bool,
    ) -> Result<boma_domain::notification::NotificationFailureOutcome, AppError> {
        let space_id = context.space_id().cloned().ok_or(AppError::NotFound)?;
        let user_id = context.user_id().cloned().ok_or(AppError::Forbidden)?;
        let id_value = id.to_owned();
        let for_failure = space_id.clone();
        let for_failure_user = user_id.as_str().to_owned();
        tenant::in_space(&self.pool, &space_id, |conn| {
            Box::pin(async move {
                // Bump the counter; a permanent failure at three strikes deletes the row.
                let row: Option<(i32,)> = sqlx::query_as(
                    "UPDATE notification_subscriptions
                        SET fail_count = fail_count + 1,
                            updated_at = now()
                      WHERE id = $1 AND space_id = $2 AND user_id = $3
                      RETURNING fail_count",
                )
                .bind(id_value.as_str())
                .bind(for_failure.as_str())
                .bind(for_failure_user.as_str())
                .fetch_optional(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
                let Some((count,)) = row else {
                    return Ok(boma_domain::notification::NotificationFailureOutcome::Ok);
                };
                if permanent && count >= 3 {
                    sqlx::query("DELETE FROM notification_subscriptions WHERE id = $1")
                        .bind(id_value.as_str())
                        .execute(&mut *conn)
                        .await
                        .map_err(tenant::unavailable)?;
                    return Ok(boma_domain::notification::NotificationFailureOutcome::Deleted);
                }
                Ok(
                    boma_domain::notification::NotificationFailureOutcome::Recorded(
                        u32::try_from(count).unwrap_or(0),
                    ),
                )
            })
        })
        .await
    }

    async fn note_success(
        &self,
        context: &boma_domain::TenantContext,
        id: &str,
    ) -> Result<(), AppError> {
        let space_id = context.space_id().cloned().ok_or(AppError::NotFound)?;
        let user_id = context.user_id().cloned().ok_or(AppError::Forbidden)?;
        let id_value = id.to_owned();
        let for_success = space_id.clone();
        let for_success_user = user_id.as_str().to_owned();
        tenant::in_space(&self.pool, &space_id, |conn| {
            Box::pin(async move {
                sqlx::query(
                    "UPDATE notification_subscriptions
                        SET fail_count = 0,
                            updated_at = now()
                      WHERE id = $1 AND space_id = $2 AND user_id = $3",
                )
                .bind(id_value.as_str())
                .bind(for_success.as_str())
                .bind(for_success_user.as_str())
                .execute(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
                Ok(())
            })
        })
        .await
    }
}
