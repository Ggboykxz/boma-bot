//! External AG-UI agent executors, as a provider behind the same port every run sees (REQ-059).
//!
//! An operator-side executor speaks the agent protocol over `text/event-stream`: each reply is a
//! list of events such as `TEXT_MESSAGE_START|CONTENT|END` and `TOOL_CALL_START|ARGS|END`. We
//! translate the executor's stream into the internal `ProviderTurn` so the existing engine owns
//! the vehicle — no second run loop is created; the run lifecycle and the approval gate apply
//! just as for the in-process provider.
//!
//! The bearer is read from the space's bot-secret vault at call time; it is never persisted.

use std::sync::Arc;

use async_trait::async_trait;
use secrecy::SecretString;

use crate::AppError;
use crate::ports::{ModelProvider, ProviderRequest, ProviderTurn, ToolCallRequest};

/// What an external executor is told to run and how the reply is reported.
#[derive(Debug, Clone)]
pub struct AgUiExecutor {
    /// The executor base address (the path `POST` goes to).
    pub base_url: String,
    /// Which agent the executor should run, when it exposes several.
    pub agent_name: Option<String>,
    /// Bearer token. Source: the space's bot-secret vault; never persist it here.
    pub bearer: SecretString,
}

/// The AG-UI provider, stateless per call.
#[derive(Debug, Clone)]
pub struct AgUiProvider {
    http: reqwest::Client,
    executor: Arc<AgUiExecutor>,
}

impl AgUiProvider {
    /// Builds the provider.
    #[must_use]
    pub fn new(executor: AgUiExecutor) -> Self {
        Self {
            http: reqwest::Client::new(),
            executor: Arc::new(executor),
        }
    }
}

/// The AG-UI run body the executor consumes.
fn agui_body(agent_name: Option<&String>, request: &ProviderRequest) -> serde_json::Value {
    let mut messages: Vec<serde_json::Value> = Vec::with_capacity(request.history.len() + 1);
    for message in &request.history {
        let role = match message.role {
            boma_domain::MessageRole::User => "user",
            boma_domain::MessageRole::Bot => "assistant",
            boma_domain::MessageRole::Tool => "tool",
            boma_domain::MessageRole::System => "system",
        };
        let text = message
            .blocks
            .iter()
            .filter_map(|block| match block {
                boma_domain::Block::Text { text } => Some(text.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");
        messages.push(serde_json::json!({
            "id": message.id.as_str(),
            "role": role,
            "content": text,
        }));
    }
    messages.push(serde_json::json!({
        "id": format!("msg_{}", request.thread.id.as_str()),
        "role": "user",
        "content": request.prompt,
    }));
    let tools: Vec<serde_json::Value> = request
        .tools
        .iter()
        .map(|tool| {
            serde_json::json!({
                "name": tool.key,
                "description": tool.description,
            })
        })
        .collect();
    let mut body = serde_json::json!({
        "threadId": request.thread.id.as_str(),
        "messages": messages,
        "tools": tools,
    });
    if let Some(name) = agent_name {
        body["agent"] = serde_json::Value::String(name.to_owned());
    }
    body
}

/// Coarses AG-UI events into a ProviderTurn.
fn parse_agui_stream(body: &str) -> Result<ProviderTurn, AppError> {
    let mut chunks: Vec<String> = Vec::new();
    let mut calls: Vec<ToolCallRequest> = Vec::new();
    let mut open_ids: Vec<(String, String)> = Vec::new();
    for line in body.lines() {
        let Some(payload) = line.strip_prefix("data: ") else {
            continue;
        };
        let event: serde_json::Value = match serde_json::from_str(payload) {
            Ok(value) => value,
            Err(_) => continue,
        };
        match event.get("type").and_then(serde_json::Value::as_str) {
            Some("TEXT_MESSAGE_CONTENT") => {
                let delta = event
                    .get("delta")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("");
                chunks.push(delta.to_owned());
            }
            Some("TOOL_CALL_START") => {
                let id = event
                    .get("toolCallId")
                    .and_then(serde_json::Value::as_str)
                    .ok_or(AppError::Unavailable)?;
                let name = event
                    .get("toolCallName")
                    .and_then(serde_json::Value::as_str)
                    .ok_or(AppError::Unavailable)?;
                open_ids.push((id.to_owned(), name.to_owned()));
            }
            Some("TOOL_CALL_ARGS") => {
                let id = event
                    .get("toolCallId")
                    .and_then(serde_json::Value::as_str)
                    .ok_or(AppError::Unavailable)?;
                let arguments = event
                    .get("delta")
                    .and_then(serde_json::Value::as_str)
                    .ok_or(AppError::Unavailable)?;
                calls.push(ToolCallRequest {
                    id: id.to_owned(),
                    name: open_ids
                        .iter()
                        .find(|(open_id, _)| *open_id == id)
                        .map(|(_, name)| name.clone())
                        .ok_or(AppError::Unavailable)?,
                    arguments: serde_json::from_str(arguments)
                        .unwrap_or_else(|_| serde_json::json!({ "arguments": arguments })),
                });
            }
            Some("RUN_ERROR") => {
                tracing::warn!(?event, "the external executor reported a failure");
                return Err(AppError::Unavailable);
            }
            _ => {}
        }
    }
    if !calls.is_empty() {
        return Ok(ProviderTurn::ToolCalls(calls));
    }
    Ok(ProviderTurn::Chunks(vec![chunks.join("")]))
}

#[async_trait]
impl ModelProvider for AgUiProvider {
    async fn complete(&self, request: ProviderRequest) -> Result<ProviderTurn, AppError> {
        let endpoint = self.executor.base_url.trim_end_matches('/').to_owned();
        let body = agui_body(self.executor.agent_name.as_ref(), &request);
        let response = self
            .http
            .post(endpoint)
            .header(
                reqwest::header::AUTHORIZATION,
                format!(
                    "Bearer {}",
                    secrecy::ExposeSecret::expose_secret(&self.executor.bearer)
                ),
            )
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|error| {
                tracing::warn!(%error, "the external executor did not answer");
                AppError::Unavailable
            })?;
        let status = response.status();
        if !status.is_success() {
            tracing::warn!(%status, "the external executor refused the request");
            return Err(AppError::Unavailable);
        }
        let text = response.text().await.map_err(|_| AppError::Unavailable)?;
        parse_agui_stream(&text)
    }
}
