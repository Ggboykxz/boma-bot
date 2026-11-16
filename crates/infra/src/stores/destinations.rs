//! Outgoing notification destinations (REQ-065): the space's Slack/Teams incoming webhooks,
//! sealed under the space's DEK exactly as bot secrets are.

use async_trait::async_trait;
use boma_app::{AppError, ports};
use boma_domain::{SpaceId, UserId};
use sqlx::PgPool;

use crate::{secret_box::SecretBox, stores::secrets::space_dek, tenant};

/// The Postgres adapter for [`ports::NotificationDestinationStore`].
#[derive(Debug, Clone)]
pub struct PostgresNotificationDestinationStore {
    pool: PgPool,
    secrets: SecretBox,
}

impl PostgresNotificationDestinationStore {
    /// Builds the store on a pool and the vault.
    #[must_use]
    pub fn new(pool: PgPool, secrets: SecretBox) -> Self {
        Self { pool, secrets }
    }
}

#[derive(sqlx::FromRow)]
struct DestinationRow {
    id: String,
    space_id: String,
    kind: String,
    label: String,
    ciphertext: Vec<u8>,
    nonce: Vec<u8>,
    created_by: String,
    created_at: String,
}

fn row_into_domain(
    row: DestinationRow,
) -> Result<boma_domain::notification::NotificationDestination, AppError> {
    let kind = match row.kind.as_str() {
        "slack_webhook" => boma_domain::notification::NotificationDestinationKind::SlackWebhook,
        "teams_webhook" => boma_domain::notification::NotificationDestinationKind::TeamsWebhook,
        _ => return Err(AppError::Unavailable),
    };
    Ok(boma_domain::notification::NotificationDestination {
        id: row.id,
        space_id: SpaceId::new(row.space_id).map_err(|_| AppError::Unavailable)?,
        kind,
        label: row.label,
        created_by: UserId::new(row.created_by).map_err(|_| AppError::Unavailable)?,
        created_at: row.created_at,
    })
}

fn kind_str(kind: boma_domain::notification::NotificationDestinationKind) -> &'static str {
    match kind {
        boma_domain::notification::NotificationDestinationKind::SlackWebhook => "slack_webhook",
        boma_domain::notification::NotificationDestinationKind::TeamsWebhook => "teams_webhook",
    }
}

#[async_trait]
impl ports::NotificationDestinationStore for PostgresNotificationDestinationStore {
    async fn create(
        &self,
        context: &boma_domain::TenantContext,
        destination: boma_domain::notification::NewNotificationDestination,
    ) -> Result<boma_domain::notification::NotificationDestination, AppError> {
        let space_id = context.space_id().cloned().ok_or(AppError::NotFound)?;
        let user_id = context.user_id().cloned().ok_or(AppError::Forbidden)?;
        let secrets = self.secrets.clone();
        let for_scope = space_id.clone();
        let for_birds = space_id.clone();
        let for_insert = for_birds.clone();
        let for_struct = for_birds.clone();
        let for_dek_inner = for_birds;
        let pool = self.pool.clone();
        tenant::in_space(&self.pool, &for_scope, |conn| {
            Box::pin(async move {
                let dek = space_dek(&pool, &for_dek_inner, &secrets).await?;
                let webhook = boma_domain::format_webhook(&destination.webhook);
                // check the Slack webhook shape before sealing it — a misshapen URL can
                // never be reached later anyway, but refusing it now keeps a typo from
                // becoming a permanent row nobody can ever deliver.
                if let Some(prefix) = destination.kind.webhook_prefix()
                    && !webhook.starts_with(prefix)
                {
                    return Err(AppError::Rule(boma_domain::DomainError::InvalidName(
                        "the Slack webhook must start with https://hooks.slack.com/".to_owned(),
                    )));
                }
                if !webhook.starts_with("https://") {
                    return Err(AppError::Rule(boma_domain::DomainError::InvalidName(
                        "the webhook must be an https URL".to_owned(),
                    )));
                }
                if webhook.contains('@') {
                    return Err(AppError::Rule(boma_domain::DomainError::InvalidName(
                        "the webhook carries no credentials".to_owned(),
                    )));
                }
                let sealed = SecretBox::seal_with_data_key(&dek, &secrecy::SecretString::from(webhook.clone()))?;
                sqlx::query(
                    "INSERT INTO notification_destinations (id, space_id, kind, label, ciphertext, nonce, created_by, created_at)
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8::timestamptz)",
                )
                .bind(&destination.id)
                .bind(for_insert.as_str())
                .bind(kind_str(destination.kind))
                .bind(&destination.label)
                .bind(&sealed.ciphertext)
                .bind(&sealed.nonce)
                .bind(user_id.as_str())
                .bind(&destination.created_at)
                .execute(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
                Ok(boma_domain::notification::NotificationDestination {
                    id: destination.id,
                    space_id: for_struct,
                    kind: destination.kind,
                    label: destination.label,
                    created_by: destination.created_by,
                    created_at: destination.created_at,
                })
            })
        })
        .await
    }

