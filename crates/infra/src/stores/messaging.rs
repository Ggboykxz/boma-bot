//! Messaging installations and their external threads (REQ-048).
//!
//! An installation seals its platform credential under the space's data key exactly like a bot
//! secret. The ingress resolves an installation by id inside a system context; a removed
//! installation routes to nothing. External threads live in `threads` with kind `external`, so
//! messages, events, paging and unread counts work unchanged (REQ-005).

use boma_app::{
    AppError,
    ports::{InstallState, MessagingInstallation, MessagingStore, NewMessagingInstallation},
};
use boma_domain::{BotId, ChannelPlatform, SpaceId, TenantContext, Thread, ThreadId, UserId};
use sqlx::PgPool;

use super::rows;
use crate::{secret_box::SecretBox, tenant};

/// The Postgres adapter for [`MessagingStore`].
#[derive(Debug, Clone)]
pub struct PostgresMessagingStore {
    pool: PgPool,
    secrets: SecretBox,
}

type InstallationRow = (
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    Option<String>,
    bool,
    String,
    String,
    String,
);

fn parse_platform(value: &str) -> ChannelPlatform {
    match value {
        "slack" => ChannelPlatform::Slack,
        "telegram" => ChannelPlatform::Telegram,
        _ => ChannelPlatform::Generic,
    }
}

fn installation(row: InstallationRow) -> MessagingInstallation {
    let (
        id,
        space_id,
        bot_id,
        installed_by,
        platform,
        workspace_id,
        workspace_name,
        reply_url,
        allow_private,
        status,
        created_at,
        updated_at,
    ) = row;
    MessagingInstallation {
        id,
        space_id,
        bot_id,
        installed_by,
        platform: parse_platform(&platform),
        workspace_id,
        workspace_name,
        reply_url,
        allow_private,
        status,
        created_at,
        updated_at,
    }
}

impl PostgresMessagingStore {
    /// Builds the store on a pool and the vault.
    #[must_use]
    pub const fn new(pool: PgPool, secrets: SecretBox) -> Self {
        Self { pool, secrets }
    }

