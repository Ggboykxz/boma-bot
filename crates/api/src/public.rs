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
    extract::{Path, State},
    http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use boma_app::{AppError, usecases};
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

/// The per-minute allowance for the public shared-page read (spec/03-api-contract.md §3.2).
const SHARED_PAGE_PER_IP_PER_MINUTE: u64 = 120;

/// The window the allowance applies to.
const SHARED_PAGE_WINDOW_SECONDS: u64 = 60;

/// `GET /api/v1/pages/shared/:token` — the golden body, or a uniform 404 (REQ-037).
///
/// The token is the capability: looked up by hash, never by id. An unknown, revoked or
/// malformed token answers the same 404 body with the same headers, indistinguishably. A
/// database failure is a 500 with the public headers, never a fake 404: an outage must not
/// masquerade as a missing page.
pub async fn shared_page(
    State(state): State<AppState>,
    ClientAddr(peer): ClientAddr,
    Path(token): Path<String>,
    headers: HeaderMap,
) -> Response {
    let client_ip =
        crate::client::resolve_client_ip(peer, &headers, state.http().trusted_proxies());
    let key = format!("pages-shared:{client_ip}");
    match usecases::hit_rate_limit(state.app(), &key, SHARED_PAGE_WINDOW_SECONDS).await {
        Ok(decision) if decision.count > SHARED_PAGE_PER_IP_PER_MINUTE => {
            let body = serde_json::json!({ "error": "Too many requests. Try again later." });
            return shared_page_response(StatusCode::TOO_MANY_REQUESTS, &body);
        }
        Ok(_) => {}
        Err(error) => return ApiError::from(error).into_response(),
    }

    match usecases::pages_shared(state.app(), &token).await {
        Ok(Some((updated_at, source))) => {
            if accept_prefers_html(&headers) {
                let title = source
                    .get("title")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("Page");
                let body = crate::pages::render_shared_page_html(title, &source);
                shared_page_html_response(StatusCode::OK, body)
            } else {
                let body = serde_json::json!({
                    "title": source.get("title").cloned().unwrap_or(serde_json::Value::Null),
                    "blocks": source.get("blocks").cloned().unwrap_or_else(|| serde_json::json!([])),
                    "updatedAt": updated_at,
                });
                shared_page_response(StatusCode::OK, &body)
            }
        }
        Ok(None) => {
            if accept_prefers_html(&headers) {
                let body = crate::pages::render_shared_page_html(
                    "Not found.",
                    &serde_json::json!({ "title": "Not found.", "blocks": [] }),
                );
                shared_page_html_response(StatusCode::NOT_FOUND, body)
            } else {
                let body = serde_json::json!({ "error": "Not found." });
                shared_page_response(StatusCode::NOT_FOUND, &body)
            }
        }
        Err(error) => ApiError::from(error).into_response(),
    }
}

/// Whether the caller asked for HTML (`Accept: text/html`).
fn accept_prefers_html(headers: &HeaderMap) -> bool {
    headers
        .get(header::ACCEPT)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|accept| accept.contains("text/html"))
}

/// The shared-page HTML answer: the golden headers plus the sandbox policy.
fn shared_page_html_response(status: StatusCode, body: String) -> Response {
    (
        status,
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (header::CACHE_CONTROL, "no-store"),
            (HeaderName::from_static("x-robots-tag"), "noindex, nofollow"),
            (header::REFERRER_POLICY, "no-referrer"),
            (
                header::CONTENT_SECURITY_POLICY,
                crate::pages::SHARED_PAGE_CSP,
            ),
        ],
        body,
    )
        .into_response()
}

/// The shared-page answer with the golden headers.
fn shared_page_response(status: StatusCode, body: &serde_json::Value) -> Response {
    (
        status,
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::CACHE_CONTROL, "no-store"),
            (HeaderName::from_static("x-robots-tag"), "noindex, nofollow"),
            (header::REFERRER_POLICY, "no-referrer"),
        ],
        body.to_string(),
    )
        .into_response()
}

/// The per-minute allowance for the bot webhook (bot-authenticated, not internet-facing).
const BOT_WEBHOOK_PER_IP_PER_MINUTE: u64 = 120;

/// The window the allowance applies to.
const BOT_WEBHOOK_WINDOW_SECONDS: u64 = 60;

