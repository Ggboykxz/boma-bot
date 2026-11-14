//! Integration tests for the HTTP surface.
//!
//! These run against the real router with no network and no database: the probes are fakes, which
//! is what the readiness contract allows. The tenant and persistence suites arrive with M1.

// An integration test file is compiled as its own crate, so clippy's `allow-*-in-tests` (which
// covers `#[test]` functions and `#[cfg(test)]` modules only) does not reach the helpers below.
// AGENTS.md allows unwrap, expect and panic in test code; this is where that permission is taken,
// once and in writing.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::{net::SocketAddr, sync::Arc, time::Duration};

use async_trait::async_trait;
use axum::{
    body::Body,
    http::{Request, StatusCode, header},
    response::Response,
};
use boma_api::{AppState, HttpConfig, router};
use boma_app::{Capabilities, Readiness, ReadinessCheck};
use http_body_util::BodyExt;
use tower::ServiceExt;

#[derive(Debug)]
struct Probe {
    healthy: bool,
}

#[async_trait]
impl ReadinessCheck for Probe {
    fn name(&self) -> &'static str {
        "database"
    }

    async fn check(&self) -> Result<(), &'static str> {
        if self.healthy {
            Ok(())
        } else {
            Err("database unavailable")
        }
    }
}

fn http_config() -> HttpConfig {
    HttpConfig::new(
        SocketAddr::from(([127, 0, 0, 1], 3000)),
        Duration::from_secs(30),
        Duration::from_millis(500),
        64 * 1024,
        Vec::new(),
    )
    .expect("the fixture configuration is valid")
}

fn capabilities() -> Capabilities {
    Capabilities {
        version: "0.1.0-test".to_owned(),
        runtime: "sandboxed".to_owned(),
        sandbox: "none".to_owned(),
        integrations: Vec::new(),
        messaging: false,
        email: None,
        billing: None,
        jobs: "inline".to_owned(),
        realtime: "postgres".to_owned(),
        degraded: false,
        revision: Some("test".to_owned()),
    }
}

fn state(healthy: bool) -> AppState {
    AppState::new(
        Readiness::new(vec![Arc::new(Probe { healthy })]),
        capabilities(),
        auth_capabilities(),
        http_config(),
        boma_app::App::new(
            boma_app::Stores::new(
                std::sync::Arc::new(boma_testkit::MemoryAccountStore::default()),
                std::sync::Arc::new(boma_testkit::MemoryUserStore::default()),
                std::sync::Arc::new(boma_testkit::MemorySessionStore::default()),
                std::sync::Arc::new(boma_testkit::MemorySpaceStore::default()),
                std::sync::Arc::new(boma_testkit::MemoryBotStore::default()),
                std::sync::Arc::new(boma_testkit::MemoryThreadStore::default()),
                std::sync::Arc::new(boma_testkit::MemoryMessageStore::default()),
                std::sync::Arc::new(boma_testkit::MemoryRunStore::default()),
                std::sync::Arc::new(boma_testkit::MemoryEventStore::default()),
                std::sync::Arc::new(boma_testkit::MemoryInvitationStore::default()),
                std::sync::Arc::new(boma_testkit::MemoryCardStore::default()),
                std::sync::Arc::new(boma_testkit::MemoryPendingToolCallStore::default()),
                std::sync::Arc::new(boma_testkit::MemoryEffectStore::default()),
                std::sync::Arc::new(boma_testkit::MemoryToolStore::default()),
                std::sync::Arc::new(boma_testkit::MemoryRateLimitStore::default()),
                std::sync::Arc::new(boma_testkit::MemorySecretStore::default()),
                std::sync::Arc::new(boma_testkit::MemoryPluginSubmissionStore::default()),
                std::sync::Arc::new(boma_testkit::MemorySkillStore::default()),
                std::sync::Arc::new(boma_testkit::MemoryMcpServerStore::default()),
                std::sync::Arc::new(boma_testkit::MemoryAuditStore::default()),
                std::sync::Arc::new(boma_testkit::MemoryConnectorStore::default()),
                std::sync::Arc::new(boma_testkit::MemoryPluginInstallStore::default()),
                std::sync::Arc::new(boma_testkit::MemoryArtifactStore::default()),
                std::sync::Arc::new(boma_testkit::MemoryReactionStore::default()),
                std::sync::Arc::new(boma_testkit::MemoryReadStateStore::default()),
                std::sync::Arc::new(boma_testkit::MemoryRosterStore::default()),
                std::sync::Arc::new(boma_testkit::MemoryMemoryStore::default()),
                std::sync::Arc::new(boma_testkit::MemoryRoutineStore::default()),
                std::sync::Arc::new(boma_testkit::MemoryComputerStore::default()),
                std::sync::Arc::new(boma_testkit::MemoryScreenCapabilityStore::default()),
            ),
            boma_app::Services::new(
                std::sync::Arc::new(boma_testkit::MemoryEventPublisher::default()),
                std::sync::Arc::new(boma_testkit::FixedHasher),
                std::sync::Arc::new(boma_testkit::ScriptedTokens::default()),
                std::sync::Arc::new(boma_testkit::ScriptedIds::counter()),
                std::sync::Arc::new(boma_testkit::ScriptedClock::fixed("2026-01-01T00:00:00Z")),
                boma_domain::model::DeploymentModels {
                    reachable: vec![
                        "openai/gpt-5".to_owned(),
                        "anthropic/claude-sonnet".to_owned(),
                    ],
                    default: "anthropic/claude-sonnet".to_owned(),
                },
                std::sync::Arc::new(boma_testkit::ScriptedMcpTransport::new()),
                std::sync::Arc::new(boma_testkit::ScriptedPluginFetcher::new()),
                std::sync::Arc::new(boma_testkit::ScriptedOAuthClient::new()),
                std::sync::Arc::new(boma_testkit::MemoryArtifactContentStore::default()),
            ),
        ),
    )
}

