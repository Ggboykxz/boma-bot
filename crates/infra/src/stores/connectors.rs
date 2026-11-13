//! Connector installs (REQ-032).
//!
//! The catalogue is code; this table is what a space installed. Credentials are sealed under the
//! space's data-encryption key like every other secret in this deployment (ADR-020); the list
//! never carries them, and `connected_credential` is the only read path.

use boma_app::{
    AppError,
    ports::{ConnectorStore, ConnectorSummary, SealedSecret},
};
use boma_domain::TenantContext;
use secrecy::SecretString;
use sqlx::PgPool;

use crate::{secret_box::SecretBox, space_keys, tenant};

/// The Postgres adapter for the connector store.
#[derive(Debug, Clone)]
pub struct PostgresConnectorStore {
    pool: PgPool,
    secrets: SecretBox,
}

impl PostgresConnectorStore {
    /// Builds the store on a pool and the vault.
    #[must_use]
    pub fn new(pool: PgPool, secrets: SecretBox) -> Self {
        Self { pool, secrets }
    }
}

/// The catalogue data for one builtin connector id, or `None`.
fn builtin(connector_id: &str) -> Option<boma_domain::ConnectorSpec> {
    boma_domain::builtin_connectors()
        .into_iter()
        .find(|connector| connector.id == connector_id)
}