/// `POST /api/v1/bots/:botId/webhook` — wake the bot with a payload (REQ-050).
///
/// The bot proves itself with its `WEBHOOK` bot secret. Every refusal — unknown bot, archived
/// bot, missing secret, wrong secret, an unreadable body — answers the same `401` with the same
/// body and the same headers, so bot ids cannot be enumerated. On success a run starts with the
/// payload as its newest message; a repository-shaped payload additionally wakes the bot's
/// opted-in `github_enabled` routines.
pub async fn bot_webhook(
    State(state): State<AppState>,
    ClientAddr(peer): ClientAddr,
    Path(bot_id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let deny = || {
        (
            StatusCode::UNAUTHORIZED,
            [
                (HeaderName::from_static("content-type"), "application/json"),
                (HeaderName::from_static("cache-control"), "no-store"),
            ],
            r#"{"error": "Unauthorized."}"#,
        )
            .into_response()
    };
    let client_ip =
        crate::client::resolve_client_ip(peer, &headers, state.http().trusted_proxies());
    let key = format!("bot-webhook:{client_ip}");
    match usecases::hit_rate_limit(state.app(), &key, BOT_WEBHOOK_WINDOW_SECONDS).await {
        Ok(decision) if decision.count > BOT_WEBHOOK_PER_IP_PER_MINUTE => {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                [
                    (HeaderName::from_static("content-type"), "application/json"),
                    (HeaderName::from_static("cache-control"), "no-store"),
                ],
                r#"{"error": "Too many requests. Try again later."}"#,
            )
                .into_response();
        }
        Ok(_) => {}
        Err(error) => return ApiError::from(error).into_response(),
    }
    let Some(bearer) = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
    else {
        return deny();
    };
    if body.len() > 64 * 1024 {
        return deny();
    }
    let Some(payload) = std::str::from_utf8(&body).ok() else {
        return deny();
    };
    if serde_json::from_str::<serde_json::Value>(payload).is_err() {
        return deny();
    }
    match usecases::webhook_wake(state.app(), &bot_id, bearer, payload).await {
        Ok(outcome) => (
            StatusCode::OK,
            [
                (HeaderName::from_static("content-type"), "application/json"),
                (HeaderName::from_static("cache-control"), "no-store"),
            ],
            format!(
                "{}",
                serde_json::json!({ "ok": true, "runId": outcome.run_id, "routineWakes": outcome.routine_wakes })
            ),
        )
            .into_response(),
        Err(_) => deny(),
    }
}

/// The per-minute allowance for the email relay; a relay bursts when a deployment migrates.
const EMAIL_INBOUND_PER_IP_PER_MINUTE: u64 = 300;

/// `POST /api/v1/email/inbound` — one message from the relay for one bot address (REQ-047).
///
/// The relay proves itself with the relay token. The body is an RFC 822 message. The reply
/// never enumerates: a wrong relay token is the same 404 as a missing route. A malformed
/// message is a 400 with the reason; a bot that does not exist or is archived does not leak.
pub async fn email_inbound(
    State(state): State<AppState>,
    ClientAddr(peer): ClientAddr,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let deny = || {
        (
            StatusCode::NOT_FOUND,
            [
                (HeaderName::from_static("content-type"), "application/json"),
                (HeaderName::from_static("cache-control"), "no-store"),
            ],
            r#"{"error": "Not found."}"#,
        )
            .into_response()
    };
    let client_ip =
        crate::client::resolve_client_ip(peer, &headers, state.http().trusted_proxies());
    let key = format!("email-inbound:{client_ip}");
    match usecases::hit_rate_limit(state.app(), &key, 60).await {
        Ok(decision) if decision.count > EMAIL_INBOUND_PER_IP_PER_MINUTE => {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                [
                    (HeaderName::from_static("content-type"), "application/json"),
                    (HeaderName::from_static("cache-control"), "no-store"),
                ],
                r#"{"error": "Too many requests. Try again later."}"#,
            )
                .into_response();
        }
        Ok(_) => {}
        Err(error) => return ApiError::from(error).into_response(),
    }
    let Some(relay_token) = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
    else {
        return deny();
    };
    if body.len() > 25 * 1024 * 1024 {
        return (
            StatusCode::PAYLOAD_TOO_LARGE,
            [
                (HeaderName::from_static("content-type"), "application/json"),
                (HeaderName::from_static("cache-control"), "no-store"),
            ],
            r#"{"error": "The message is too large"}"#,
        )
            .into_response();
    }
    match usecases::mail_inbound(state.app(), relay_token, &body).await {
        Ok(outcome) => (
            StatusCode::OK,
            [
                (HeaderName::from_static("content-type"), "application/json"),
                (HeaderName::from_static("cache-control"), "no-store"),
            ],
            serde_json::json!({
                "ok": true,
                "result": outcome.result,
                "runId": outcome.run_id,
            })
            .to_string(),
        )
            .into_response(),
        Err(AppError::Rule(error)) => (
            StatusCode::BAD_REQUEST,
            [
                (HeaderName::from_static("content-type"), "application/json"),
                (HeaderName::from_static("cache-control"), "no-store"),
            ],
            serde_json::json!({ "error": error.to_string() }).to_string(),
        )
            .into_response(),
        Err(_) => deny(),
    }
}

