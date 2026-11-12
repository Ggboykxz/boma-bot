//! Integration tests for the RPC surface.
//!
//! These drive the real router with the in-memory stores: no network, no database, and no
//! production adapter. What they pin is the contract — the paths, the bodies, the error envelope
//! and the rules a client depends on (REQ-001, REQ-002, REQ-005, REQ-040, REQ-041, REQ-042,
//! REQ-043).

// An integration test file is compiled as its own crate, so clippy's `allow-*-in-tests` (which
// covers `#[test]` functions and `#[cfg(test)]` modules only) does not reach the helpers below.
// AGENTS.md allows unwrap, expect and panic in test code; this is where that permission is taken,
// once and in writing.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::{net::SocketAddr, sync::Arc, time::Duration};

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
    response::Response,
};
use boma_api::{AppState, AuthCapabilities, HttpConfig, router};
use boma_app::{Capabilities, Readiness, ReadinessCheck};
use boma_testkit::TestApp;
use http_body_util::BodyExt;
use tower::ServiceExt;

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

fn http_config() -> HttpConfig {
    HttpConfig::new(
        SocketAddr::from(([127, 0, 0, 1], 3000)),
        Duration::from_secs(30),
        Duration::from_millis(500),
        64 * 1024,
        Vec::new(),
    )
    .expect("the fixture configuration is valid")
    .with_secure_cookies(false)
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

fn state(test_app: &TestApp) -> AppState {
    AppState::new(
        Readiness::new(vec![Arc::new(Probe)]),
        capabilities(),
        AuthCapabilities::password_only(
            Some("http://127.0.0.1:5173/reset-password".to_owned()),
            Some("127.0.0.1".to_owned()),
        ),
        http_config(),
        test_app.app().clone(),
    )
}

async fn post(
    state: AppState,
    path: &str,
    caller: Option<&Caller>,
    body: serde_json::Value,
) -> Response {
    let mut builder = Request::builder()
        .method("POST")
        .uri(path)
        .header("content-type", "application/json");
    if let Some(caller) = caller {
        builder = builder
            .header(header::COOKIE, &caller.cookie)
            .header("x-space-id", &caller.space_id);
    }
    let request = builder
        .body(Body::from(body.to_string()))
        .expect("the request builds");
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

/// Signs a seeded person in and returns the session cookie.
async fn sign_in_as(test_app: &TestApp, email: &str) -> String {
    let request = Request::builder()
        .method("POST")
        .uri("/api/auth/sign-in")
        .header("content-type", "application/json")
        .body(Body::from(format!(
            r#"{{"email":"{email}","password":"correct horse battery"}}"#
        )))
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
        .expect("the sign-in sets a cookie")
        .split(';')
        .next()
        .expect("the cookie has a value")
        .to_owned()
}

/// Who a request acts as: the session cookie and the space it acts in.
#[derive(Debug, Clone)]
struct Caller {
    cookie: String,
    space_id: String,
}

/// A signed-in member with a space and a bot.
struct World {
    app: TestApp,
    caller: Caller,
    bot_id: String,
    thread_id: String,
}

async fn world() -> World {
    let test_app = TestApp::builder().build();
    let user_id = test_app
        .seed_user("person@example.com", "correct horse battery")
        .await
        .expect("the person exists");
    let space_id = test_app
        .seed_space(&user_id, "Golden space")
        .await
        .expect("the space exists");
    let (bot_id, thread_id) = test_app
        .seed_bot(&space_id, &user_id, "Analyst")
        .await
        .expect("the bot exists");
    let cookie = sign_in_as(&test_app, "person@example.com").await;
    World {
        app: test_app,
        caller: Caller {
            cookie,
            space_id: space_id.as_str().to_owned(),
        },
        bot_id: bot_id.as_str().to_owned(),
        thread_id: thread_id.as_str().to_owned(),
    }
}

#[tokio::test]
async fn req_041_an_unsigned_call_is_refused_with_the_stable_envelope() {
    let world = world().await;
    let response = post(
        state(&world.app),
        "/rpc/threads/send",
        None,
        serde_json::json!({"botId": world.bot_id, "text": "hello"}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let body = json(response).await;
    assert_eq!(body["code"], serde_json::json!("UNAUTHORIZED"));
    assert_eq!(body["error"], serde_json::json!("Sign in to continue."));
}

#[tokio::test]
async fn req_001_a_send_writes_one_message_and_starts_one_queued_run() {
    let world = world().await;
    let response = post(
        state(&world.app),
        "/rpc/threads/send",
        Some(&world.caller),
        serde_json::json!({"botId": world.bot_id, "text": "Summarise the last three commits."}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    assert!(body["messageId"].is_string());
    assert!(body["runId"].is_string());
    assert_eq!(body["status"], serde_json::json!("queued"));
}

#[tokio::test]
async fn req_002_the_same_nonce_returns_the_same_message_and_run() {
    let world = world().await;
    let request = serde_json::json!({
        "botId": world.bot_id,
        "text": "Summarise the last three commits.",
        "clientNonce": "nonce-golden-0001",
    });

    let first = post(
        state(&world.app),
        "/rpc/threads/send",
        Some(&world.caller),
        request.clone(),
    )
    .await;
    assert_eq!(first.status(), StatusCode::OK);
    let first_body = json(first).await;

    let second = post(
        state(&world.app),
        "/rpc/threads/send",
        Some(&world.caller),
        request,
    )
    .await;
    let second_body = json(second).await;

    assert_eq!(first_body, second_body, "the replay must be identical");
}

#[tokio::test]
async fn req_005_the_message_page_is_newest_first_and_carries_a_cursor() {
    let world = world().await;
    let state = state(&world.app);
    for text in ["one", "two", "three"] {
        let response = post(
            state.clone(),
            "/rpc/threads/send",
            Some(&world.caller),
            serde_json::json!({"botId": world.bot_id, "text": text}),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
    }

    let response = post(
        state,
        "/rpc/threads/messages",
        Some(&world.caller),
        serde_json::json!({"threadId": world.thread_id, "limit": 2}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    let messages = body["messages"]
        .as_array()
        .expect("the page carries messages");
    assert_eq!(messages.len(), 2);
    let first_seq = messages[0]["seq"].as_i64().expect("a sequence");
    let second_seq = messages[1]["seq"].as_i64().expect("a sequence");
    assert!(
        first_seq > second_seq,
        "the page must be newest first: {first_seq} then {second_seq}"
    );
    assert!(body["nextCursor"].is_string(), "the page carries a cursor");
}

#[tokio::test]
async fn req_043_a_bot_in_another_space_is_not_found() {
    let world = world().await;

    // The bot is in the caller's space: it is found.
    let response = post(
        state(&world.app),
        "/rpc/bots/get",
        Some(&world.caller),
        serde_json::json!({"botId": world.bot_id}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    assert_eq!(body["id"], serde_json::json!(world.bot_id));

    // The same bot, asked for from another space, is not found rather than forbidden: the
    // difference would confirm that the id exists (REQ-043).
    let other_user = world
        .app
        .seed_user("other@example.com", "correct horse battery")
        .await
        .expect("the other person exists");
    let _other_space = world
        .app
        .seed_space(&other_user, "Other space")
        .await
        .expect("the other space exists");
    let other_cookie = sign_in_as(&world.app, "other@example.com").await;
    let other_caller = Caller {
        cookie: other_cookie,
        space_id: world.caller.space_id.clone(),
    };

    let response = post(
        state(&world.app),
        "/rpc/bots/get",
        Some(&other_caller),
        serde_json::json!({"botId": world.bot_id}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body = json(response).await;
    assert_eq!(body["code"], serde_json::json!("NOT_FOUND"));
}

#[tokio::test]
async fn req_042_creating_a_space_makes_the_caller_its_owner() {
    let world = world().await;
    let response = post(
        state(&world.app),
        "/rpc/spaces/create",
        Some(&world.caller),
        serde_json::json!({"name": "Platform team"}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    assert_eq!(body["name"], serde_json::json!("Platform team"));
    assert_eq!(body["role"], serde_json::json!("owner"));
    assert_eq!(body["isDefault"], serde_json::json!(false));
    assert!(body["id"].is_string());
}

#[tokio::test]
async fn req_004_creating_a_bot_creates_its_thread() {
    let world = world().await;
    let response = post(
        state(&world.app),
        "/rpc/bots/create",
        Some(&world.caller),
        serde_json::json!({"name": "Analyst", "description": "Reads commits.", "color": "blue"}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    assert_eq!(body["name"], serde_json::json!("Analyst"));
    assert!(body["id"].is_string());
    assert!(body["threadId"].is_string());
    assert_eq!(body["archivedAt"], serde_json::Value::Null);
}

#[tokio::test]
async fn req_041_an_unknown_procedure_is_not_found() {
    let world = world().await;
    let response = post(
        state(&world.app),
        "/rpc/threads/teleport",
        Some(&world.caller),
        serde_json::json!({}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body = json(response).await;
    assert_eq!(body["code"], serde_json::json!("NOT_FOUND"));
}

#[tokio::test]
async fn req_041_an_unknown_field_in_a_body_is_refused() {
    let world = world().await;
    let response = post(
        state(&world.app),
        "/rpc/threads/send",
        Some(&world.caller),
        serde_json::json!({"botId": world.bot_id, "text": "hello", "admin": true}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = json(response).await;
    assert_eq!(body["code"], serde_json::json!("BAD_REQUEST"));
}

#[tokio::test]
async fn req_040_the_capabilities_route_reports_the_sign_in_methods() {
    let world = world().await;
    let request = Request::builder()
        .uri("/api/auth/capabilities")
        .body(Body::empty())
        .expect("the request builds");
    let response = router(state(&world.app))
        .oneshot(request)
        .await
        .expect("the router answers");
    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    assert_eq!(body["passwordAuth"], serde_json::json!(true));
    assert_eq!(body["emailCode"], serde_json::json!(false));
    assert_eq!(body["sso"], serde_json::Value::Null);
}

#[tokio::test]
async fn req_041_a_sign_in_sets_a_session_cookie_that_carries_no_token_in_the_body() {
    let test_app = TestApp::builder().build();
    test_app
        .seed_user("person@example.com", "correct horse battery")
        .await
        .expect("the person exists");

    let request = Request::builder()
        .method("POST")
        .uri("/api/auth/sign-in")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"email":"person@example.com","password":"correct horse battery"}"#,
        ))
        .expect("the request builds");
    let response = router(state(&test_app))
        .oneshot(request)
        .await
        .expect("the router answers");
    assert_eq!(response.status(), StatusCode::OK);

    let cookie = response
        .headers()
        .get(header::SET_COOKIE)
        .and_then(|value| value.to_str().ok())
        .expect("the sign-in sets a cookie");
    assert!(cookie.contains("session="), "{cookie}");
    assert!(cookie.contains("HttpOnly"), "{cookie}");
    assert!(cookie.contains("SameSite=Lax"), "{cookie}");

    let body = json(response).await;
    assert!(body.get("token").is_none(), "the token stays in the cookie");
    assert_eq!(
        body["user"]["email"],
        serde_json::json!("person@example.com")
    );
}

#[tokio::test]
async fn req_041_a_sign_in_with_the_wrong_password_is_refused_without_saying_why() {
    let test_app = TestApp::builder().build();
    test_app
        .seed_user("person@example.com", "correct horse battery")
        .await
        .expect("the person exists");

    let request = Request::builder()
        .method("POST")
        .uri("/api/auth/sign-in")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"email":"person@example.com","password":"wrong password entirely"}"#,
        ))
        .expect("the request builds");
    let response = router(state(&test_app))
        .oneshot(request)
        .await
        .expect("the router answers");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let body = json(response).await;
    assert_eq!(body["code"], serde_json::json!("UNAUTHORIZED"));
    assert!(
        !body["error"]
            .as_str()
            .unwrap_or_default()
            .contains("password"),
        "the answer must not say which half was wrong"
    );
}
