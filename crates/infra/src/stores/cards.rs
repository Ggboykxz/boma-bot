//! Cards and effects: the questions and approvals on a thread, and the exactly-once record of
//! the side effects they authorize (REQ-007, REQ-017).

use boma_app::{
    AppError,
    ports::{CardStore, EffectStore, NewEffect},
};
use boma_domain::{Card, CardId, ExternalEffect, TenantContext};
use sqlx::{PgPool, types::Json};

use super::rows::{self, CardRow, EffectRow};
use crate::tenant;

/// The Postgres adapter for [`CardStore`].
#[derive(Debug, Clone)]
pub struct PostgresCardStore {
    pool: PgPool,
}

impl PostgresCardStore {
    /// Builds the store on a pool.
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl CardStore for PostgresCardStore {
    async fn find(&self, context: &TenantContext, id: &CardId) -> Result<Option<Card>, AppError> {
        let id = id.as_str().to_owned();
        let row = tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                sqlx::query_as::<_, CardRow>(
                    "SELECT id, space_id, thread_id, run_id, kind, status, answer,
                            boma.rfc3339(created_at) AS created_at,
                            boma.rfc3339(answered_at) AS answered_at
                     FROM cards
                     WHERE id = $1",
                )
                .bind(id.as_str())
                .fetch_optional(&mut *conn)
                .await
                .map_err(tenant::unavailable)
            })
        })
        .await?;
        row.map(rows::card).transpose()
    }

    async fn answer(
        &self,
        context: &TenantContext,
        id: &CardId,
        answer: &str,
    ) -> Result<Card, AppError> {
        let id = id.as_str().to_owned();
        let answer = answer.to_owned();
        let row = tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                sqlx::query_as::<_, CardRow>(
                    "UPDATE cards
                     SET status = 'answered', answer = $2, answered_at = now()
                     WHERE id = $1 AND status = 'pending'
                     RETURNING id, space_id, thread_id, run_id, kind, status, answer,
                               boma.rfc3339(created_at) AS created_at,
                               boma.rfc3339(answered_at) AS answered_at",
                )
                .bind(id.as_str())
                .bind(answer.as_str())
                .fetch_optional(&mut *conn)
                .await
                .map_err(tenant::unavailable)
            })
        })
        .await?;
        match row {
            Some(row) => rows::card(row),
            // Either the card does not exist in this tenant, or it was already answered.
            None => Err(AppError::Conflict),
        }
    }
}

/// The Postgres adapter for [`EffectStore`].
#[derive(Debug, Clone)]
pub struct PostgresEffectStore {
    pool: PgPool,
}

impl PostgresEffectStore {
    /// Builds the store on a pool.
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl EffectStore for PostgresEffectStore {
    async fn record(
        &self,
        context: &TenantContext,
        effect: NewEffect,
    ) -> Result<ExternalEffect, AppError> {
        let space_id = context.space_id().ok_or(AppError::NotFound)?.clone();
        let row = tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                sqlx::query_as::<_, EffectRow>(
                    "INSERT INTO external_effects
                         (run_id, effect_key, space_id, kind, payload, applied_at)
                     VALUES ($1, $2, $3, $4, $5, now())
                     RETURNING run_id, effect_key, kind, payload,
                               boma.rfc3339(applied_at) AS applied_at",
                )
                .bind(effect.run_id.as_str())
                .bind(effect.effect_key.as_str())
                .bind(space_id.as_str())
                .bind(effect.kind.as_str())
                .bind(Json(effect.payload))
                .fetch_one(&mut *conn)
                .await
                .map_err(|error| {
                    if tenant::is_unique_violation(&error) {
                        AppError::Conflict
                    } else {
                        tenant::unavailable(error)
                    }
                })
            })
        })
        .await?;
        rows::effect(row)
    }
}