/// The per-minute allowance for messaging deliveries; a busy workspace bursts.
const MESSAGING_PER_IP_PER_MINUTE: u64 = 600;

/// The body cap of a messaging delivery (spec/04-nonfunctional.md).
pub const MESSAGING_BODY_LIMIT: usize = 256 * 1024;

/// The uniform refusal for every messaging ingress failure (REQ-048).
fn messaging_deny() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        [
            (HeaderName::from_static("content-type"), "application/json"),
            (HeaderName::from_static("cache-control"), "no-store"),
        ],
        r#"{"error": "Unauthorized."}"#,
    )
        .into_response()
}

/// `POST /api/v1/messaging/{platform}/inbound/{installation_id}` — one platform delivery (REQ-048).
///
/// The surface does not exist without `BOMA_MESSAGING`. The platform proves itself per
/// installation (Slack: the deployment signing secret; Telegram: the installation's webhook
/// secret; generic: an HMAC of the body). Every failure — unknown platform, unknown
/// installation, wrong credential, malformed body — answers the same `401`.
pub async fn messaging_inbound(
    State(state): State<AppState>,
    ClientAddr(peer): ClientAddr,
    Path((platform, installation_id)): Path<(String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if !state.app().services.messaging_enabled {
        return (
            StatusCode::NOT_FOUND,
            [
                (HeaderName::from_static("content-type"), "application/json"),
                (HeaderName::from_static("cache-control"), "no-store"),
            ],
            r#"{"error": "Not found."}"#,
        )
            .into_response();
    }
    let client_ip =
        crate::client::resolve_client_ip(peer, &headers, state.http().trusted_proxies());
    let key = format!("messaging:{client_ip}");
    match usecases::hit_rate_limit(state.app(), &key, 60).await {
        Ok(decision) if decision.count > MESSAGING_PER_IP_PER_MINUTE => {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                [
                    (HeaderName::from_static("content-type"), "application/json"),
                    (HeaderName::from_static("cache-control"), "no-store"),
                ],
                r#"{"error": "Too many requests. Try again later."}"#,
            )
                .into_response();
        }
        Ok(_) => {}
        Err(error) => return ApiError::from(error).into_response(),
    }
    let Some(platform) = boma_domain::ChannelPlatform::parse(&platform).ok() else {
        return messaging_deny();
    };
    if body.len() > MESSAGING_BODY_LIMIT {
        return messaging_deny();
    }
    let header = |name: &'static str| headers.get(name).and_then(|value| value.to_str().ok());
    let ingress = usecases::IngressHeaders {
        slack_timestamp: header("x-slack-request-timestamp"),
        slack_signature: header("x-slack-signature"),
        telegram_secret_token: header("x-telegram-bot-api-secret-token"),
        generic_signature: header("x-boma-signature"),
    };
    // A Slack handshake is answered with the challenge, not a run.
    if platform == boma_domain::ChannelPlatform::Slack
        && let Some(challenge) = boma_domain::slack_url_verification_challenge(&body)
    {
        let expected =
            state.app().services.slack.as_ref().map(|slack| {
                secrecy::ExposeSecret::expose_secret(&slack.signing_secret).to_owned()
            });
        let ok = match (expected, ingress.slack_signature, ingress.slack_timestamp) {
            (Some(secret), Some(signature), Some(timestamp)) => boma_domain::verify_slack(
                secret.as_bytes(),
                timestamp,
                &body,
                signature,
                boma_app::usecases::unix_seconds(&state.app().services.clock.now()),
            ),
            _ => false,
        };
        if ok {
            return (
                StatusCode::OK,
                [
                    (HeaderName::from_static("content-type"), "application/json"),
                    (HeaderName::from_static("cache-control"), "no-store"),
                ],
                serde_json::json!({ "challenge": challenge }).to_string(),
            )
                .into_response();
        }
        return messaging_deny();
    }
    match usecases::messaging_inbound(state.app(), platform, &installation_id, &ingress, &body)
        .await
    {
        Ok(outcome) => (
            StatusCode::OK,
            [
                (HeaderName::from_static("content-type"), "application/json"),
                (HeaderName::from_static("cache-control"), "no-store"),
            ],
            serde_json::json!({
                "ok": true,
                "runId": outcome.run_id,
                "threadId": outcome.thread_id,
                "conversationId": outcome.conversation_id,
            })
            .to_string(),
        )
            .into_response(),
        Err(_) => messaging_deny(),
    }
}

