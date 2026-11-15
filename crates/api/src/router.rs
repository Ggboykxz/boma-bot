//! The router, with the middleware every route gets.

use axum::{
    Router,
    extract::DefaultBodyLimit,
    http::{HeaderValue, StatusCode, header},
    routing::{get, post},
};
use tower_http::{set_header::SetResponseHeaderLayer, timeout::TimeoutLayer};

use crate::{
    authroutes, correlation, error::ApiError, health, pages, public, rpc, screen, state::AppState,
};

/// Builds the HTTP surface.
///
/// The spec's contract is `/health` and `/ready` (`spec/openapi.yaml`). `/healthz` and `/readyz`
/// are aliases required by AGENTS.md "Observability"; they are deliberately absent from the
/// generated document, and the contract test asserts that (docs/STATE.md records the deviation).
pub fn router(state: AppState) -> Router {
    let http = state.http().clone();
    let middleware_state = state.clone();

    Router::new()
        .route("/health", get(health::liveness))
        .route("/healthz", get(health::liveness))
        .route("/ready", get(health::readiness))
        .route("/readyz", get(health::readiness))
        .route("/internal/health", get(health::internal_health))
        .route("/metrics", get(health::metrics))
        .route("/api/auth/capabilities", get(health::auth_capabilities))
        .route(
            "/api/auth/sign-up",
            axum::routing::post(authroutes::sign_up),
        )
        .route(
            "/api/auth/sign-in",
            axum::routing::post(authroutes::sign_in),
        )
        .route(
            "/api/auth/sign-out",
            axum::routing::post(authroutes::sign_out),
        )
        .route(
            "/api/internal/screen-target",
            post(screen::screen_target).route_layer(DefaultBodyLimit::max(16 * 1024)),
        )
        // Public and cookieless: the 32 KiB cap is the route's own (D-07), like screen-target.
        .route(
            "/api/v1/plugin-submissions",
            post(public::plugin_submissions)
                .options(public::plugin_submissions_preflight)
                .route_layer(DefaultBodyLimit::max(33 * 1024)),
        )
        .route(
            "/api/v1/pages/shared/{token}",
            get(public::shared_page),
        )
        .route(
            "/api/v1/bots/{botId}/webhook",
            post(public::bot_webhook).route_layer(DefaultBodyLimit::max(64 * 1024)),
        )
        .route(
            "/api/v1/email/inbound",
            post(public::email_inbound).route_layer(DefaultBodyLimit::max(25 * 1024 * 1024 + 1024)),
        )
        .route(
            "/api/v1/messaging/{platform}/inbound/{installationId}",
            post(public::messaging_inbound).route_layer(DefaultBodyLimit::max(256 * 1024 + 1024)),
        )
        .route(
            "/api/v1/messaging/install/slack/callback",
            get(public::messaging_slack_callback),
        )
        .route(
            "/api/v1/billing/webhook",
            post(public::billing_webhook).route_layer(DefaultBodyLimit::max(65 * 1024)),
        )
        .route(
            "/api/csp-report",
            post(public::csp_report).route_layer(DefaultBodyLimit::max(17 * 1024)),
        )
        .route(
            "/api/v1/assets/page-runtime.js",
            get(pages::page_runtime_js),
        )
        .merge(rpc::router())
        .fallback(not_found)
        .with_state(state)
        .layer(DefaultBodyLimit::max(http.body_limit_bytes()))
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            http.request_timeout(),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::REFERRER_POLICY,
            HeaderValue::from_static("no-referrer"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-store"),
        ))
        .layer(axum::middleware::from_fn_with_state(
            middleware_state,
            correlation::log_http,
        ))
}

async fn not_found() -> ApiError {
    ApiError::not_found()
}

#[cfg(test)]
mod tests {
    use crate::health::Liveness;

    #[test]
    fn req_053_liveness_serialises_to_the_golden_body() {
        let body = serde_json::to_value(Liveness { ok: true }).expect("liveness serialises");
        assert_eq!(body, serde_json::json!({ "ok": true }));
    }
}
