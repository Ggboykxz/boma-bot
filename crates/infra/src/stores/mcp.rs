//! MCP servers a space connects to (REQ-033).
//!
//! Credentials are sealed under the space's data-encryption key like every other secret in this
//! deployment (ADR-020); listings never carry them, and `reveal_bearer` is the only read path,
//! used exclusively by the wire call. The stdio allow-list lives in the deployment's
//! configuration, not in this store.

use boma_app::{
    AppError,
    ports::{McpServerRecord, McpServerStore, McpServerSummary, NewMcpServer, SealedSecret},
};
use boma_domain::TenantContext;
use secrecy::SecretString;
use sqlx::PgPool;

use crate::{secret_box::SecretBox, space_keys, tenant};

/// The Postgres adapter for the MCP server store.
#[derive(Debug, Clone)]
pub struct PostgresMcpServerStore {
    pool: PgPool,
    secrets: SecretBox,
}

impl PostgresMcpServerStore {
    /// Builds the store on a pool and the vault.
    #[must_use]
    pub fn new(pool: PgPool, secrets: SecretBox) -> Self {
        Self { pool, secrets }
    }
}

#[async_trait::async_trait]
impl McpServerStore for PostgresMcpServerStore {
    async fn list(&self, context: &TenantContext) -> Result<Vec<McpServerSummary>, AppError> {
        tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                let rows = sqlx::query_as::<
                    _,
                    (
                        String,
                        String,
                        String,
                        Option<String>,
                        Option<String>,
                        String,
                        bool,
                        bool,
                        Option<i32>,
                        String,
                    ),
                >(
                    "SELECT id, name, transport, url, command, auth_state, allow_private,
                            oauth_authorization_url IS NOT NULL AS oauth_configured, tool_count,
                            boma.rfc3339(created_at) AS created_at
                       FROM mcp_servers
                      ORDER BY name ASC",
                )
                .fetch_all(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
                Ok(rows.into_iter().map(summary_from).collect())
            })
        })
        .await
    }

    async fn create(
        &self,
        context: &TenantContext,
        server: NewMcpServer,
    ) -> Result<McpServerSummary, AppError> {
        let space_id = match context {
            TenantContext::Member { space_id, .. } => space_id.as_str().to_owned(),
            // The transport decides nothing here; MCP servers are member-registered.
            TenantContext::System { .. } => return Err(AppError::NotFound),
        };
        let id = server.id.clone();
        let name = server.name.clone();
        let transport = server.transport.as_str().to_owned();
        let url = server.url.clone();
        let command = server.command.clone();
        let allow_private = server.allow_private;
        let created_at = server.created_at.clone();
        let authorization_url = server
            .oauth
            .as_ref()
            .map(|oauth| oauth.authorization_url.clone());
        let token_url = server.oauth.as_ref().map(|oauth| oauth.token_url.clone());
        let client_id = server.oauth.as_ref().map(|oauth| oauth.client_id.clone());
        let secrets = self.secrets.clone();
        let space_id_in = space_id.clone();
        let id_in = id.clone();
        let name_in = name.clone();
        let transport_in = transport.clone();
        let url_in = url.clone();
        let command_in = command.clone();
        let authorization_url_in = authorization_url.clone();
        let token_url_in = token_url.clone();
        let client_id_in = client_id.clone();
        let created_at_in = created_at.clone();
        tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                let bearer = match &server.bearer {
                    Some(token) => {
                        Some(space_keys::seal_for_space(conn, &secrets, &space_id_in, token).await?)
                    }
                    None => None,
                };
                let client_secret = match server
                    .oauth
                    .as_ref()
                    .and_then(|oauth| oauth.client_secret.as_ref())
                {
                    Some(secret) => Some(
                        space_keys::seal_for_space(conn, &secrets, &space_id_in, secret).await?,
                    ),
                    None => None,
                };
                sqlx::query(
                    "INSERT INTO mcp_servers
                         (id, space_id, name, transport, url, command, allow_private,
                          oauth_authorization_url, oauth_token_url, oauth_client_id,
                          bearer_token, bearer_token_nonce,
                          sealed_client_secret, sealed_client_secret_nonce,
                          auth_state, created_at, updated_at)
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14,
                             'none', $15::timestamptz, $15::timestamptz)",
                )
                .bind(&id_in)
                .bind(&space_id_in)
                .bind(&name_in)
                .bind(&transport_in)
                .bind(&url_in)
                .bind(&command_in)
                .bind(allow_private)
                .bind(&authorization_url_in)
                .bind(&token_url_in)
                .bind(&client_id_in)
                .bind(bearer.as_ref().map(|sealed| sealed.ciphertext.as_slice()))
                .bind(bearer.as_ref().map(|sealed| sealed.nonce.as_slice()))
                .bind(
                    client_secret
                        .as_ref()
                        .map(|sealed| sealed.ciphertext.as_slice()),
                )
                .bind(client_secret.as_ref().map(|sealed| sealed.nonce.as_slice()))
                .bind(&created_at_in)
                .execute(&mut *conn)
                .await
                .map_err(|error| match &error {
                    sqlx::Error::Database(database) if database.is_unique_violation() => {
                        AppError::Conflict
                    }
                    _ => tenant::unavailable(error),
                })?;
                Ok(())
            })
        })
        .await?;
        Ok(McpServerSummary {
            id,
            name,
            transport,
            url,
            command,
            auth_state: "none".to_owned(),
            allow_private,
            oauth_configured: authorization_url.is_some(),
            tool_count: None,
            created_at,
        })
    }

    async fn get(
        &self,
        context: &TenantContext,
        id: &str,
    ) -> Result<Option<McpServerRecord>, AppError> {
        let id = id.to_owned();
        tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                let row = sqlx::query_as::<
                    _,
                    (
                        String,
                        String,
                        String,
                        Option<String>,
                        Option<String>,
                        String,
                        bool,
                        Option<String>,
                        Option<String>,
                        Option<String>,
                        Option<i32>,
                        String,
                    ),
                >(
                    "SELECT id, name, transport, url, command, auth_state, allow_private,
                            oauth_authorization_url, oauth_token_url, oauth_client_id, tool_count,
                            boma.rfc3339(created_at) AS created_at
                       FROM mcp_servers
                      WHERE id = $1",
                )
                .bind(&id)
                .fetch_optional(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
                Ok(row.map(
                    |(
                        id,
                        name,
                        transport,
                        url,
                        command,
                        auth_state,
                        allow_private,
                        authorization_url,
                        token_url,
                        client_id,
                        tool_count,
                        created_at,
                    )| {
                        McpServerRecord {
                            id,
                            name,
                            transport,
                            url,
                            command,
                            auth_state,
                            allow_private,
                            authorization_url,
                            token_url,
                            client_id,
                            tool_count,
                            created_at,
                        }
                    },
                ))
            })
        })
        .await
    }

    async fn remove(&self, context: &TenantContext, id: &str) -> Result<bool, AppError> {
        let id = id.to_owned();
        let result = tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                sqlx::query("DELETE FROM mcp_servers WHERE id = $1")
                    .bind(&id)
                    .execute(&mut *conn)
                    .await
                    .map_err(tenant::unavailable)
            })
        })
        .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn reveal_bearer(
        &self,
        context: &TenantContext,
        id: &str,
    ) -> Result<Option<SecretString>, AppError> {
        let id = id.to_owned();
        let secrets = self.secrets.clone();
        tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                let row = sqlx::query_as::<_, (String, Option<Vec<u8>>, Option<Vec<u8>>)>(
                    "SELECT space_id, bearer_token, bearer_token_nonce FROM mcp_servers WHERE id = $1",
                )
                .bind(&id)
                .fetch_optional(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
                let Some((space_id, ciphertext, nonce)) = row else {
                    return Ok(None);
                };
                let (Some(ciphertext), Some(nonce)) = (ciphertext, nonce) else {
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

    async fn replace_tools(
        &self,
        context: &TenantContext,
        server_id: &str,
        tools: &[boma_app::ports::McpTool],
    ) -> Result<(), AppError> {
        let server_id = server_id.to_owned();
        let tools: Vec<(String, String, serde_json::Value)> = tools
            .iter()
            .map(|tool| {
                (
                    tool.name.clone(),
                    tool.description.clone(),
                    tool.input_schema.clone(),
                )
            })
            .collect();
        tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                sqlx::query("DELETE FROM mcp_server_tools WHERE server_id = $1")
                    .bind(&server_id)
                    .execute(&mut *conn)
                    .await
                    .map_err(tenant::unavailable)?;
                for (name, description, schema) in tools {
                    sqlx::query(
                        "INSERT INTO mcp_server_tools (server_id, name, description, input_schema)
                         VALUES ($1, $2, $3, $4::jsonb)",
                    )
                    .bind(&server_id)
                    .bind(&name)
                    .bind(&description)
                    .bind(&schema)
                    .execute(&mut *conn)
                    .await
                    .map_err(tenant::unavailable)?;
                }
                Ok(())
            })
        })
        .await
    }

    async fn catalogue_tools(
        &self,
        context: &TenantContext,
    ) -> Result<Vec<(String, boma_app::ports::McpTool)>, AppError> {
        tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                let rows = sqlx::query_as::<_, (String, String, String, serde_json::Value)>(
                    "SELECT s.name, t.name, t.description, t.input_schema
                       FROM mcp_server_tools t
                       JOIN mcp_servers s ON s.id = t.server_id
                      WHERE s.auth_state = 'connected'
                      ORDER BY s.name ASC, t.name ASC",
                )
                .fetch_all(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
                Ok(rows
                    .into_iter()
                    .map(|(server, name, description, input_schema)| {
                        (
                            server,
                            boma_app::ports::McpTool {
                                name,
                                description,
                                input_schema,
                            },
                        )
                    })
                    .collect())
            })
        })
        .await
    }

    async fn set_auth_state(
        &self,
        context: &TenantContext,
        id: &str,
        auth_state: &str,
        tool_count: Option<i32>,
    ) -> Result<(), AppError> {
        let id = id.to_owned();
        let auth_state = auth_state.to_owned();
        tenant::with_tenant(&self.pool, context, |conn| {
            Box::pin(async move {
                sqlx::query(
                    "UPDATE mcp_servers
                        SET auth_state = $2, tool_count = $3, updated_at = now()
                      WHERE id = $1",
                )
                .bind(&id)
                .bind(&auth_state)
                .bind(tool_count)
                .execute(&mut *conn)
                .await
                .map_err(tenant::unavailable)?;
                Ok(())
            })
        })
        .await
    }
}

/// The tuple a summary query returns: id, name, transport, url, command, auth_state,
/// allow_private, oauth_configured, tool_count, created_at.
type SummaryRow = (
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    String,
    bool,
    bool,
    Option<i32>,
    String,
);

/// Maps a row to the summary the API exposes.
fn summary_from(row: SummaryRow) -> McpServerSummary {
    let (
        id,
        name,
        transport,
        url,
        command,
        auth_state,
        allow_private,
        oauth_configured,
        tool_count,
        created_at,
    ) = row;
    McpServerSummary {
        id,
        name,
        transport,
        url,
        command,
        auth_state,
        allow_private,
        oauth_configured,
        tool_count,
        created_at,
    }
}
