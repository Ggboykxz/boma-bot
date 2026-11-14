//! The OAuth client for MCP servers (REQ-033).
//!
//! Two jobs: mint a PKCE pair, and exchange a code at the token endpoint. The authorization
//! endpoint is driven by the member's browser at the server itself — the client never sees a
//! browser flow, only the token exchange, and it runs no JavaScript. Codes are single-shot in
//! this build by convention; the session row is deleted at exchange time either way.

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use boma_app::{AppError, ports::OAuthClient};
use secrecy::SecretString;
use sha2::{Digest, Sha256};

/// The OAuth token endpoint client.
#[derive(Debug, Clone)]
pub struct OAuthTokenClient {
    http: reqwest::Client,
}

impl OAuthTokenClient {
    /// Builds the client with a 30-second timeout and no redirects.
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
}

#[async_trait::async_trait]
impl OAuthClient for OAuthTokenClient {
    async fn pkce_pair(&self) -> Result<(SecretString, String), AppError> {
        let mut bytes = [0u8; 32];
        getrandom::fill(&mut bytes).map_err(|_| AppError::Unavailable)?;
        let verifier = URL_SAFE_NO_PAD.encode(bytes);
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
        Ok((SecretString::from(verifier), challenge))
    }

    async fn exchange_grant(
        &self,
        token_url: &str,
        form: &[(String, String)],
    ) -> Result<serde_json::Value, AppError> {
        // Form encoding, not JSON: the AS speaks what RFC 6749 says.
        let encoded = form
            .iter()
            .map(|(key, value)| format!("{key}={}", boma_domain::percent_encode(value)))
            .collect::<Vec<_>>()
            .join("&");
        let response = self
            .http
            .post(token_url)
            .header(
                reqwest::header::CONTENT_TYPE,
                "application/x-www-form-urlencoded",
            )
            .body(encoded)
            .send()
            .await
            .map_err(|error| {
                tracing::warn!(%error, "the token endpoint did not answer");
                AppError::Unavailable
            })?;
        let status = response.status();
        let body = response.bytes().await.map_err(|_| AppError::Unavailable)?;
        if body.len() > 64 * 1024 {
            return Err(AppError::Rule(boma_domain::DomainError::InvalidName(
                "the token endpoint answered with an oversized body".to_owned(),
            )));
        }
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap_or_else(|_| {
            // A non-JSON failure body: wrap status into a stable refusal.
            serde_json::json!({ "error": format!("the token endpoint answered with status {status}") })
        });
        if !status.is_success() {
            let description = json
                .get("error_description")
                .or_else(|| json.get("error"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or("the authorization server refused the code");
            return Err(AppError::Rule(boma_domain::DomainError::InvalidName(
                description.to_owned(),
            )));
        }
        Ok(json)
    }
}