    /// Finds one installation by id, any status (the caller authorizes).
    async fn installation_by_id(
        &self,
        context: &TenantContext,
        installation_id: &str,
    ) -> Result<Option<MessagingInstallation>, AppError> {
        let installation_id = installation_id.to_owned();
        let row = tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                sqlx::query_as::<_, InstallationRow>(
                    "SELECT id, space_id, bot_id, installed_by, platform, workspace_id, workspace_name, reply_url,
                            allow_private, status,
                            boma.rfc3339(created_at) AS created_at, boma.rfc3339(updated_at) AS updated_at
                       FROM messaging_installations WHERE id = $1",
                )
                .bind(&installation_id)
                .fetch_optional(&mut *conn)
                .await
                .map_err(tenant::unavailable)
            })
        })
        .await?;
        Ok(row.map(installation))
    }

    /// Reveals the sealed credential of one installation (no tenant filter; the caller authorizes).
    async fn sealed_credential(
        &self,
        context: &TenantContext,
        installation_id: &str,
        _webhook: bool,
    ) -> Result<Option<(String, Vec<u8>, Vec<u8>, Option<Vec<u8>>, Option<Vec<u8>>)>, AppError>
    {
        let installation_id = installation_id.to_owned();
        let row = tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                sqlx::query_as(
                    "SELECT space_id, token_ciphertext, token_nonce, webhook_ciphertext, webhook_nonce
                       FROM messaging_installations WHERE id = $1",
                )
                .bind(&installation_id)
                .fetch_optional(&mut *conn)
                .await
                .map_err(tenant::unavailable)
            })
        })
        .await?;
        Ok(row)
    }

    async fn reveal(
        &self,
        context: &TenantContext,
        installation_id: &str,
        webhook: bool,
    ) -> Result<Option<secrecy::SecretString>, AppError> {
        let Some((space_id, token_ciphertext, token_nonce, webhook_ciphertext, webhook_nonce)) =
            self.sealed_credential(context, installation_id, webhook)
                .await?
        else {
            return Ok(None);
        };
        let wrapped: Option<(Vec<u8>, Vec<u8>)> =
            tenant::with_tenant(&self.pool, context, |conn| {
                Box::pin(async move {
                    sqlx::query_as("SELECT wrapped_key, nonce FROM space_keys WHERE space_id = $1")
                        .bind(&space_id)
                        .fetch_optional(&mut *conn)
                        .await
                        .map_err(tenant::unavailable)
                })
            })
            .await?;
        let Some((wrapped_key, nonce)) = wrapped else {
            return Err(AppError::Unavailable);
        };
        let dek = self
            .secrets
            .unwrap_data_key(&boma_app::ports::SealedSecret {
                ciphertext: wrapped_key,
                nonce,
            })?;
        let (ciphertext, nonce) = if webhook {
            match (webhook_ciphertext, webhook_nonce) {
                (Some(ciphertext), Some(nonce)) => (ciphertext, nonce),
                _ => return Ok(None),
            }
        } else {
            (token_ciphertext, token_nonce)
        };
        Ok(Some(SecretBox::open_with_data_key(
            &dek,
            &boma_app::ports::SealedSecret { ciphertext, nonce },
        )?))
    }

    async fn thread(
        &self,
        context: &TenantContext,
        thread_id: &str,
    ) -> Result<Option<Thread>, AppError> {
        let thread_id = thread_id.to_owned();
        let row = tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                sqlx::query_as::<_, rows::ThreadRow>(
                    "SELECT id, space_id, bot_id, user_id, next_message_seq, next_event_seq,
                            boma.rfc3339(created_at) AS created_at, kind,
                            external_platform, external_workspace_id, external_conversation_id
                       FROM threads
                      WHERE id = $1",
                )
                .bind(&thread_id)
                .fetch_optional(&mut *conn)
                .await
                .map_err(tenant::unavailable)
            })
        })
        .await?;
        row.map(rows::thread).transpose()
    }
}

