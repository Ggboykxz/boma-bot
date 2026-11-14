//! The HTTP client behind the messaging ports (REQ-048).
//!
//! One client, no redirects, 30-second timeout. Slack and Telegram targets are fixed hosts
//! chosen by the product; a member-supplied generic target is checked against the SSRF rules
//! (HTTPS, no private hosts unless the installation opted in) before the transport is called.

use boma_app::ports::MessagingTransport;

/// The HTTPS client for platform calls.
#[derive(Debug, Clone)]
pub struct HttpMessagingTransport {
    http: reqwest::Client,
}

impl HttpMessagingTransport {
    /// Builds the transport with a 30-second timeout and no redirects.
    ///
    /// # Errors
    ///
    /// Returns [`boma_app::AppError::Unavailable`] when the TLS backend cannot be initialised.
    pub fn new() -> Result<Self, boma_app::AppError> {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| boma_app::AppError::Unavailable)?;
        Ok(Self { http })
    }
}

#[async_trait::async_trait]
impl MessagingTransport for HttpMessagingTransport {
    async fn get_json(
        &self,
        url: &str,
        headers: &[(String, String)],
    ) -> Result<serde_json::Value, boma_app::AppError> {
        let mut request = self.http.get(url);
        for (name, value) in headers {
            request = request.header(name.as_str(), value.as_str());
        }
        let response = request.send().await.map_err(|error| {
            tracing::warn!(%error, "a messaging request failed");
            boma_app::AppError::Unavailable
        })?;
        response
            .json::<serde_json::Value>()
            .await
            .map_err(|_| boma_app::AppError::Unavailable)
    }

    async fn post_form(
        &self,
        url: &str,
        form: &[(String, String)],
    ) -> Result<serde_json::Value, boma_app::AppError> {
        let body = form
            .iter()
            .map(|(key, value)| {
                format!(
                    "{}={}",
                    boma_domain::percent_encode(key),
                    boma_domain::percent_encode(value)
                )
            })
            .collect::<Vec<_>>()
            .join("&");
        let response = self
            .http
            .post(url)
            .header(
                reqwest::header::CONTENT_TYPE,
                "application/x-www-form-urlencoded",
            )
            .body(body)
            .send()
            .await
            .map_err(|error| {
                tracing::warn!(%error, "a messaging handshake failed");
                boma_app::AppError::Unavailable
            })?;
        response
            .json::<serde_json::Value>()
            .await
            .map_err(|_| boma_app::AppError::Unavailable)
    }

    async fn post_json(
        &self,
        url: &str,
        headers: &[(String, String)],
        body: &serde_json::Value,
        allow_private: bool,
    ) -> Result<u16, boma_app::AppError> {
        crate::net_safety::check_outbound_url(url, allow_private)
            .await
            .map_err(|reason| {
                boma_app::AppError::Rule(boma_domain::DomainError::InvalidName(reason.to_string()))
            })?;
        let mut request = self.http.post(url).json(body);
        for (name, value) in headers {
            request = request.header(name.as_str(), value.as_str());
        }
        let response = request.send().await.map_err(|error| {
            tracing::warn!(%error, "a messaging delivery failed");
            boma_app::AppError::Unavailable
        })?;
        Ok(response.status().as_u16())
    }
}
