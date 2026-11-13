//! The streamable-HTTP JSON-RPC client for MCP servers (REQ-033).
//!
//! One HTTP POST per call: `initialize` → `tools/list` → `tools/call`. Each call is
//! self-contained; servers that demand session-scoped streams are out of scope for this build
//! (recorded in docs/STATE.md). Every URL passes the outbound guard (D-05), redirects are
//! refused, the body is capped at 1 MiB and the whole call is bounded. Authentication is a
//! bearer when the server was registered with one.

use boma_app::{
    AppError,
    ports::{McpServerRecord, McpTool, McpTransport},
};
use secrecy::{ExposeSecret, SecretString};

use crate::net_safety::{NetSafetyError, check_outbound_url};

/// The maximum size of one JSON-RPC response body: 1 MiB.
const MAX_RESPONSE_BYTES: usize = 1024 * 1024;

/// The MCP protocol version this deployment speaks.
const PROTOCOL_VERSION: &str = "2024-11-05";

/// The HTTP JSON-RPC transport.
#[derive(Debug, Clone)]
pub struct HttpMcpTransport {
    http: reqwest::Client,
}

impl HttpMcpTransport {
    /// Builds the transport with a 30-second timeout and no redirects.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Unavailable`] when the TLS backend cannot be initialised.
    pub fn new() -> Result<Self, AppError> {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| AppError::Unavailable)?;
        Ok(Self { http })
    }

    /// One JSON-RPC call, guarded.
    async fn rpc(
        &self,
        server: &McpServerRecord,
        bearer: Option<&SecretString>,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, AppError> {
        let Some(url) = server.url.as_deref() else {
            return Err(AppError::Rule(boma_domain::DomainError::InvalidName(
                "an HTTP MCP server needs a URL".to_owned(),
            )));
        };
        // D-05: the outbound guard decides, and the caller audits private uses.
        check_outbound_url(url, server.allow_private)
            .await
            .map_err(|reason| match reason {
                NetSafetyError::NotResolvableOrPrivate => {
                    AppError::Rule(boma_domain::DomainError::InvalidName(
                        "the server's address is refused: it does not resolve to a public address"
                            .to_owned(),
                    ))
                }
                other => AppError::Rule(boma_domain::DomainError::InvalidName(other.to_string())),
            })?;
        let mut request = self
            .http
            .post(url)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .header(reqwest::header::ACCEPT, "application/json")
            .json(&serde_json::json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": method,
                "params": params,
            }));
        if let Some(token) = bearer {
            request = request.header(
                reqwest::header::AUTHORIZATION,
                format!("Bearer {}", token.expose_secret()),
            );
        }
        let response = request.send().await.map_err(|error| {
            tracing::warn!(%error, server = %server.name, "the MCP request failed");
            AppError::Unavailable
        })?;
        let status = response.status();
        let bytes = response.bytes().await.map_err(|_| AppError::Unavailable)?;
        if bytes.len() > MAX_RESPONSE_BYTES {
            return Err(AppError::Rule(boma_domain::DomainError::InvalidName(
                "the server answered with more than 1 MiB".to_owned(),
            )));
        }
        if !status.is_success() {
            return Err(AppError::Rule(boma_domain::DomainError::InvalidName(
                format!("the server answered with status {status}"),
            )));
        }
        let body: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|_| AppError::Unavailable)?;
        if let Some(error) = body.get("error") {
            let message = error
                .get("message")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("the server refused the call");
            return Err(AppError::Rule(boma_domain::DomainError::InvalidName(
                message.to_owned(),
            )));
        }
        Ok(body
            .get("result")
            .cloned()
            .unwrap_or(serde_json::Value::Null))
    }
}

#[async_trait::async_trait]
impl McpTransport for HttpMcpTransport {
    async fn list_tools(
        &self,
        server: &McpServerRecord,
        bearer: Option<&SecretString>,
    ) -> Result<Vec<McpTool>, AppError> {
        self.rpc(
            server,
            bearer,
            "initialize",
            serde_json::json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": {
                    "name": "boma-bot",
                    "version": env!("CARGO_PKG_VERSION"),
                },
            }),
        )
        .await?;
        let result = self
            .rpc(server, bearer, "tools/list", serde_json::json!({}))
            .await?;
        let tools = result
            .get("tools")
            .and_then(serde_json::Value::as_array)
            .cloned()
            .unwrap_or_default();
        let mut discovered = Vec::with_capacity(tools.len());
        for tool in tools {
            let name = tool.get("name").and_then(serde_json::Value::as_str);
            let description = tool
                .get("description")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("");
            let schema = tool
                .get("inputSchema")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({ "type": "object" }));
            match name.map(boma_domain::sanitize_mcp_tool_name) {
                Some(Ok(name)) => discovered.push(McpTool {
                    name,
                    description: description.to_owned(),
                    input_schema: schema,
                }),
                // A server that names tools unsafely loses those tools, not the install.
                _ => {
                    tracing::warn!(server = %server.name, "an MCP server named a tool unsafely; it was skipped");
                }
            }
        }
        Ok(discovered)
    }

    async fn call_tool(
        &self,
        server: &McpServerRecord,
        bearer: Option<&SecretString>,
        tool: &str,
        input: &serde_json::Value,
    ) -> Result<serde_json::Value, AppError> {
        boma_domain::sanitize_mcp_tool_name(tool)?;
        self.rpc(
            server,
            bearer,
            "tools/call",
            serde_json::json!({
                "name": tool,
                "arguments": input,
            }),
        )
        .await
    }
}
