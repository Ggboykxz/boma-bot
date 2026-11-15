//! Correlation and per-request structured logging.
//!
//! One JSON log per event (`BOMA_LOG_FORMAT=json`); every request carries a request id, a
//! trace id and a span id. The W3C `traceparent` header is honoured when it is well-formed
//! (`00-<32hex>-<16hex>-<2hex>`); anything else — missing, malformed, zero ids, wrong version —
//! is absent as far as the log is concerned, because a wrongly-parsed header makes worse triage
//! than a fresh id. The request id is echoed in `x-request-id` (REQ-054).
//!
//! The log records the **route template**, never the raw path: a page's share token or a
//! signed installation id never reach a log line. The template enumerates every dynamic segment
//! of the router today; an unknown shape is logged as-is, which is honest for static paths.

use std::time::Instant;

use axum::{
    extract::{Request, State},
    http::{HeaderValue, header},
    middleware::Next,
    response::Response as RouterResponse,
};
use uuid::Uuid;

use crate::state::AppState;

/// The ids one request carries through the log tree.
#[derive(Debug, Clone)]
pub struct Correlation {
    /// The request id, also echoed to the caller in `x-request-id`.
    pub request_id: String,
    /// The W3C trace id, 32 lowercase hex characters.
    pub trace_id: String,
    /// The W3C span id, 16 lowercase hex characters.
    pub span_id: String,
}

impl Correlation {
    /// Reads `x-request-id` and `traceparent`, validating both. An absent or malformed header
    /// means "mint one", never "fail the request".
    #[must_use]
    pub fn from_headers(headers: &axum::http::HeaderMap) -> Self {
        let (trace_id, span_id) =
            parse_traceparent(headers.get("traceparent").and_then(|v| v.to_str().ok()))
                .unwrap_or_else(mint_trace);
        let request_id = headers
            .get("x-request-id")
            .and_then(|value| value.to_str().ok())
            .filter(|value| is_reasonable_request_id(value))
            .map_or_else(|| Uuid::new_v4().to_string(), str::to_owned);
        Self {
            request_id,
            trace_id,
            span_id,
        }
    }

    /// The `x-request-id` header value for the response.
    #[must_use]
    pub fn x_request_id(&self) -> HeaderValue {
        HeaderValue::from_str(&self.request_id)
            .unwrap_or_else(|_| HeaderValue::from_static("request"))
    }
}

/// Mints a fresh trace id and span id.
fn mint_trace() -> (String, String) {
    let trace_id = Uuid::new_v4().simple().to_string(); // 32 lower hex
    let raw = Uuid::new_v4().as_u128() & 0x0000_0000_0000_ffff_ffff_ffff_ffff_ffff;
    let span_id = format!("{raw:016x}");
    (trace_id, span_id)
}

fn is_reasonable_request_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= 64 && value.bytes().all(|byte| byte.is_ascii_graphic())
}

/// Parses a well-formed W3C `traceparent` header, or refuses it wholesale.
fn parse_traceparent(header: Option<&str>) -> Option<(String, String)> {
    let header = header?.trim();
    let mut parts = header.split('-');
    let version = parts.next()?;
    let trace_id = parts.next()?;
    let span_id = parts.next()?;
    let flags = parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    if version != "00" {
        return None;
    }
    if trace_id.len() != 32 || !trace_id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    if span_id.len() != 16 || !span_id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    if flags.len() != 2 || !flags.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    if trace_id.bytes().all(|byte| byte == b'0') || span_id.bytes().all(|byte| byte == b'0') {
        return None;
    }
    Some((trace_id.to_ascii_lowercase(), span_id.to_ascii_lowercase()))
}

/// The route template the log records, never the raw path.
fn template(path: &str) -> String {
    if path.starts_with("/api/v1/pages/shared/") {
        return "/api/v1/pages/shared/{token}".to_owned();
    }
    // The invite token is a credential: one-time, space-joining, only the hash is stored.
    // It must never reach a log line, so the path is rendered with the slot, not the value.
    if path.starts_with("/api/v1/invites/") {
        return "/api/v1/invites/{token}".to_owned();
    }
    if let Some(suffix) = path.strip_prefix("/api/v1/bots/")
        && suffix.ends_with("/webhook")
        && suffix.len() > "/webhook".len()
    {
        return "/api/v1/bots/{botId}/webhook".to_owned();
    }
    if path == "/api/v1/messaging/install/slack/callback" {
        return path.to_owned();
    }
    if let Some(rest) = path.strip_prefix("/api/v1/messaging/") {
        let mut segments = rest.split('/');
        let (Some(platform), Some(command), Some(installation)) =
            (segments.next(), segments.next(), segments.next())
        else {
            return path.to_owned();
        };
        if segments.next().is_some()
            || command != "inbound"
            || platform.is_empty()
            || installation.is_empty()
        {
            return path.to_owned();
        }
        return "/api/v1/messaging/{platform}/inbound/{installationId}".to_owned();
    }
    if let Some(rest) = path.strip_prefix("/rpc/") {
        let mut segments = rest.split('/');
        let (Some(namespace), Some(procedure)) = (segments.next(), segments.next()) else {
            return path.to_owned();
        };
        if segments.next().is_some() || namespace.is_empty() || procedure.is_empty() {
            return path.to_owned();
        }
        return "/rpc/{namespace}/{procedure}".to_owned();
    }
    if path.starts_with("/api/v1/billing/webhook") {
        return "/api/v1/billing/webhook".to_owned();
    }
    if let Some(rest) = path.strip_prefix("/api/v1/scim/v2/Users") {
        let _ = rest;
        return "/api/v1/scim/v2/Users/{id?}".to_owned();
    }
    path.to_owned()
}

