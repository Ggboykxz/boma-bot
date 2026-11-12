//! The generated OpenAPI document for the plain HTTP surface.
//!
//! The document is generated from the handlers, never hand-written, and the contract suite compares
//! it with `spec/openapi.yaml`. A path that exists in the router but not in the spec is a
//! deviation; a path in the spec that the rebuild does not serve yet is a milestone gap.

use utoipa::{OpenApi, openapi::OpenApi as OpenApiDocument};

use crate::{error, health};

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
    paths(health::liveness, health::readiness, health::internal_health),
    components(schemas(
        health::Liveness,
        health::ReadinessBody,
        health::InternalHealth,
        error::ErrorBody,
        error::ErrorCode
    )),
    tags(
        (name = "ops", description = "Liveness, readiness and worker health.")
    )
)]
pub struct ApiDoc;

/// Returns the generated document.
#[must_use]
pub fn document() -> OpenApiDocument {
    <ApiDoc as OpenApi>::openapi()
}