    async fn list(
        &self,
        context: &boma_domain::TenantContext,
        kind: Option<boma_domain::notification::NotificationDestinationKind>,
    ) -> Result<Vec<boma_domain::notification::NotificationDestination>, AppError> {
        let space_id = context.space_id().cloned().ok_or(AppError::NotFound)?;
        let for_scope = space_id.clone();
        let for_list = space_id.clone();
        tenant::in_space(&self.pool, &for_scope, |conn| {
            Box::pin(async move {
                let kind_value = kind.map(kind_str);
                let rows = if let Some(kind_value) = kind_value {
                    sqlx::query_as::<_, DestinationRow>(
                        "SELECT id, space_id, kind, label, ciphertext, nonce, created_by,
                                boma.rfc3339(created_at) AS created_at
                           FROM notification_destinations
                          WHERE space_id = $1 AND kind = $2
                          ORDER BY created_at DESC",
                    )
                    .bind(for_list.as_str())
                    .bind(kind_value)
                    .fetch_all(&mut *conn)
                    .await
                    .map_err(tenant::unavailable)?
                } else {
                    sqlx::query_as::<_, DestinationRow>(
                        "SELECT id, space_id, kind, label, ciphertext, nonce, created_by,
                                boma.rfc3339(created_at) AS created_at
                           FROM notification_destinations
                          WHERE space_id = $1
                          ORDER BY created_at DESC",
                    )
                    .bind(for_list.as_str())
                    .fetch_all(&mut *conn)
                    .await
                    .map_err(tenant::unavailable)?
                };
                rows.into_iter().map(row_into_domain).collect()
            })
        })
        .await
    }

    async fn delete(
        &self,
        context: &boma_domain::TenantContext,
        id: &str,
    ) -> Result<bool, AppError> {
        let space_id = context.space_id().cloned().ok_or(AppError::NotFound)?;
        let id_value = id.to_owned();
        let for_delete = space_id.clone();
        tenant::in_space(&self.pool, &space_id, |conn| {
            Box::pin(async move {
                let result = sqlx::query(
                    "DELETE FROM notification_destinations WHERE id = $1 AND space_id = $2",
                )
                .bind(id_value.as_str())
                .bind(for_delete.as_str())
                .execute(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
                Ok(result.rows_affected() == 1)
            })
        })
        .await
    }

    async fn destination_webhook(
        &self,
        context: &boma_domain::TenantContext,
        id: &str,
    ) -> Result<secrecy::SecretString, AppError> {
        let space_id = context.space_id().cloned().ok_or(AppError::NotFound)?;
        let secrets = self.secrets.clone();
        let id_value = id.to_owned();
        let for_scope = space_id.clone();
        let for_birds = space_id.clone();
        let for_open = for_birds.clone();
        let for_dek = for_birds;
        let pool = self.pool.clone();
        tenant::in_space(&self.pool, &for_scope, |conn| {
            Box::pin(async move {
                let row = sqlx::query_as::<_, DestinationRow>(
                    "SELECT id, space_id, kind, label, ciphertext, nonce, created_by,
                            boma.rfc3339(created_at) AS created_at
                       FROM notification_destinations
                      WHERE id = $1 AND space_id = $2",
                )
                .bind(id_value.as_str())
                .bind(for_open.as_str())
                .fetch_optional(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
                let row = row.ok_or(AppError::NotFound)?;
                let dek = space_dek(&pool, &for_dek, &secrets).await?;
                SecretBox::open_with_data_key(
                    &dek,
                    &boma_app::ports::SealedSecret {
                        ciphertext: row.ciphertext,
                        nonce: row.nonce,
                    },
                )
            })
        })
        .await
    }
}
