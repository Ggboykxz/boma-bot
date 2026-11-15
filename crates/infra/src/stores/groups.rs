//! Groups (REQ-010).
//!
//! One shared thread per group, one membership table for persons and bots alike. The create
//! path writes the thread, the group and every membership in ONE tenant transaction; the
//! `find`/`list_for_member` paths never leak another tenant's group (REQ-043).

use async_trait::async_trait;
use boma_app::{AppError, ports};
use boma_domain::{BotId, Group, GroupId, UserId};
use sqlx::PgPool;

use super::rows::{self, GroupRow};
use crate::tenant;

/// The Postgres adapter for [`ports::GroupStore`].
#[derive(Debug, Clone)]
pub struct PostgresGroupStore {
    pool: PgPool,
}

impl PostgresGroupStore {
    /// Builds the store on a pool.
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ports::GroupStore for PostgresGroupStore {
    async fn create(&self, group: ports::NewGroup) -> Result<Group, AppError> {
        let space_id = group.space_id.clone();
        tenant::in_space(&self.pool, &space_id, |conn| {
            Box::pin(async move {
                let owner_bot = group.bot_ids.first().ok_or(AppError::Unavailable)?;
                sqlx::query(
                    "INSERT INTO threads (id, space_id, bot_id, user_id, created_at)
                     VALUES ($1, $2, $3, $4, $5::timestamptz)",
                )
                .bind(group.thread_id.as_str())
                .bind(group.space_id.as_str())
                .bind(owner_bot.as_str())
                .bind(group.owner.as_str())
                .bind(&group.created_at)
                .execute(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;

                sqlx::query(
                    "INSERT INTO groups (id, space_id, name, thread_id, created_at)
                     VALUES ($1, $2, $3, $4, $5::timestamptz)",
                )
                .bind(group.id.as_str())
                .bind(group.space_id.as_str())
                .bind(&group.name)
                .bind(group.thread_id.as_str())
                .bind(&group.created_at)
                .execute(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;

                for bot_id in &group.bot_ids {
                    sqlx::query(
                        "INSERT INTO group_memberships (group_id, member_kind, member_id, created_at)
                         VALUES ($1, 'bot', $2, $3::timestamptz)",
                    )
                    .bind(group.id.as_str())
                    .bind(bot_id.as_str())
                    .bind(&group.created_at)
                    .execute(&mut *conn)
                    .await
                    .map_err(tenant::unavailable)?;
                }
                sqlx::query(
                    "INSERT INTO group_memberships (group_id, member_kind, member_id, created_at)
                     VALUES ($1, 'person', $2, $3::timestamptz)",
                )
                .bind(group.id.as_str())
                .bind(group.owner.as_str())
                .bind(&group.created_at)
                .execute(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;

                Ok(Group {
                    id: GroupId::new(group.id.as_str()).map_err(|_| AppError::Unavailable)?,
                    space_id: group.space_id.clone(),
                    name: group.name.clone(),
                    thread_id: group.thread_id.clone(),
                    created_at: group.created_at.clone(),
                    bot_ids: group.bot_ids.clone(),
                })
            })
        })
        .await
    }

    async fn find(
        &self,
        context: &boma_domain::TenantContext,
        id: &GroupId,
    ) -> Result<Option<Group>, AppError> {
        let space_id = context.space_id().cloned().ok_or(AppError::NotFound)?;
        let owner_space = space_id.clone();
        let id = id.as_str().to_owned();
        tenant::in_space(&self.pool, &owner_space, |conn| {
            Box::pin(async move {
                let row = sqlx::query_as::<_, GroupRow>(
                    "SELECT id, space_id, name, thread_id, boma.rfc3339(created_at) AS created_at
                       FROM groups
                      WHERE id = $1 AND space_id = $2",
                )
                .bind(&id)
                .bind(space_id.as_str())
                .fetch_optional(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
                let Some(row) = row else {
                    return Ok(None);
                };
                let bots = sqlx::query_scalar::<_, String>(
                    "SELECT member_id FROM group_memberships
                      WHERE group_id = $1 AND member_kind = 'bot'
                      ORDER BY created_at, member_id",
                )
                .bind(&row.id)
                .fetch_all(&mut *conn)
                .await
                .map_err(tenant::unavailable)?
                .into_iter()
                .map(BotId::new)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| AppError::Unavailable)?;
                Ok(Some(rows::group(row, bots)?))
            })
        })
        .await
    }

    async fn list_for_member(
        &self,
        context: &boma_domain::TenantContext,
        user_id: &UserId,
    ) -> Result<Vec<Group>, AppError> {
        let space_id = context.space_id().cloned().ok_or(AppError::NotFound)?;
        let owner_space = space_id.clone();
        let user_id = user_id.as_str().to_owned();
        tenant::in_space(&self.pool, &owner_space, |conn| {
            Box::pin(async move {
                let rows = sqlx::query_as::<_, GroupRow>(
                    "SELECT g.id, g.space_id, g.name, g.thread_id, boma.rfc3339(g.created_at) AS created_at
                       FROM groups g
                      WHERE g.space_id = $1
                        AND EXISTS (
                            SELECT 1 FROM group_memberships p
                             WHERE p.group_id = g.id AND p.member_kind = 'person' AND p.member_id = $2
                        )
                      ORDER BY g.created_at, g.id",
                )
                .bind(space_id.as_str())
                .bind(&user_id)
                .fetch_all(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
                let mut groups = Vec::with_capacity(rows.len());
                for row in rows {
                    let bots = sqlx::query_scalar::<_, String>(
                        "SELECT member_id FROM group_memberships
                          WHERE group_id = $1 AND member_kind = 'bot'
                          ORDER BY created_at, member_id",
                    )
                    .bind(&row.id)
                    .fetch_all(&mut *conn)
                    .await
                    .map_err(tenant::unavailable)?
                    .into_iter()
                    .map(BotId::new)
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|_| AppError::Unavailable)?;
                    groups.push(rows::group(row, bots)?);
                }
                Ok(groups)
            })
        })
        .await
    }
}
