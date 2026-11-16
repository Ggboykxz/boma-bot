//! Loopback local model server discovery (REQ-075).
//!
//! Ollama (`/api/tags`) and LM Studio's OpenAI-compatible endpoint (`/v1/models`) are probed on
//! every call; the answer is best effort and never blocks startup. A server that does not answer
//! — or answers garbage — contributes nothing, and discovery never fails.

use serde_json::Value;

/// One model offered by a local server.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
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

/// The discovery with the deployment's defaults (env-overridable for tests and odd setups).
pub async fn discover() -> LocalModels {
    let ollama =
        std::env::var("BOMA_OLLAMA_URL").unwrap_or_else(|_| "http://127.0.0.1:11434".to_owned());
    let lmstudio =
        std::env::var("BOMA_LMSTUDIO_URL").unwrap_or_else(|_| "http://127.0.0.1:1234".to_owned());
    discover_with_urls(&ollama, &lmstudio).await
}

/// The discovery against explicit base URLs. Never fails; every server contributes or nothing.
pub async fn discover_with_urls(ollama: &str, lmstudio: &str) -> LocalModels {
    let Ok(client) = reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(TIMEOUT_MS))
        .build()
    else {
        return LocalModels::default();
    };
    let mut models = Vec::new();
    if let Ok(ollama_body) = get_json(&client, &format!("{ollama}/api/tags")).await {
        models.extend(parse_ollama_tags(&ollama_body));
    }
    if let Ok(lmstudio_body) = get_json(&client, &format!("{lmstudio}/v1/models")).await {
        models.extend(parse_lmstudio_models(&lmstudio_body));
    }
    LocalModels { models }
}

async fn get_json(client: &reqwest::Client, url: &str) -> Result<Value, AppError> {
    client
        .get(url)
        .send()
        .await
        .map_err(|_| AppError::Unavailable)?
        .json()
        .await
        .map_err(|_| AppError::Unavailable)
}

/// The Ollama reply shape: `{ "models": [{ "name": "..." }] }`. Garbage contributes nothing.
#[must_use]
pub fn parse_ollama_tags(body: &Value) -> Vec<LocalDiscoveredModel> {
    body.get("models")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.get("name").and_then(Value::as_str))
        .map(|id| LocalDiscoveredModel {
            source: "ollama".to_owned(),
            id: id.to_owned(),
        })
        .collect()
}

/// The OpenAI-compatible reply shape: `{ "data": [{ "id": "..." }] }`. Garbage contributes
/// nothing.
#[must_use]
pub fn parse_lmstudio_models(body: &Value) -> Vec<LocalDiscoveredModel> {
    body.get("data")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.get("id").and_then(Value::as_str))
        .map(|id| LocalDiscoveredModel {
            source: "lm-studio".to_owned(),
            id: id.to_owned(),
        })
        .collect()
}

use crate::AppError;

#[cfg(test)]
mod tests {
    use super::{LocalDiscoveredModel, parse_lmstudio_models, parse_ollama_tags};

    #[test]
    fn req_075_the_ollama_reply_is_parsed_and_garbage_is_dropped() {
        let body = serde_json::json!({
            "models": [
                {"name": "qwen3:8b"},
                {"name": 42},
                "not-a-model",
                {"name": "deepseek-r1:7b"}
            ]
        });
        let models = parse_ollama_tags(&body);
        assert_eq!(
            models,
            vec![
                LocalDiscoveredModel {
                    source: "ollama".into(),
                    id: "qwen3:8b".into()
                },
                LocalDiscoveredModel {
                    source: "ollama".into(),
                    id: "deepseek-r1:7b".into()
                },
            ]
        );
        assert_eq!(
            parse_ollama_tags(&serde_json::json!({"models": "nope"})),
            [] as [LocalDiscoveredModel; 0]
        );
        assert_eq!(
            parse_ollama_tags(&serde_json::json!(42)),
            [] as [LocalDiscoveredModel; 0]
        );
    }

    #[test]
    fn req_075_the_lmstudio_reply_is_parsed_and_garbage_is_dropped() {
        let body = serde_json::json!({
            "data": [
                {"id": "llama-3.1-8b"},
                {"id": null},
                {"no_id": true},
                {"id": "mistral-7b"}
            ]
        });
        let models = parse_lmstudio_models(&body);
        assert_eq!(
            models,
            vec![
                LocalDiscoveredModel {
                    source: "lm-studio".into(),
                    id: "llama-3.1-8b".into()
                },
                LocalDiscoveredModel {
                    source: "lm-studio".into(),
                    id: "mistral-7b".into()
                },
            ]
        );
        assert_eq!(
            parse_lmstudio_models(&serde_json::json!({})),
            [] as [LocalDiscoveredModel; 0]
        );
    }
}
