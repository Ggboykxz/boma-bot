//! The model catalogue this deployment serves (REQ-052).
//!
//! A deployment declares the catalogue keys it reaches (`DeploymentModels::reachable`); this
//! module gives those keys display metadata — a name, a context window, the roles a model
//! supports, and the indicative prices used for month-to-date spend accounting (REQ-018).
//! The catalogue is illustrative for the built-in providers and deliberately small: it exists
//! so the product can list what a deployment offers without a live call to the provider.
//! Prices are US dollars per million tokens, and are *indicative*, not billing: the spending
//! limit accounts them, not the user's invoice.

use serde::{Deserialize, Serialize};

use crate::model::DeploymentModels;

/// The price of one model row, in minor currency units per million tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelPrice {
    /// Input tokens, minor units (cents) per million.
    pub input_per_million: u64,
    /// Output tokens, minor units (cents) per million.
    pub output_per_million: u64,
}

/// One model row of a provider, as the catalogue exposes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogModel {
    /// The catalogue key of the model (`provider/model`).
    pub key: String,
    /// The model identifier the provider calls it.
    pub id: String,
    /// A name a person can read.
    pub name: String,
    /// The context window in tokens.
    pub context_window: u64,
    /// The roles the model supports; `chat` is the one this product uses.
    pub roles: Vec<String>,
    /// The indicative price. Zero means the deployment pays nothing for it (local, scripted).
    pub price: ModelPrice,
}

/// One provider of the catalogue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogProvider {
    /// The provider key of the catalogue (`anthropic`).
    pub id: String,
    /// A name a person can read.
    pub name: String,
    /// The models the deployment reaches for this provider.
    pub models: Vec<CatalogModel>,
}

/// The shape a model endpoint expects credentials in, and where it lives by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialShape {
    /// OpenAI-compatible `GET /models` with a Bearer key.
    OpenAiCompatible,
    /// Anthropic's `/v1/models` with `x-api-key`.
    Anthropic,
    /// Ollama's unauthenticated `/api/tags`.
    Ollama,
}

/// One built-in provider endpoint the catalogue knows how to validate credentials against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderEndpoint {
    /// The absolute models URL, when the built-in endpoint exists.
    pub url: &'static str,
    /// The auth shape the endpoint expects.
    pub shape: CredentialShape,
}

/// The built-in endpoint of a provider, for credential validation.
///
/// `None` for a provider the catalogue cannot place: connecting its credential is refused with
/// "unknown provider", never guessed at.
#[must_use]
pub fn provider_endpoint(provider: &str) -> Option<ProviderEndpoint> {
    match provider {
        "openai" => Some(ProviderEndpoint {
            url: "https://api.openai.com/v1/models",
            shape: CredentialShape::OpenAiCompatible,
        }),
        "anthropic" => Some(ProviderEndpoint {
            url: "https://api.anthropic.com/v1/models",
            shape: CredentialShape::Anthropic,
        }),
        "openrouter" => Some(ProviderEndpoint {
            url: "https://openrouter.ai/api/v1/models",
            shape: CredentialShape::OpenAiCompatible,
        }),
        "ollama" => Some(ProviderEndpoint {
            url: "http://127.0.0.1:11434/api/tags",
            shape: CredentialShape::Ollama,
        }),
        _ => None,
    }
}

/// The built-in catalogue rows of a provider: `(id, name, contextWindow, input$/M, output$/M)`.
///
/// Prices in cents per million tokens; fictional illustrative values, matching the tier of the
/// model — the point of the table is that the spending limit can account a call, not that the
/// numbers match any invoice.
fn provider_models(provider: &str) -> Vec<CatalogModel> {
    let row = |id: &str, name: &str, window: u64, input: u64, output: u64| CatalogModel {
        key: format!("{provider}/{id}"),
        id: id.to_owned(),
        name: name.to_owned(),
        context_window: window,
        roles: vec!["chat".to_owned()],
        price: ModelPrice {
            input_per_million: input,
            output_per_million: output,
        },
    };
    match provider {
        "openai" => vec![
            row("gpt-5", "GPT-5", 400_000, 125, 1_000),
            row("gpt-5-mini", "GPT-5 mini", 400_000, 25, 200),
        ],
        "anthropic" => vec![
            row("claude-opus", "Claude Opus", 200_000, 500, 2_500),
            row("claude-sonnet", "Claude Sonnet", 200_000, 300, 1_500),
            row("claude-haiku", "Claude Haiku", 200_000, 80, 400),
        ],
        "openrouter" => vec![
            row("router-auto", "OpenRouter Auto", 200_000, 200, 800),
            row("router-fast", "OpenRouter Fast", 128_000, 60, 300),
        ],
        "ollama" => vec![
            row("llama-3.3-70b", "Llama 3.3 70B (local)", 128_000, 0, 0),
            row(
                "qwen2.5-coder-32b",
                "Qwen 2.5 Coder 32B (local)",
                32_000,
                0,
                0,
            ),
        ],
        _ => Vec::new(),
    }
}

