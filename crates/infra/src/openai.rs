//! The bring-your-own-key adapter for OpenAI-compatible providers (REQ-015).
//!
//! The request is built inline from the run's own context — the history, the scoped memory and
//! the tools catalogue — and the wire response is mapped back onto the same [`ProviderTurn`]
//! values the scripted provider produces. `finish_reason` and `tool_calls` are translated, and a
//! refusal from the provider is `Unavailable`, which is the one error the worker re-tries on
//! (REQ-024). No live network call is made by tests: the fixture replay suite drives this
//! adapter against a loopback wire server, exactly what a deployment calls in production.

use boma_app::{
    AppError,
    ports::{ModelProvider, ProviderRequest, ProviderTurn, ToolCallRequest},
};
use secrecy::ExposeSecret;

/// How the deployment reaches the provider.
///
/// The key is a [`secrecy::SecretString`]: the adapter never logs, formats or returns it. The
/// model is pinned per deployment; a per-run model override would ride the same port with
/// another milestone (REQ-015 per-run selection already lands as the run's resolved model).
#[derive(Clone)]
pub struct OpenAiConfig {
    /// The base URL, e.g. `https://api.openai.com/v1`. A trailing slash is tolerated.
    pub endpoint: String,
    /// The model identifier the deployment calls.
    pub model: String,
    /// The operator's key.
    pub api_key: secrecy::SecretString,
}

impl std::fmt::Debug for OpenAiConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The key is deliberately absent: the Debug output is safe to log.
        f.debug_struct("OpenAiConfig")
            .field("endpoint", &self.endpoint)
            .field("model", &self.model)
            .finish_non_exhaustive()
    }
}

/// The adapter is a [`reqwest`] client bound to one deployment configuration.
#[derive(Clone)]
pub struct OpenAiCompatibleAdapter {
    http: reqwest::Client,
    config: OpenAiConfig,
}

impl OpenAiCompatibleAdapter {
    /// Builds the adapter with a 60-second request timeout.
    ///
    /// # Errors
    ///
    /// Returns an error when the TLS backend cannot be initialised.
    pub fn new(config: OpenAiConfig) -> Result<Self, AppError> {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .map_err(|_| AppError::Unavailable)?;
        Ok(Self { http, config })
    }
}

/// The parameters schemas this deployment offers to the model for the built-in tools.
///
/// The schemas describe exactly what the dispatcher accepts; anything a model invents outside
/// them is refused at the gate, never executed.
fn parameters_for(key: &str) -> serde_json::Value {
    match key {
        "memory.read" => serde_json::json!({
            "type": "object",
            "properties": {
                "scope": { "type": "string", "enum": ["bot", "shared"] },
                "botId": { "type": "string" },
                "path": { "type": "string" }
            },
            "required": ["path"]
        }),
        "memory.write" => serde_json::json!({
            "type": "object",
            "properties": {
                "scope": { "type": "string", "enum": ["bot", "shared"] },
                "botId": { "type": "string" },
                "path": { "type": "string" },
                "text": { "type": "string" }
            },
            "required": ["path", "text"]
        }),
        "delegate.helper" => serde_json::json!({
            "type": "object",
            "properties": {
                "prompt": { "type": "string" }
            },
            "required": ["prompt"]
        }),
        "computer.observe" => serde_json::json!({
            "type": "object",
            "properties": {},
            "additionalProperties": false
        }),
        _ => serde_json::json!({
            "type": "object",
            "additionalProperties": true
        }),
    }
}

