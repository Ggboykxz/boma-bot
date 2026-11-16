//! Loopback local model server discovery (REQ-075).
//!
//! Ollama (`127.0.0.1:11434/api/tags`) and LM Studio's OpenAI-compatible endpoint
//! (`127.0.0.1:1234/v1/models`) are probed on every list; the answer is best effort and never
//! blocks startup. A server that does not answer, or answers garbage, contributes nothing.

use serde_json::Value;

/// One model offered by a local server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalDiscoveredModel {
    /// Which server offered it: `ollama` or `lm-studio`.
    pub source: String,
    /// The model identifier the server listed.
    pub id: String,
}

/// What the local servers answered, tagged by source. Empty means none reachable.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LocalModels {
    /// The discovered models, Ollama first.
    pub models: Vec<LocalDiscoveredModel>,
}

const TIMEOUT_MS: u64 = 2_000;

/// Probes the canonical loopback ports; never fails, never blocks for long.
pub async fn discover() -> LocalModels {
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(TIMEOUT_MS))
        .build()
    {
        Ok(client) => client,
        Err(_) => return LocalModels::default(),
    };
    let mut models = Vec::new();
    if let Ok(body) = ollama_tags(&client).await {
        models.extend(body);
    }
    if let Ok(body) = lmstudio_models(&client).await {
        models.extend(body);
    }
    LocalModels { models }
}

async fn ollama_tags(client: &reqwest::Client) -> Result<Vec<LocalDiscoveredModel>, AppError> {
    let body: Value = client
        .get("http://127.0.0.1:11434/api/tags")
        .send()
        .await
        .map_err(|_| AppError::Unavailable)?
        .json()
        .await
        .map_err(|_| AppError::Unavailable)?;
    let mut out = Vec::new();
    for entry in body
        .get("models")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if let Some(id) = entry.get("name").and_then(Value::as_str) {
            out.push(LocalDiscoveredModel {
                source: "ollama".to_owned(),
                id: id.to_owned(),
            });
        }
    }
    Ok(out)
}

async fn lmstudio_models(client: &reqwest::Client) -> Result<Vec<LocalDiscoveredModel>, AppError> {
    let body: Value = client
        .get("http://127.0.0.1:1234/v1/models")
        .send()
        .await
        .map_err(|_| AppError::Unavailable)?
        .json()
        .await
        .map_err(|_| AppError::Unavailable)?;
    let mut out = Vec::new();
    for entry in body
        .get("data")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if let Some(id) = entry.get("id").and_then(Value::as_str) {
            out.push(LocalDiscoveredModel {
                source: "lm-studio".to_owned(),
                id: id.to_owned(),
            });
        }
    }
    Ok(out)
}

use boma_app::AppError;
