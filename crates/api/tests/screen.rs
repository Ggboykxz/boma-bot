//! Integration tests for the sealed screen stream (REQ-030).
//!
//! These prove the seal: bytes flow only through a capability that names bot, computer and
//! generation, valid only while that computer is current. They run against the in-memory stores
//! with a canned PNG from the scripted computer runtime; the Postgres and real-docker paths are
//! proved separately in `boma-infra`.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::{net::SocketAddr, sync::Arc, time::Duration};

use axum::{
    body::Body,
    http::{Method, Request, StatusCode, header},
    response::Response,
};
use boma_api::{AppState, HttpConfig, router};
use boma_app::{Capabilities, Readiness, ReadinessCheck};
use http_body_util::BodyExt;
use tower::ServiceExt;

#[derive(Clone)]
struct Caller {
    cookie: String,
    space_id: String,
}

#[derive(Debug)]
struct Probe;

#[async_trait::async_trait]
impl ReadinessCheck for Probe {
    fn name(&self) -> &'static str {
        "database"
    }

    async fn check(&self) -> Result<(), &'static str> {
        Ok(())
    }
}

fn capabilities() -> Capabilities {
    Capabilities {
        version: "0.1.0-test".to_owned(),
        runtime: "sandboxed".to_owned(),
        sandbox: "docker".to_owned(),
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

fn state(test_app: &boma_testkit::TestApp) -> AppState {
    AppState::new(
        Readiness::new(vec![Arc::new(Probe)]),
        capabilities(),
        boma_api::AuthCapabilities::password_only(None, None),
        HttpConfig::new(
            SocketAddr::from(([127, 0, 0, 1], 3000)),
            Duration::from_secs(30),
            Duration::from_millis(500),
            64 * 1024,
            Vec::new(),
        )
        .expect("the fixture configuration is valid"),
        test_app.app().clone(),
    )
}

async fn post(state: AppState, caller: &Caller, path: &str, body: serde_json::Value) -> Response {
    let request = Request::builder()
        .method(Method::POST)
        .uri(path)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::COOKIE, &caller.cookie)
        .header("x-space-id", &caller.space_id)
        .body(Body::from(body.to_string()))
        .expect("the request builds");
    router(state)
        .oneshot(request)
        .await
        .expect("the router answers")
}

async fn post_raw(
    state: AppState,
    caller: &Caller,
    path: &str,
    capability: Option<&str>,
    body: Vec<u8>,
) -> Response {
    let mut builder = Request::builder()
        .method(Method::POST)
        .uri(path)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::COOKIE, &caller.cookie)
        .header("x-space-id", &caller.space_id);
    if let Some(capability) = capability {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {capability}"));
    }
    let request = builder.body(Body::from(body)).expect("the request builds");
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
    serde_json::from_slice(&bytes).expect("the body is JSON")
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // one live end-to-end scenario; splitting it loses the story
async fn req_030_a_valid_capability_streams_the_frame_and_a_stale_one_is_refused() {
    let test = boma_testkit::TestApp::builder()
        .ids(vec![
            "usr_1".to_owned(),
            "org_1".to_owned(),
            "spc_1".to_owned(),
            "bot_1".to_owned(),
            "thr_1".to_owned(),
            "run_1".to_owned(),
            "cmp_1".to_owned(),
            "msg_1".to_owned(),
            "msg_2".to_owned(),
        ])
        // A stopped computer starts again for real: started=true bumps the generation.
        .computer_answers(vec![
            boma_testkit::ComputerAnswer::Started {
                external_id: "ctr_1".to_owned(),
                home_id: "vol_1".to_owned(),
            },
            boma_testkit::ComputerAnswer::Started {
                external_id: "ctr_1".to_owned(),
                home_id: "vol_1".to_owned(),
            },
        ])
        .fixed_clock("2026-01-01T00:00:00Z")
        .build();
    let user_id = test
        .seed_user("person@example.com", "correct horse battery")
        .await
        .expect("the person exists");
    let space = test
        .app()
        .stores
        .spaces
        .default_for(&user_id)
        .await
        .expect("the store answers")
        .expect("the space exists");
    let context = boma_testkit::TestApp::member_context(&space.id, &user_id);
    let bot = boma_app::usecases::create_bot(test.app(), &context, "Analyst", None, None)
        .await
        .expect("the bot is created");

    let cookie = sign_in_as(&test, "person@example.com").await;
    let caller = Caller {
        cookie,
        space_id: space.id.as_str().to_owned(),
    };
    let state = state(&test);
    // Start the computer.
    let response = post(
        state.clone(),
        &caller,
        "/rpc/computer/start",
        serde_json::json!({ "botId": bot.bot.id.as_str() }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    // A member mints a screen capability for their computer.
    let response = post(
        state.clone(),
        &caller,
        "/rpc/computer/openScreen",
        serde_json::json!({ "botId": bot.bot.id.as_str() }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    let capability = body["capability"]
        .as_str()
        .expect("a capability is issued")
        .to_owned();
    assert_eq!(body["screenGeneration"], serde_json::json!(1));
    assert_ne!(capability, "");

    // The sealed capability streams the frame the deployment produced.
    let response = post_raw(
        state.clone(),
        &caller,
        "/api/internal/screen-target",
        Some(&capability),
        br#"{ "path": "/screen" }"#.to_vec(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    assert_eq!(content_type.as_deref(), Some("text/event-stream"));
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("the body is in memory")
        .to_bytes();
    let frame = String::from_utf8_lossy(&bytes);
    assert!(frame.contains("event: frame"), "{frame}");
    assert!(frame.contains("screenGeneration\": 1"), "{frame}");

    // The screen generation moves when the computer restarts; the capability dies with it.
    let response = post(
        state.clone(),
        &caller,
        "/rpc/computer/stop",
        serde_json::json!({ "botId": bot.bot.id.as_str() }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    // A stopped computer opens nothing: minting is refused, plainly.
    let response = post(
        state.clone(),
        &caller,
        "/rpc/computer/openScreen",
        serde_json::json!({ "botId": bot.bot.id.as_str() }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body = json(response).await;
    assert_eq!(body["code"], serde_json::json!("CONFLICT"));

    let response = post(
        state.clone(),
        &caller,
        "/rpc/computer/start",
        serde_json::json!({ "botId": bot.bot.id.as_str() }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    let response = post_raw(
        state.clone(),
        &caller,
        "/api/internal/screen-target",
        Some(&capability),
        br#"{ "path": "/screen" }"#.to_vec(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let body = json(response).await;
    assert_eq!(body["code"], serde_json::json!("FORBIDDEN"));

    // Without a bearer the answer is the same 403; the capability is the auth.
    let response = post_raw(
        state,
        &caller,
        "/api/internal/screen-target",
        None,
        br#"{ "path": "/screen" }"#.to_vec(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

/// Signs in as the given account and returns the session cookie.
async fn sign_in_as(test_app: &boma_testkit::TestApp, email: &str) -> String {
    let body = serde_json::json!({
        "email": email,
        "password": "correct horse battery",
    });
    let request = Request::builder()
        .method(Method::POST)
        .uri("/api/auth/sign-in")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .expect("the request builds");
    let response = router(state(test_app))
        .oneshot(request)
        .await
        .expect("the router answers");
    assert_eq!(response.status(), StatusCode::OK);
    response
        .headers()
        .get(header::SET_COOKIE)
        .and_then(|value| value.to_str().ok())
        .expect("the session cookie is set")
        .split(';')
        .next()
        .expect("the cookie has a value")
        .to_owned()
}