#[async_trait::async_trait]
impl ConnectorStore for PostgresConnectorStore {
    async fn list(&self, context: &TenantContext) -> Result<Vec<ConnectorSummary>, AppError> {
        let rows = tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                sqlx::query_as::<_, (String, String)>(
                    "SELECT connector_id, status FROM connector_installs ORDER BY connector_id ASC",
                )
                .fetch_all(&mut *conn)
                .await
                .map_err(tenant::unavailable)
            })
        })
        .await?;
        let installed: std::collections::HashMap<String, String> = rows.into_iter().collect();
        Ok(boma_domain::builtin_connectors()
            .into_iter()
            .map(|connector| {
                let installed = installed
                    .get(&connector.id)
                    .is_some_and(|status| status == "connected");
                ConnectorSummary {
                    connector_id: connector.id,
                    name: connector.name,
                    description: connector.description,
                    auth_kind: connector.auth_kind.as_str().to_owned(),
                    installed,
                }
            })
            .collect())
    }

    async fn install(
        &self,
        context: &TenantContext,
        connector_id: &str,
        credential: Option<SecretString>,
    ) -> Result<ConnectorSummary, AppError> {
        let space_id = match context {
            TenantContext::Member { space_id, .. } => space_id.as_str().to_owned(),
            TenantContext::System { .. } => return Err(AppError::NotFound),
        };
        let Some(connector) = builtin(connector_id) else {
            return Err(AppError::NotFound);
        };
        let needs_key = connector.auth_kind == boma_domain::ConnectorAuth::ApiKey;
        if needs_key && credential.is_none() {
            return Err(AppError::Rule(boma_domain::DomainError::InvalidName(
                "this connector needs an `apiKey`".to_owned(),
            )));
        }
        if !needs_key && credential.is_some() {
            return Err(AppError::Rule(boma_domain::DomainError::InvalidName(
                "this connector takes no credential".to_owned(),
            )));
        }
        let auth_kind = connector.auth_kind.as_str().to_owned();
        let connector_id_in = connector.id.clone();
        let connector_id = connector_id_in.clone();
        let auth_kind_in = auth_kind.clone();
        let secrets = self.secrets.clone();
        tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                let sealed = match &credential {
                    Some(token) => Some(
                        space_keys::seal_for_space(conn, &secrets, &space_id, token).await?,
                    ),
                    None => None,
                };
                sqlx::query(
                    "INSERT INTO connector_installs (space_id, connector_id, auth_kind, sealed_credential, sealed_credential_nonce, status, created_at, updated_at)
                     VALUES ($1, $2, $3, $4, $5, 'connected', now(), now())
                     ON CONFLICT (space_id, connector_id) DO UPDATE
                       SET sealed_credential = EXCLUDED.sealed_credential,
                           sealed_credential_nonce = EXCLUDED.sealed_credential_nonce,
                           status = 'connected',
                           updated_at = now()",
                )
                .bind(&space_id)
                .bind(&connector_id_in)
                .bind(&auth_kind_in)
                .bind(sealed.as_ref().map(|sealed| sealed.ciphertext.as_slice()))
                .bind(sealed.as_ref().map(|sealed| sealed.nonce.as_slice()))
                .execute(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
                Ok(())
            })
        })
        .await?;
        let name = connector.name;
        let description = connector.description;
        Ok(ConnectorSummary {
            connector_id,
            name,
            description,
            auth_kind,
            installed: true,
        })
    }

    async fn revoke(&self, context: &TenantContext, connector_id: &str) -> Result<bool, AppError> {
        if builtin(connector_id).is_none() {
            // The connector does not exist: forgetting it is a success, but nothing happened.
            return Ok(false);
        }
        let connector_id = connector_id.to_owned();
        let result = tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                sqlx::query(
                    "UPDATE connector_installs
                        SET status = 'revoked', updated_at = now()
                      WHERE connector_id = $1",
                )
                .bind(&connector_id)
                .execute(&mut *conn)
                .await
                .map_err(tenant::unavailable)
            })
        })
        .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn is_connected(
        &self,
        context: &TenantContext,
        connector_id: &str,
    ) -> Result<bool, AppError> {
        let connector_id = connector_id.to_owned();
        let row: Option<String> = tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                sqlx::query_scalar("SELECT status FROM connector_installs WHERE connector_id = $1")
                    .bind(&connector_id)
                    .fetch_optional(&mut *conn)
                    .await
                    .map_err(tenant::unavailable)
            })
        })
        .await?;
        Ok(row.as_deref() == Some("connected"))
    }

    async fn connected_credential(
        &self,
        context: &TenantContext,
        connector_id: &str,
    ) -> Result<Option<SecretString>, AppError> {
        let connector_id = connector_id.to_owned();
        let secrets = self.secrets.clone();
        tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                let row = sqlx::query_as::<_, (String, Option<Vec<u8>>, Option<Vec<u8>>)>(
                    "SELECT space_id, sealed_credential, sealed_credential_nonce
                       FROM connector_installs
                      WHERE connector_id = $1 AND status = 'connected'",
                )
                .bind(&connector_id)
                .fetch_optional(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
                let Some((space_id, Some(ciphertext), Some(nonce))) = row else {
                    return Ok(None);
                };
                let value = space_keys::open_for_space(
                    conn,
                    &secrets,
                    &space_id,
                    &SealedSecret { ciphertext, nonce },
                )
                .await?;
                Ok(Some(value))
            })
        })
        .await
    }

    async fn catalogue_tools(
        &self,
        context: &TenantContext,
    ) -> Result<Vec<boma_domain::ToolSpec>, AppError> {
        let ids: Vec<String> = tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                sqlx::query_scalar(
                    "SELECT connector_id FROM connector_installs WHERE status = 'connected' ORDER BY connector_id ASC",
                )
                .fetch_all(&mut *conn)
                .await
                .map_err(tenant::unavailable)
            })
        })
        .await?;
        Ok(catalogue_tools(&ids))
    }
}

/// The tools of the connected connectors, from the catalogue.
#[must_use]
fn catalogue_tools(ids: &[String]) -> Vec<boma_domain::ToolSpec> {
    boma_domain::builtin_connectors()
        .into_iter()
        .filter(|connector| ids.iter().any(|id| id == &connector.id))
        .flat_map(|connector| {
            connector
                .tools
                .iter()
                .map(|tool| boma_domain::ToolSpec {
                    key: connector.tool_key(tool),
                    description: tool.description.clone(),
                    kind: boma_domain::ToolKind::Connector,
                    effect: boma_domain::ToolEffect::Read,
                })
                .collect::<Vec<_>>()
        })
        .collect()
}
