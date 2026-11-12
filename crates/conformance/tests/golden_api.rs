//! Golden replay: every fixture marked implemented is sent to the real router and compared byte for
//! byte, field for field.
//!
//! Pending fixtures are not silently ignored: each one prints `SKIP <name> (<req>): <reason>`, and
//! `tests/golden_manifest.rs` refuses a pending entry without a reason.
//!
//! The environment around a request is part of the capture, so the harness rebuilds it: the
//! `golden_world` module describes the space, the bot, the thread and the messages the fixtures
//! were captured against, and the scripted identifiers and clock make the ids and the timestamps
//! come out exactly as the fixture expects.
//!
//! Two placeholders are substituted, never interpreted: `session=<redacted>` becomes the world's
//! session cookie and `<synthetic-address>` becomes a real address.

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
use boma_api::{AppState, AuthCapabilities, HttpConfig, router};
use boma_app::{Capabilities, Readiness, ReadinessCheck};
use boma_conformance::{Fixture, Manifest, Status, golden_world, load_fixture};
use http_body_util::BodyExt;
use tokio_stream::StreamExt;
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

/// The sign-in methods the deployment profile offers (`spec/golden/auth-capabilities.json`).
fn golden_auth_capabilities() -> AuthCapabilities {
    AuthCapabilities::password_only(
        Some("http://127.0.0.1:5173/reset-password".to_owned()),
        Some("127.0.0.1".to_owned()),
    )
}

fn golden_state(app: boma_app::App) -> AppState {
    let http = HttpConfig::new(
        SocketAddr::from(([127, 0, 0, 1], 3000)),
        Duration::from_secs(30),
        Duration::from_secs(5),
        64 * 1024,
        Vec::new(),
    )
    .expect("the fixture configuration is valid")
    .with_secure_cookies(false);
    AppState::new(
        Readiness::new(vec![Arc::new(HealthyProbe)]),
        golden_capabilities(),
        golden_auth_capabilities(),
        http,
        app,
    )
}

/// The fixtures that carry no world: they are answered from configuration alone.
const WORLDLESS: &[&str] = &["health", "ready", "auth-capabilities", "unauthorized"];

/// The fixture whose world is the account alone, without a bot.
const ACCOUNT_WORLD: &str = "send-message";

/// Builds the state for one fixture.
async fn state_for(fixture: &str) -> AppState {
    if WORLDLESS.contains(&fixture) {
        return golden_state(
            golden_world::world_for(ACCOUNT_WORLD)
                .await
                .expect("a world")
                .app,
        );
    }
    let world = golden_world::world_for(fixture)
        .await
        .unwrap_or_else(|| panic!("golden {fixture} needs a world"));
    golden_state(world.app)
}

/// The cookie a fixture's request carries, with the placeholder substituted.
fn cookie_of(fixture: &Fixture, world: Option<&golden_world::GoldenWorld>) -> Option<String> {
    let header = fixture.request.headers.get("cookie")?;
    let value = header.strip_prefix("session=")?;
    if value == "<redacted>" {
        return Some(
            world
                .expect("a redacted cookie needs a world")
                .cookie
                .clone(),
        );
    }
    Some(value.to_owned())
}

/// Substitutes the two placeholders a fixture may carry.
fn substitute(value: &str, world: Option<&golden_world::GoldenWorld>) -> String {
    let value = value.replace("session=<redacted>", "session=placeholder");
    match world {
        Some(world) => value
            .replace("session=placeholder", &world.cookie)
            .replace("<synthetic-address>", "invitee@example.com"),
        None => value,
    }
}