/// The full request body, as the wire expects it.
fn request_body(config: &OpenAiConfig, request: &ProviderRequest) -> serde_json::Value {
    let mut messages = Vec::with_capacity(request.history.len() + 2);
    if !request.memory.is_empty() {
        let mut documents = String::from("Documents the bot may use, by path:\n");
        for document in &request.memory {
            use std::fmt::Write as _;
            let _ = write!(
                documents,
                "--- {} ({}) ---\n{}\n\n",
                document.path,
                document.scope.as_str(),
                document.content
            );
        }
        messages.push(serde_json::json!({ "role": "system", "content": documents }));
    }
    for message in &request.history {
        let text = message
            .blocks
            .iter()
            .filter_map(boma_domain::Block::as_text)
            .collect::<Vec<_>>()
            .join("\n");
        match message.role {
            boma_domain::MessageRole::User => {
                messages.push(serde_json::json!({ "role": "user", "content": text }));
            }
            boma_domain::MessageRole::Bot => {
                messages.push(serde_json::json!({ "role": "assistant", "content": text }));
            }
            boma_domain::MessageRole::System => {
                messages.push(serde_json::json!({ "role": "system", "content": text }));
            }
            boma_domain::MessageRole::Tool => {
                // Tool results ride the tool role when they carry the correlation envelope
                // the product writes; anything else is shown as a user note instead of a
                // malformed tool message the provider would reject.
                match serde_json::from_str::<serde_json::Value>(&text) {
                    Ok(envelope) => match envelope.get("id").and_then(serde_json::Value::as_str) {
                        Some(id) => messages.push(serde_json::json!({
                            "role": "tool",
                            "tool_call_id": id,
                            "content": envelope.get("body").unwrap_or(&envelope).to_string(),
                        })),
                        None => messages.push(serde_json::json!({ "role": "user", "content": format!("[tool] {text}") })),
                    },
                    Err(_) => messages.push(serde_json::json!({ "role": "user", "content": format!("[tool] {text}") })),
                }
            }
        }
    }
    messages.push(serde_json::json!({ "role": "user", "content": request.prompt }));
    let tools: Vec<serde_json::Value> = request
        .tools
        .iter()
        .map(|tool| {
            serde_json::json!({
                "type": "function",
                "function": {
                    "name": tool.key,
                    "description": tool.description,
                    "parameters": parameters_for(&tool.key),
                }
            })
        })
        .collect();
    serde_json::json!({
        "model": config.model,
        "messages": messages,
        "tools": tools,
        "tool_choice": "auto",
        "stream": false,
    })
}

/// The answer of one `chat/completions` call, mapped back onto the port's turn.
fn parse_message(message: &serde_json::Value) -> Result<ProviderTurn, AppError> {
    if let Some(tool_calls) = message
        .get("tool_calls")
        .and_then(serde_json::Value::as_array)
        && !tool_calls.is_empty()
    {
        let mut calls = Vec::with_capacity(tool_calls.len());
        for call in tool_calls {
            let id = call
                .get("id")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("call")
                .to_owned();
            let function = call.get("function").cloned().unwrap_or_default();
            let name = function
                .get("name")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("")
                .to_owned();
            let arguments = function
                .get("arguments")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("{}");
            // A malformed argument set from the model retries fresh: the run fails legibly
            // instead of parsing a guess into an action (REQ-016).
            let arguments = serde_json::from_str(arguments).map_err(|_| AppError::Unavailable)?;
            calls.push(ToolCallRequest {
                id,
                name,
                arguments,
            });
        }
        return Ok(ProviderTurn::ToolCalls(calls));
    }
    let content = message
        .get("content")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .to_owned();
    let chunks: Vec<String> = content.split_inclusive(' ').map(str::to_owned).collect();
    Ok(ProviderTurn::Chunks(if chunks.is_empty() {
        vec![String::new()]
    } else {
        chunks
    }))
}

#[async_trait::async_trait]
impl ModelProvider for OpenAiCompatibleAdapter {
    async fn complete(&self, request: ProviderRequest) -> Result<ProviderTurn, AppError> {
        let endpoint = format!(
            "{}/chat/completions",
            self.config.endpoint.trim_end_matches('/')
        );
        let body = request_body(&self.config, &request);
        let response = self
            .http
            .post(&endpoint)
            .header(
                reqwest::header::AUTHORIZATION,
                format!("Bearer {}", self.config.api_key.expose_secret()),
            )
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|error| {
                tracing::warn!(%error, "the provider did not answer");
                AppError::Unavailable
            })?;
        let status = response.status();
        if !status.is_success() {
            tracing::warn!(%status, "the provider refused the request");
            return Err(AppError::Unavailable);
        }
        let answer: serde_json::Value = response.json().await.map_err(|error| {
            tracing::warn!(%error, "the provider answered in a shape this build cannot read");
            AppError::Unavailable
        })?;
        let Some(message) = answer
            .get("choices")
            .and_then(serde_json::Value::as_array)
            .and_then(|choices| choices.first())
            .and_then(|choice| choice.get("message"))
            .cloned()
        else {
            tracing::warn!("the provider answered without a message");
            return Err(AppError::Unavailable);
        };
        parse_message(&message)
    }
}
