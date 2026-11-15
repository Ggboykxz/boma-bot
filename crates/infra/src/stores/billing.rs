//! The money spine: spending limits, purchase payments, subscriptions (REQ-018, REQ-051).

use boma_app::{
    AppError,
    ports::{BillingStore, NewPurchasePayment, NewSubscription, SpendingLimit, SubscriptionState},
};
use boma_domain::TenantContext;
use sqlx::PgPool;

use crate::tenant;

/// The Postgres adapter for [`BillingStore`].
#[derive(Debug, Clone)]
pub struct PostgresBillingStore {
    pool: PgPool,
}

impl PostgresBillingStore {
    /// Builds the store on a pool.
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn free() -> SubscriptionState {
    SubscriptionState {
        plan: "free".to_owned(),
        status: "active".to_owned(),
        provider_ref: None,
        credits_minor_units: 0,
        current_period_end: None,
    }
}

#[async_trait::async_trait]
impl BillingStore for PostgresBillingStore {
    async fn limit(&self, context: &TenantContext) -> Result<SpendingLimit, AppError> {
        let row: Option<(bool, String, i64, String)> =
            tenant::with_tenant(&self.pool, context, |conn| {
                Box::pin(async move {
                    sqlx::query_as(
                        "SELECT enabled, currency, cap_minor_units, boma.rfc3339(updated_at)
                       FROM space_spending_limits",
                    )
                    .fetch_optional(&mut *conn)
                    .await
                    .map_err(tenant::unavailable)
                })
            })
            .await?;
        Ok(match row {
            Some((enabled, currency, cap_minor_units, updated_at)) => SpendingLimit {
                enabled,
                currency,
                cap_minor_units,
                updated_at,
            },
            None => SpendingLimit {
                enabled: false,
                currency: "USD".to_owned(),
                cap_minor_units: 0,
                updated_at: String::new(),
            },
        })
    }

    async fn set_limit(
        &self,
        context: &TenantContext,
        enabled: bool,
        currency: &str,
        cap_minor_units: i64,
    ) -> Result<SpendingLimit, AppError> {
        let currency = currency.trim().to_ascii_uppercase();
        if currency.len() != 3 || !currency.bytes().all(|byte| byte.is_ascii_uppercase()) {
            return Err(AppError::Rule(boma_domain::DomainError::InvalidName(
                "the currency must be a three-letter code (USD, EUR, …)".to_owned(),
            )));
        }
        if cap_minor_units < 0 {
            return Err(AppError::Rule(boma_domain::DomainError::InvalidName(
                "the cap cannot be negative".to_owned(),
            )));
        }
        let space_id = match context {
            TenantContext::Member { space_id, .. } => space_id.as_str().to_owned(),
            TenantContext::System { .. } => return Err(AppError::NotFound),
        };
        tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                sqlx::query(
                    "INSERT INTO space_spending_limits (space_id, enabled, currency, cap_minor_units, updated_at)
                     VALUES ($1, $2, $3, $4, now())
                     ON CONFLICT (space_id) DO UPDATE
                       SET enabled = EXCLUDED.enabled,
                           currency = EXCLUDED.currency,
                           cap_minor_units = EXCLUDED.cap_minor_units,
                           updated_at = now()",
                )
                .bind(&space_id)
                .bind(enabled)
                .bind(&currency)
                .bind(cap_minor_units)
                .execute(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
                Ok(())
            })
        })
        .await?;
        self.limit(context).await
    }

    async fn remaining_allowance(
        &self,
        context: &TenantContext,
        month_prefix: &str,
    ) -> Result<Option<i64>, AppError> {
        let limit = self.limit(context).await?;
        if !limit.enabled {
            return Ok(None);
        }
        let spent = self.spent_in_month(context, month_prefix).await?;
        Ok(Some((limit.cap_minor_units - spent).max(0)))
    }

    async fn spent_in_month(
        &self,
        context: &TenantContext,
        month_prefix: &str,
    ) -> Result<i64, AppError> {
        let month_prefix = month_prefix.to_owned();
        let spent: Option<i64> = tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                sqlx::query_scalar(
                    "SELECT COALESCE(SUM(amount_minor_units), 0)::bigint
                       FROM purchase_payments
                      WHERE to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM') = $1",
                )
                .bind(&month_prefix)
                .fetch_one(&mut *conn)
                .await
                .map_err(tenant::unavailable)
            })
        })
        .await?;
        Ok(spent.unwrap_or(0))
    }

    async fn record_payment(
        &self,
        context: &TenantContext,
        payment: NewPurchasePayment,
    ) -> Result<(), AppError> {
        let space_id = match context {
            TenantContext::Member { space_id, .. } => space_id.as_str().to_owned(),
            TenantContext::System { .. } => return Err(AppError::NotFound),
        };
        tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                sqlx::query(
                    "INSERT INTO purchase_payments
                        (id, space_id, user_id, card_id, item, amount_minor_units, currency, created_at)
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8::timestamptz)",
                )
                .bind(&payment.id)
                .bind(&space_id)
                .bind(payment.user_id.as_str())
                .bind(payment.card_id.as_deref())
                .bind(&payment.item)
                .bind(payment.amount_minor_units)
                .bind(&payment.currency)
                .bind(&payment.created_at)
                .execute(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
                Ok(())
            })
        })
        .await
    }

    async fn subscription(&self, context: &TenantContext) -> Result<SubscriptionState, AppError> {
        let row: Option<(String, String, Option<String>, i64, Option<String>)> =
            tenant::with_tenant(&self.pool, context, |conn| {
                Box::pin(async move {
                    sqlx::query_as(
                        "SELECT plan, status, provider_ref, credits_minor_units,
                                boma.rfc3339(current_period_end)
                           FROM subscriptions",
                    )
                    .fetch_optional(&mut *conn)
                    .await
                    .map_err(tenant::unavailable)
                })
            })
            .await?;
        Ok(match row {
            Some((plan, status, provider_ref, credits_minor_units, current_period_end)) => {
                SubscriptionState {
                    plan,
                    status,
                    provider_ref,
                    credits_minor_units,
                    current_period_end,
                }
            }
            None => free(),
        })
    }

    async fn activate_subscription(
        &self,
        context: &TenantContext,
        subscription: NewSubscription,
    ) -> Result<(), AppError> {
        tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                sqlx::query(
                    "INSERT INTO subscriptions
                        (space_id, plan, status, provider_ref, credits_minor_units, current_period_end, created_at, updated_at)
                     VALUES ($1, $2, $3, $4, $5, $6::timestamptz, $7::timestamptz, $7::timestamptz)
                     ON CONFLICT (space_id) DO UPDATE
                       SET plan = EXCLUDED.plan,
                           status = EXCLUDED.status,
                           provider_ref = EXCLUDED.provider_ref,
                           credits_minor_units = EXCLUDED.credits_minor_units,
                           current_period_end = EXCLUDED.current_period_end,
                           updated_at = now()",
                )
                .bind(&subscription.space_id)
                .bind(&subscription.plan)
                .bind(&subscription.status)
                .bind(subscription.provider_ref.as_deref())
                .bind(subscription.credits_minor_units)
                .bind(subscription.current_period_end.as_deref())
                .bind(&subscription.created_at)
                .execute(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
                Ok(())
            })
        })
        .await
    }
}
