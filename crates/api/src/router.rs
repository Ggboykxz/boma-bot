//! The router, with the middleware every route gets.

use axum::{
    Router,
    body::Body,
    extract::{DefaultBodyLimit, State},
    http::{HeaderValue, Method, Request, StatusCode, header},
    response::{IntoResponse, Response},
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
#[allow(clippy::too_many_lines)] // the route table; splitting it hides the contract
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
            "/api/auth/forgot-password",
            axum::routing::post(authroutes::forgot_password),
        )
        .route(
            "/api/auth/reset-password",
            axum::routing::post(authroutes::reset_password),
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
            "/api/v1/invites/{token}",
            get(public::invite_preview),
        )
        .route(
            "/api/v1/me/exports/{id}",
            get(public::account_export_download),
        )
        .route(
            "/api/v1/me/account/import",
            post(public::account_import).route_layer(DefaultBodyLimit::max(33 * 1024 * 1024)),
        )
        // Directory sync (REQ-045): SCIM 2.0 /Users behind one bearer, all-or-nothing.
        .route(
            "/api/auth/scim/v2/ServiceProviderConfig",
            get(public::scim_service_provider_config),
        )
        .route(
            "/api/auth/scim/v2/Users",
            get(public::scim_list).post(public::scim_create),
        )
        .route(
            "/api/auth/scim/v2/Users/{id}",
            get(public::scim_get)
                .patch(public::scim_patch),
        )
        .route(
            "/api/v1/assets/page-runtime.js",
            get(pages::page_runtime_js),
        )
        .merge(rpc::router())
        .fallback(web_fallback)
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

/// The static-adjacent fallback (REQ-055).
///
/// The API's rule is that every unknown path answers the stable `404` JSON of REQ-040. A
/// deployment with `BOMA_WEB_ROOT` pointing at the static export widens it in one place only:
///
/// - `GET`/`HEAD` to an unknown path resolves against the export directory (directory URLs map
///   to `<dir>/index.html`), serving the export's file with its own type, or `404.html` with a
///   `404` status when the export carries one;
/// - `/api`, `/rpc`, `/internal`, the probes and the metrics route are never resolved against
///   the export: an unknown API path gets the JSON `404` it always got;
/// - any other method, or any accept that is not HTML, keeps the JSON `404`;
/// - with no `BOMA_WEB_ROOT`, the fallback is the JSON `404` it has always been.
fn resolve_static(root: &str, path: &str) -> Option<std::path::PathBuf> {
    let relative = path.trim_start_matches('/');
    if relative
        .split('/')
        .any(|segment| segment.contains('.') || segment.contains('\\'))
    {
        return None;
    }
    // `/` and `/app/` are the export's directory-style URLs; they resolve to `<dir>/index.html`.
    let segments: Vec<&str> = relative
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    let mut base = std::path::PathBuf::from(root);
    for segment in segments {
        if segment.is_empty() {
            continue;
        }
        base.push(segment);
    }
    if base.is_file() {
        return Some(base);
    }
    let index = base.join("index.html");
    if index.is_file() {
        return Some(index);
    }
    None
}

/// The app-prefix fallbacks of ADR-013.
///
/// The static export carries one shell per screen family; the screens read the URL at runtime,
/// so an arbitrary deep link under the prefix must land on the same shell rather than the
/// `404.html`. The first prefix that matches `path` wins; `app` is listed last so the
/// artifacts detail subtree is matched before the generic application shell.
const PREFIX_FALLBACKS: &[(&str, &str)] = &[
    ("/app/artifacts/", "app/artifacts/artifact_golden/index.html"),
    ("/p/", "p/token_unknown/index.html"),
    ("/app/", "app/index.html"),
];

fn resolve_prefix_fallback(root: &str, path: &str) -> Option<std::path::PathBuf> {
    PREFIX_FALLBACKS
        .iter()
        .find(|(prefix, _)| path.starts_with(prefix))
        .map(|(_, fallback)| std::path::PathBuf::from(root).join(fallback))
        .filter(|candidate| candidate.is_file())
}

fn deny_list(path: &str) -> bool {
    path == "/api"
        || path.starts_with("/api/")
        || path == "/rpc"
        || path.starts_with("/rpc/")
        || path == "/internal"
        || path.starts_with("/internal/")
        || path == "/health"
        || path == "/healthz"
        || path == "/ready"
        || path == "/readyz"
        || path == "/metrics"
}

fn content_type(path: &std::path::Path) -> &'static str {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default()
    {
        "html" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "json" | "map" => "application/json; charset=utf-8",
        "txt" => "text/plain; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        _ => "application/octet-stream",
    }
}

async fn web_fallback(State(state): State<AppState>, request: Request<Body>) -> Response {
    let method = request.method().clone();
    let path = request.uri().path().to_owned();
    let accepts_html = request
        .headers()
        .get(header::ACCEPT)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.contains("text/html") || value.contains("*/*"));

    let html_request = (method == Method::GET || method == Method::HEAD) && !deny_list(&path);
    if html_request && let Some(root) = state.http().web_root() {
        let resolved = resolve_static(root, &path)
            .or_else(|| resolve_prefix_fallback(root, &path));
        if let Some(file) = resolved
            && let Ok(body) = tokio::fs::read(&file).await
        {
            if method == Method::HEAD {
                return (
                    StatusCode::OK,
                    [
                        (header::CONTENT_TYPE, content_type(&file)),
                        (header::CONTENT_LENGTH, &body.len().to_string()),
                        (header::CACHE_CONTROL, "no-cache"),
                    ],
                )
                    .into_response();
            }
            return (
                StatusCode::OK,
                [
                    (header::CONTENT_TYPE, content_type(&file)),
                    (header::CACHE_CONTROL, "no-cache"),
                ],
                body,
            )
                .into_response();
        }
        if accepts_html {
            let not_found_page = std::path::PathBuf::from(root).join("404.html");
            if let Ok(body) = tokio::fs::read(&not_found_page).await {
                return (
                    StatusCode::NOT_FOUND,
                    [
                        (header::CONTENT_TYPE, "text/html; charset=utf-8"),
                        (header::CACHE_CONTROL, "no-store"),
                    ],
                    body,
                )
                    .into_response();
            }
        }
    }
    not_found().into_response()
}

fn not_found() -> ApiError {
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
