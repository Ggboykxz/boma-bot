//! The account export (REQ-044): one read of everything the person owns.
//!
//! **A system path by design** (read with `is_system()`). The export cannot answer in a
//! single tenant because it crosses all of them; only the exporting person's rows are chosen,
//! and the system reason carries that justification in the calling chain.

use boma_app::{
    AppError,
    ports::{AccountExportStore, ArtifactMeta, ExportBundle, ExportRow},
};
use boma_domain::UserId;
use sqlx::PgPool;

use crate::stores::rows;
use crate::tenant;

/// A row of `account_exports`.
#[derive(Debug, sqlx::FromRow)]
struct ExportRowRaw {
    id: String,
    user_id: String,
    status: String,
    payload_bytes: i64,
    skipped: Vec<String>,
    created_at: String,
}

/// A row of `artifacts`, as needed for the archive.
#[derive(Debug, sqlx::FromRow)]
struct ArtifactMetaRow {
    id: String,
    name: String,
    declared_mime: String,
    mime_type: String,
    size: i64,
    hash: String,
    storage_key: String,
    root_artifact_id: String,
    version: i64,
    created_at: String,
}

/// The Postgres adapter for [`AccountExportStore`].
#[derive(Debug, Clone)]
pub struct PostgresAccountExportStore {
    pool: PgPool,
}

impl PostgresAccountExportStore {
    /// Builds the store on a pool.
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[allow(clippy::too_many_lines)] // the bundle is one SQL transaction; splitting hides the contract
#[async_trait::async_trait]
impl AccountExportStore for PostgresAccountExportStore {
    async fn bundle(&self, user_id: &UserId) -> Result<ExportBundle, AppError> {
        let user_id = user_id.clone();
        tenant::as_system(
            &self.pool,
            boma_domain::SystemReason::AccountExport,
            |conn| {
                Box::pin(async move {
                    let profile = sqlx::query_as::<_, rows::UserRow>(
                        "SELECT id, email, name, email_verified, locale, password_hash,
                                boma.rfc3339(deactivated_at) AS deactivated_at
                           FROM users WHERE id = $1",
                    )
                    .bind(user_id.as_str())
                    .fetch_one(&mut *conn)
                    .await
                    .map_err(tenant::unavailable)?;
                    let profile = rows::user(profile)?;

                    let bots = sqlx::query_as::<_, rows::BotRow>(
                        "SELECT id, space_id, user_id, name, description, color, thread_id,
                                boma.rfc3339(archived_at) AS archived_at,
                                boma.rfc3339(created_at) AS created_at,
                                enabled_tools, model_provider, model_id, model_pinned
                           FROM bots WHERE user_id = $1",
                    )
                    .bind(user_id.as_str())
                    .fetch_all(&mut *conn)
                    .await
                    .map_err(tenant::unavailable)?
                    .into_iter()
                    .map(rows::bot)
                    .collect::<Result<Vec<_>, _>>()?;

                    let bot_ids: Vec<String> = bots
                        .iter()
                        .map(|bot| bot.id.as_str().to_owned())
                        .collect();

                    let threads = sqlx::query_as::<_, rows::ThreadRow>(
                        "SELECT id, space_id, bot_id, user_id, kind,
                                external_platform, external_workspace_id, external_conversation_id,
                                next_message_seq, next_event_seq,
                                boma.rfc3339(created_at) AS created_at
                           FROM threads WHERE bot_id = ANY($1)",
                    )
                    .bind(&bot_ids)
                    .fetch_all(&mut *conn)
                    .await
                    .map_err(tenant::unavailable)?
                    .into_iter()
                    .map(rows::thread)
                    .collect::<Result<Vec<_>, _>>()?;
                    let thread_ids: Vec<String> = threads
                        .iter()
                        .map(|thread| thread.id.as_str().to_owned())
                        .collect();

                    let messages = if thread_ids.is_empty() {
                        Vec::new()
                    } else {
                        sqlx::query_as::<_, rows::MessageRow>(
                            "SELECT id, thread_id, seq, role, blocks, bot_id,
                                    author_user_id, run_id, client_nonce,
                                    reply_to_message_id, reply_quote,
                                    boma.rfc3339(created_at) AS created_at
                               FROM messages WHERE thread_id = ANY($1)
                              ORDER BY thread_id, seq",
                        )
                        .bind(&thread_ids)
                        .fetch_all(&mut *conn)
                        .await
                        .map_err(tenant::unavailable)?
                        .into_iter()
                        .map(rows::message)
                        .collect::<Result<Vec<_>, _>>()?
                    };

                    let bot_memory = if bot_ids.is_empty() {
                        Vec::new()
                    } else {
                        sqlx::query_as::<_, rows::MemoryDocumentRow>(
                            "SELECT space_id, bot_id, path, content, revision,
                                    boma.rfc3339(updated_at) AS updated_at
                               FROM memory_documents WHERE bot_id = ANY($1)",
                        )
                        .bind(&bot_ids)
                        .fetch_all(&mut *conn)
                        .await
                        .map_err(tenant::unavailable)?
                        .into_iter()
                        .map(rows::memory_document)
                        .collect::<Result<Vec<_>, _>>()?
                    };

                    let routines = sqlx::query_as::<_, rows::RoutineRow>(
                        "SELECT id, space_id, bot_id, thread_id, user_id, name, prompt, crons, timezone,
                                active, notify, github_enabled, email_senders,
                                boma.rfc3339(last_run_at) AS last_run_at,
                                boma.rfc3339(next_run_at) AS next_run_at,
                                account_failures,
                                boma.rfc3339(created_at) AS created_at,
                                boma.rfc3339(updated_at) AS updated_at
                           FROM routines WHERE user_id = $1",
                    )
                    .bind(user_id.as_str())
                    .fetch_all(&mut *conn)
                    .await
                    .map_err(tenant::unavailable)?
                    .into_iter()
                    .map(rows::routine)
                    .collect::<Result<Vec<_>, _>>()?;

                    let artifacts = sqlx::query_as::<_, ArtifactMetaRow>(
                        "SELECT id, name, declared_mime, mime_type, size, hash, storage_key,
                                root_artifact_id, version, boma.rfc3339(created_at) AS created_at
                           FROM artifacts WHERE user_id = $1",
                    )
                    .bind(user_id.as_str())
                    .fetch_all(&mut *conn)
                    .await
                    .map_err(tenant::unavailable)?
                    .into_iter()
                    .map(|row| ArtifactMeta {
                        id: row.id,
                        name: row.name,
                        declared_mime: row.declared_mime,
                        mime_type: row.mime_type,
                        size: row.size,
                        hash: row.hash,
                        storage_key: row.storage_key,
                        root_artifact_id: row.root_artifact_id,
                        version: row.version,
                        created_at: row.created_at,
                    })
                    .collect();

                    Ok(ExportBundle {
                        profile,
                        bots,
                        threads,
                        messages,
                        bot_memory,
                        routines,
                        artifacts,
                    })
                })
            },
        )
        .await
    }