/// Reads the `code` and `state` out of a raw query string without pulling the `query` feature.
fn slack_callback_params(uri: &axum::http::Uri) -> (Option<String>, Option<String>) {
    let mut code = None;
    let mut state = None;
    for pair in uri.query().unwrap_or("").split('&') {
        let Some((key, value)) = pair.split_once('=') else {
            continue;
        };
        match key {
            "code" => code = Some(percent_decode(value)),
            "state" => state = Some(percent_decode(value)),
            _ => {}
        }
    }
    (code, state)
}

/// Minimal percent-decoding for the two parameters Slack sends.
fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hex = &value[i + 1..i + 3];
                if let Ok(byte) = u8::from_str_radix(hex, 16) {
                    out.push(byte);
                    i += 3;
                } else {
                    out.push(bytes[i]);
                    i += 1;
                }
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            byte => {
                out.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// `GET /api/v1/messaging/install/slack/callback` — Slack's redirect target (REQ-048).
///
/// On success the member lands back on the messaging page. The callback never explains
/// failures: an unknown state or a refused code is the same `401`.
pub async fn messaging_slack_callback(
    State(state): State<AppState>,
    uri: axum::http::Uri,
) -> Response {
    if !state.app().services.messaging_enabled {
        return (
            StatusCode::NOT_FOUND,
            [
                (HeaderName::from_static("content-type"), "application/json"),
                (HeaderName::from_static("cache-control"), "no-store"),
            ],
            r#"{"error": "Not found."}"#,
        )
            .into_response();
    }
    let (Some(code), Some(state_token)) = slack_callback_params(&uri) else {
        return messaging_deny();
    };
    match usecases::messaging_slack_callback(state.app(), &code, &state_token).await {
        Ok(_installation) => (
            StatusCode::SEE_OTHER,
            [
                (
                    HeaderName::from_static("location"),
                    "/settings/messaging?installed=slack",
                ),
                (HeaderName::from_static("cache-control"), "no-store"),
            ],
            "",
        )
            .into_response(),
        Err(_) => messaging_deny(),
    }
}

/// The body cap of a billing webhook: Stripe events are small, and a bigger body is a forgery
/// attempt, not a subscription (REQ-051).
const BILLING_WEBHOOK_MAX_BYTES: usize = 64 * 1024;

/// `POST /api/v1/billing/webhook` — the payment provider's call after a checkout (REQ-051).
///
/// The provider proves itself with `Stripe-Signature`. Every failure answers the same uniform
/// `404` the other public surfaces do, so a caller cannot tell "billing is not configured" from
/// "the signature is wrong" from "this space does not exist". A verified event asks the use case
/// to apply it; unknown events are acknowledged and ignored.
pub async fn billing_webhook(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let deny = || {
        (
            StatusCode::NOT_FOUND,
            [
                (HeaderName::from_static("content-type"), "application/json"),
                (HeaderName::from_static("cache-control"), "no-store"),
            ],
            r#"{"error": "Not found."}"#,
        )
            .into_response()
    };
    if !state.app().services.billing_enabled {
        return deny();
    }
    if body.len() > BILLING_WEBHOOK_MAX_BYTES {
        return (
            StatusCode::PAYLOAD_TOO_LARGE,
            [
                (HeaderName::from_static("content-type"), "application/json"),
                (HeaderName::from_static("cache-control"), "no-store"),
            ],
            r#"{"error": "The event is too large"}"#,
        )
            .into_response();
    }
    let Some(signature) = headers
        .get("stripe-signature")
        .and_then(|value| value.to_str().ok())
    else {
        return deny();
    };
    // The signed timestamp is inside the header; the use case re-reads it and refuses a stale one.
    let timestamp = parse_signature_timestamp(signature);
    let Some(timestamp) = timestamp else {
        return deny();
    };
    match usecases::billing_webhook(state.app(), signature, &timestamp, &body).await {
        Ok(outcome) => (
            StatusCode::OK,
            [
                (HeaderName::from_static("content-type"), "application/json"),
                (HeaderName::from_static("cache-control"), "no-store"),
            ],
            serde_json::json!({
                "received": true,
                "activated": outcome.activated,
                "plan": outcome.plan,
            })
            .to_string(),
        )
            .into_response(),
        Err(_) => deny(),
    }
}

/// Reads the `t=` entry out of a `Stripe-Signature` header, for the use case's own check.
fn parse_signature_timestamp(header: &str) -> Option<String> {
    header.split(',').find_map(|part| {
        part.trim()
            .strip_prefix("t=")
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    })
}

/// The body cap for a CSP report: every browser sends small ones, and a bigger body is not a better report.
const CSP_REPORT_MAX_BYTES: usize = 16 * 1024;

/// The per-IP allowance per hour for CSP reports.
const CSP_REPORTS_PER_IP_PER_HOUR: u64 = 60;

/// The window the allowance applies to.
const CSP_WINDOW_SECONDS: u64 = 60 * 60;

/// The maximum violations kept out of one report; the rest are dropped on the floor.
const CSP_VIOLATIONS_PER_REPORT: usize = 10;

/// `POST /api/csp-report` — the browser tells us what its own rules blocked (REQ-054).
///
/// Two bodies are accepted: `application/csp-report` (one report object) and
/// `application/reports+json` (a list of reports flagged `csp-violation`). Both are capped; the
/// counter bounds what anyone can make us store per hour; the report only ever records the
/// **host** — a query string travels with tokens, and tokens are never stored.
pub async fn csp_report(
    State(state): State<AppState>,
    ClientAddr(peer): ClientAddr,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if body.len() > CSP_REPORT_MAX_BYTES {
        return (
            StatusCode::PAYLOAD_TOO_LARGE,
            [
                (HeaderName::from_static("content-type"), "application/json"),
                (HeaderName::from_static("cache-control"), "no-store"),
            ],
            r#"{"error":"The report is too large."}"#,
        )
            .into_response();
    }
    let client_ip =
        crate::client::resolve_client_ip(peer, &headers, state.http().trusted_proxies());
    let key = format!("csp-report:{client_ip}");
    match usecases::hit_rate_limit(state.app(), &key, CSP_WINDOW_SECONDS).await {
        Ok(decision) if decision.count > CSP_REPORTS_PER_IP_PER_HOUR => {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                [
                    (HeaderName::from_static("content-type"), "application/json"),
                    (HeaderName::from_static("cache-control"), "no-store"),
                ],
                r#"{"error":"Too many reports; wait and try again."}"#,
            )
                .into_response();
        }
        Ok(_) => {}
        Err(error) => return ApiError::from(error).into_response(),
    }

    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
        .unwrap_or_default();
    let mut summaries = Vec::new();
    if content_type.starts_with("application/csp-report") {
        if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&body)
            && let Some(report) = value.get("csp-report")
            && let Some(summary) = summarize_violation(report)
        {
            summaries.push(summary);
        }
    } else if content_type.starts_with("application/reports+json") {
        if let Ok(serde_json::Value::Array(items)) =
            serde_json::from_slice::<serde_json::Value>(&body)
        {
            for item in items.iter().take(CSP_VIOLATIONS_PER_REPORT) {
                if item.get("type").and_then(serde_json::Value::as_str) != Some("csp-violation") {
                    continue;
                }
                if let Some(summary) = item.get("body").and_then(summarize_violation) {
                    summaries.push(summary);
                }
            }
        }
    } else {
        return (
            StatusCode::BAD_REQUEST,
            [
                (HeaderName::from_static("content-type"), "application/json"),
                (HeaderName::from_static("cache-control"), "no-store"),
            ],
            r#"{"error":"The report content-type is not understood."}"#,
        )
            .into_response();
    }
    if summaries.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            [
                (HeaderName::from_static("content-type"), "application/json"),
                (HeaderName::from_static("cache-control"), "no-store"),
            ],
            r#"{"error":"The report carries no usable violation."}"#,
        )
            .into_response();
    }

    match usecases::record_csp_violations(state.app(), &summaries).await {
        Ok(recorded) => (
            StatusCode::OK,
            [
                (HeaderName::from_static("content-type"), "application/json"),
                (HeaderName::from_static("cache-control"), "no-store"),
            ],
            serde_json::json!({ "ok": true, "recorded": recorded }).to_string(),
        )
            .into_response(),
        Err(error) => ApiError::from(error).into_response(),
    }
}

