//! The HTTP transports behind the money ports (REQ-051, REQ-052).

use boma_app::ports::{BillingProvider, CheckoutSession, ModelTransport, ModelTransportError};
use secrecy::ExposeSecret;

/// The OpenAI-compatible `/models` probe behind credential validation (REQ-052).
#[derive(Debug, Clone)]
pub struct HttpModelTransport {
    http: reqwest::Client,
}

impl HttpModelTransport {
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

const MAX_ERROR_BODY: usize = 4 * 1024;

#[async_trait::async_trait]
impl ModelTransport for HttpModelTransport {
    async fn get_json(
        &self,
        url: &str,
        auth: &[(String, String)],
    ) -> Result<serde_json::Value, ModelTransportError> {
        let mut request = self.http.get(url);
        for (name, value) in auth {
            request = request.header(name.as_str(), value.as_str());
        }
        let response = request.send().await.map_err(|error| {
            tracing::warn!(%error, "the credential probe failed");
            ModelTransportError::Unavailable
        })?;
        let status = response.status().as_u16();
        if status >= 400 {
            let body = response
                .bytes()
                .await
                .map(|bytes| {
                    String::from_utf8_lossy(&bytes[..bytes.len().min(MAX_ERROR_BODY)]).into_owned()
                })
                .unwrap_or_default();
            return Err(ModelTransportError::Http { status, body });
        }
        response
            .json::<serde_json::Value>()
            .await
            .map_err(|_| ModelTransportError::Unavailable)
    }
}

/// Stripe as the checkout provider (REQ-051), wired when a deployment configures it.
///
/// The webhook needs no client: it is an HMAC-verified call with a documented payload shape.
/// This client only creates the Checkout Session the member's browser completes.
#[derive(Clone)]
pub struct StripeBillingProvider {
    http: reqwest::Client,
    secret_key: secrecy::SecretString,
    price_pro: String,
}

impl std::fmt::Debug for StripeBillingProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StripeBillingProvider")
            .field("price_pro", &self.price_pro)
            .finish_non_exhaustive()
    }
}

impl StripeBillingProvider {
    /// Builds the provider against the live Stripe API.
    ///
    /// # Errors
    ///
    /// Returns [`boma_app::AppError::Unavailable`] when the TLS backend cannot be initialised.
    pub fn new(
        secret_key: secrecy::SecretString,
        price_pro: String,
    ) -> Result<Self, boma_app::AppError> {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| boma_app::AppError::Unavailable)?;
        Ok(Self {
            http,
            secret_key,
            price_pro,
        })
    }
}

#[async_trait::async_trait]
impl BillingProvider for StripeBillingProvider {
    async fn create_checkout(
        &self,
        _context: &boma_domain::TenantContext,
        space_id: &str,
        plan: &str,
        success_url: &str,
        cancel_url: &str,
    ) -> Result<CheckoutSession, boma_app::AppError> {
        if plan != "pro" {
            return Err(boma_app::AppError::Rule(
                boma_domain::DomainError::InvalidName(
                    "only the `pro` plan can be checked out".to_owned(),
                ),
            ));
        }
        let form = [
            ("mode", "subscription".to_owned()),
            ("line_items[0][price]", self.price_pro.clone()),
            ("line_items[0][quantity]", "1".to_owned()),
            ("success_url", success_url.to_owned()),
            ("cancel_url", cancel_url.to_owned()),
            ("client_reference_id", space_id.to_owned()),
            ("metadata[space_id]", space_id.to_owned()),
            ("metadata[plan]", plan.to_owned()),
        ];
        let body = form
            .into_iter()
            .map(|(key, value)| {
                format!(
                    "{}={}",
                    boma_domain::percent_encode(key),
                    boma_domain::percent_encode(&value)
                )
            })
            .collect::<Vec<_>>()
            .join("&");
        let response = self
            .http
            .post("https://api.stripe.com/v1/checkout/sessions")
            .header(
                reqwest::header::CONTENT_TYPE,
                "application/x-www-form-urlencoded",
            )
            .basic_auth(self.secret_key.expose_secret(), Option::<&str>::None)
            .body(body)
            .send()
            .await
            .map_err(|error| {
                tracing::warn!(%error, "the checkout could not be created");
                boma_app::AppError::Unavailable
            })?;
        if !response.status().is_success() {
            tracing::warn!(status = %response.status(), "Stripe refused the checkout");
            return Err(boma_app::AppError::Unavailable);
        }
        let session: serde_json::Value = response
            .json()
            .await
            .map_err(|_| boma_app::AppError::Unavailable)?;
        let url = session
            .get("url")
            .and_then(serde_json::Value::as_str)
            .ok_or(boma_app::AppError::Unavailable)?
            .to_owned();
        let provider_ref = session
            .get("id")
            .and_then(serde_json::Value::as_str)
            .ok_or(boma_app::AppError::Unavailable)?
            .to_owned();
        Ok(CheckoutSession { url, provider_ref })
    }
}