    async fn create(&self, id: &str, user_id: &UserId, created_at: &str) -> Result<(), AppError> {
        tenant::as_system(
            &self.pool,
            boma_domain::SystemReason::AccountExport,
            |conn| {
                let id = id.to_owned();
                let user_id = user_id.as_str().to_owned();
                let created_at = created_at.to_owned();
                Box::pin(async move {
                    sqlx::query(
                        "INSERT INTO account_exports (id, user_id, status, created_at, updated_at)
                         VALUES ($1, $2, 'pending', $3::timestamptz, $3::timestamptz)",
                    )
                    .bind(id)
                    .bind(user_id)
                    .bind(created_at)
                    .execute(&mut *conn)
                    .await
                    .map_err(tenant::unavailable)?;
                    Ok(())
                })
            },
        )
        .await
    }

    async fn find(&self, id: &str, user_id: &UserId) -> Result<Option<ExportRow>, AppError> {
        let id = id.to_owned();
        let user_id = user_id.as_str().to_owned();
        tenant::as_system(
            &self.pool,
            boma_domain::SystemReason::AccountExport,
            |conn| {
                Box::pin(async move {
                    sqlx::query_as::<_, ExportRowRaw>(
                        "SELECT id, user_id, status, payload_bytes, skipped,
                                boma.rfc3339(created_at) AS created_at
                           FROM account_exports WHERE id = $1 AND user_id = $2",
                    )
                    .bind(&id)
                    .bind(&user_id)
                    .fetch_optional(&mut *conn)
                    .await
                    .map(|row| {
                        row.map(|row| ExportRow {
                            id: row.id,
                            user_id: row.user_id,
                            status: row.status,
                            payload_bytes: row.payload_bytes,
                            skipped: row.skipped,
                            created_at: row.created_at,
                        })
                    })
                    .map_err(tenant::unavailable)
                })
            },
        )
        .await
    }

    async fn mark_ready(
        &self,
        id: &str,
        payload_bytes: i64,
        skipped: Vec<String>,
        now: &str,
    ) -> Result<(), AppError> {
        let id = id.to_owned();
        let now = now.to_owned();
        tenant::as_system(
            &self.pool,
            boma_domain::SystemReason::AccountExport,
            |conn| {
                Box::pin(async move {
                    sqlx::query(
                        "UPDATE account_exports
                            SET status = 'ready',
                                payload_bytes = $2,
                                skipped = $3,
                                updated_at = $4::timestamptz
                          WHERE id = $1",
                    )
                    .bind(id)
                    .bind(payload_bytes)
                    .bind(&skipped)
                    .bind(now)
                    .execute(&mut *conn)
                    .await
                    .map_err(tenant::unavailable)?;
                    Ok(())
                })
            },
        )
        .await
    }
}