fn auth_capabilities() -> boma_api::AuthCapabilities {
    boma_api::AuthCapabilities::password_only(
        Some("http://127.0.0.1:5173/reset-password".to_owned()),
        Some("127.0.0.1".to_owned()),
    )
}

async fn get(state: AppState, path: &str) -> Response {
    let request = Request::builder()
        .uri(path)
        .body(Body::empty())
        .expect("a static request builds");
    router(state)
        .oneshot(request)
        .await
        .expect("the router answers")
}

async fn json(response: Response) -> serde_json::Value {
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("the body is in memory")
        .to_bytes();
    serde_json::from_slice(&bytes).expect("the response is JSON")
}

#[tokio::test]
async fn req_053_health_is_live_even_when_every_dependency_is_down() {
    let response = get(state(false), "/health").await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(json(response).await, serde_json::json!({ "ok": true }));
}

#[tokio::test]
async fn req_053_ready_answers_200_with_the_capability_report_when_healthy() {
    let response = get(state(true), "/ready").await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    assert_eq!(body["ok"], serde_json::json!(true));
    assert_eq!(body["version"], serde_json::json!("0.1.0-test"));
    assert_eq!(body["degraded"], serde_json::json!(false));
    assert_eq!(body["email"], serde_json::Value::Null);
    assert_eq!(body["billing"], serde_json::Value::Null);
}

#[tokio::test]
async fn req_053_ready_answers_503_with_the_error_envelope_when_the_database_is_down() {
    let response = get(state(false), "/ready").await;
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body = json(response).await;
    assert_eq!(body["code"], serde_json::json!("INTERNAL"));
    assert_eq!(
        body.as_object().map(serde_json::Map::len),
        Some(2),
        "a readiness failure must not leak the dependency's error"
    );
}

#[tokio::test]
async fn req_053_health_aliases_answer_like_the_canonical_paths() {
    for (canonical, alias) in [("/health", "/healthz"), ("/ready", "/readyz")] {
        let expected = get(state(true), canonical).await;
        let actual = get(state(true), alias).await;
        assert_eq!(expected.status(), actual.status(), "{alias}");
        assert_eq!(json(expected).await, json(actual).await, "{alias}");
    }
}

/// D-02: the reference served every captured mail, tokens included, on a development route. The
/// rebuild has no development mail reader at all, so the route must stay unknown — a 404, never a
/// body. Named after the defect, not a requirement: the route belongs to no milestone's REQ.
#[tokio::test]
async fn d_02_the_development_mail_route_is_not_served() {
    for path in ["/api/dev/emails", "/api/dev/mail", "/api/dev/inbox"] {
        let response = get(state(true), path).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{path}");
        let body = json(response).await;
        assert_eq!(body["code"], serde_json::json!("NOT_FOUND"), "{path}");
    }
}

#[tokio::test]
async fn req_040_an_unknown_route_uses_the_stable_error_envelope() {
    let response = get(state(true), "/nope").await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body = json(response).await;
    assert_eq!(body["code"], serde_json::json!("NOT_FOUND"));
    assert!(body["error"].is_string());
}

#[tokio::test]
async fn req_054_every_health_answer_carries_a_request_id_and_no_store() {
    for path in ["/health", "/ready", "/internal/health"] {
        let response = get(state(true), path).await;
        assert!(
            response.headers().contains_key("x-request-id"),
            "{path} must echo a request id"
        );
        assert_eq!(
            response
                .headers()
                .get(header::CACHE_CONTROL)
                .and_then(|value| value.to_str().ok()),
            Some("no-store"),
            "{path}"
        );
        assert_eq!(
            response
                .headers()
                .get(header::X_CONTENT_TYPE_OPTIONS)
                .and_then(|value| value.to_str().ok()),
            Some("nosniff"),
            "{path}"
        );
    }
}