#[async_trait::async_trait]
impl MessagingStore for PostgresMessagingStore {
    async fn create(
        &self,
        context: &TenantContext,
        installation: NewMessagingInstallation,
    ) -> Result<MessagingInstallation, AppError> {
        let installed_id = installation.id.clone();
        let space_id = installation.space_id.as_str().to_owned();
        let bot_id = installation.bot_id.as_str().to_owned();
        let platform = installation.platform;
        let workspace_id = installation.workspace_id.clone();
        let secrets = self.secrets.clone();
        let workspace_name = installation.workspace_name.clone();
        let reply_url = installation.reply_url.clone();
        let allow_private = installation.allow_private;
        let created_at = installation.created_at.clone();
        let token = installation.token.clone();
        let webhook_secret = installation.webhook_secret.clone();
        let installed_by = installation.installed_by.as_str().to_owned();
        tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                let owner: Option<String> = sqlx::query_scalar(
                    "SELECT user_id FROM bots WHERE id = $1 AND archived_at IS NULL",
                )
                .bind(&bot_id)
                .fetch_optional(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
                if owner.is_none() {
                    return Err(AppError::NotFound);
                }
                let dek = Self::unwrap_or_fresh_in(&secrets, conn, &space_id).await?;
                let token_sealed = SecretBox::seal_with_data_key(&dek, &token)?;
                let webhook_sealed = webhook_secret
                    .as_ref()
                    .map(|secret| SecretBox::seal_with_data_key(&dek, secret))
                    .transpose()?;
                sqlx::query(
                    "INSERT INTO messaging_installations
                        (id, space_id, bot_id, installed_by, platform, workspace_id, workspace_name,
                         token_ciphertext, token_nonce, webhook_ciphertext, webhook_nonce,
                         reply_url, allow_private, status, created_at, updated_at)
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, 'active',
                             $14::timestamptz, $14::timestamptz)
                     ON CONFLICT (platform, workspace_id) DO NOTHING",
                )
                .bind(&installed_id)
                .bind(&space_id)
                .bind(&bot_id)
                .bind(&installed_by)
                .bind(platform.as_str())
                .bind(&workspace_id)
                .bind(&workspace_name)
                .bind(&token_sealed.ciphertext)
                .bind(&token_sealed.nonce)
                .bind(webhook_sealed.as_ref().map(|sealed| sealed.ciphertext.as_slice()))
                .bind(webhook_sealed.as_ref().map(|sealed| sealed.nonce.as_slice()))
                .bind(reply_url.as_deref())
                .bind(allow_private)
                .bind(&created_at)
                .execute(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
                let exists: Option<String> = sqlx::query_scalar(
                    "SELECT id FROM messaging_installations WHERE platform = $1 AND workspace_id = $2",
                )
                .bind(platform.as_str())
                .bind(&workspace_id)
                .fetch_optional(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
                if exists.as_deref() != Some(installed_id.as_str()) {
                    return Err(AppError::Conflict);
                }
                Ok(())
            })
        })
        .await?;
        self.find(context, &installation.id)
            .await?
            .ok_or(AppError::Unavailable)
    }

    async fn find(
        &self,
        context: &TenantContext,
        installation_id: &str,
    ) -> Result<Option<MessagingInstallation>, AppError> {
        self.installation_by_id(context, installation_id).await
    }

    async fn find_active_by_platform_workspace(
        &self,
        context: &TenantContext,
        platform: ChannelPlatform,
        workspace_id: &str,
    ) -> Result<Option<MessagingInstallation>, AppError> {
        let workspace_id = workspace_id.to_owned();
        let id: Option<String> = tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                sqlx::query_scalar(
                    "SELECT id FROM messaging_installations
                     WHERE platform = $1 AND workspace_id = $2 AND status = 'active'",
                )
                .bind(platform.as_str())
                .bind(&workspace_id)
                .fetch_optional(&mut *conn)
                .await
                .map_err(tenant::unavailable)
            })
        })
        .await?;
        match id {
            Some(id) => self.find_active(context, &id).await,
            None => Ok(None),
        }
    }

    async fn find_active(
        &self,
        context: &TenantContext,
        installation_id: &str,
    ) -> Result<Option<MessagingInstallation>, AppError> {
        Ok(self
            .find(context, installation_id)
            .await?
            .filter(|installation| installation.status == "active"))
    }

    async fn list(&self, context: &TenantContext) -> Result<Vec<MessagingInstallation>, AppError> {
        let rows = tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                sqlx::query_as::<_, InstallationRow>(
                    "SELECT id, space_id, bot_id, installed_by, platform, workspace_id, workspace_name, reply_url,
                            allow_private, status,
                            boma.rfc3339(created_at) AS created_at, boma.rfc3339(updated_at) AS updated_at
                       FROM messaging_installations
                      ORDER BY created_at DESC LIMIT 50",
                )
                .fetch_all(&mut *conn)
                .await
                .map_err(tenant::unavailable)
            })
        })
        .await?;
        Ok(rows.into_iter().map(installation).collect())
    }

    async fn mark_removed(
        &self,
        context: &TenantContext,
        installation_id: &str,
    ) -> Result<bool, AppError> {
        let installation_id = installation_id.to_owned();
        let updated = tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                sqlx::query(
                    "UPDATE messaging_installations
                        SET status = 'removed', updated_at = now()
                      WHERE id = $1 AND status = 'active'",
                )
                .bind(&installation_id)
                .execute(&mut *conn)
                .await
                .map_err(tenant::unavailable)
            })
        })
        .await?;
        Ok(updated.rows_affected() > 0)
    }

    async fn reveal_token(
        &self,
        context: &TenantContext,
        installation_id: &str,
    ) -> Result<Option<secrecy::SecretString>, AppError> {
        self.reveal(context, installation_id, false).await
    }

    async fn reveal_webhook_secret(
        &self,
        context: &TenantContext,
        installation_id: &str,
    ) -> Result<Option<secrecy::SecretString>, AppError> {
        self.reveal(context, installation_id, true).await
    }

    async fn resolve_external_thread(
        &self,
        context: &TenantContext,
        installation_id: &str,
        conversation_id: &str,
    ) -> Result<Thread, AppError> {
        let installation = self
            .find_active(context, installation_id)
            .await?
            .ok_or(AppError::NotFound)?;
        let platform = installation.platform;
        let workspace_id = installation.workspace_id.clone();
        let conversation_id = conversation_id.to_owned();
        let bot_id = BotId::new(installation.bot_id.clone()).map_err(|_| AppError::NotFound)?;
        let installation_id = installation_id.to_owned();
        let thread_id = tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                let existing: Option<String> = sqlx::query_scalar(
                    "SELECT id FROM threads
                     WHERE external_platform = $1 AND external_workspace_id = $2
                       AND external_conversation_id = $3 AND kind = 'external'",
                )
                .bind(platform.as_str())
                .bind(&workspace_id)
                .bind(&conversation_id)
                .fetch_optional(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
                if let Some(id) = existing {
                    return Ok(id);
                }
                let owner: Option<(String, String)> = sqlx::query_as(
                    "SELECT user_id, space_id FROM bots WHERE id = $1 AND archived_at IS NULL",
                )
                .bind(bot_id.as_str())
                .fetch_optional(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
                let Some((owner, space_id)) = owner else {
                    return Err(AppError::NotFound);
                };
                let new_thread = ThreadId::new(format!("thr_{}", uuid::Uuid::new_v4().simple()))
                    .map_err(|_| AppError::Unavailable)?;
                sqlx::query(
                    "INSERT INTO threads
                        (id, space_id, bot_id, user_id, kind, external_platform,
                         external_workspace_id, external_conversation_id, created_at)
                     VALUES ($1, $2, $3, $4, 'external', $5, $6, $7, now())
                     ON CONFLICT DO NOTHING",
                )
                .bind(new_thread.as_str())
                .bind(&space_id)
                .bind(bot_id.as_str())
                .bind(&owner)
                .bind(platform.as_str())
                .bind(&workspace_id)
                .bind(&conversation_id)
                .execute(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
                let id: Option<String> = sqlx::query_scalar(
                    "SELECT id FROM threads
                     WHERE external_platform = $1 AND external_workspace_id = $2
                       AND external_conversation_id = $3 AND kind = 'external'",
                )
                .bind(platform.as_str())
                .bind(&workspace_id)
                .bind(&conversation_id)
                .fetch_optional(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
                id.ok_or(AppError::Unavailable)
            })
        })
        .await?;
        let _ = installation_id;
        self.thread(context, &thread_id)
            .await?
            .ok_or(AppError::NotFound)
    }

    async fn list_external_threads(
        &self,
        context: &TenantContext,
        installation_id: &str,
    ) -> Result<Vec<Thread>, AppError> {
        let installation = self
            .find_active(context, installation_id)
            .await?
            .ok_or(AppError::NotFound)?;
        let rows = tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                sqlx::query_as::<_, rows::ThreadRow>(
                    "SELECT id, space_id, bot_id, user_id, next_message_seq, next_event_seq,
                            boma.rfc3339(created_at) AS created_at, kind,
                            external_platform, external_workspace_id, external_conversation_id
                       FROM threads
                      WHERE kind = 'external' AND external_platform = $1
                        AND external_workspace_id = $2
                      ORDER BY created_at DESC
                      LIMIT 50",
                )
                .bind(installation.platform.as_str())
                .bind(&installation.workspace_id)
                .fetch_all(&mut *conn)
                .await
                .map_err(tenant::unavailable)
            })
        })
        .await?;
        rows.into_iter().map(rows::thread).collect()
    }

    async fn store_install_state(
        &self,
        context: &TenantContext,
        state: &str,
        bot_id: &BotId,
        expires_at: &str,
    ) -> Result<(), AppError> {
        let (space_id, user_id) = match context {
            TenantContext::Member { space_id, user_id } => {
                (space_id.as_str().to_owned(), user_id.as_str().to_owned())
            }
            TenantContext::System { .. } => return Err(AppError::NotFound),
        };
        let state = state.to_owned();
        let expires_at = expires_at.to_owned();
        let bot_id = bot_id.as_str().to_owned();
        tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                sqlx::query(
                    "INSERT INTO messaging_install_states (state, space_id, bot_id, user_id, expires_at, created_at)
                     VALUES ($1, $2, $3, $4, $5::timestamptz, now())",
                )
                .bind(&state)
                .bind(&space_id)
                .bind(&bot_id)
                .bind(&user_id)
                .bind(&expires_at)
                .execute(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
                Ok(())
            })
        })
        .await
    }

    async fn take_install_state(
        &self,
        context: &TenantContext,
        state: &str,
    ) -> Result<Option<InstallState>, AppError> {
        let state = state.to_owned();
        let row: Option<(String, String, String)> =
            tenant::with_tenant(&self.pool, context, |conn| {
                Box::pin(async move {
                    sqlx::query_as(
                        "DELETE FROM messaging_install_states
                     WHERE state = $1 AND expires_at > now()
                 RETURNING space_id, bot_id, user_id",
                    )
                    .bind(&state)
                    .fetch_optional(&mut *conn)
                    .await
                    .map_err(tenant::unavailable)
                })
            })
            .await?;
        row.map(|(space_id, bot_id, user_id)| {
            Ok(InstallState {
                space_id: SpaceId::new(space_id).map_err(|_| AppError::Unavailable)?,
                bot_id: BotId::new(bot_id).map_err(|_| AppError::Unavailable)?,
                user_id: UserId::new(user_id).map_err(|_| AppError::Unavailable)?,
            })
        })
        .transpose()
    }
}

