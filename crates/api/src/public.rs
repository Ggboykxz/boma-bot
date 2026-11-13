//! Public, cookieless endpoints (spec/03-api-contract.md §3.2).
//!
//! These routes exist for callers with no account: plugin authors submitting work for review.
//! They are deliberately outside the RPC surface: no session, no envelope, no space. Abuse is
//! bounded by the shared counter from D-01 (per IP, per hour) and by strict input handling; a
//! body is read with a cap, a source is parsed before anything happens, and nothing from the
//! reply tells the submitter whether someone else submitted the same plugin.

use axum::{
    Json,
    body::Bytes,
    extract::State,
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use boma_app::usecases;
use serde::Deserialize;

use crate::{client::ClientAddr, error::ApiError, state::AppState};

/// The per-IP allowance per hour (spec/golden/plugin-submission.json, D-06).
const SUBMISSIONS_PER_IP_PER_HOUR: u64 = 10;

/// The window the allowance applies to.
const SUBMISSION_WINDOW_SECONDS: u64 = 60 * 60;

/// The body cap for a submission (spec/04-nonfunctional.md §4.3).
const MAX_BODY_BYTES: usize = 32 * 1024;

/// The headers a cross-origin POST may carry, and nothing else.
const ALLOW_HEADERS: &str = "content-type";

/// The methods the submission route accepts.
const ALLOW_METHODS: &str = "POST, OPTIONS";

/// `POST /api/v1/plugin-submissions` — record an anonymous plugin submission (REQ-034).
///
/// The reply carries an id and nothing about the submitter. An over-limit caller gets `429`;
/// a malformed source gets `400` with a fixed wording; the words never differ, so the endpoint
/// answers the same way no matter who has submitted what before.
pub async fn plugin_submissions(
    State(state): State<AppState>,
    ClientAddr(peer): ClientAddr,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let client_ip =
        crate::client::resolve_client_ip(peer, &headers, state.http().trusted_proxies());
    let key = format!("plugin-submissions:{client_ip}");
    match usecases::hit_rate_limit(state.app(), &key, SUBMISSION_WINDOW_SECONDS).await {
        Ok(decision) if decision.count > SUBMISSIONS_PER_IP_PER_HOUR => {
            return json_with_cors(
                StatusCode::TOO_MANY_REQUESTS,
                serde_json::json!({ "error": "Too many submissions. Try again later." }),
                &[(header::RETRY_AFTER, SUBMISSION_WINDOW_SECONDS.to_string())],
            );
        }
        Ok(_) => {}
        Err(error) => return ApiError::from(error).into_response(),
    }

    if body.len() > MAX_BODY_BYTES {
        return plain(
            StatusCode::PAYLOAD_TOO_LARGE,
            r#"{"error": "Submission is too large"}"#,
        );
    }
    let payload: SubmissionRequest = match serde_json::from_slice(&body) {
        Ok(payload) => payload,
        Err(_) => {
            return plain(
                StatusCode::BAD_REQUEST,
                r#"{"error": "Invalid submission"}"#,
            );
        }
    };
    let Some(source) = payload.source.as_deref() else {
        return plain(StatusCode::BAD_REQUEST, r#"{"error": "Invalid source."}"#);
    };
    let Ok(parsed) = boma_domain::parse_plugin_source(source) else {
        return plain(StatusCode::BAD_REQUEST, r#"{"error": "Invalid source."}"#);
    };
    match usecases::submit_plugin(state.app(), &parsed, payload.name, payload.contact, None).await {
        Ok(id) => json_with_cors(
            StatusCode::OK,
            serde_json::json!({ "ok": true, "id": id }),
            &[],
        ),
        Err(error) => ApiError::from(error).into_response(),
    }
}

/// `OPTIONS /api/v1/plugin-submissions` — the preflight the browser sends before the POST.
pub async fn plugin_submissions_preflight() -> Response {
    (
        StatusCode::NO_CONTENT,
        [
            (header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"),
            (header::ACCESS_CONTROL_ALLOW_METHODS, ALLOW_METHODS),
            (header::ACCESS_CONTROL_ALLOW_HEADERS, ALLOW_HEADERS),
            (header::ACCESS_CONTROL_MAX_AGE, "600"),
            (header::CACHE_CONTROL, "no-store"),
        ],
    )
        .into_response()
}

/// The request body of a submission.
#[derive(Debug, Deserialize)]
struct SubmissionRequest {
    /// The source reference; parsed before anything happens.
    #[serde(default)]
    source: Option<String>,
    /// The plugin's display name.
    #[serde(default)]
    name: Option<String>,
    /// How to reach the author. Never returned by any API.
    #[serde(default)]
    contact: Option<String>,
}

/// A JSON body with `access-control-allow-origin: *` and nothing else of ours.
fn json_with_cors(
    status: StatusCode,
    body: serde_json::Value,
    extra_headers: &[(header::HeaderName, String)],
) -> Response {
    let mut response = Json(body).into_response();
    *response.status_mut() = status;
    let cors = HeaderValue::from_static("*");
    response
        .headers_mut()
        .insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, cors);
    for (name, value) in extra_headers {
        if let Ok(value) = HeaderValue::from_str(value) {
            response.headers_mut().insert(name.clone(), value);
        }
    }
    response
}

/// A plain JSON body with a fixed wording; content type included, CORS included.
fn plain(status: StatusCode, body: &'static str) -> Response {
    (
        status,
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        body,
    )
        .into_response()
}