async fn post_bytes(state: AppState, path: &str, bytes: usize) -> Response {
    let request = Request::builder()
        .method("POST")
        .uri(path)
        .header("content-length", bytes.to_string())
        .header("content-type", "application/octet-stream")
        .body(Body::from(vec![b'x'; bytes]))
        .expect("the request builds");
    router(state)
        .oneshot(request)
        .await
        .expect("the router answers")
}

#[tokio::test]
async fn d_07_the_global_rpc_cap_refuses_an_oversized_body() {
    // 128 KiB is past the fixture's 64 KiB global limit; the body never reaches the handler.
    let response = post_bytes(state(true), "/rpc/threads/send", 128 * 1024).await;
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn d_07_per_procedure_caps_hold() {
    // The screen route is capped at 16 KiB even though the global limit is higher in this
    // fixture: per-procedure caps are enforced before the handler runs.
    let response = post_bytes(state(true), "/api/internal/screen-target", 32 * 1024).await;
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    let response = post_bytes(state(true), "/api/internal/screen-target", 8 * 1024).await;
    assert_ne!(
        response.status(),
        StatusCode::PAYLOAD_TOO_LARGE,
        "a body within the cap reaches the handler"
    );
}

#[tokio::test]
async fn req_058_internal_health_reports_a_worker_without_a_job_yet() {
    let response = get(state(true), "/internal/health").await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    assert_eq!(body["ok"], serde_json::json!(true));
    assert_eq!(body["lastCompletedAt"], serde_json::Value::Null);
}

fn fresh_state() -> AppState {
    let app = boma_testkit::TestApp::builder().build().app().clone();
    AppState::new(
        Readiness::new(vec![Arc::new(Probe { healthy: true })]),
        capabilities(),
        auth_capabilities(),
        http_config(),
        app,
    )
}

async fn post_plugin_submission(state: AppState, body: &str) -> Response {
    router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/plugin-submissions")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::ORIGIN, "https://example.test")
                .body(Body::from(body.to_owned()))
                .expect("the request builds"),
        )
        .await
        .expect("the router answers")
}

async fn body_string(response: Response) -> String {
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("the body is in memory")
        .to_bytes();
    String::from_utf8(bytes.to_vec()).expect("the body is UTF-8")
}

/// REQ-034, D-06: the eleventh submission from the same address in an hour is refused.
#[tokio::test]
async fn req_034_an_over_limit_ip_is_refused_with_429() {
    // A fresh app: the twelve posts share one in-memory database, like twelve replicas on one.
    let shared = fresh_state();
    let body = r#"{"source": "https://example.test/plugin.git#main", "name": "Example", "contact": "author@example.test"}"#;
    for attempt in 1..=10 {
        let response = post_plugin_submission(shared.clone(), body).await;
        assert_eq!(response.status(), StatusCode::OK, "attempt {attempt}");
    }
    let response = post_plugin_submission(shared, body).await;
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(
        response
            .headers()
            .get(header::RETRY_AFTER)
            .expect("retry-after"),
        "3600"
    );
}

/// REQ-034, D-06: the browser preflight for a cross-origin POST is answered.
#[tokio::test]
async fn req_034_options_preflight_is_answered_with_cors() {
    let response = router(fresh_state())
        .oneshot(
            Request::builder()
                .method("OPTIONS")
                .uri("/api/v1/plugin-submissions")
                .body(Body::empty())
                .expect("the request builds"),
        )
        .await
        .expect("the router answers");
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        response
            .headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .expect("allow-origin"),
        "*"
    );
    assert_eq!(
        response
            .headers()
            .get(header::ACCESS_CONTROL_ALLOW_METHODS)
            .expect("allow-methods"),
        "POST, OPTIONS"
    );
    assert_eq!(
        response
            .headers()
            .get(header::ACCESS_CONTROL_ALLOW_HEADERS)
            .expect("allow-headers"),
        "content-type"
    );
}

/// REQ-034: a body larger than 32 KiB is refused by the handler with the fixed wording.
///
/// Bodies above the route's outer cap (33 KiB) are rejected by the HTTP layer before they reach
/// the handler; the interesting boundary is the one the handler owns: just over 32 KiB.
#[tokio::test]
async fn req_034_a_body_over_the_cap_is_refused() {
    let source = format!("https://example.test/plugin.git#{}", "a".repeat(32 * 1024));
    let body = serde_json::json!({ "source": source }).to_string();
    assert!(
        body.len() > 32 * 1024 && body.len() < 33 * 1024,
        "{}",
        body.len()
    );
    let response = post_plugin_submission(fresh_state(), &body).await;
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    let body = body_string(response).await;
    assert!(body.contains("too large"), "{body}");
}

/// REQ-034: a malformed source is refused with the words the golden fixture pins.
#[tokio::test]
async fn req_034_an_invalid_source_is_refused_with_the_fixed_message() {
    let response = post_plugin_submission(fresh_state(), r#"{"source": "not a repository"}"#).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = body_string(response).await;
    assert_eq!(body, r#"{"error": "Invalid source."}"#);
}