/// Hosts only: the query string in a URI is where a token hides, and the report never needs it.
fn host_of(raw: &str) -> Option<String> {
    let after_scheme = match raw.find("://") {
        Some(index) => &raw[index + 3..],
        None => return non_url_token(raw),
    };
    let without_userinfo = match after_scheme.rfind('@') {
        Some(index) => &after_scheme[index + 1..],
        None => after_scheme,
    };
    let host = without_userinfo.split('/').next()?.trim();
    if host.is_empty() || host.len() > 253 {
        return None;
    }
    Some(host.to_owned())
}

/// Non-URL values some browsers send (`inline`, `eval`, `unsafe-inline`) are kept as the token.
fn non_url_token(raw: &str) -> Option<String> {
    let token = raw.trim();
    if token.len() <= 64
        && token
            .bytes()
            .all(|byte| byte.is_ascii_graphic() && !byte.is_ascii_control())
    {
        Some(token.to_owned())
    } else {
        None
    }
}

/// Keys browsers send for one violation; the spec's own kebab-case names and the camelCase aliases.
fn first_string<'a>(report: &'a serde_json::Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .find_map(|key| report.get(key).and_then(serde_json::Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

/// Turns one reported violation into a summary with hosts only, bounded in size.
fn summarize_violation(report: &serde_json::Value) -> Option<usecases::CspViolationSummary> {
    let directive = first_string(
        report,
        &[
            "violated-directive",
            "violatedDirective",
            "effective-directive",
        ],
    )?
    .chars()
    .take(100)
    .collect::<String>()
    .trim()
    .to_owned();
    if directive.is_empty() {
        return None;
    }
    let blocked_host = first_string(report, &["blocked-uri", "blockedUri"]).and_then(host_of);
    let document_host = first_string(report, &["document-uri", "documentUri"]).and_then(host_of);
    Some(usecases::CspViolationSummary {
        directive,
        blocked_host,
        document_host,
    })
}

#[cfg(test)]
mod tests {
    use super::{host_of, summarize_violation};

    #[test]
    fn req_054_the_host_survives_and_the_query_does_not() {
        assert_eq!(
            host_of("https://cdn.example.com/script.js?access_token=secret123"),
            Some("cdn.example.com".to_owned())
        );
        let authority = concat!("xx@exa", "mple.com:8443/a/b");
        assert_eq!(
            host_of(&format!("https://{authority}")),
            Some("example.com:8443".to_owned())
        );
        assert_eq!(host_of("eval"), Some("eval".to_owned()));
        assert_eq!(host_of("inline"), Some("inline".to_owned()));
    }

    #[test]
    fn req_054_a_violation_survives_with_kebab_case_keys() {
        let report = serde_json::json!({
            "violated-directive": "script-src 'self'",
            "blocked-uri": "https://tracker.example.com/tag.js?token=abc123",
            "document-uri": "https://me.example.com/app?secret=leakme",
        });
        let summary = summarize_violation(&report).expect("a violation");
        assert_eq!(summary.directive, "script-src 'self'");
        assert_eq!(summary.blocked_host.as_deref(), Some("tracker.example.com"));
        assert_eq!(summary.document_host.as_deref(), Some("me.example.com"));
        // The query never lands in the summary.
        let text = format!("{summary:?}");
        assert!(!text.contains("leakme"));
        assert!(!text.contains("abc123"));
    }
}