/// The display name of a provider.
#[must_use]
pub fn provider_name(provider: &str) -> &'static str {
    match provider {
        "openai" => "OpenAI",
        "anthropic" => "Anthropic",
        "openrouter" => "OpenRouter",
        "ollama" => "Ollama (local)",
        _ => "Custom",
    }
}

/// The providers this deployment reaches, with display metadata.
///
/// Reachable keys the built-in table does not know are listed with their raw key as the id and
/// a default context window, so a deployment naming its own provider is never silently dropped
/// from the catalogue.
#[must_use]
pub fn catalogue_for(models: &DeploymentModels) -> Vec<CatalogProvider> {
    let mut providers: Vec<CatalogProvider> = Vec::new();
    for key in &models.reachable {
        let (provider, model) = key.split_once('/').unwrap_or(("custom", key.as_str()));
        let known = provider_models(provider)
            .into_iter()
            .find(|row| row.id == model);
        let row = known.unwrap_or_else(|| CatalogModel {
            key: key.clone(),
            id: model.to_owned(),
            name: model.to_owned(),
            context_window: 128_000,
            roles: vec!["chat".to_owned()],
            price: ModelPrice {
                input_per_million: 0,
                output_per_million: 0,
            },
        });
        match providers.iter_mut().find(|entry| entry.id == provider) {
            Some(entry) => entry.models.push(row),
            None => providers.push(CatalogProvider {
                id: provider.to_owned(),
                name: provider_name(provider).to_owned(),
                models: vec![row],
            }),
        }
    }
    providers
}

/// The indicative price of a catalogue key, when the catalogue knows it.
#[must_use]
pub fn price_of(key: &str) -> Option<ModelPrice> {
    catalogue_for(&DeploymentModels {
        reachable: vec![key.to_owned()],
        default: key.to_owned(),
    })
    .into_iter()
    .flat_map(|provider| provider.models)
    .next()
    .map(|model| model.price)
}

#[cfg(test)]
mod tests {
    use super::{catalogue_for, price_of, provider_endpoint};
    use crate::model::DeploymentModels;

    fn deployment() -> DeploymentModels {
        DeploymentModels {
            reachable: vec![
                "anthropic/claude-sonnet".to_owned(),
                "openai/gpt-5".to_owned(),
            ],
            default: "anthropic/claude-sonnet".to_owned(),
        }
    }

    #[test]
    fn req_052_the_catalogue_lists_what_the_deployment_reaches_in_declaration_order() {
        let providers = catalogue_for(&deployment());
        assert_eq!(providers.len(), 2);
        assert_eq!(providers[0].id, "anthropic");
        assert_eq!(providers[0].models[0].id, "claude-sonnet");
        assert_eq!(providers[0].models[0].context_window, 200_000);
        assert!(providers[0].models[0].roles.contains(&"chat".to_owned()));
        assert_eq!(providers[1].id, "openai");
        assert!(providers[1].models[0].price.output_per_million > 0);
    }

    #[test]
    fn req_052_an_unknown_key_is_listed_with_a_documented_default_never_dropped() {
        let providers = catalogue_for(&DeploymentModels {
            reachable: vec!["mystery/qwen-99".to_owned()],
            default: "mystery/qwen-99".to_owned(),
        });
        assert_eq!(providers.len(), 1);
        assert_eq!(providers[0].models[0].id, "qwen-99");
        assert_eq!(providers[0].models[0].context_window, 128_000);
        assert_eq!(providers[0].models[0].price.input_per_million, 0);
    }

    #[test]
    fn req_052_prices_are_indicative_and_known_for_builtin_models() {
        let price = price_of("anthropic/claude-sonnet").expect("a builtin price");
        assert_eq!(price.input_per_million, 300);
        assert!(price_of("ollama/llama-3.3-70b").is_some_and(|price| price.input_per_million == 0));
        assert!(price_of("custom/x").is_some());
    }

    #[test]
    fn req_052_credential_endpoints_exist_for_builtin_providers_only() {
        assert!(provider_endpoint("openai").is_some());
        assert!(provider_endpoint("anthropic").is_some());
        assert!(provider_endpoint("ollama").is_some());
        assert!(provider_endpoint("mystery").is_none());
    }
}
