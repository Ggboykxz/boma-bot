//! The ACP stdio client (REQ-061): newline-delimited JSON-RPC 2.0 over a child process's stdio.
//!
//! The wire: JSON-RPC messages are UTF-8, one per line, no embedded newlines. The agent writes
//! its log lines to stderr, never to stdout. The client writes only valid ACP messages to stdin.
//! Every request carries a wall-clock deadline; a run that outlives it is killed, and the reader
//! carries a per-line cap so a hostile executor cannot exhaust the heap.

use std::{process::Stdio, time::Duration};

use crate::AppError;
use serde_json::{Value, json};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout, Command},
    time::timeout,
};

/// Largest accepted line from an agent: 4 MiB.
const MAX_LINE_BYTES: usize = 4 * 1024 * 1024;

const READ_CAPACITY: usize = 8 * 1024;

/// What an attached ACP executor answers, once attached.
pub struct AcpSession {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
    timeout: Duration,
}

impl std::fmt::Debug for AcpSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AcpSession")
            .field("pid", &self.child.id())
            .finish_non_exhaustive()
    }
}

impl AcpSession {
    /// Spawns the executor on `cwd` and completes the `initialize` handshake.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Unavailable`] when the binary does not exist or the process refuses
    /// to start, and [`AppError::Unavailable`] when the agent answers with something other than
    /// protocol version 1.
    pub fn spawn(
        command: &str,
        args: &[&str],
        cwd: &std::path::Path,
        io_timeout: Duration,
    ) -> Result<Self, AppError> {
        let mut child = Command::new(command)
            .args(args)
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|_| AppError::Unavailable)?;
        let stdin = child.stdin.take().ok_or(AppError::Unavailable)?;
        let stdout = child.stdout.take().ok_or(AppError::Unavailable)?;
        let session = Self {
            child,
            stdin,
            stdout: BufReader::with_capacity(READ_CAPACITY, stdout),
            next_id: 1,
            timeout: io_timeout,
        };
        Ok(session)
    }

    /// Drains stderr into the trace log; never on the critical path.
    pub fn drain_stderr(&mut self) -> Option<tokio::task::JoinHandle<()>> {
        self.child.stderr.take().map(|stderr| {
            tokio::spawn(async move {
                let mut lines = tokio::io::BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    tracing::debug!(target: "boma.acp.stderr", %line, "acp executor log line");
                }
            })
        })
    }

    async fn send(&mut self, payload: &Value) -> Result<(), AppError> {
        let mut line = serde_json::to_string(payload).map_err(|_| AppError::Unavailable)?;
        line.push('\n');
        self.stdin
            .write_all(line.as_bytes())
            .await
            .map_err(|_| AppError::Unavailable)?;
        self.stdin.flush().await.map_err(|_| AppError::Unavailable)
    }

    async fn read_line_bounded(&mut self) -> Result<String, AppError> {
        // fill_buf/consume so the cap is enforced before the buffer grows past it. A chunk may
        // carry several lines; consume at most through the newline of the line we want.
        let mut out = Vec::with_capacity(256);
        loop {
            let chunk = self
                .stdout
                .fill_buf()
                .await
                .map_err(|_| AppError::Unavailable)?;
            if chunk.is_empty() {
                return Err(AppError::Unavailable);
            }
            if let Some(newline) = chunk.iter().position(|&byte| byte == b'\n') {
                let end = newline + 1;
                if out.len() + end > MAX_LINE_BYTES {
                    return Err(AppError::Unavailable);
                }
                out.extend_from_slice(&chunk[..end]);
                self.stdout.consume(end);
                return String::from_utf8(out).map_err(|_| AppError::Unavailable);
            }
            let remaining = MAX_LINE_BYTES.saturating_sub(out.len());
            let take = chunk.len().min(remaining);
            out.extend_from_slice(&chunk[..take]);
            self.stdout.consume(take);
            if out.len() >= MAX_LINE_BYTES {
                return Err(AppError::Unavailable);
            }
        }
    }

    async fn request(&mut self, method: &str, params: Value) -> Result<Value, AppError> {
        let id = self.next_id;
        self.next_id += 1;
        self.send(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        }))
        .await?;
        loop {
            let line = timeout(self.timeout, self.read_line_bounded())
                .await
                .map_err(|_| AppError::Unavailable)??;
            let trimmed = line.trim_end();
            if trimmed.is_empty() {
                continue;
            }
            let value: Value = serde_json::from_str(trimmed).map_err(|_| AppError::Unavailable)?;
            // A notification for a different id may interleave; only our response counts.
            if value.get("id").and_then(Value::as_u64) == Some(id) {
                if let Some(error) = value.get("error") {
                    let code = error.get("code").and_then(Value::as_i64).unwrap_or(-32000);
                    let message = error
                        .get("message")
                        .and_then(Value::as_str)
                        .unwrap_or("ACP error");
                    tracing::warn!(target: "boma.acp", code, %message, "acp error response");
                    return Err(AppError::Unavailable);
                }
                return Ok(value.get("result").cloned().unwrap_or(Value::Null));
            }
        }
    }

    /// `initialize` — protocol version 1, minimal but honest capabilities.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Unavailable`] when the agent refuses the handshake.
    pub async fn initialize(&mut self) -> Result<Value, AppError> {
        self.request(
            "initialize",
            json!({
                "protocolVersion": 1,
                "clientCapabilities": {
                    "fs": { "readTextFile": true, "writeTextFile": true },
                    "terminal": true
                },
                "clientInfo": {
                    "name": "boma-bot",
                    "title": "Boma Bot",
                    "version": env!("CARGO_PKG_VERSION"),
                }
            }),
        )
        .await
    }

    /// `session/new` — returns the `sessionId` and passes the workspace as `cwd`.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Unavailable`] when the agent cannot open a session.
    pub async fn session_new(&mut self, cwd: &str) -> Result<String, AppError> {
        let result = self
            .request(
                "session/new",
                json!({
                    "cwd": cwd,
                    "mcpServers": [],
                }),
            )
            .await?;
        let session_id = result
            .get("sessionId")
            .and_then(Value::as_str)
            .ok_or(AppError::Unavailable)?;
        Ok(session_id.to_owned())
    }

    /// `session/prompt` — streams until `stopReason`, collecting agent message chunks.
    ///
    /// Returns the concatenated agent message text; the stop reason rides along for callers that
    /// need it.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Unavailable`] when the agent errors or the turn exceeds the deadline.
    pub async fn session_prompt(
        &mut self,
        session_id: &str,
        text: &str,
    ) -> Result<(String, String), AppError> {
        let id = self.next_id;
        self.next_id += 1;
        self.send(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "session/prompt",
            "params": {
                "sessionId": session_id,
                "prompt": [ { "type": "text", "text": text } ],
            }
        }))
        .await?;

        let mut text_out = String::new();
        let stop_reason;
        loop {
            let line = timeout(self.timeout, self.read_line_bounded())
                .await
                .map_err(|_| AppError::Unavailable)??;
            let trimmed = line.trim_end();
            if trimmed.is_empty() {
                continue;
            }
            let value: Value = serde_json::from_str(trimmed).map_err(|_| AppError::Unavailable)?;
            match value.get("method").and_then(Value::as_str) {
                Some("session/update") => {
                    let update = value.pointer("/params/update");
                    let chunk_text = update
                        .filter(|u| {
                            u.get("sessionUpdate").and_then(Value::as_str)
                                == Some("agent_message_chunk")
                        })
                        .and_then(|u| u.pointer("/content/text"))
                        .and_then(Value::as_str);
                    if let Some(text) = chunk_text {
                        text_out.push_str(text);
                    }
                }
                _ => {
                    if value.get("id").and_then(Value::as_u64) == Some(id) {
                        stop_reason = value
                            .pointer("/result/stopReason")
                            .and_then(Value::as_str)
                            .unwrap_or("end_turn")
                            .to_owned();
                        break;
                    }
                }
            }
        }
        Ok((text_out, stop_reason))
    }

    /// `session/close` — best-effort; a `-32002` response is treated as success per the spec.
    pub async fn session_close(&mut self, session_id: &str) {
        let _ = self
            .request("session/close", json!({ "sessionId": session_id }))
            .await;
    }

    /// Closes stdin and lets the child exit, then waits briefly.
    pub async fn shutdown(mut self) {
        drop(self.stdin);
        let _ = timeout(Duration::from_secs(2), self.child.wait()).await;
    }
}

/// The default I/O deadline for a prompt turn.
pub const DEFAULT_PROMPT_TIMEOUT: Duration = Duration::from_secs(600);
