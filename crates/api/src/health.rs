//! Liveness, readiness and worker health (REQ-053).
//!
//! `/health` answers from memory and never touches a dependency. `/ready` probes each declared
//! dependency with a deadline and reports the deployment's capabilities. Neither route is
//! authenticated — that is the point of a probe — so neither may disclose a secret (REQ-054).

use axum::{
    Json,
    extract::State,
    response::{IntoResponse, Response},
};
use boma_app::Capabilities;
use serde::{Deserialize, Serialize};

use crate::{
    authcap::AuthCapabilities,
    error::{ApiError, ErrorBody},
    state::AppState,
};

/// Body of `/health`: the process is scheduled and can answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct Liveness {
    /// Always `true`; the field exists so a probe keeps a stable shape.
    pub ok: bool,
}

/// Body of `/ready`: what this deployment can do, without a secret in it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ReadinessBody {
    /// `true` only because every required dependency answered; a failure is a `503` instead.
    pub ok: bool,
    /// Build version.
    pub version: String,
    /// Where the agent executes.
    pub runtime: String,
    /// Computer provider in use.
    pub sandbox: String,
    /// Integration families that are configured.
    pub integrations: Vec<String>,
    /// Whether a messaging channel is configured.
    pub messaging: bool,
    /// Inbound e-mail address, when the deployment accepts mail.
    pub email: Option<String>,
    /// Billing provider name, when one is configured.
    pub billing: Option<String>,
    /// Job execution mode.
    pub jobs: String,
    /// Realtime transport.
    pub realtime: String,
    /// Whether an optional subsystem is switched off.
    pub degraded: bool,
    /// Source revision, when known.
    pub revision: Option<String>,
}

impl ReadinessBody {
    /// Projects the deployment's capabilities onto the wire shape.
    #[must_use]
    pub fn from_capabilities(capabilities: &Capabilities) -> Self {
        Self {
            ok: true,
            version: capabilities.version.clone(),
            runtime: capabilities.runtime.clone(),
            sandbox: capabilities.sandbox.clone(),
            integrations: capabilities.integrations.clone(),
            messaging: capabilities.messaging,
            email: capabilities.email.clone(),
            billing: capabilities.billing.clone(),
            jobs: capabilities.jobs.clone(),
            realtime: capabilities.realtime.clone(),
            degraded: capabilities.degraded,
            revision: capabilities.revision.clone(),
        }
    }
}

/// Body of `/internal/health`: what the supervisor reads for a worker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InternalHealth {
    /// Whether the worker is inside its expected heartbeat window.
    pub ok: bool,
    /// Last time a job completed, or `null` when none has yet.
    pub last_completed_at: Option<String>,
}

/// `GET /health` — liveness only.
#[utoipa::path(
    get,
    path = "/health",
    tag = "ops",
    operation_id = "health",
    responses(
        (status = 200, description = "Process is alive.", body = Liveness)
    )
)]
pub async fn liveness() -> Json<Liveness> {
    Json(Liveness { ok: true })
}

/// `GET /ready` — readiness of every declared dependency.
#[utoipa::path(
    get,
    path = "/ready",
    tag = "ops",
    operation_id = "ready",
    responses(
        (status = 200, description = "Every required dependency answered.", body = ReadinessBody),
        (status = 503, description = "A dependency is unavailable.", body = ErrorBody)
    )
)]
pub async fn readiness(State(state): State<AppState>) -> Response {
    let report = state
        .readiness()
        .report(state.http().readiness_timeout())
        .await;
    if report.is_ready() {
        return Json(ReadinessBody::from_capabilities(state.capabilities())).into_response();
    }
    // Names only: a probe's own error can quote a connection target (REQ-054).
    tracing::warn!(
        failing = ?report.failing(),
        "readiness refused: a required dependency did not answer"
    );
    ApiError::unavailable().into_response()
}

/// `GET /internal/health` — worker health for the supervisor.
#[utoipa::path(
    get,
    path = "/internal/health",
    tag = "ops",
    operation_id = "internalHealth",
    responses(
        (status = 200, description = "Worker health for the supervisor.", body = InternalHealth),
        (status = 503, description = "Worker heartbeat is stale.", body = ErrorBody)
    )
)]
pub async fn internal_health() -> Json<InternalHealth> {
    // M0 has no worker loop yet, so there is no completed job to report. The field stays in the
    // contract and reports `null` rather than disappearing (docs/STATE.md, M1 adds the heartbeat).
    Json(InternalHealth {
        ok: true,
        last_completed_at: None,
    })
}

/// `GET /api/auth/capabilities` — which sign-in methods exist (REQ-040).
///
/// The reply is unauthenticated: the sign-in screen must render before there is a session. A method
/// that is absent is absent from the reply, so the client never renders a disabled button.
#[utoipa::path(
    get,
    path = "/api/auth/capabilities",
    tag = "auth",
    operation_id = "authCapabilities",
    responses(
        (status = 200, description = "Which sign-in methods the deployment offers.", body = AuthCapabilities)
    )
)]
pub async fn auth_capabilities(State(state): State<AppState>) -> Json<AuthCapabilities> {
    Json(state.auth_capabilities().clone())
}

#[cfg(test)]
mod tests {
    use boma_app::Capabilities;

    use super::ReadinessBody;

    #[test]
    fn req_053_the_readiness_body_names_every_documented_field() {
        let capabilities = Capabilities {
            version: "1.2.3".to_owned(),
            runtime: "sandboxed".to_owned(),
            sandbox: "docker".to_owned(),
            integrations: vec!["github".to_owned()],
            messaging: true,
            email: None,
            billing: Some("none".to_owned()),
            jobs: "postgres".to_owned(),
            realtime: "postgres".to_owned(),
            degraded: false,
            revision: Some("abc123".to_owned()),
        };
        let body = ReadinessBody::from_capabilities(&capabilities);
        let json = serde_json::to_value(&body).expect("the body is serialisable");
        let keys: Vec<&String> = json
            .as_object()
            .expect("the body is an object")
            .keys()
            .collect();
        for expected in [
            "ok",
            "version",
            "runtime",
            "sandbox",
            "integrations",
            "messaging",
            "email",
            "billing",
            "jobs",
            "realtime",
            "degraded",
            "revision",
        ] {
            assert!(
                keys.iter().any(|key| key.as_str() == expected),
                "{expected}"
            );
        }
        assert_eq!(json["ok"], serde_json::json!(true));
    }
}
