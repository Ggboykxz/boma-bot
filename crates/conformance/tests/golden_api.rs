//! Golden replay: every fixture marked implemented is sent to the real router and compared byte for
//! byte, field for field.
//!
//! Pending fixtures are not silently ignored: each one prints `SKIP <name> (<req>): <reason>`, and
//! `tests/golden_manifest.rs` refuses a pending entry without a reason.

// An integration test file is compiled as its own crate, so clippy's `allow-*-in-tests` (which
// covers `#[test]` functions and `#[cfg(test)]` modules only) does not reach the helpers below.
// AGENTS.md allows unwrap, expect and panic in test code; this is where that permission is taken,
// once and in writing.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::{net::SocketAddr, sync::Arc, time::Duration};

use async_trait::async_trait;
use axum::{
    body::Body,
    http::{HeaderName, HeaderValue, Request, Response},
};
use boma_api::{AppState, HttpConfig, router};
use boma_app::{Capabilities, Readiness, ReadinessCheck};
use boma_conformance::{Fixture, Manifest, Status, load_fixture};
use http_body_util::BodyExt;
use tower::ServiceExt;

#[derive(Debug)]
struct HealthyProbe;

#[async_trait]
impl ReadinessCheck for HealthyProbe {
    fn name(&self) -> &'static str {
        "database"
    }

    async fn check(&self) -> Result<(), &'static str> {
        Ok(())
    }
}

/// The deployment profile the fixtures were captured from (`spec/golden/ready.json`).
///
/// Pinning these values here is what makes the replay strict: the router builds the response from
/// capabilities, so a field renamed or dropped in the projection fails the comparison.
fn golden_capabilities() -> Capabilities {
    Capabilities {
        version: "0.0.0-golden".to_owned(),
        runtime: "sandboxed".to_owned(),
        sandbox: "docker".to_owned(),
        integrations: Vec::new(),
        messaging: false,
        email: None,
        billing: None,
        jobs: "inline".to_owned(),
        realtime: "postgres".to_owned(),
        degraded: false,
        revision: Some("golden".to_owned()),
    }
}

fn golden_state() -> AppState {
    let http = HttpConfig::new(
        SocketAddr::from(([127, 0, 0, 1], 3000)),
        Duration::from_secs(30),
        Duration::from_secs(5),
        64 * 1024,
        Vec::new(),
    )
    .expect("the fixture configuration is valid");
    AppState::new(
        Readiness::new(vec![Arc::new(HealthyProbe)]),
        golden_capabilities(),
        http,
    )
}

async fn replay(fixture: &Fixture) -> Response<Body> {
    let mut builder = Request::builder()
        .method(fixture.request.method.as_str())
        .uri(fixture.request.path.as_str());
    for (name, value) in &fixture.request.headers {
        builder = builder.header(
            HeaderName::from_bytes(name.as_bytes()).expect("the fixture header name is valid"),
            HeaderValue::from_str(value).expect("the fixture header value is valid"),
        );
    }
    let body = match &fixture.request.body {
        Some(value) => {
            Body::from(serde_json::to_vec(value).expect("the fixture request body is serialisable"))
        }
        None => Body::empty(),
    };
    let request = builder.body(body).expect("the fixture request builds");
    router(golden_state())
        .oneshot(request)
        .await
        .expect("the router answers")
}

#[tokio::test]
async fn req_053_implemented_goldens_replay_exactly() {
    let manifest = Manifest::load().expect("golden-status.json is valid");
    let mut replayed = 0;
    let mut pending = 0;

    for (name, entry) in &manifest.api {
        if entry.status == Status::Pending {
            let reason = entry.reason.as_deref().unwrap_or("<no reason recorded>");
            eprintln!("SKIP {name} ({}) : {reason}", entry.req);
            pending += 1;
            continue;
        }

        let fixture = load_fixture(name).expect("the fixture parses");
        let response = replay(&fixture).await;
        assert_eq!(
            response.status().as_u16(),
            fixture.response.status,
            "golden {name}: status"
        );

        let Some(expected) = fixture.response.body.as_ref() else {
            panic!("golden {name} is marked implemented but pins no body");
        };
        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("the response body is in memory")
            .to_bytes();
        let actual: serde_json::Value =
            serde_json::from_slice(&bytes).expect("the response is JSON");
        assert_eq!(&actual, expected, "golden {name}: body");
        replayed += 1;
    }

    assert!(
        replayed >= 2,
        "at least /health and /ready must be replayed at M0, got {replayed}"
    );
    assert_eq!(
        replayed + pending,
        manifest.api.len(),
        "every fixture is either replayed or pending"
    );
}