impl PostgresMessagingStore {
    async fn unwrap_or_fresh_in(
        secrets: &SecretBox,
        conn: &mut sqlx::PgConnection,
        space_id: &str,
    ) -> Result<[u8; 32], AppError> {
        let wrapped: Option<(Vec<u8>, Vec<u8>)> =
            sqlx::query_as("SELECT wrapped_key, nonce FROM space_keys WHERE space_id = $1")
                .bind(space_id)
                .fetch_optional(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
        if let Some((wrapped_key, nonce)) = wrapped {
            return secrets.unwrap_data_key(&boma_app::ports::SealedSecret {
                ciphertext: wrapped_key,
                nonce,
            });
        }
        let fresh = SecretBox::fresh_data_key();
        let sealed = secrets.wrap_data_key(&fresh)?;
        sqlx::query(
            "INSERT INTO space_keys (space_id, wrapped_key, nonce)
             VALUES ($1, $2, $3) ON CONFLICT (space_id) DO NOTHING",
        )
        .bind(space_id)
        .bind(&sealed.ciphertext)
        .bind(&sealed.nonce)
        .execute(&mut *conn)
        .await
        .map_err(tenant::unavailable)?;
        let survivor: Option<(Vec<u8>, Vec<u8>)> =
            sqlx::query_as("SELECT wrapped_key, nonce FROM space_keys WHERE space_id = $1")
                .bind(space_id)
                .fetch_optional(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
        let Some((wrapped_key, nonce)) = survivor else {
            return Err(AppError::Unavailable);
        };
        secrets.unwrap_data_key(&boma_app::ports::SealedSecret {
            ciphertext: wrapped_key,
            nonce,
        })
    }
}
