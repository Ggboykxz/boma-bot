//! The transport the binary wires: HTTP everywhere, plus stdio for allow-listed programs.
//!
//! One port, one dispatcher, by `server.transport`. The stdio arm runs the child on the
//! deployment's allow-list and the HTTP arm goes anywhere the outbound guard allows — the
//! private endpoint guard is untouched. Nothing else routes here: adding a third transport
//! means extending this match, nothing else.

use boma_app::{
    AppError,
    ports::{McpServerRecord, McpTool, McpTransport},
};
use secrecy::SecretString;

use crate::{mcp_http::HttpMcpTransport, mcp_stdio::StdioMcpTransport};

/// The transport every deployment builds.
#[derive(Debug, Clone)]
pub struct McpTransportDispatch {
    http: HttpMcpTransport,
    stdio: StdioMcpTransport,
}

impl McpTransportDispatch {
    /// Builds the dispatcher. The stdio arm carries the deployment's allow-list; an empty one
    /// refuses every stdio server before any process exists.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Unavailable`] when the HTTP client cannot be built.
    pub fn new(stdio_allowed: Vec<String>) -> Result<Self, AppError> {
        Ok(Self {
            http: HttpMcpTransport::new()?,
            stdio: StdioMcpTransport::new(stdio_allowed),
        })
    }
}

#[async_trait::async_trait]
impl McpTransport for McpTransportDispatch {
    async fn list_tools(
        &self,
        server: &McpServerRecord,
        bearer: Option<&SecretString>,
    ) -> Result<Vec<McpTool>, AppError> {
        match server.transport.as_str() {
            "stdio" => self.stdio.list_tools(server, bearer).await,
            _ => self.http.list_tools(server, bearer).await,
        }
    }

    async fn call_tool(
        &self,
        server: &McpServerRecord,
        bearer: Option<&SecretString>,
        tool: &str,
        input: &serde_json::Value,
    ) -> Result<serde_json::Value, AppError> {
        match server.transport.as_str() {
            "stdio" => self.stdio.call_tool(server, bearer, tool, input).await,
            _ => self.http.call_tool(server, bearer, tool, input).await,
        }
    }
}