/// Replays one fixture and returns its response.
async fn replay(fixture_name: &str, fixture: &Fixture) -> Response<Body> {
    let world = golden_world::world_for(fixture_name).await;
    let state = state_for(fixture_name).await;

    let mut builder = Request::builder()
        .method(fixture.request.method.as_str())
        .uri(fixture.request.path.as_str());
    for (name, value) in &fixture.request.headers {
        if name == "cookie" {
            if let Some(cookie) = cookie_of(fixture, world.as_ref()) {
                builder = builder.header(
                    HeaderName::from_bytes(name.as_bytes()).expect("the header name is valid"),
                    HeaderValue::from_str(&cookie).expect("the cookie is a valid header value"),
                );
            }
            continue;
        }
        builder = builder.header(
            HeaderName::from_bytes(name.as_bytes()).expect("the fixture header name is valid"),
            HeaderValue::from_str(&substitute(value, world.as_ref()))
                .expect("the fixture header value is valid"),
        );
    }
    let body = match &fixture.request.body {
        Some(value) => {
            let json =
                serde_json::to_string(value).expect("the fixture request body is serialisable");
            Body::from(substitute(&json, world.as_ref()))
        }
        None => Body::empty(),
    };
    let request = builder.body(body).expect("the fixture request builds");
    router(state)
        .oneshot(request)
        .await
        .expect("the router answers")
}

/// Reads a response body as JSON.
async fn json(response: Response<Body>) -> serde_json::Value {
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("the response body is in memory")
        .to_bytes();
    serde_json::from_slice(&bytes).expect("the response is JSON")
}

/// Reads the `data:` frames of an event stream, up to the number the fixture expects.
///
/// A stream stays open: the heartbeat proves the connection is alive, so the whole body would
/// never arrive. The harness reads exactly the frames the fixture pins and then stops, which is
/// also what a client does when it has what it needs.
async fn frames(response: Response<Body>, expected: usize) -> Vec<serde_json::Value> {
    let mut stream = response.into_body().into_data_stream();
    let mut buffer = String::new();
    let mut frames = Vec::new();
    let deadline = tokio::time::sleep(Duration::from_secs(10));
    tokio::pin!(deadline);

    while frames.len() < expected {
        tokio::select! {
            () = &mut deadline => panic!("the stream did not produce {expected} frames"),
            chunk = stream.next() => {
                let Some(chunk) = chunk else { break };
                let bytes = chunk.expect("a chunk of the stream");
                let text = std::str::from_utf8(&bytes).expect("the stream is UTF-8");
                buffer.push_str(text);
                while let Some(end) = buffer.find('\n') {
                    let line: String = buffer.drain(..=end).collect();
                    if let Some(data) = line.trim_end().strip_prefix("data:") {
                        let data = data.trim();
                        if !data.is_empty() {
                            frames.push(serde_json::from_str(data).expect("a frame is JSON"));
                        }
                    }
                }
            }
        }
    }
    frames
}

/// Whether a fixture's request carries a client nonce, so replaying it twice must be identical.
fn carries_nonce(fixture: &Fixture) -> bool {
    fixture
        .request
        .body
        .as_ref()
        .and_then(|body| body.get("clientNonce"))
        .is_some()
}

#[tokio::test]
async fn req_001_implemented_goldens_replay_exactly() {
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
        let response = replay(name, &fixture).await;
        assert_eq!(
            response.status().as_u16(),
            fixture.response.status,
            "golden {name}: status"
        );

        // A streamed fixture compares its frames; a JSON fixture compares its body.
        if let Some(expected_frames) = fixture.response.events.as_ref() {
            let expected: Vec<serde_json::Value> = expected_frames
                .as_array()
                .expect("the events are an array")
                .to_owned();
            let actual = frames(response, expected.len()).await;
            assert_eq!(
                actual.len(),
                expected.len(),
                "golden {name}: the stream carries {} frames, expected {}",
                actual.len(),
                expected.len()
            );
            for (index, (actual_frame, expected_frame)) in
                actual.iter().zip(expected.iter()).enumerate()
            {
                assert_eq!(actual_frame, expected_frame, "golden {name}: frame {index}");
            }
            replayed += 1;
            continue;
        }

        let Some(expected) = fixture.response.body.as_ref() else {
            panic!("golden {name} is marked implemented but pins no body");
        };

        // A retried send must answer twice with the same ids (REQ-002): the fixture itself is the
        // proof, so the harness sends it twice and requires two identical answers.
        if carries_nonce(&fixture) {
            let first = json(response).await;
            let second_response = replay(name, &fixture).await;
            let second = json(second_response).await;
            assert_eq!(first, second, "golden {name}: a replay must be identical");
            assert_eq!(&first, expected, "golden {name}: body");
        } else {
            let actual = json(response).await;
            assert_eq!(&actual, expected, "golden {name}: body");
        }
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
