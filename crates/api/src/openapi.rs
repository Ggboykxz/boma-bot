//! The generated OpenAPI document for the plain HTTP surface.
//!
//! The document is generated from the handlers, never hand-written, and the contract suite compares
//! it with `spec/openapi.yaml`. A path that exists in the router but not in the spec is a
//! deviation; a path in the spec that the rebuild does not serve yet is a milestone gap.
//!
//! The procedure interface (`/rpc/{namespace}/{procedure}`) is specified separately in
//! `spec/03-api-contract.md` and replayed by the golden fixtures, so it is not part of this
//! document: one body per procedure with a free-form payload would say nothing a client could
//! check.

use utoipa::{OpenApi, openapi::OpenApi as OpenApiDocument};

use crate::{authcap, error, health};

/// The document, assembled from the annotated handlers.
#[derive(OpenApi)]
#[openapi(
    info(
        title = "Boma Bot HTTP interface",
        version = "1.0.0",
        description = "Plain HTTP endpoints served next to the RPC interface: probes, webhooks, \
                       public links and device flows. The RPC interface is specified separately \
                       in spec/03-api-contract.md."
    ),
    paths(
        health::liveness,
        health::readiness,
        health::internal_health,
        health::auth_capabilities
    ),
    components(schemas(
        health::Liveness,
        health::ReadinessBody,
        health::InternalHealth,
        authcap::AuthCapabilities,
        authcap::SsoProvider,
        error::ErrorBody,
        error::ErrorCode
    )),
    tags(
        (name = "ops", description = "Liveness, readiness and worker health."),
        (name = "auth", description = "Which sign-in methods the deployment offers.")
    )
)]
pub struct ApiDoc;

/// Returns the generated document.
#[must_use]
pub fn document() -> OpenApiDocument {
    <ApiDoc as OpenApi>::openapi()
}

#[cfg(test)]
mod tests {
    use super::document;

    #[test]
    fn req_040_the_capabilities_route_is_in_the_generated_document() {
        let document = document();
        let paths = &document.paths.paths;
        assert!(
            paths.contains_key("/api/auth/capabilities"),
            "the client cannot render the sign-in screen without it"
        );
    }

    #[test]
    fn req_041_the_health_aliases_stay_out_of_the_public_document() {
        let document = document();
        let paths = &document.paths.paths;
        for alias in ["/healthz", "/readyz"] {
            assert!(
                !paths.contains_key(alias),
                "{alias} is an AGENTS.md alias, not a contract path"
            );
        }
    }
}
