//! The outbound endpoint checker behind `push`'s SSRF boundary (REQ-064).

use boma_app::{AppError, ports};

/// The production checker: the same rules messaging installs use — HTTPS, no credentials in the
/// authority, no metadata endpoints, no private/loopback/link-local targets, unique ports only.
#[derive(Debug, Default, Clone, Copy)]
pub struct InfraEndpointSafety;

#[async_trait::async_trait]
impl ports::EndpointSafetyChecker for InfraEndpointSafety {
    async fn check(&self, url: &str) -> Result<(), AppError> {
        crate::net_safety::check_outbound_url(url, false)
            .await
            .map(|_| ())
            .map_err(|_| {
                AppError::Rule(boma_domain::DomainError::InvalidName(
                    "the endpoint address is refused".to_owned(),
                ))
            })
    }
}
