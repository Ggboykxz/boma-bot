//! The stdio MCP transport (REQ-033).
//!
//! `MCP_STDIO_ALLOWED_COMMANDS` gates this transport: empty by default, and empty means every
//! stdio server is refused — the spec's default. An operator opting in names the *bare programs*
//! they trust (`python3,node`); the check runs at add time (in the use cases, with a friendly
//! error) and again at spawn time (here, before anything executes).
//!
//! One RPC call = one spawn, like the CDP client's fresh connection per call: no process
//! lingers, a crashed child dies with its request, and memory across calls goes through the
//! filesystem, not through a persistent server. The protocol is newline-delimited JSON-RPC 2.0:
//! `initialize`, `notifications/initialized`, then the actual call.

use std::process::Stdio;

use boma_app::{
    AppError,
    ports::{McpServerRecord, McpTool, McpTransport},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::Command,
};

/// The MCP protocol version this deployment speaks.
const PROTOCOL_VERSION: &str = "2024-11-05";

/// The maximum size of one JSON-RPC line: 1 MiB.
const MAX_LINE_BYTES: u64 = 1024 * 1024;

/// The maximum time for one request/response round trip on the child's stdio.
const REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// The stdio MCP transport.
#[derive(Debug, Clone)]
pub struct StdioMcpTransport {
    allowed: Vec<String>,
}

impl StdioMcpTransport {
    /// Builds the transport on the deployment's allow-list.
    #[must_use]
    pub fn new(allowed: Vec<String>) -> Self {
        Self { allowed }
    }
}

impl StdioMcpTransport {
    /// Runs one JSON-RPC round trip: initialize, initialized, then `method`.
    async fn rpc(
        &self,
        server: &McpServerRecord,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, AppError> {
        let Some(command) = server.command.as_deref() else {
            return Err(AppError::Rule(boma_domain::DomainError::InvalidName(
                "a stdio MCP server needs a `command`".to_owned(),
            )));
        };
        let (program, args) = boma_domain::split_command(command)?;
        if !boma_domain::program_allowed(&program, &self.allowed) {
            return Err(AppError::StdioDisabled);
        }
        let mut child = Command::new(&program)
            .args(&args)
            // No inherited environment except PATH: the child works with what it is given.
            .env_clear()
            .env("PATH", std::env::var("PATH").unwrap_or_else(|_| "/usr/bin:/bin".to_owned()))
            .env("HOME", std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_owned()))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|error| {
                tracing::warn!(%error, server = %server.name, "the stdio server did not start");
                AppError::Unavailable
            })?;
        let mut stdin = child.stdin.take().ok_or(AppError::Unavailable)?;
        let stdout = child.stdout.take().ok_or(AppError::Unavailable)?;
        let mut reader = BufReader::new(stdout).take(MAX_LINE_BYTES);

        // initialize
        let init = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": { "name": "boma-bot", "version": env!("CARGO_PKG_VERSION") },
            },
        });
        self.write_line(&mut stdin, &init).await?;
        let initialized = Self::read_line(&mut reader, 1).await?;

        // initialized notification (no response expected)
        self.write_line(
            &mut stdin,
            &serde_json::json!({
                "jsonrpc": "2.0",
                "method": "notifications/initialized",
            }),
        )
        .await?;

        // The actual call
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": method,
            "params": params,
        });
        self.write_line(&mut stdin, &request).await?;
        let _ = initialized;
        let result = Self::read_line(&mut reader, 2).await?;

        // The child answers, or the call is lost when the child dies with its stdin closing.
        drop(stdin);
        let _ = child.wait().await;
        Ok(result)
    }

    /// Writes one newline-delimited JSON-RPC message to the child's stdin.
    async fn write_line(
        &self,
        stdin: &mut tokio::process::ChildStdin,
        message: &serde_json::Value,
    ) -> Result<(), AppError> {
        let line = format!("{message}\n");
        tokio::time::timeout(REQUEST_TIMEOUT, stdin.write_all(line.as_bytes()))
            .await
            .map_err(|_| AppError::Unavailable)?
            .map_err(|_| AppError::Unavailable)
    }

    /// Reads lines until the JSON-RPC message with `id` arrives, bounded in size and time.
    async fn read_line(
        reader: &mut tokio::io::Take<BufReader<tokio::process::ChildStdout>>,
        id: i64,
    ) -> Result<serde_json::Value, AppError> {
        loop {
            let mut line = String::new();
            let Ok(Ok(read)) =
                tokio::time::timeout(REQUEST_TIMEOUT, reader.read_line(&mut line)).await
            else {
                return Err(AppError::Unavailable);
            };
            if read == 0 {
                // The server closed its stdout without answering: it cannot answer.
                return Err(AppError::Unavailable);
            }
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let Ok(parsed) = serde_json::from_str::<serde_json::Value>(trimmed) else {
                continue;
            };
            if parsed.get("id").and_then(serde_json::Value::as_i64) != Some(id) {
                // A notification or another id's message: not ours, keep reading.
                continue;
            }
            if let Some(error) = parsed.get("error") {
                let message = error
                    .get("message")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("the server refused the call");
                return Err(AppError::Rule(boma_domain::DomainError::InvalidName(
                    message.to_owned(),
                )));
            }
            return Ok(parsed
                .get("result")
                .cloned()
                .unwrap_or(serde_json::Value::Null));
        }
    }
}

#[async_trait::async_trait]
impl McpTransport for StdioMcpTransport {
    async fn list_tools(
        &self,
        server: &McpServerRecord,
        _bearer: Option<&secrecy::SecretString>,
    ) -> Result<Vec<McpTool>, AppError> {
        let result = self
            .rpc(server, "tools/list", serde_json::json!({}))
            .await?;
        Ok(crate::mcp_http::decode_tools(&result))
    }

    async fn call_tool(
        &self,
        server: &McpServerRecord,
        _bearer: Option<&secrecy::SecretString>,
        tool: &str,
        input: &serde_json::Value,
    ) -> Result<serde_json::Value, AppError> {
        boma_domain::sanitize_mcp_tool_name(tool)?;
        self.rpc(
            server,
            "tools/call",
            serde_json::json!({
                "name": tool,
                "arguments": input,
            }),
        )
        .await
    }
}
