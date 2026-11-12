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
        http_config(),
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

#[tokio::test]
async fn req_058_internal_health_reports_a_worker_without_a_job_yet() {
    let response = get(state(true), "/internal/health").await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    assert_eq!(body["ok"], serde_json::json!(true));
    assert_eq!(body["lastCompletedAt"], serde_json::Value::Null);
}