/// The middleware every request passes through: correlation, a span, and the completion line.
///
/// A failure never escapes as anything but a response; the log path owns no behaviour of its
/// own — it reads the method, route template and response status, renders the subtree.
pub async fn log_http(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> RouterResponse {
    let started = Instant::now();
    let correlation = Correlation::from_headers(request.headers());
    let method = request.method().clone();
    let route = template(request.uri().path());
    let span = tracing::info_span!(
        "http.request",
        method = %method,
        route = %route,
        request_id = %correlation.request_id,
        trace_id = %correlation.trace_id,
        span_id = %correlation.span_id,
    );
    let mut response = span.in_scope(|| next.run(request)).await;
    let latency_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    let status = response.status().as_u16();
    let status_class = match status {
        200..=299 => "2xx",
        300..=399 => "3xx",
        400..=499 => "4xx",
        _ => "5xx",
    };
    state.app().services.metrics.increment(
        "boma_http_requests_total",
        &[("route", route.as_str()), ("status_class", status_class)],
    );
    tracing::info!(
        target: "boma.http",
        method = %method,
        route = %route,
        status = status,
        latency_ms = latency_ms,
        request_id = %correlation.request_id,
        trace_id = %correlation.trace_id,
        span_id = %correlation.span_id,
        "request finished"
    );
    response.headers_mut().insert(
        header::HeaderName::from_static("x-request-id"),
        HeaderValue::from_str(&correlation.request_id)
            .unwrap_or_else(|_| HeaderValue::from_static("request")),
    );
    response
}

#[cfg(test)]
mod tests {
    use super::{parse_traceparent, template};

    #[test]
    fn req_054_parses_a_well_formed_traceparent() {
        let parsed = parse_traceparent(Some(
            "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01",
        ));
        assert_eq!(
            parsed,
            Some((
                "0af7651916cd43dd8448eb211c80319c".to_owned(),
                "b7ad6b7169203331".to_owned()
            ))
        );
    }

    #[test]
    fn req_054_refuses_bad_versions_zero_ids_and_short_flags() {
        assert_eq!(
            parse_traceparent(Some(
                "01-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01"
            )),
            None
        );
        assert_eq!(
            parse_traceparent(Some(
                "00-00000000000000000000000000000000-b7ad6b7169203331-01"
            )),
            None
        );
        assert_eq!(
            parse_traceparent(Some(
                "00-0af7651916cd43dd8448eb211c80319c-0000000000000000-01"
            )),
            None
        );
        assert_eq!(
            parse_traceparent(Some("00-short-b7ad6b7169203331-01")),
            None
        );
        assert_eq!(
            parse_traceparent(Some(
                "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-1"
            )),
            None
        );
        assert_eq!(parse_traceparent(Some("not-a-trace-parent")), None);
        assert_eq!(parse_traceparent(None), None);
    }

    #[test]
    fn req_054_the_template_hides_the_invite_token() {
        // REQ-042: the invite token is one-time and joins a space; it must never reach a log.
        assert_eq!(
            template("/api/v1/invites/invite-secret-token-abc"),
            "/api/v1/invites/{token}"
        );
        assert_eq!(
            template("/api/v1/invites/dead-beef-cafe"),
            "/api/v1/invites/{token}"
        );
    }

    #[test]
    fn req_054_the_template_hides_the_share_token() {
        assert_eq!(
            template("/api/v1/pages/shared/abc123"),
            "/api/v1/pages/shared/{token}"
        );
        assert_eq!(
            template("/api/v1/bots/bot_1234567890/webhook"),
            "/api/v1/bots/{botId}/webhook"
        );
        assert_eq!(template("/health"), "/health");
        assert_eq!(
            template("/rpc/threads/send"),
            "/rpc/{namespace}/{procedure}"
        );
        assert_eq!(
            template("/api/v1/messaging/slack/inbound/ins_123"),
            "/api/v1/messaging/{platform}/inbound/{installationId}"
        );
        assert_eq!(
            template("/api/v1/billing/webhook"),
            "/api/v1/billing/webhook"
        );
    }
}
