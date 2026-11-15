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

/// A raw POST with a plain text body (RFC 822 messages are not JSON).
async fn post_bytes_path(
    app_state: AppState,
    path: &str,
    body: &str,
    bearer: Option<&str>,
) -> Response {
    let mut builder = Request::builder()
        .method("POST")
        .uri(path)
        .header("content-type", "message/rfc822");
    if let Some(bearer) = bearer {
        builder = builder.header("authorization", bearer);
    }
    let request = builder
        .body(Body::from(body.to_owned()))
        .expect("the request builds");
    router(app_state)
        .oneshot(request)
        .await
        .expect("the router answers")
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
        builder = builder.header(header::COOKIE, &caller.cookie);
        if !caller.space_id.is_empty() {
            builder = builder.header("x-space-id", &caller.space_id);
        }
    }
    let request = builder
        .body(Body::from(body.to_string()))
        .expect("the request builds");
    router(state)
        .oneshot(request)
        .await
        .expect("the router answers")
}

/// A raw cookieless POST with an explicit Authorization header: the bot-webhook shape.
async fn raw_post(
    app_state: AppState,
    path: &str,
    bearer: Option<&str>,
    body: serde_json::Value,
) -> Response {
    let mut builder = Request::builder()
        .method("POST")
        .uri(path)
        .header("content-type", "application/json");
    if let Some(bearer) = bearer {
        builder = builder.header("authorization", format!("Bearer {bearer}"));
    }
    let request = builder
        .body(Body::from(body.to_string()))
        .expect("the request builds");
    router(app_state)
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

async fn world_with_board(enabled: bool) -> World {
    let test_app = TestApp::builder().board(enabled).build();
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

async fn world() -> World {
    world_with_board(false).await
}

async fn world_with_messaging() -> (
    World,
    std::sync::Arc<boma_testkit::ScriptedMessagingTransport>,
) {
    let transport = std::sync::Arc::new(
        boma_testkit::ScriptedMessagingTransport::new().with_get_responses(vec![
            serde_json::json!({
                "ok": true,
                "result": { "id": 424_242, "username": "boma_bot" },
            }),
        ]),
    );
    let test_app = TestApp::builder()
        .messaging(true)
        .slack("client-id-1", "client-secret-1", "signing-secret-1")
        .messaging_transport(std::sync::Arc::clone(&transport))
        .build();
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
    (
        World {
            app: test_app,
            caller: Caller {
                cookie,
                space_id: space_id.as_str().to_owned(),
            },
            bot_id: bot_id.as_str().to_owned(),
            thread_id: thread_id.as_str().to_owned(),
        },
        transport,
    )
}

async fn world_with_mail() -> World {
    let test_app = TestApp::builder()
        .email_inbound("mail.example.com", "relay-token-1234567890")
        .build();
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
async fn req_017_answering_a_card_resumes_the_run_and_is_refused_twice() {
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
    let caller = Caller {
        cookie,
        space_id: space_id.as_str().to_owned(),
    };

    let send = post(
        state(&test_app),
        "/rpc/threads/send",
        Some(&caller),
        serde_json::json!({"botId": bot_id, "text": "Do the purchase."}),
    )
    .await;
    let run_id = json(send).await["runId"]
        .as_str()
        .expect("a run id")
        .to_owned();

    let card = boma_domain::Card {
        id: boma_domain::CardId::new("card_golden").expect("an id is accepted"),
        space_id: space_id.clone(),
        thread_id: thread_id.clone(),
        run_id: boma_domain::RunId::new(run_id.clone()).ok(),
        kind: boma_domain::CardKind::Approval,
        status: boma_domain::CardStatus::Pending,
        answer: None,
        created_at: "2026-01-01T00:00:00Z".to_owned(),
        answered_at: None,
    };
    test_app.database().seed_card(space_id.clone(), card);

    let response = post(
        state(&test_app),
        "/rpc/threads/answer",
        Some(&caller),
        serde_json::json!({"cardId": "card_golden", "answer": "approve"}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    assert_eq!(body["ok"], serde_json::json!(true));
    assert_eq!(body["runId"], serde_json::json!(run_id));

    // A second answer to the same card is refused (REQ-017).
    let second = post(
        state(&test_app),
        "/rpc/threads/answer",
        Some(&caller),
        serde_json::json!({"cardId": "card_golden", "answer": "approve"}),
    )
    .await;
    assert_eq!(second.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn req_006_a_cancel_stops_the_run_and_is_idempotent() {
    let world = world().await;
    let send = post(
        state(&world.app),
        "/rpc/threads/send",
        Some(&world.caller),
        serde_json::json!({"botId": world.bot_id, "text": "Summarise the last three commits."}),
    )
    .await;
    let run_id = json(send).await["runId"]
        .as_str()
        .expect("a run id")
        .to_owned();

    for _ in 0..2 {
        let response = post(
            state(&world.app),
            "/rpc/runs/cancel",
            Some(&world.caller),
            serde_json::json!({"runId": run_id}),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = json(response).await;
        assert_eq!(body["id"], serde_json::json!(run_id));
        assert_eq!(body["status"], serde_json::json!("cancelled"));
    }

    // A run the caller cannot see is refused with the same stable envelope.
    let response = post(
        state(&world.app),
        "/rpc/runs/cancel",
        Some(&world.caller),
        serde_json::json!({"runId": "run_does_not_exist"}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body = json(response).await;
    assert_eq!(body["code"], serde_json::json!("NOT_FOUND"));
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
async fn d_01_sign_in_is_refused_after_the_rate_limit_for_one_ip() {
    let test_app = TestApp::builder().build();
    test_app
        .seed_user("person@example.com", "correct horse battery")
        .await
        .expect("the person exists");

    // Ten wrong attempts pass through the password check (and are refused with 401); the
    // eleventh must be refused before the password is ever checked, with 429 and the shared
    // envelope. The frozen test clock keeps every attempt in one bucket.
    for attempt in 1..=10 {
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
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "attempt {attempt} is still served"
        );
    }

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
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    let body = json(response).await;
    assert_eq!(body["code"], serde_json::json!("TOO_MANY_REQUESTS"));
    // The person is told why without saying which half was wrong.
    assert!(
        body["error"]
            .as_str()
            .is_some_and(|message| !message.contains("password"))
    );
}

#[tokio::test]
async fn d_01_sign_up_has_a_tighter_limit_than_sign_in() {
    let test_app = TestApp::builder().build();
    // Five sign-ups pass with the validation the request meets; the sixth must be refused
    // before any validation runs, with 429.
    for attempt in 1..=5 {
        let email = format!("person{attempt}@example.com");
        let body = serde_json::json!({
            "email": email,
            "password": "correct horse battery",
        });
        let request = Request::builder()
            .method("POST")
            .uri("/api/auth/sign-up")
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .expect("the request builds");
        let response = router(state(&test_app))
            .oneshot(request)
            .await
            .expect("the router answers");
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "attempt {attempt} is still served"
        );
    }

    let body = serde_json::json!({
        "email": "person6@example.com",
        "password": "correct horse battery",
    });
    let request = Request::builder()
        .method("POST")
        .uri("/api/auth/sign-up")
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .expect("the request builds");
    let response = router(state(&test_app))
        .oneshot(request)
        .await
        .expect("the router answers");
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
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

#[tokio::test]
async fn req_008_reacting_to_a_message_reads_back_the_truth() {
    let world = world().await;
    let send = post(
        state(&world.app),
        "/rpc/threads/send",
        Some(&world.caller),
        serde_json::json!({"botId": world.bot_id, "text": "a message to react to"}),
    )
    .await;
    let sent_body = json(send).await;
    let message_id = sent_body["messageId"]
        .as_str()
        .expect("a message id")
        .to_owned();

    // Adding a reaction returns the truth for the message.
    let response = post(
        state(&world.app),
        "/rpc/threads/react",
        Some(&world.caller),
        serde_json::json!({"messageId": message_id, "emoji": "\u{1F44D}", "action": "add"}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    assert_eq!(body["messageId"], serde_json::json!(message_id));
    assert_eq!(
        body["reactions"][0]["emoji"],
        serde_json::json!("\u{1F44D}")
    );
    assert_eq!(body["reactions"][0]["count"], serde_json::json!(1));
    assert_eq!(body["reactions"][0]["reacted"], serde_json::json!(true));

    // The page of history carries the reaction (REQ-008): it survives a reload.
    let response = post(
        state(&world.app),
        "/rpc/threads/messages",
        Some(&world.caller),
        serde_json::json!({"threadId": world.thread_id, "limit": 10}),
    )
    .await;
    let page = json(response).await;
    let found = page["messages"]
        .as_array()
        .expect("messages are listed")
        .iter()
        .find(|message| message["id"] == serde_json::json!(message_id))
        .expect("the message is on the page");
    assert_eq!(
        found["reactions"][0]["emoji"],
        serde_json::json!("\u{1F44D}")
    );

    // Replaying the same add changes nothing.
    let response = post(
        state(&world.app),
        "/rpc/threads/react",
        Some(&world.caller),
        serde_json::json!({"messageId": message_id, "emoji": "\u{1F44D}", "action": "add"}),
    )
    .await;
    let body = json(response).await;
    assert_eq!(body["reactions"][0]["count"], serde_json::json!(1));

    // Removing empties the list.
    let response = post(
        state(&world.app),
        "/rpc/threads/react",
        Some(&world.caller),
        serde_json::json!({"messageId": message_id, "emoji": "\u{1F44D}", "action": "remove"}),
    )
    .await;
    let body = json(response).await;
    assert_eq!(body["reactions"].as_array().map(Vec::len), Some(0),);

    // A message the caller cannot see is not found.
    let response = post(
        state(&world.app),
        "/rpc/threads/react",
        Some(&world.caller),
        serde_json::json!({"messageId": "msg_does_not_exist", "emoji": "\u{1F44D}", "action": "add"}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    // An action outside the schema is refused with the stable envelope.
    let response = post(
        state(&world.app),
        "/rpc/threads/react",
        Some(&world.caller),
        serde_json::json!({"messageId": message_id, "emoji": "\u{1F44D}", "action": "adore"}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn req_009_head_and_marks_move_the_read_position() {
    let world = world().await;
    let send = post(
        state(&world.app),
        "/rpc/threads/send",
        Some(&world.caller),
        serde_json::json!({"botId": world.bot_id, "text": "a message to read"}),
    )
    .await;
    assert_eq!(send.status(), StatusCode::OK);

    // The head of an unread thread: nothing read, everything above zero is unread.
    let head = post(
        state(&world.app),
        "/rpc/threads/head",
        Some(&world.caller),
        serde_json::json!({"threadId": world.thread_id}),
    )
    .await;
    assert_eq!(head.status(), StatusCode::OK);
    let body = json(head).await;
    assert_eq!(body["readSeq"], serde_json::json!(0));
    assert!(body["unreadCount"].as_u64().expect("a count") >= 1);

    // Marking read moves the position to the newest message.
    let marked = post(
        state(&world.app),
        "/rpc/threads/markRead",
        Some(&world.caller),
        serde_json::json!({"threadId": world.thread_id}),
    )
    .await;
    assert_eq!(marked.status(), StatusCode::OK);
    let body = json(marked).await;
    assert_eq!(body["readSeq"], body["lastSeq"]);
    assert_eq!(body["unreadCount"], serde_json::json!(0));

    // A reload keeps the position: zero separators next visit.
    let again = post(
        state(&world.app),
        "/rpc/threads/head",
        Some(&world.caller),
        serde_json::json!({"threadId": world.thread_id}),
    )
    .await;
    let body = json(again).await;
    assert_eq!(body["readSeq"], body["lastSeq"]);

    // Marking unread moves the position back to zero and the badge counts again.
    let cleared = post(
        state(&world.app),
        "/rpc/threads/markUnread",
        Some(&world.caller),
        serde_json::json!({"threadId": world.thread_id}),
    )
    .await;
    let body = json(cleared).await;
    assert_eq!(body["readSeq"], serde_json::json!(0));
    assert!(body["unreadCount"].as_u64().expect("a count") >= 1);

    // A thread the caller cannot see is not found, not zero.
    let missing = post(
        state(&world.app),
        "/rpc/threads/head",
        Some(&world.caller),
        serde_json::json!({"threadId": "thr_does_not_exist"}),
    )
    .await;
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
}

/// The caller's first bot in the roster (the world() fixture seeds exactly one).
async fn roster_bot(test_app: &TestApp, caller: &Caller) -> serde_json::Value {
    let list = post(
        state(test_app),
        "/rpc/bots/list",
        Some(caller),
        serde_json::json!({}),
    )
    .await;
    assert_eq!(list.status(), StatusCode::OK);
    let body = json(list).await;
    body["bots"][0].clone()
}

#[tokio::test]
async fn req_009_the_roster_snooze_and_unsnooze_round_trip() {
    let world = world().await;

    // The roster of a quiet bot: read position at zero, no snooze.
    let bot = roster_bot(&world.app, &world.caller).await;
    assert_eq!(bot["id"], serde_json::json!(world.bot_id));
    assert_eq!(bot["threadId"], serde_json::json!(world.thread_id));
    assert_eq!(bot["snoozedUntil"], serde_json::Value::Null);
    assert_eq!(bot["unreadCount"], serde_json::json!(0));

    // A message lands: the badge counts it.
    let send = post(
        state(&world.app),
        "/rpc/threads/send",
        Some(&world.caller),
        serde_json::json!({"botId": world.bot_id, "text": "a message to snooze with"}),
    )
    .await;
    assert_eq!(send.status(), StatusCode::OK);

    let bot = roster_bot(&world.app, &world.caller).await;
    assert_eq!(bot["unreadCount"], serde_json::json!(1));

    // Marking read clears the badge again.
    let marked = post(
        state(&world.app),
        "/rpc/threads/markRead",
        Some(&world.caller),
        serde_json::json!({"threadId": world.thread_id}),
    )
    .await;
    assert_eq!(marked.status(), StatusCode::OK);
    let bot = roster_bot(&world.app, &world.caller).await;
    assert_eq!(bot["unreadCount"], serde_json::json!(0));

    // Snoozing moves the bot off the regular list until the deadline.
    let snoozed = post(
        state(&world.app),
        "/rpc/threads/snooze",
        Some(&world.caller),
        serde_json::json!({"threadId": world.thread_id, "until": "2999-01-01T00:00:00Z"}),
    )
    .await;
    assert_eq!(snoozed.status(), StatusCode::OK);
    let body = json(snoozed).await;
    assert_eq!(
        body["snoozedUntil"],
        serde_json::json!("2999-01-01T00:00:00Z")
    );

    let bot = roster_bot(&world.app, &world.caller).await;
    assert_eq!(
        bot["snoozedUntil"],
        serde_json::json!("2999-01-01T00:00:00Z")
    );

    // An instant the roster cannot read is refused, not stored.
    let refused = post(
        state(&world.app),
        "/rpc/threads/snooze",
        Some(&world.caller),
        serde_json::json!({"threadId": world.thread_id, "until": "tomorrow"}),
    )
    .await;
    assert_eq!(refused.status(), StatusCode::BAD_REQUEST);

    // A snooze on a thread the caller cannot see is refused with the same envelope.
    let missing = post(
        state(&world.app),
        "/rpc/threads/snooze",
        Some(&world.caller),
        serde_json::json!({"threadId": "thr_does_not_exist", "until": "2999-01-01T00:00:00Z"}),
    )
    .await;
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);

    // Unsnoozing brings the chat back.
    let unsnoozed = post(
        state(&world.app),
        "/rpc/threads/unsnooze",
        Some(&world.caller),
        serde_json::json!({"threadId": world.thread_id}),
    )
    .await;
    assert_eq!(unsnoozed.status(), StatusCode::OK);
    let body = json(unsnoozed).await;
    assert_eq!(body["snoozedUntil"], serde_json::Value::Null);

    let bot = roster_bot(&world.app, &world.caller).await;
    assert_eq!(bot["snoozedUntil"], serde_json::Value::Null);
}

#[tokio::test]
async fn req_012_retry_only_from_failed() {
    let world = world().await;

    // A failed run belongs to the world's bot and is re-queued with one more attempt.
    let failed_id = boma_domain::RunId::new("run_rpc_failed").expect("an id is accepted");
    let now = "2026-01-01T00:00:00Z";
    world.app.database().seed_run(boma_domain::Run {
        id: failed_id.clone(),
        space_id: boma_domain::SpaceId::new(world.caller.space_id.clone())
            .expect("an id is accepted"),
        bot_id: boma_domain::BotId::new(world.bot_id.clone()).expect("an id is accepted"),
        thread_id: boma_domain::ThreadId::new(world.thread_id.clone()).expect("an id is accepted"),
        user_id: boma_domain::UserId::new("usr_golden_unused").expect("an id is accepted"),
        status: boma_domain::RunStatus::Failed,
        trigger: boma_domain::RunTrigger::User,
        client_nonce: None,
        source_message_id: None,
        created_at: now.to_owned(),
        started_at: Some(now.to_owned()),
        completed_at: Some(now.to_owned()),
        error: Some("the provider refused the request".to_owned()),
        lease_owner: None,
        lease_fence: 0,
        lease_expires_at: None,
        steering_from_seq: None,
        steering_consumed_seq: 0,
        attempt: 1,
        model_provider: None,
        model_id: None,
        model_pinned: false,
        parent_run_id: None,
        routine_id: None,
        not_before: None,
    });

    let response = post(
        state(&world.app),
        "/rpc/runs/retry",
        Some(&world.caller),
        serde_json::json!({"runId": "run_rpc_failed"}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    assert_eq!(body["runId"], serde_json::json!("run_rpc_failed"));
    assert_eq!(body["status"], serde_json::json!("queued"));
    assert_eq!(body["attempt"], serde_json::json!(2));

    // Retrying a run that is queued now is refused: failed is the only way in.
    let response = post(
        state(&world.app),
        "/rpc/runs/retry",
        Some(&world.caller),
        serde_json::json!({"runId": "run_rpc_failed"}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);

    // A run that never existed is not found.
    let response = post(
        state(&world.app),
        "/rpc/runs/retry",
        Some(&world.caller),
        serde_json::json!({"runId": "run_does_not_exist"}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

async fn memory_rpc(
    world: &World,
    procedure: &str,
    body: serde_json::Value,
) -> (StatusCode, serde_json::Value) {
    let response = post(
        state(&world.app),
        &format!("/rpc/memory/{procedure}"),
        Some(&world.caller),
        body,
    )
    .await;
    let status = response.status();
    let body = json(response).await;
    (status, body)
}

#[tokio::test]
async fn req_022_memory_write_grows_the_revision() {
    let world = world().await;

    // Writing to the bot's memory returns the revision as the golden pins it.
    let (status, body) = memory_rpc(
        &world,
        "write",
        serde_json::json!({"scope": "bot", "botId": world.bot_id, "path": "notes/commits.md", "content": "# commits\n"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["path"], serde_json::json!("notes/commits.md"));
    assert_eq!(body["revision"], serde_json::json!(1));

    // A second write grows the revision.
    let (status, body) = memory_rpc(
        &world,
        "write",
        serde_json::json!({"scope": "bot", "botId": world.bot_id, "path": "notes/commits.md", "content": "# commits v2\n"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["revision"], serde_json::json!(2));

    // Reading it back returns the newest revision.
    let (status, body) = memory_rpc(
        &world,
        "read",
        serde_json::json!({"scope": "bot", "botId": world.bot_id, "path": "notes/commits.md"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["content"], serde_json::json!("# commits v2\n"));
    assert_eq!(body["revision"], serde_json::json!(2));
}

#[tokio::test]
async fn req_022_memory_scope_rules_and_removal() {
    let world = world().await;

    // The bot exists, so the writes it owns are allowed; the read-back still proves the chain.
    let (status, _) = memory_rpc(
        &world,
        "write",
        serde_json::json!({"scope": "bot", "botId": world.bot_id, "path": "keep.md", "content": "kept"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // A path that escapes the memory tree is refused, not silently stored.
    let (status, _) = memory_rpc(
        &world,
        "write",
        serde_json::json!({"scope": "bot", "botId": world.bot_id, "path": "../escape.md", "content": "nope"}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // scope `bot` without a bot id is refused; scope `shared` with one is refused too.
    let (status, _) = memory_rpc(
        &world,
        "write",
        serde_json::json!({"scope": "bot", "path": "notes.md", "content": "x"}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = memory_rpc(
        &world,
        "write",
        serde_json::json!({"scope": "shared", "botId": world.bot_id, "path": "shared.md", "content": "x"}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // Shared memory: written once, read by anyone in the space.
    let (status, _) = memory_rpc(
        &world,
        "write",
        serde_json::json!({"scope": "shared", "path": "team.md", "content": "team notes"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, body) = memory_rpc(
        &world,
        "read",
        serde_json::json!({"scope": "shared", "path": "team.md"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["content"], serde_json::json!("team notes"));

    // A scope the wire cannot express is refused.
    let (status, _) = memory_rpc(
        &world,
        "read",
        serde_json::json!({"scope": "entries", "path": "team.md"}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // Listing returns both documents the world has.
    let (status, body) = memory_rpc(
        &world,
        "list",
        serde_json::json!({"scope": "bot", "botId": world.bot_id}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let paths: Vec<&str> = body["documents"]
        .as_array()
        .expect("documents are listed")
        .iter()
        .map(|entry| entry["path"].as_str().expect("a path is a string"))
        .collect();
    assert_eq!(paths, ["keep.md"]);

    // Removing returns what existed, and a second removal says it did not.
    let (status, body) = memory_rpc(
        &world,
        "remove",
        serde_json::json!({"scope": "bot", "botId": world.bot_id, "path": "keep.md"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["removed"], serde_json::json!(true));
    let (status, body) = memory_rpc(
        &world,
        "remove",
        serde_json::json!({"scope": "bot", "botId": world.bot_id, "path": "keep.md"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["removed"], serde_json::json!(false));
}

#[tokio::test]
async fn req_023_a_created_routine_wakes_on_its_next_instant_the_wire_agrees_on() {
    let world = world().await;

    let create = post(
        state(&world.app),
        "/rpc/routines/create",
        Some(&world.caller),
        serde_json::json!({
            "botId": world.bot_id,
            "name": "Morning digest",
            "prompt": "Summarise overnight commits.",
            "crons": ["0 8 * * 1-5"],
            "timezone": "Europe/Paris",
            "notify": true,
            "active": true
        }),
    )
    .await;
    assert_eq!(create.status(), StatusCode::OK);
    let body = json(create).await;
    assert!(body["id"].is_string());
    assert_eq!(body["active"], serde_json::json!(true));
    assert_eq!(body["timezone"], serde_json::json!("Europe/Paris"));
    assert_eq!(body["nextRunAt"], serde_json::json!("2026-01-01T07:00:00Z"));

    // A timezone the server does not know is refused with the stable envelope.
    let refused = post(
        state(&world.app),
        "/rpc/routines/create",
        Some(&world.caller),
        serde_json::json!({
            "botId": world.bot_id,
            "name": "X",
            "prompt": "Run",
            "crons": ["0 8 * * *"],
            "timezone": "Nowhere/Special"
        }),
    )
    .await;
    assert_eq!(refused.status(), StatusCode::BAD_REQUEST);

    // The @once sentinel fires once and never gets a second wake.
    let once = post(
        state(&world.app),
        "/rpc/routines/create",
        Some(&world.caller),
        serde_json::json!({
            "botId": world.bot_id,
            "name": "One shot",
            "prompt": "Do it once.",
            "crons": ["@once"],
            "timezone": "UTC"
        }),
    )
    .await;
    assert_eq!(once.status(), StatusCode::OK);
    let body = json(once).await;
    assert_eq!(body["nextRunAt"], serde_json::json!("2026-01-01T00:05:00Z"));
    assert_eq!(body["active"], serde_json::json!(true));

    // The routines list carries the created routine.
    let list = post(
        state(&world.app),
        "/rpc/routines/list",
        Some(&world.caller),
        serde_json::json!({ "botId": world.bot_id }),
    )
    .await;
    assert_eq!(list.status(), StatusCode::OK);
    let body = json(list).await;
    let routines = body["routines"].as_array().expect("a list");
    assert_eq!(routines.len(), 2);
    assert_eq!(routines[0]["name"], serde_json::json!("Morning digest"));
}

#[tokio::test]
async fn req_025_an_on_demand_trial_runs_the_routine_as_a_normal_turn() {
    let world = world().await;
    let create = post(
        state(&world.app),
        "/rpc/routines/create",
        Some(&world.caller),
        serde_json::json!({
            "botId": world.bot_id,
            "name": "Trial",
            "prompt": "Summarise the week.",
            "crons": ["@once"],
            "timezone": "UTC",
            "active": true,
            "notify": false
        }),
    )
    .await;
    assert_eq!(create.status(), StatusCode::OK);
    let created = json(create).await;
    let routine_id = created["id"].as_str().expect("a routine id").to_owned();

    // Firing it on demand starts a queued run — a normal turn, not a cron tick.
    let fired = post(
        state(&world.app),
        "/rpc/routines/runNow",
        Some(&world.caller),
        serde_json::json!({"routineId": routine_id}),
    )
    .await;
    assert_eq!(fired.status(), StatusCode::OK);
    let body = json(fired).await;
    assert_eq!(body["routineId"], serde_json::json!(routine_id));
    assert_eq!(body["status"], serde_json::json!("queued"));

    // The run exists, attributed to the routine; a member can see what fired and why.
    let run_id = boma_domain::RunId::new(body["runId"].as_str().expect("a run id").to_owned())
        .expect("an id is valid");
    let context = TestApp::member_context(
        &boma_domain::SpaceId::new(world.caller.space_id.clone()).expect("an id is valid"),
        &test_app_seed_user_id(&world.app, "person@example.com").await,
    );
    let run = world
        .app
        .app()
        .stores
        .runs
        .find(&context, &run_id)
        .await
        .expect("the run answers")
        .expect("the run exists");
    assert_eq!(run.trigger, boma_domain::RunTrigger::User);
    assert_eq!(
        run.routine_id.map(|id| id.as_str().to_owned()).as_deref(),
        Some(routine_id.as_str())
    );

    // The trial does not move the routine: it is still active, next wake unchanged.
    let after = post(
        state(&world.app),
        "/rpc/routines/list",
        Some(&world.caller),
        serde_json::json!({ "botId": world.bot_id }),
    )
    .await;
    let body = json(after).await;
    let routine = &body["routines"][0];
    assert_eq!(routine["id"], serde_json::json!(routine_id));
    assert_eq!(routine["active"], serde_json::json!(true));

    // A routine the caller cannot see is not found: a trial cannot leak to another space.
    let missing = post(
        state(&world.app),
        "/rpc/routines/runNow",
        Some(&world.caller),
        serde_json::json!({"routineId": "rtn_does_not_exist"}),
    )
    .await;
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
}

/// Reads the seeded user id back out of the in-memory store, as the member context needs it.
async fn test_app_seed_user_id(test_app: &TestApp, email: &str) -> boma_domain::UserId {
    test_app
        .app()
        .stores
        .users
        .find_by_email(&boma_domain::Email::new(email).expect("the address is valid"))
        .await
        .expect("the lookup answers")
        .expect("the person exists")
        .0
        .id
}

#[tokio::test]
async fn req_027_a_deployment_without_a_computer_backend_answers_computers_unavailable() {
    // A `none` deployment: computers cannot be reached, and the API says so plainly.
    let test_app = TestApp::builder().without_computers().build();
    let user_id = test_app
        .seed_user("person@example.com", "correct horse battery")
        .await
        .expect("the person exists");
    let space_id = test_app
        .seed_space(&user_id, "Golden space")
        .await
        .expect("the space exists");
    let (bot_id, _thread_id) = test_app
        .seed_bot(&space_id, &user_id, "Analyst")
        .await
        .expect("the bot exists");
    let cookie = sign_in_as(&test_app, "person@example.com").await;
    let caller = Caller {
        cookie,
        space_id: space_id.as_str().to_owned(),
    };
    let response = post(
        state(&test_app),
        "/rpc/computer/status",
        Some(&caller),
        serde_json::json!({ "botId": bot_id.as_str() }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body = json(response).await;
    assert_eq!(body["code"], serde_json::json!("COMPUTERS_UNAVAILABLE"));
}

#[tokio::test]
async fn req_027_a_bot_with_no_computer_answers_stopped_never_an_error() {
    // A `docker` deployment with no computer started yet: `stopped`, 200, nothing else.
    let world = world().await;
    let response = post(
        state(&world.app),
        "/rpc/computer/status",
        Some(&world.caller),
        serde_json::json!({ "botId": world.bot_id }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    assert_eq!(
        body,
        serde_json::json!({"status": "stopped", "provider": "docker"})
    );

    // Another space's bot is not found: the computer is not an oracle.
    let missing = post(
        state(&world.app),
        "/rpc/computer/status",
        Some(&world.caller),
        serde_json::json!({ "botId": "bot_does_not_exist" }),
    )
    .await;
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn req_026_a_bot_asks_for_a_computer_is_answered_and_the_answer_holds() {
    let test_app = TestApp::builder()
        .ids(vec![
            "usr_1".to_owned(),
            "org_1".to_owned(),
            "spc_1".to_owned(),
            "bot_1".to_owned(),
            "thr_1".to_owned(),
            "cmp_1".to_owned(),
        ])
        .computer_answers(vec![
            boma_testkit::ComputerAnswer::Started {
                external_id: "ctr_1".to_owned(),
                home_id: "vol_1".to_owned(),
            },
            boma_testkit::ComputerAnswer::Restarted {
                external_id: "ctr_1".to_owned(),
                home_id: "vol_1".to_owned(),
            },
        ])
        .fixed_clock("2026-01-01T00:00:00Z")
        .build();
    let user_id = test_app
        .seed_user("person@example.com", "correct horse battery")
        .await
        .expect("the person exists");
    let space_id = test_app
        .seed_space(&user_id, "Golden space")
        .await
        .expect("the space exists");
    let (bot_id, _thread_id) = test_app
        .seed_bot(&space_id, &user_id, "Analyst")
        .await
        .expect("the bot exists");
    let cookie = sign_in_as(&test_app, "person@example.com").await;
    let caller = Caller {
        cookie,
        space_id: space_id.as_str().to_owned(),
    };

    // The first ask boots the computer; exactly one generation moves.
    let response = post(
        state(&test_app),
        "/rpc/computer/start",
        Some(&caller),
        serde_json::json!({ "botId": bot_id.as_str() }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    assert_eq!(
        body,
        serde_json::json!({
            "status": "running",
            "provider": "docker",
            "startedAt": "2026-01-01T00:00:00Z",
            "screenGeneration": 1,
        })
    );

    // The second ask reuses the same computer; nothing boots, nothing moves.
    let response = post(
        state(&test_app),
        "/rpc/computer/start",
        Some(&caller),
        serde_json::json!({ "botId": bot_id.as_str() }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    assert_eq!(
        body,
        serde_json::json!({
            "status": "running",
            "provider": "docker",
            "startedAt": "2026-01-01T00:00:00Z",
            "screenGeneration": 1,
        })
    );

    // status and start agree on where the computer is.
    let response = post(
        state(&test_app),
        "/rpc/computer/status",
        Some(&caller),
        serde_json::json!({ "botId": bot_id.as_str() }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    assert_eq!(body["status"], serde_json::json!("running"));
    assert_eq!(body["screenGeneration"], serde_json::json!(1));

    // stop releases it; the home stays with the provider.
    let response = post(
        state(&test_app),
        "/rpc/computer/stop",
        Some(&caller),
        serde_json::json!({ "botId": bot_id.as_str() }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    assert_eq!(
        body,
        serde_json::json!({ "status": "stopped", "provider": "docker" })
    );

    // stopping again is a no-op that answers the same.
    let response = post(
        state(&test_app),
        "/rpc/computer/stop",
        Some(&caller),
        serde_json::json!({ "botId": bot_id.as_str() }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    assert_eq!(
        body,
        serde_json::json!({ "status": "stopped", "provider": "docker" })
    );
}

#[tokio::test]
async fn req_027_computer_provider_lists_what_this_build_can_run() {
    let world = world().await;
    let response = post(
        state(&world.app),
        "/rpc/computer/provider",
        Some(&world.caller),
        serde_json::json!({ "botId": world.bot_id }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    assert_eq!(body["active"], serde_json::json!("docker"));
    let providers = body["providers"].as_array().expect("providers are listed");
    let docker = providers
        .iter()
        .find(|provider| provider["name"] == serde_json::json!("docker"))
        .expect("docker is listed");
    assert_eq!(docker["available"], serde_json::json!(true));
    assert_eq!(docker["shell"], serde_json::json!(true));
    let none = providers
        .iter()
        .find(|provider| provider["name"] == serde_json::json!("none"))
        .expect("none is listed");
    assert_eq!(none["available"], serde_json::json!(true));
    assert_eq!(none["shell"], serde_json::json!(false));
}

#[tokio::test]
async fn req_026_computer_settings_read_and_update_within_deployment_limits() {
    let world = world().await;

    // A fresh world has no computer; settings read the defaults.
    let response = post(
        state(&world.app),
        "/rpc/computer/settings",
        Some(&world.caller),
        serde_json::json!({ "botId": world.bot_id }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    assert_eq!(body["memoryLimitMb"], serde_json::json!(512));
    assert_eq!(body["cpuLimit"], serde_json::json!(1.0));
    assert_eq!(body["idleMinutes"], serde_json::json!(10));
}

#[tokio::test]
async fn req_029_a_member_takes_control_and_releases_it() {
    let test_app = TestApp::builder()
        .ids(vec![
            "usr_1".to_owned(),
            "org_1".to_owned(),
            "spc_1".to_owned(),
            "bot_1".to_owned(),
            "thr_1".to_owned(),
            "cmp_1".to_owned(),
        ])
        .computer_answers(vec![boma_testkit::ComputerAnswer::Started {
            external_id: "ctr_1".to_owned(),
            home_id: "vol_1".to_owned(),
        }])
        .fixed_clock("2026-01-01T00:00:00Z")
        .build();
    let user_id = test_app
        .seed_user("person@example.com", "correct horse battery")
        .await
        .expect("the person exists");
    let space_id = test_app
        .seed_space(&user_id, "Golden space")
        .await
        .expect("the space exists");
    let (bot_id, _thread_id) = test_app
        .seed_bot(&space_id, &user_id, "Analyst")
        .await
        .expect("the bot exists");
    let cookie = sign_in_as(&test_app, "person@example.com").await;
    let caller = Caller {
        cookie,
        space_id: space_id.as_str().to_owned(),
    };
    let bot_id = bot_id.as_str().to_owned();

    // The computer is not started yet: takeover is refused.
    let response = post(
        state(&test_app),
        "/rpc/computer/takeover",
        Some(&caller),
        serde_json::json!({ "botId": bot_id }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let response = post(
        state(&test_app),
        "/rpc/computer/start",
        Some(&caller),
        serde_json::json!({ "botId": bot_id }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    let response = post(
        state(&test_app),
        "/rpc/computer/takeover",
        Some(&caller),
        serde_json::json!({ "botId": bot_id }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    assert_eq!(body["status"], serde_json::json!("running"));
    let holder = body["takeoverByUser"]
        .as_str()
        .expect("the holder is named");
    assert_ne!(holder, "");

    let response = post(
        state(&test_app),
        "/rpc/computer/release",
        Some(&caller),
        serde_json::json!({ "botId": bot_id }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    assert!(body.get("takeoverByUser").is_none());

    // Release is a one-way door: releasing again is a conflict.
    let response = post(
        state(&test_app),
        "/rpc/computer/release",
        Some(&caller),
        serde_json::json!({ "botId": bot_id }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn req_046_a_member_sets_lists_and_forgets_a_bot_secret_without_ever_seeing_the_value() {
    let world = world().await;
    let state = state(&world.app);

    // Set: the value goes in, the reply carries the name only.
    let response = post(
        state.clone(),
        "/rpc/botSecrets/set",
        Some(&world.caller),
        serde_json::json!({
            "botId": world.bot_id,
            "name": "OPENAI_KEY",
            "value": "s3cr3t-value",
        }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    assert_eq!(body["name"], serde_json::json!("OPENAI_KEY"));
    assert!(
        !body.to_string().contains("s3cr3t-value"),
        "the reply never echoes the value"
    );

    // List: names only.
    let response = post(
        state.clone(),
        "/rpc/botSecrets/list",
        Some(&world.caller),
        serde_json::json!({"botId": world.bot_id}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    assert_eq!(body["secrets"][0]["name"], serde_json::json!("OPENAI_KEY"));
    assert!(
        !body.to_string().contains("s3cr3t-value"),
        "the list never carries the value"
    );

    // Forget: idempotent success.
    let response = post(
        state.clone(),
        "/rpc/botSecrets/forget",
        Some(&world.caller),
        serde_json::json!({"botId": world.bot_id, "name": "OPENAI_KEY"}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    assert_eq!(body["removed"], serde_json::json!(true));

    let response = post(
        state,
        "/rpc/botSecrets/list",
        Some(&world.caller),
        serde_json::json!({"botId": world.bot_id}),
    )
    .await;
    let body = json(response).await;
    assert_eq!(body["secrets"].as_array().expect("a list").len(), 0);
}

#[tokio::test]
async fn req_046_a_secret_name_that_is_not_an_env_var_is_refused() {
    let world = world().await;
    let response = post(
        state(&world.app),
        "/rpc/botSecrets/set",
        Some(&world.caller),
        serde_json::json!({
            "botId": world.bot_id,
            "name": "not a name",
            "value": "s3cr3t-value",
        }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = json(response).await;
    assert_eq!(body["code"], serde_json::json!("BAD_REQUEST"));
    assert!(
        !body.to_string().contains("s3cr3t-value"),
        "the error never echoes the value"
    );
}

#[tokio::test]
async fn req_035_a_member_teaches_lists_and_reads_a_skill() {
    let world = world().await;
    let state = state(&world.app);

    // List before teaching: the built-in rides along.
    let response = post(
        state.clone(),
        "/rpc/skills/list",
        Some(&world.caller),
        serde_json::json!({}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    let names: Vec<&str> = body["skills"]
        .as_array()
        .expect("a list")
        .iter()
        .map(|skill| skill["name"].as_str().expect("a name"))
        .collect();
    assert!(names.contains(&"interrogate"), "the built-in: {names:?}");
    assert_eq!(names.len(), 1, "no space skills yet: {names:?}");

    // Teach: the reply names the skill.
    let response = post(
        state.clone(),
        "/rpc/skills/teach",
        Some(&world.caller),
        serde_json::json!({
            "name": "commits",
            "description": "How this space summarises commits.",
            "instructions": "Group by intent; quote the riskiest line.",
        }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    assert_eq!(body["name"], serde_json::json!("commits"));

    // List after: the taught skill is there, origin taught.
    let response = post(
        state.clone(),
        "/rpc/skills/list",
        Some(&world.caller),
        serde_json::json!({}),
    )
    .await;
    let body = json(response).await;
    let names: Vec<&str> = body["skills"]
        .as_array()
        .expect("a list")
        .iter()
        .map(|skill| skill["name"].as_str().expect("a name"))
        .collect();
    assert!(names.contains(&"commits"), "the taught skill: {names:?}");
    let taught = body["skills"]
        .as_array()
        .expect("a list")
        .iter()
        .find(|skill| skill["name"] == serde_json::json!("commits"))
        .expect("the taught skill");
    assert_eq!(taught["origin"], serde_json::json!("taught"));

    // Get: the instructions ride along.
    let response = post(
        state,
        "/rpc/skills/get",
        Some(&world.caller),
        serde_json::json!({ "name": "commits" }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    assert_eq!(
        body["instructions"],
        serde_json::json!("Group by intent; quote the riskiest line.")
    );
    assert_eq!(body["origin"], serde_json::json!("taught"));
}

#[tokio::test]
async fn req_035_teaching_over_a_builtin_name_is_refused() {
    let world = world().await;
    let response = post(
        state(&world.app),
        "/rpc/skills/teach",
        Some(&world.caller),
        serde_json::json!({
            "name": "interrogate",
            "description": "Shadowing a built-in must be refused.",
            "instructions": "Not allowed.",
        }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn req_035_getting_an_unknown_skill_is_a_not_found() {
    let world = world().await;
    let response = post(
        state(&world.app),
        "/rpc/skills/get",
        Some(&world.caller),
        serde_json::json!({ "name": "does-not-exist" }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn req_033_a_member_registers_lists_and_removes_an_mcp_server() {
    let world = world().await;
    let state = state(&world.app);

    let response = post(
        state.clone(),
        "/rpc/mcp/servers.add",
        Some(&world.caller),
        serde_json::json!({
            "name": "company-files",
            "transport": "http",
            "url": "https://mcp.example.com/mcp",
            "bearerToken": "token-1",
        }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    assert_eq!(body["server"]["name"], serde_json::json!("company-files"));
    assert_eq!(body["server"]["authState"], serde_json::json!("none"));
    assert!(
        !body.to_string().contains("token-1"),
        "the bearer never appears in a reply"
    );
    let server_id = body["server"]["id"].as_str().expect("an id").to_owned();

    let response = post(
        state.clone(),
        "/rpc/mcp/servers.list",
        Some(&world.caller),
        serde_json::json!({}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    assert_eq!(body["servers"].as_array().expect("a list").len(), 1);

    let response = post(
        state,
        "/rpc/mcp/servers.remove",
        Some(&world.caller),
        serde_json::json!({ "serverId": server_id }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    assert_eq!(body["removed"], serde_json::json!(true));
}

#[tokio::test]
async fn req_033_a_stdio_registration_is_refused_over_rpc_with_the_reason() {
    let world = world().await;
    let response = post(
        state(&world.app),
        "/rpc/mcp/servers.add",
        Some(&world.caller),
        serde_json::json!({
            "name": "local-things",
            "transport": "stdio",
            "command": "/usr/bin/python3 -m mcp_server",
        }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = json(response).await;
    assert_eq!(
        body["error"],
        serde_json::json!(
            "Local command servers are disabled in this deployment. An HTTPS server is the supported path."
        ),
        "{body}"
    );
}

#[tokio::test]
async fn req_032_a_member_installs_lists_and_removes_a_connector() {
    let world = world().await;
    let state = state(&world.app);

    // The catalogue lists both builtin connectors, none installed.
    let response = post(
        state.clone(),
        "/rpc/connections/list",
        Some(&world.caller),
        serde_json::json!({}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    let entries = body["connections"].as_array().expect("a list");
    assert_eq!(entries.len(), 2);
    assert!(
        entries
            .iter()
            .all(|entry| entry["installed"] == serde_json::json!(false))
    );

    // An api-key connector without its key refuses.
    let response = post(
        state.clone(),
        "/rpc/connections/install",
        Some(&world.caller),
        serde_json::json!({ "connectorId": "http-json" }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    // Install with the key; the reply never carries it.
    let response = post(
        state.clone(),
        "/rpc/connections/install",
        Some(&world.caller),
        serde_json::json!({ "connectorId": "http-json", "apiKey": "key-1" }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    assert_eq!(body["connector"]["installed"], serde_json::json!(true));
    assert!(!body.to_string().contains("key-1"));

    // The list shows it installed, not the key.
    let response = post(
        state.clone(),
        "/rpc/connections/list",
        Some(&world.caller),
        serde_json::json!({}),
    )
    .await;
    let body = json(response).await;
    let http_json = body["connections"]
        .as_array()
        .expect("a list")
        .iter()
        .find(|entry| entry["connectorId"] == serde_json::json!("http-json"))
        .expect("the connector is listed")
        .clone();
    assert_eq!(http_json["installed"], serde_json::json!(true));
    assert_eq!(http_json["authKind"], serde_json::json!("api_key"));

    // Remove; the reply confirms something was there; removing again answers false.
    let response = post(
        state.clone(),
        "/rpc/connections/remove",
        Some(&world.caller),
        serde_json::json!({ "connectorId": "http-json" }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    assert_eq!(body["removed"], serde_json::json!(true));
}

#[tokio::test]
async fn req_034_a_member_submits_and_installs_a_plugin() {
    let world = world().await;
    let app_state = state(&world.app);

    // Submit: the reply carries an id and nothing about the submitter.
    let response = post(
        app_state.clone(),
        "/rpc/marketplace/submit",
        Some(&world.caller),
        serde_json::json!({ "source": "https://example.test/acme/widget-pack.git#main" }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    assert_eq!(body["ok"], serde_json::json!(true));
    assert!(body["id"].as_str().expect("an id").len() > 3);

    // A malformed source is refused before anything happens.
    let response = post(
        app_state.clone(),
        "/rpc/marketplace/submit",
        Some(&world.caller),
        serde_json::json!({ "source": "not a repository" }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    // Install with an empty fixture fetcher errors instead of fabricating an install.
    let response = post(
        app_state,
        "/rpc/marketplace/install",
        Some(&world.caller),
        serde_json::json!({ "source": "https://example.test/acme/widget-pack.git#main" }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn req_034_the_marketplace_list_answers() {
    let world = world().await;
    let app_state = state(&world.app);
    let response = post(
        app_state,
        "/rpc/marketplace/list",
        Some(&world.caller),
        serde_json::json!({}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    assert_eq!(body["connectors"].as_array().expect("a list").len(), 2);
    assert_eq!(
        body["installedPlugins"].as_array().expect("a list").len(),
        0
    );
}

#[tokio::test]
async fn req_033_a_member_opens_and_abandons_an_oauth_sign_in() {
    let world = world().await;
    let app_state = state(&world.app);

    // A server without OAuth configured answers the start with a refusal, not a crash.
    let response = post(
        app_state.clone(),
        "/rpc/mcp/servers.add",
        Some(&world.caller),
        serde_json::json!({
            "name": "plain",
            "transport": "http",
            "url": "https://mcp.example.test/plain",
        }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    let server_id = json(response).await["server"]["id"]
        .as_str()
        .expect("an id")
        .to_owned();
    let response = post(
        app_state.clone(),
        "/rpc/mcp/oauth.start",
        Some(&world.caller),
        serde_json::json!({ "serverId": server_id }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn req_033_an_oauth_start_runs_end_to_end_and_a_replay_is_refused() {
    let world = world().await;
    let app_state = state(&world.app);

    // Register a server with OAuth configured.
    let response = post(
        app_state.clone(),
        "/rpc/mcp/servers.add",
        Some(&world.caller),
        serde_json::json!({
            "name": "warehouse",
            "transport": "http",
            "url": "https://mcp.example.test/warehouse",
            "oauth": {
                "authorizationUrl": "https://auth.example.test/authorize",
                "tokenUrl": "https://auth.example.test/token",
                "clientId": "boma-cli",
                "clientSecret": "shh",
            },
        }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let server_id = json(response).await["server"]["id"]
        .as_str()
        .expect("an id")
        .to_owned();

    // Start the sign-in.
    let response = post(
        app_state.clone(),
        "/rpc/mcp/oauth.start",
        Some(&world.caller),
        serde_json::json!({ "serverId": server_id }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    assert!(
        body["authorizationUrl"]
            .as_str()
            .expect("a url")
            .starts_with("https://auth.example.test/authorize?")
    );
    assert!(
        body["authorizationUrl"]
            .as_str()
            .expect("a url")
            .contains("code_challenge_method=S256")
    );
    let state_token = body["state"].as_str().expect("a state").to_owned();

    // The default scripted OAuth client has no answer scripted: the complete fails as an
    // unreachable authorization server, and the failure never leaks the secret into a reply.
    let response = post(
        app_state.clone(),
        "/rpc/mcp/oauth.complete",
        Some(&world.caller),
        serde_json::json!({ "serverId": server_id, "code": "code-1", "state": state_token }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    // The state is single-shot: a replayed complete with the same state is also refused before
    // reaching the server, and the reply never carries the client secret.
    let response = post(
        app_state.clone(),
        "/rpc/mcp/oauth.complete",
        Some(&world.caller),
        serde_json::json!({ "serverId": server_id, "code": "code-1", "state": state_token }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    // Servers list shows nothing about the secret.
    let response = post(
        app_state.clone(),
        "/rpc/mcp/servers.list",
        Some(&world.caller),
        serde_json::json!({}),
    )
    .await;
    let body = json(response).await;
    assert!(!body.to_string().contains("shh"));
}

#[tokio::test]
async fn req_036_a_member_creates_gets_lists_and_removes_an_artifact() {
    let world = world().await;
    let app_state = state(&world.app);

    // Create: a real PNG, declared honestly.
    let png = b"\x89PNG\r\n\x1a\nfake-png-body";
    let response = post(
        app_state.clone(),
        "/rpc/artifacts/create",
        Some(&world.caller),
        serde_json::json!({
            "name": "chart.png",
            "mimeType": "image/png",
            "contentBase64": base64::Engine::encode(&base64::engine::general_purpose::STANDARD, png),
        }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    let id = body["id"].as_str().expect("an id").to_owned();
    assert_eq!(body["version"], serde_json::json!(1));
    assert_eq!(body["rootArtifactId"], serde_json::json!(id));
    assert_eq!(body["size"], serde_json::json!(png.len() as u64));
    assert!(
        body["hash"]
            .as_str()
            .expect("a hash")
            .starts_with("sha256:")
    );
    assert!(
        !body.to_string().contains("PNG"),
        "the reply never carries the bytes"
    );

    // Get: metadata plus contentBase64.
    let response = post(
        app_state.clone(),
        "/rpc/artifacts/get",
        Some(&world.caller),
        serde_json::json!({ "artifactId": id }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    assert_eq!(body["artifact"]["name"], serde_json::json!("chart.png"));
    assert_eq!(body["artifact"]["mimeType"], serde_json::json!("image/png"));
    let decoded = base64::Engine::decode(
        &base64::engine::general_purpose::STANDARD,
        body["contentBase64"].as_str().expect("contentBase64"),
    )
    .expect("the bytes decode");
    assert_eq!(decoded, png);

    // listVersions: the golden's shape, newest first, version 1.
    let response = post(
        app_state.clone(),
        "/rpc/artifacts/listVersions",
        Some(&world.caller),
        serde_json::json!({ "artifactId": id }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    let versions = body["versions"].as_array().expect("a list");
    assert_eq!(versions.len(), 1);
    assert_eq!(versions[0]["id"], serde_json::json!(id));
    assert_eq!(versions[0]["version"], serde_json::json!(1));
    assert_eq!(versions[0]["rootArtifactId"], versions[0]["id"]);

    // listSpace: the artifact shows up as the latest version of its family.
    let response = post(
        app_state.clone(),
        "/rpc/artifacts/listSpace",
        Some(&world.caller),
        serde_json::json!({}),
    )
    .await;
    let body = json(response).await;
    let artifacts = body["artifacts"].as_array().expect("a list");
    assert!(
        artifacts
            .iter()
            .any(|entry| entry["id"] == serde_json::json!(id))
    );

    // Remove: the family is gone, and so is the content.
    let response = post(
        app_state.clone(),
        "/rpc/artifacts/remove",
        Some(&world.caller),
        serde_json::json!({ "artifactId": id }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    assert_eq!(body["removed"], serde_json::json!(1));
    let response = post(
        app_state,
        "/rpc/artifacts/get",
        Some(&world.caller),
        serde_json::json!({ "artifactId": id }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn d03_an_html_upload_declared_as_png_is_refused_over_rpc() {
    let world = world().await;
    let response = post(
        state(&world.app),
        "/rpc/artifacts/create",
        Some(&world.caller),
        serde_json::json!({
            "name": "evil.png",
            "mimeType": "image/png",
            "contentBase64": base64::Engine::encode(&base64::engine::general_purpose::STANDARD, b"<!DOCTYPE html><html>x</html>"),
        }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn req_037_a_member_saves_shares_views_and_revokes_a_page() {
    let world = world().await;
    let app_state = state(&world.app);

    // Save: the reply carries the page id, the version and the name.
    let response = post(
        app_state.clone(),
        "/rpc/pages/save",
        Some(&world.caller),
        serde_json::json!({
            "name": "Weekly report",
            "source": {
                "title": "Weekly report",
                "blocks": [
                    { "type": "text", "text": "All green." },
                    { "type": "chart", "label": "signups", "points": [{ "label": "mon", "value": 12.0 }] },
                ],
            },
        }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    let page_id = body["pageId"].as_str().expect("a page id").to_owned();
    assert_eq!(body["version"], serde_json::json!(1));
    assert_eq!(body["name"], serde_json::json!("Weekly report"));

    // Get: the member sees the source and the private visibility.
    let response = post(
        app_state.clone(),
        "/rpc/pages/get",
        Some(&world.caller),
        serde_json::json!({ "pageId": page_id }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    assert_eq!(body["visibility"], serde_json::json!("private"));
    assert_eq!(body["source"]["title"], serde_json::json!("Weekly report"));

    // Share: the token comes back exactly once, with the golden path.
    let response = post(
        app_state.clone(),
        "/rpc/pages/share",
        Some(&world.caller),
        serde_json::json!({ "pageId": page_id }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    let token = body["token"].as_str().expect("a token").to_owned();
    assert_eq!(
        body["path"],
        serde_json::json!(format!("/api/v1/pages/shared/{token}"))
    );

    // The public read: the golden body and headers, with no session at all.
    let unauth = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/pages/shared/{token}"))
        .header("accept", "application/json")
        .body(Body::empty())
        .expect("the request builds");
    let response = router(app_state.clone())
        .oneshot(unauth)
        .await
        .expect("the router answers");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["x-robots-tag"], "noindex, nofollow");
    assert_eq!(response.headers()["referrer-policy"], "no-referrer");
    assert_eq!(response.headers()["cache-control"], "no-store");
    let body = json(response).await;
    assert_eq!(body["title"], serde_json::json!("Weekly report"));
    assert_eq!(body["blocks"][0]["type"], serde_json::json!("text"));
    assert!(body["updatedAt"].as_str().is_some());

    // The public read of an unknown token: the same 404 shape.
    let unauth = Request::builder()
        .method("GET")
        .uri("/api/v1/pages/shared/definitely-not-a-token")
        .body(Body::empty())
        .expect("the request builds");
    let response = router(app_state.clone())
        .oneshot(unauth)
        .await
        .expect("the router answers");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body = json(response).await;
    assert_eq!(body, serde_json::json!({ "error": "Not found." }));

    // Revoke: the public read answers the same 404, and the member sees private again.
    let response = post(
        app_state.clone(),
        "/rpc/pages/revokeShare",
        Some(&world.caller),
        serde_json::json!({ "pageId": page_id }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(json(response).await["revoked"], serde_json::json!(true));

    let unauth = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/pages/shared/{token}"))
        .body(Body::empty())
        .expect("the request builds");
    let response = router(app_state)
        .oneshot(unauth)
        .await
        .expect("the router answers");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn req_037_an_invalid_source_is_refused_over_rpc() {
    let world = world().await;
    let app_state = state(&world.app);
    let response = post(
        app_state,
        "/rpc/pages/save",
        Some(&world.caller),
        serde_json::json!({
            "name": "Page",
            "source": {
                "title": "Page",
                "blocks": [{ "type": "link", "label": "x", "href": "javascript:alert(1)" }],
            },
        }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // the sandbox contract is eleven assertions in order
async fn req_039_the_shared_page_html_runs_only_inside_the_sandbox() {
    let world = world().await;
    let app_state = state(&world.app);

    // Seed a shared page with the exact source we will assert against.
    let context = TestApp::member_context(
        &boma_domain::SpaceId::new(world.caller.space_id.clone()).expect("the space id is valid"),
        &test_app_seed_user_id(&world.app, "person@example.com").await,
    );
    let seeded = boma_app::usecases::pages_save(
        world.app.app(),
        &context,
        boma_app::usecases::PageSaveInput {
            page_id: None,
            name: Some("Weekly report".to_owned()),
            bot_id: None,
            run_id: None,
            source: serde_json::json!({
                "title": "Weekly report",
                "blocks": [
                    { "type": "text", "text": "All green." },
                    { "type": "link", "label": "Docs", "href": "https://example.com/docs" },
                ],
            }),
            client_nonce: None,
        },
    )
    .await
    .expect("the page saves");
    let token = boma_app::usecases::pages_share(world.app.app(), &context, &seeded.id)
        .await
        .expect("the page shares");

    // The HTML rendering: strict CSP, inert island, pinned runtime, no inline script.
    let request = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/pages/shared/{token}"))
        .header("accept", "text/html")
        .body(Body::empty())
        .expect("the request builds");
    let response = router(app_state.clone())
        .oneshot(request)
        .await
        .expect("the router answers");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()["content-security-policy"],
        "default-src 'none'; sandbox allow-scripts; script-src 'self'; style-src 'unsafe-inline'; img-src data: https:; connect-src 'none'; form-action 'none'; base-uri 'none'"
    );
    assert_eq!(response.headers()["x-robots-tag"], "noindex, nofollow");
    assert_eq!(response.headers()["referrer-policy"], "no-referrer");
    assert_eq!(response.headers()["cache-control"], "no-store");
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("the body is in memory")
        .to_bytes();
    let body = String::from_utf8(bytes.to_vec()).expect("the page is utf-8");
    assert!(body.contains("<script id=\"page-data\" type=\"application/json\">"));
    assert!(body.contains("\"title\":\"Weekly report\""));
    assert!(body.contains("\"type\":\"link\""));
    assert!(body.contains("<script src=\"/api/v1/assets/page-runtime.js\" defer></script>"));
    assert_eq!(
        body.matches("</script>").count(),
        2,
        "exactly the island and the runtime close"
    );
    assert!(
        !body.contains("alert("),
        "nothing executable rides the document"
    );

    // A title with markup is escaped in the element.
    let injected = boma_app::usecases::pages_save(
        world.app.app(),
        &context,
        boma_app::usecases::PageSaveInput {
            page_id: None,
            name: Some("Evil".to_owned()),
            bot_id: None,
            run_id: None,
            source: serde_json::json!({
                "title": "x</title><script>alert(1)</script>",
                "blocks": [{ "type": "text", "text": "break: </script><script>alert(1)</script>" }],
            }),
            client_nonce: None,
        },
    )
    .await
    .expect("the poisoned page saves");
    let token2 = boma_app::usecases::pages_share(world.app.app(), &context, &injected.id)
        .await
        .expect("the poisoned page shares");
    let request = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/pages/shared/{token2}"))
        .header("accept", "text/html")
        .body(Body::empty())
        .expect("the request builds");
    let response = router(app_state.clone())
        .oneshot(request)
        .await
        .expect("the router answers");
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("the body is in memory")
        .to_bytes();
    let body = String::from_utf8(bytes.to_vec()).expect("the page is utf-8");
    assert!(
        !body.contains("</title><script>"),
        "the title cannot break out"
    );
    assert!(
        !body.contains("<script>alert(1)</script>"),
        "the island neutralises </script>"
    );
    assert_eq!(
        body.matches("</script>").count(),
        2,
        "exactly the island and the runtime close"
    );

    // The pinned runtime asset: served once, pinned ETag, immutable caching, 304 on match.
    let request = Request::builder()
        .method("GET")
        .uri("/api/v1/assets/page-runtime.js")
        .body(Body::empty())
        .expect("the request builds");
    let response = router(app_state.clone())
        .oneshot(request)
        .await
        .expect("the router answers");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()["content-type"],
        "text/javascript; charset=utf-8"
    );
    assert_eq!(
        response.headers()["cache-control"],
        "public, max-age=31536000, immutable"
    );
    let etag = response.headers()["etag"]
        .to_str()
        .expect("an etag")
        .to_owned();
    assert!(etag.starts_with('"'));
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("the body is in memory")
        .to_bytes();
    let runtime = String::from_utf8(bytes.to_vec()).expect("the asset is utf-8");
    assert!(runtime.contains("page-data"));
    assert!(
        !runtime.contains(".innerHTML"),
        "the renderer builds elements, never markup"
    );

    let request = Request::builder()
        .method("GET")
        .uri("/api/v1/assets/page-runtime.js")
        .header("if-none-match", &etag)
        .body(Body::empty())
        .expect("the request builds");
    let response = router(app_state)
        .oneshot(request)
        .await
        .expect("the router answers");
    assert_eq!(response.status(), StatusCode::NOT_MODIFIED);
    assert_eq!(
        response.headers()["cache-control"],
        "public, max-age=31536000, immutable"
    );
}

#[tokio::test]
async fn req_038_the_board_is_absent_when_the_deployment_does_not_enable_it() {
    let world = world().await;
    let app_state = state(&world.app);

    // Signed in: the board procedures answer 404, not 403, indistinguishable from absent.
    for path in [
        "/rpc/boards/get",
        "/rpc/tickets/create",
        "/rpc/tickets/move",
        "/rpc/tickets/comment",
    ] {
        let response = post(
            app_state.clone(),
            path,
            Some(&world.caller),
            serde_json::json!({"title": "x"}),
        )
        .await;
        assert_eq!(
            response.status(),
            StatusCode::NOT_FOUND,
            "{path} must not exist"
        );
    }
    // Signed out: the auth layer refuses before anything, as for every procedure.
    let response = post(app_state, "/rpc/boards/get", None, serde_json::json!({})).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn req_038_a_member_creates_moves_and_comments_on_the_board() {
    let world = world_with_board(true).await;
    let app_state = state(&world.app);

    // boards.get: empty to start.
    let response = post(
        app_state.clone(),
        "/rpc/boards/get",
        Some(&world.caller),
        serde_json::json!({}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(json(response).await["tickets"], serde_json::json!([]));

    // tickets.create allocates keys per space, monotonic.
    let response = post(
        app_state.clone(),
        "/rpc/tickets/create",
        Some(&world.caller),
        serde_json::json!({"title": "Write the release report"}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let first = json(response).await["ticket"].clone();
    assert_eq!(first["key"], serde_json::json!("BOT-1"));
    assert_eq!(first["state"], serde_json::json!("backlog"));
    let first_id = first["id"].as_str().expect("an id").to_owned();

    let response = post(
        app_state.clone(),
        "/rpc/tickets/create",
        Some(&world.caller),
        serde_json::json!({"title": "Cut the release", "assigneeBotId": world.bot_id}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let second = json(response).await["ticket"].clone();
    assert_eq!(second["key"], serde_json::json!("BOT-2"));
    assert_eq!(second["assigneeBotId"], serde_json::json!(world.bot_id));

    // An assignee outside the space is refused.
    let response = post(
        app_state.clone(),
        "/rpc/tickets/create",
        Some(&world.caller),
        serde_json::json!({"title": "x", "assigneeBotId": "bot_does_not_exist"}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    // Move: a bogus state is refused; an unknown ticket is not found.
    let response = post(
        app_state.clone(),
        "/rpc/tickets/move",
        Some(&world.caller),
        serde_json::json!({"ticketId": first_id, "state": "closed"}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let response = post(
        app_state.clone(),
        "/rpc/tickets/move",
        Some(&world.caller),
        serde_json::json!({"ticketId": "tkt_missing", "state": "done"}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let response = post(
        app_state.clone(),
        "/rpc/tickets/move",
        Some(&world.caller),
        serde_json::json!({"ticketId": first_id, "state": "in_progress"}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        json(response).await["ticket"]["state"],
        serde_json::json!("in_progress")
    );

    // Comments bump the counter atomically.
    for _ in 0..2 {
        let response = post(
            app_state.clone(),
            "/rpc/tickets/comment",
            Some(&world.caller),
            serde_json::json!({"ticketId": first_id, "body": "Working on it."}),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
    }

    // boards.get: both tickets, key order, comment count, assignment.
    let response = post(
        app_state,
        "/rpc/boards/get",
        Some(&world.caller),
        serde_json::json!({}),
    )
    .await;
    let body = json(response).await;
    let tickets = body["tickets"].as_array().expect("a list");
    assert_eq!(tickets.len(), 2);
    assert_eq!(tickets[0]["key"], serde_json::json!("BOT-1"));
    assert_eq!(tickets[0]["commentCount"], serde_json::json!(2));
    assert_eq!(tickets[0]["state"], serde_json::json!("in_progress"));
    assert_eq!(tickets[1]["key"], serde_json::json!("BOT-2"));
    assert_eq!(tickets[1]["assigneeBotId"], serde_json::json!(world.bot_id));
}

#[tokio::test]
async fn req_050_an_unknown_bot_and_a_wrong_secret_get_the_same_401() {
    let world = world().await;
    let app_state = state(&world.app);

    // Unknown bot: the golden's shape.
    let response = raw_post(
        app_state.clone(),
        "/api/v1/bots/bot_unknown/webhook",
        Some("invalid"),
        serde_json::json!({ "event": "push" }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let unknown = json(response).await;

    // A real bot with no WEBHOOK secret yet: the same refusal.
    let response = raw_post(
        app_state.clone(),
        &format!("/api/v1/bots/{}/webhook", world.bot_id),
        Some("hook-secret-1"),
        serde_json::json!({ "event": "push" }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let no_secret = json(response).await;

    // A real bot with a secret set, the wrong bearer: indistinguishable.
    let response = post(
        app_state.clone(),
        "/rpc/botSecrets/set",
        Some(&world.caller),
        serde_json::json!({ "botId": world.bot_id, "name": "WEBHOOK", "value": "hook-secret-1" }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    let response = raw_post(
        app_state,
        &format!("/api/v1/bots/{}/webhook", world.bot_id),
        Some("definitely-not-the-secret"),
        serde_json::json!({ "event": "push" }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let wrong_secret = json(response).await;

    assert_eq!(unknown, serde_json::json!({ "error": "Unauthorized." }));
    assert_eq!(unknown, no_secret);
    assert_eq!(unknown, wrong_secret);
}

#[tokio::test]
async fn req_050_a_valid_bearer_starts_a_run_and_wakes_github_routines() {
    let world = world().await;
    let app_state = state(&world.app);

    // Set the webhook secret.
    let response = post(
        app_state.clone(),
        "/rpc/botSecrets/set",
        Some(&world.caller),
        serde_json::json!({ "botId": world.bot_id, "name": "WEBHOOK", "value": "hook-secret-1" }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    // One routine opts into repository events, one does not.
    for (name, github_enabled) in [("repo watcher", true), ("quiet routine", false)] {
        let response = post(
            app_state.clone(),
            "/rpc/routines/create",
            Some(&world.caller),
            serde_json::json!({
                "botId": world.bot_id,
                "name": name,
                "prompt": "Watch the repository and summarise what changed.",
                "crons": ["@once"],
                "timezone": "UTC",
                "active": true,
                "githubEnabled": github_enabled,
            }),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
    }

    // A repository-shaped payload wakes the bot and the opted-in routine only.
    let response = raw_post(
        app_state.clone(),
        &format!("/api/v1/bots/{}/webhook", world.bot_id),
        Some("hook-secret-1"),
        serde_json::json!({
            "ref": "refs/heads/main",
            "repository": { "full_name": "acme/widgets", "name": "widgets" },
            "pusher": { "name": "dev" },
        }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    assert_eq!(body["ok"], serde_json::json!(true));
    assert_eq!(body["routineWakes"], serde_json::json!(1));
    let run_id = body["runId"].as_str().expect("a run id").to_owned();
    assert_ne!(run_id.len(), 0);

    // A non-repository payload wakes only the bot.
    let response = raw_post(
        app_state.clone(),
        &format!("/api/v1/bots/{}/webhook", world.bot_id),
        Some("hook-secret-1"),
        serde_json::json!({ "event": "ping" }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(json(response).await["routineWakes"], serde_json::json!(0));

    // The githubEnabled flag survived a round trip through the API.
    let response = post(
        app_state,
        "/rpc/routines/list",
        Some(&world.caller),
        serde_json::json!({ "botId": world.bot_id }),
    )
    .await;
    let routines = json(response).await["routines"]
        .as_array()
        .expect("a list")
        .clone();
    let flags: Vec<bool> = routines
        .iter()
        .map(|routine| routine["githubEnabled"].as_bool().expect("a bool"))
        .collect();
    assert_eq!(flags.iter().filter(|&&flag| flag).count(), 1);
}

#[tokio::test]
async fn req_047_the_relay_route_is_absent_when_mail_is_not_configured() {
    let world = world().await;
    let app_state = state(&world.app);
    let raw = format!(
        "From: x@example.com\r\nTo: boma+{}@mail.example.com\r\nSubject: hi\r\n\r\nhello\r\n",
        world.bot_id
    );
    let response = post_bytes_path(
        app_state,
        "/api/v1/email/inbound",
        &raw,
        Some("Bearer relay-token-1234567890"),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body = json(response).await;
    assert_eq!(body, serde_json::json!({ "error": "Not found." }));
}

#[tokio::test]
async fn req_047_a_configured_relay_routes_member_mail_to_a_run() {
    let world = world_with_mail().await;
    let app_state = state(&world.app);

    let raw = format!(
        "From: Person <person@example.com>\r\nTo: boma+{}@mail.example.com\r\nSubject: Ship it\r\nContent-Type: text/plain\r\n\r\nWrite the report.\r\n",
        world.bot_id
    );

    // The relay proves itself: absent or wrong token is 404, the same as a missing route.
    let response = post_bytes_path(app_state.clone(), "/api/v1/email/inbound", &raw, None).await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let response = post_bytes_path(
        app_state.clone(),
        "/api/v1/email/inbound",
        &raw,
        Some("Bearer wrong"),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    // A member's mail starts a run.
    let response = post_bytes_path(
        app_state.clone(),
        "/api/v1/email/inbound",
        &raw,
        Some("Bearer relay-token-1234567890"),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    assert_eq!(body["result"], serde_json::json!("started"));
    assert!(body["runId"].as_str().is_some());

    // A stranger is held, then listed, released through RPC, and the queue drains.
    let raw = format!(
        "From: Stranger <stranger@elsewhere.example>\r\nTo: boma+{}@mail.example.com\r\nSubject: Cold outreach\r\nContent-Type: text/plain\r\n\r\nBuy our thing.\r\n",
        world.bot_id
    );
    let response = post_bytes_path(
        app_state.clone(),
        "/api/v1/email/inbound",
        &raw,
        Some("Bearer relay-token-1234567890"),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(json(response).await["result"], serde_json::json!("held"));

    let response = post(
        app_state.clone(),
        "/rpc/mail/heldList",
        Some(&world.caller),
        serde_json::json!({ "botId": world.bot_id }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let held = json(response).await["held"]
        .as_array()
        .expect("a list")
        .clone();
    assert_eq!(held.len(), 1);
    let held_id = held[0]["id"].as_str().expect("an id").to_owned();

    let response = post(
        app_state.clone(),
        "/rpc/mail/heldRelease",
        Some(&world.caller),
        serde_json::json!({ "heldId": held_id }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert!(json(response).await["runId"].as_str().is_some());

    let response = post(
        app_state,
        "/rpc/mail/heldList",
        Some(&world.caller),
        serde_json::json!({ "botId": world.bot_id }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        json(response).await["held"]
            .as_array()
            .expect("a list")
            .len(),
        0
    );
}

#[tokio::test]
async fn req_047_the_mail_policy_rpc_round_trips() {
    let world = world_with_mail().await;
    let app_state = state(&world.app);

    let response = post(
        app_state.clone(),
        "/rpc/mail/policyGet",
        Some(&world.caller),
        serde_json::json!({ "botId": world.bot_id }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        json(response).await["policy"]["policy"],
        serde_json::json!("hold")
    );

    let response = post(
        app_state.clone(),
        "/rpc/mail/policySet",
        Some(&world.caller),
        serde_json::json!({
            "botId": world.bot_id,
            "policy": "drop",
            "allowedSenders": ["vendor@goods.example"],
        }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    assert_eq!(body["policy"]["policy"], serde_json::json!("drop"));
    assert_eq!(
        body["policy"]["allowedSenders"],
        serde_json::json!(["vendor@goods.example"])
    );

    // A malformed policy is refused.
    let response = post(
        app_state,
        "/rpc/mail/policySet",
        Some(&world.caller),
        serde_json::json!({ "botId": world.bot_id, "policy": "bogus", "allowedSenders": [] }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn req_048_the_messaging_surface_is_absent_without_the_flag() {
    let world = world().await;
    let app_state = state(&world.app);

    for procedure in [
        "installUrl",
        "installTelegram",
        "installGeneric",
        "list",
        "remove",
        "conversations",
    ] {
        let response = post(
            app_state.clone(),
            &format!("/rpc/messaging/{procedure}"),
            Some(&world.caller),
            serde_json::json!({}),
        )
        .await;
        assert_eq!(
            response.status(),
            StatusCode::NOT_FOUND,
            "messaging.{procedure}"
        );
    }

    let response = post_bytes_path(
        app_state,
        "/api/v1/messaging/slack/inbound/msg_1",
        "{}",
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // the round trip is the test
async fn req_048_messaging_rpc_install_and_inbound_round_trip() {
    let (world, _transport) = world_with_messaging().await;
    let app_state = state(&world.app);

    // The install handshake URL names the deployment and the bot's space.
    let response = post(
        app_state.clone(),
        "/rpc/messaging/installUrl",
        Some(&world.caller),
        serde_json::json!({ "botId": world.bot_id, "platform": "slack" }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let url = json(response).await["url"]
        .as_str()
        .expect("an authorize url")
        .to_owned();
    assert!(url.contains("client-id-1"));
    assert!(url.contains("state="));

    // Telegram installs with a token validated against getMe (scripted).
    let response = post(
        app_state.clone(),
        "/rpc/messaging/installTelegram",
        Some(&world.caller),
        serde_json::json!({
            "botId": world.bot_id,
            "botToken": "424242:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let installed = json(response).await;
    let installation_id = installed["installationId"]
        .as_str()
        .expect("an installation id")
        .to_owned();
    let webhook_secret = installed["webhookSecret"]
        .as_str()
        .expect("the webhook secret")
        .to_owned();
    assert_eq!(installed["registered"], serde_json::json!(true));
    assert_eq!(installed["botUsername"], serde_json::json!("boma_bot"));

    // A wrong webhook secret is the same refusal as an unknown installation.
    let unknown_body = br#"{"update_id":7,"message":{"message_id":42,"from":{"id":424242},"chat":{"id":-100123,"type":"supergroup"},"text":"build the report"}}"#;
    let response = post_bytes_path(
        app_state.clone(),
        &format!("/api/v1/messaging/telegram/inbound/{installation_id}"),
        std::str::from_utf8(unknown_body).expect("utf-8"),
        Some("Bearer unused"),
    )
    .await;
    // The telegram route reads the secret from a dedicated header; a wrong value is refused.
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let refused = json(response).await;
    assert_eq!(refused, serde_json::json!({ "error": "Unauthorized." }));

    // The right secret delivers.
    let request = Request::builder()
        .method("POST")
        .uri(format!(
            "/api/v1/messaging/telegram/inbound/{installation_id}"
        ))
        .header("content-type", "application/json")
        .header("x-telegram-bot-api-secret-token", webhook_secret)
        .body(Body::from(unknown_body.as_ref().to_owned()))
        .expect("the request builds");
    let response = router(app_state.clone())
        .oneshot(request)
        .await
        .expect("the router answers");
    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    assert_eq!(body["ok"], serde_json::json!(true));
    assert_eq!(body["conversationId"], serde_json::json!("-100123"));
    assert!(body["runId"].as_str().is_some());

    // The member sees the installation and the conversation.
    let response = post(
        app_state.clone(),
        "/rpc/messaging/list",
        Some(&world.caller),
        serde_json::json!({}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let installations = json(response).await["installations"]
        .as_array()
        .expect("a list")
        .clone();
    assert_eq!(installations.len(), 1);
    assert_eq!(installations[0]["platform"], serde_json::json!("telegram"));

    let response = post(
        app_state.clone(),
        "/rpc/messaging/conversations",
        Some(&world.caller),
        serde_json::json!({ "installationId": installation_id }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let conversations = json(response).await["conversations"]
        .as_array()
        .expect("a list")
        .clone();
    assert_eq!(conversations.len(), 1);
    assert_eq!(
        conversations[0]["conversationId"],
        serde_json::json!("-100123")
    );

    // Removing the installation stops deliveries; history stays.
    let response = post(
        app_state.clone(),
        "/rpc/messaging/remove",
        Some(&world.caller),
        serde_json::json!({ "installationId": installation_id }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(json(response).await["removed"], serde_json::json!(true));

    let response = post_bytes_path(
        app_state,
        &format!("/api/v1/messaging/telegram/inbound/{installation_id}"),
        std::str::from_utf8(unknown_body).expect("utf-8"),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn req_048_slack_not_configured_is_a_clear_refusal_not_a_crash() {
    let (world, _transport) = world_with_slack_missing().await;
    let app_state = state(&world.app);
    let response = post(
        app_state,
        "/rpc/messaging/installUrl",
        Some(&world.caller),
        serde_json::json!({ "botId": world.bot_id, "platform": "slack" }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

async fn world_with_slack_missing() -> (
    World,
    std::sync::Arc<boma_testkit::ScriptedMessagingTransport>,
) {
    let transport = std::sync::Arc::new(boma_testkit::ScriptedMessagingTransport::new());
    let test_app = TestApp::builder()
        .messaging(true)
        .messaging_transport(std::sync::Arc::clone(&transport))
        .build();
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
    (
        World {
            app: test_app,
            caller: Caller {
                cookie,
                space_id: space_id.as_str().to_owned(),
            },
            bot_id: bot_id.as_str().to_owned(),
            thread_id: thread_id.as_str().to_owned(),
        },
        transport,
    )
}

// ---------------------------------------------------------------------------
// M8 — models (REQ-052)
// ---------------------------------------------------------------------------

async fn world_with_models() -> (World, std::sync::Arc<boma_testkit::ScriptedModelTransport>) {
    let transport = std::sync::Arc::new(boma_testkit::ScriptedModelTransport::new());
    let test_app = TestApp::builder()
        .model_transport(std::sync::Arc::clone(&transport))
        .build();
    let user_id = test_app
        .seed_user("person@example.com", "correct horse battery")
        .await
        .expect("the person exists");
    let space_id = test_app
        .seed_space(&user_id, "Models space")
        .await
        .expect("the space exists");
    let (bot_id, thread_id) = test_app
        .seed_bot(&space_id, &user_id, "Analyst")
        .await
        .expect("the bot exists");
    let cookie = sign_in_as(&test_app, "person@example.com").await;
    (
        World {
            app: test_app,
            caller: Caller {
                cookie,
                space_id: space_id.as_str().to_owned(),
            },
            bot_id: bot_id.as_str().to_owned(),
            thread_id: thread_id.as_str().to_owned(),
        },
        transport,
    )
}

#[tokio::test]
async fn req_052_models_catalog_has_the_golden_shape_and_carries_no_key() {
    let (world, _transport) = world_with_models().await;
    let response = post(
        state(&world.app),
        "/rpc/models/catalog",
        Some(&world.caller),
        serde_json::json!({}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    // The golden's four fields and nothing else: a catalogue entry is id, name, context window,
    // roles. No keys, no account identifiers (spec/golden/models-catalog.json). The world
    // declares one model per provider, and the catalogue follows the declaration — not the
    // indicative price table in the domain.
    assert_eq!(
        body,
        serde_json::json!({
            "providers": [
                {
                    "id": "openai",
                    "name": "OpenAI",
                    "models": [
                        { "id": "gpt-5", "name": "GPT-5", "contextWindow": 400_000, "roles": ["chat"] },
                    ],
                },
                {
                    "id": "anthropic",
                    "name": "Anthropic",
                    "models": [
                        { "id": "claude-sonnet", "name": "Claude Sonnet", "contextWindow": 200_000, "roles": ["chat"] },
                    ],
                }
            ]
        })
    );
}

#[tokio::test]
async fn req_052_connect_disconnect_and_preferences_round_trip() {
    let (world, transport) = world_with_models().await;
    let app_state = state(&world.app);

    let connected = post(
        app_state.clone(),
        "/rpc/models/connect",
        Some(&world.caller),
        serde_json::json!({ "provider": "anthropic", "apiKey": "sk-ant-test-key" }),
    )
    .await;
    assert_eq!(connected.status(), StatusCode::OK);
    assert_eq!(
        json(connected).await,
        serde_json::json!({ "provider": "anthropic", "connected": true })
    );
    // The probe really called the provider, with the key, and never returns it.
    let calls = transport.calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].0, "https://api.anthropic.com/v1/models");
    assert!(calls[0].1.iter().any(|(name, _)| name == "x-api-key"));

    let listed = post(
        app_state.clone(),
        "/rpc/models/credentials",
        Some(&world.caller),
        serde_json::json!({}),
    )
    .await;
    assert_eq!(listed.status(), StatusCode::OK);
    let listed = json(listed).await;
    assert_eq!(listed["credentials"].as_array().expect("an array").len(), 1);
    assert_eq!(listed["credentials"][0]["provider"], "anthropic");
    assert_eq!(listed["credentials"][0]["status"], "connected");
    let text = listed.to_string();
    assert!(!text.contains("sk-ant-test-key"), "the key is never listed");

    let set = post(
        app_state.clone(),
        "/rpc/models/preferences.set",
        Some(&world.caller),
        serde_json::json!({ "modelKey": "openai/gpt-5" }),
    )
    .await;
    assert_eq!(set.status(), StatusCode::OK);
    assert_eq!(
        json(set).await,
        serde_json::json!({ "modelKey": "openai/gpt-5" })
    );

    let got = post(
        app_state.clone(),
        "/rpc/models/preferences.get",
        Some(&world.caller),
        serde_json::json!({}),
    )
    .await;
    assert_eq!(
        json(got).await,
        serde_json::json!({ "modelKey": "openai/gpt-5" })
    );

    let unreachable = post(
        app_state.clone(),
        "/rpc/models/preferences.set",
        Some(&world.caller),
        serde_json::json!({ "modelKey": "mystery/qwen-99" }),
    )
    .await;
    assert_eq!(unreachable.status(), StatusCode::BAD_REQUEST);

    let cleared = post(
        app_state.clone(),
        "/rpc/models/preferences.set",
        Some(&world.caller),
        serde_json::json!({ "modelKey": null }),
    )
    .await;
    assert_eq!(json(cleared).await, serde_json::json!({ "modelKey": null }));

    let disconnected = post(
        app_state,
        "/rpc/models/disconnect",
        Some(&world.caller),
        serde_json::json!({ "provider": "anthropic" }),
    )
    .await;
    assert_eq!(disconnected.status(), StatusCode::OK);
    assert_eq!(
        json(disconnected).await,
        serde_json::json!({ "provider": "anthropic", "connected": false })
    );
}

#[tokio::test]
async fn req_052_a_refused_key_answers_bad_request_with_the_providers_reason() {
    let transport =
        std::sync::Arc::new(boma_testkit::ScriptedModelTransport::with_responses(vec![
            Err(boma_app::ports::ModelTransportError::Http {
                status: 401,
                body: r#"{"error":"invalid x-api-key"}"#.to_owned(),
            }),
        ]));
    let test_app = TestApp::builder()
        .model_transport(std::sync::Arc::clone(&transport))
        .build();
    let user_id = test_app
        .seed_user("person@example.com", "correct horse battery")
        .await
        .expect("the person exists");
    let space_id = test_app
        .seed_space(&user_id, "Models space")
        .await
        .expect("the space exists");
    let cookie = sign_in_as(&test_app, "person@example.com").await;
    let caller = Caller {
        cookie,
        space_id: space_id.as_str().to_owned(),
    };
    let response = post(
        state(&test_app),
        "/rpc/models/connect",
        Some(&caller),
        serde_json::json!({ "provider": "anthropic", "apiKey": "bad-key" }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = json(response).await;
    let error = body["error"].as_str().expect("a message");
    assert!(error.contains("401"), "the status is named: {error}");
    assert!(
        error.contains("invalid x-api-key"),
        "the provider's own words are shown: {error}"
    );
}

// ---------------------------------------------------------------------------
// M8 — billing (REQ-018, REQ-051)
// ---------------------------------------------------------------------------

async fn world_with_billing() -> (World, std::sync::Arc<boma_testkit::ScriptedBillingProvider>) {
    let provider = std::sync::Arc::new(boma_testkit::ScriptedBillingProvider::new());
    let test_app = TestApp::builder()
        .billing(true)
        .billing_provider(std::sync::Arc::clone(&provider))
        .billing_webhook_secret("whsec_test_secret_1234567890")
        .build();
    let user_id = test_app
        .seed_user("person@example.com", "correct horse battery")
        .await
        .expect("the person exists");
    let space_id = test_app
        .seed_space(&user_id, "Billing space")
        .await
        .expect("the space exists");
    let (bot_id, thread_id) = test_app
        .seed_bot(&space_id, &user_id, "Analyst")
        .await
        .expect("the bot exists");
    let cookie = sign_in_as(&test_app, "person@example.com").await;
    (
        World {
            app: test_app,
            caller: Caller {
                cookie,
                space_id: space_id.as_str().to_owned(),
            },
            bot_id: bot_id.as_str().to_owned(),
            thread_id: thread_id.as_str().to_owned(),
        },
        provider,
    )
}

#[tokio::test]
async fn req_051_the_billing_surface_is_absent_without_the_flag() {
    let world = world().await;
    let app_state = state(&world.app);
    for procedure in ["status", "checkout", "limit.get", "limit.set"] {
        let response = post(
            app_state.clone(),
            &format!("/rpc/billing/{procedure}"),
            Some(&world.caller),
            serde_json::json!({}),
        )
        .await;
        assert_eq!(
            response.status(),
            StatusCode::NOT_FOUND,
            "billing.{procedure}"
        );
    }
    // And the webhook route does not answer a caller at all.
    let response = raw_post(
        app_state,
        "/api/v1/billing/webhook",
        None,
        serde_json::json!({}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        json(response).await,
        serde_json::json!({ "error": "Not found." })
    );
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // the round trip is the test
async fn req_018_and_051_the_plan_the_checkout_and_the_limit_round_trip() {
    let (world, provider) = world_with_billing().await;
    let app_state = state(&world.app);

    let status = post(
        app_state.clone(),
        "/rpc/billing/status",
        Some(&world.caller),
        serde_json::json!({}),
    )
    .await;
    assert_eq!(status.status(), StatusCode::OK);
    let status = json(status).await;
    assert_eq!(status["plan"], "free");
    assert_eq!(status["status"], "active");
    assert_eq!(status["creditsMinorUnits"], 0);
    assert_eq!(status["checkoutAllowed"], true);

    let checkout = post(
        app_state.clone(),
        "/rpc/billing/checkout",
        Some(&world.caller),
        serde_json::json!({ "plan": "pro" }),
    )
    .await;
    assert_eq!(checkout.status(), StatusCode::OK);
    assert_eq!(
        json(checkout).await,
        serde_json::json!({
            "url": "https://checkout.example.com/scripted",
            "providerRef": "cs_scripted",
        })
    );
    assert_eq!(provider.calls().len(), 1);

    let set = post(
        app_state.clone(),
        "/rpc/billing/limit.set",
        Some(&world.caller),
        serde_json::json!({
            "enabled": true,
            "currency": "usd",
            "capMinorUnits": 5000,
        }),
    )
    .await;
    assert_eq!(set.status(), StatusCode::OK);
    assert_eq!(
        json(set).await,
        serde_json::json!({
            "enabled": true,
            "currency": "USD",
            "capMinorUnits": 5000,
            "spentMinorUnits": 0,
            "remainingMinorUnits": 5000,
        })
    );

    let get = post(
        app_state,
        "/rpc/billing/limit.get",
        Some(&world.caller),
        serde_json::json!({}),
    )
    .await;
    assert_eq!(get.status(), StatusCode::OK);
    assert_eq!(json(get).await["remainingMinorUnits"], 5000);
}

/// Signs a billing webhook body the way Stripe does.
fn signed_webhook(secret: &str, body: &[u8], timestamp: &str) -> String {
    use hmac::{Hmac, KeyInit, Mac};
    let mut mac = Hmac::<sha2::Sha256>::new_from_slice(secret.as_bytes()).expect("any length");
    mac.update(timestamp.as_bytes());
    mac.update(b".");
    mac.update(body);
    let digest = mac.finalize().into_bytes();
    let digest = digest.iter().fold(String::new(), |mut acc, byte| {
        use std::fmt::Write as _;
        let _ = write!(acc, "{byte:02x}");
        acc
    });
    format!("t={timestamp},v1={digest}")
}

#[tokio::test]
async fn req_051_the_webhook_is_uniformly_denied_until_its_signature_verifies() {
    let (world, _provider) = world_with_billing().await;
    let space_id = world.caller.space_id.clone();
    let timestamp =
        boma_app::usecases::unix_seconds(boma_testkit::scripts::DEFAULT_INSTANT).to_string();
    let body = serde_json::json!({
        "type": "checkout.session.completed",
        "data": { "object": {
            "id": "cs_live_1",
            "client_reference_id": space_id,
            "metadata": { "space_id": space_id },
        }},
    })
    .to_string();

    let deny = |signature: &str| {
        let app_state = state(&world.app);
        let signature = signature.to_owned();
        let body = body.clone();
        async move {
            let request = Request::builder()
                .method("POST")
                .uri("/api/v1/billing/webhook")
                .header("content-type", "application/json")
                .header("stripe-signature", signature)
                .body(Body::from(body))
                .expect("the request builds");
            router(app_state)
                .oneshot(request)
                .await
                .expect("the router answers")
        }
    };

    let response = deny("t=1,v1=deadbeef").await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        json(response).await,
        serde_json::json!({ "error": "Not found." })
    );

    let response = deny("garbage").await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let signature = signed_webhook("whsec_test_secret_1234567890", body.as_bytes(), &timestamp);
    let response = deny(&signature).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    assert_eq!(body["received"], true);
    assert_eq!(body["activated"], true);
    assert_eq!(body["plan"], "pro");

    let status = post(
        state(&world.app),
        "/rpc/billing/status",
        Some(&world.caller),
        serde_json::json!({}),
    )
    .await;
    let status = json(status).await;
    assert_eq!(status["plan"], "pro");
    assert_eq!(
        status["creditsMinorUnits"],
        boma_app::usecases::PRO_SEEDED_CREDITS_MINOR
    );
}

// ---------------------------------------------------------------------------
// M9 — observability (REQ-045)
// ---------------------------------------------------------------------------

#[tokio::test]
#[allow(clippy::too_many_lines)] // the export is the test
async fn req_045_audit_export_redacts_secrets_and_refuses_a_plain_member() {
    let test_app = TestApp::builder().build();
    let owner_id = test_app
        .seed_user("owner@example.com", "correct horse battery")
        .await
        .expect("the owner exists");
    let space_id = test_app
        .seed_space(&owner_id, "Audit space")
        .await
        .expect("the space exists");
    let (bot_id, _thread_id) = test_app
        .seed_bot(&space_id, &owner_id, "Analyst")
        .await
        .expect("the bot exists");
    let _ = bot_id;
    let owner_cookie = sign_in_as(&test_app, "owner@example.com").await;
    let plain_id = test_app
        .seed_user("plain@example.com", "correct horse battery")
        .await
        .expect("the person exists");
    test_app.seed_member(&space_id, &plain_id, boma_domain::SpaceRole::Member);
    let plain_cookie = sign_in_as(&test_app, "plain@example.com").await;
    test_app
        .app()
        .stores
        .audit
        .append(boma_app::ports::NewAuditEvent {
            id: "aud_rpc_export_1".to_owned(),
            space_id: Some(space_id.clone()),
            actor_user_id: Some(owner_id.clone()),
            kind: boma_domain::AuditKind::McpPrivateCall,
            target: "company-files".to_owned(),
            metadata: serde_json::json!({
                "bearer": "sk-live-0123456789abcdef",
                "tool": "list-issues",
            }),
            created_at: "2026-01-01T00:00:00Z".to_owned(),
        })
        .await
        .expect("the event is written");
    let plain_caller = Caller {
        cookie: plain_cookie,
        space_id: space_id.as_str().to_owned(),
    };
    let plain = post(
        state(&test_app),
        "/rpc/audit/export",
        Some(&plain_caller),
        serde_json::json!({}),
    )
    .await;
    assert_eq!(plain.status(), StatusCode::FORBIDDEN);

    let owner_caller = Caller {
        cookie: owner_cookie,
        space_id: space_id.as_str().to_owned(),
    };
    let response = post(
        state(&test_app),
        "/rpc/audit/export",
        Some(&owner_caller),
        serde_json::json!({}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    assert_eq!(body["version"], "1");
    assert_eq!(body["events"], 1);
    let text = body["jsonl"].as_str().expect("a JSONL body");
    assert!(text.contains("\"bearer\":\"[redacted]\""), "{text}");
    assert!(text.contains("list-issues"), "{text}");
    let mut lines = text.lines();
    let header: serde_json::Value =
        serde_json::from_str(lines.next().expect("a header line")).expect("header parses");
    assert_eq!(header["version"], "1");
    let row: serde_json::Value =
        serde_json::from_str(lines.next().expect("an event line")).expect("row parses");
    assert_eq!(row["id"], "aud_rpc_export_1");
}

#[tokio::test]
async fn req_045_security_events_list_refuses_a_plain_member_and_returns_security_kinds() {
    let test_app = TestApp::builder().build();
    let owner_id = test_app
        .seed_user("owner@example.com", "correct horse battery")
        .await
        .expect("the owner exists");
    let space_id = test_app
        .seed_space(&owner_id, "Security space")
        .await
        .expect("the space exists");
    let _ = test_app
        .seed_bot(&space_id, &owner_id, "Analyst")
        .await
        .expect("the bot exists");
    let plain_id = test_app
        .seed_user("plain@example.com", "correct horse battery")
        .await
        .expect("the person exists");
    test_app.seed_member(&space_id, &plain_id, boma_domain::SpaceRole::Member);
    let plain_cookie = sign_in_as(&test_app, "plain@example.com").await;
    let owner_cookie = sign_in_as(&test_app, "owner@example.com").await;
    let plain_caller = Caller {
        cookie: plain_cookie,
        space_id: space_id.as_str().to_owned(),
    };
    let owner_caller = Caller {
        cookie: owner_cookie,
        space_id: space_id.as_str().to_owned(),
    };
    test_app
        .app()
        .stores
        .audit
        .append(boma_app::ports::NewAuditEvent {
            id: "aud_rpc_seclist_1".to_owned(),
            space_id: Some(space_id.clone()),
            actor_user_id: None,
            kind: boma_domain::AuditKind::SignInFailed,
            target: "password".to_owned(),
            metadata: serde_json::json!({}),
            created_at: "2026-01-01T00:00:00Z".to_owned(),
        })
        .await
        .expect("the event is written");

    let plain = post(
        state(&test_app),
        "/rpc/security/events.list",
        Some(&plain_caller),
        serde_json::json!({}),
    )
    .await;
    assert_eq!(plain.status(), StatusCode::FORBIDDEN);

    let response = post(
        state(&test_app),
        "/rpc/security/events.list",
        Some(&owner_caller),
        serde_json::json!({}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    let events = body["events"].as_array().expect("an array");
    assert!(!events.is_empty(), "the events are listed");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["kind"], "security.sign_in_failed");
}

// ---------------------------------------------------------------------------
// M9 — admin surfaces (REQ-058)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn req_058_an_owner_creates_a_deployment_grant_a_plain_member_is_refused() {
    let test_app = TestApp::builder().build();
    let owner_id = test_app
        .seed_user("owner@example.com", "correct horse battery")
        .await
        .expect("the owner exists");
    let space_id = test_app
        .seed_space(&owner_id, "Admin space")
        .await
        .expect("the space exists");
    let _ = test_app
        .seed_bot(&space_id, &owner_id, "Analyst")
        .await
        .expect("the bot exists");
    let plain_id = test_app
        .seed_user("plain@example.com", "correct horse battery")
        .await
        .expect("the person exists");
    test_app.seed_member(&space_id, &plain_id, boma_domain::SpaceRole::Member);
    let plain_cookie = sign_in_as(&test_app, "plain@example.com").await;
    let owner_cookie = sign_in_as(&test_app, "owner@example.com").await;
    let plain_caller = Caller {
        cookie: plain_cookie,
        space_id: space_id.as_str().to_owned(),
    };
    let owner_caller = Caller {
        cookie: owner_cookie,
        space_id: space_id.as_str().to_owned(),
    };

    let plain = post(
        state(&test_app),
        "/rpc/admin/grants.create",
        Some(&plain_caller),
        serde_json::json!({
            "email": "beta@example.com",
            "reason": "beta tester",
        }),
    )
    .await;
    assert_eq!(plain.status(), StatusCode::FORBIDDEN);

    let response = post(
        state(&test_app),
        "/rpc/admin/grants.create",
        Some(&owner_caller),
        serde_json::json!({
            "email": "beta@example.com",
            "reason": "beta tester",
        }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    assert_eq!(body["grant"]["email"], "beta@example.com");
    assert_eq!(body["grant"]["reason"], "beta tester");

    let listed = post(
        state(&test_app),
        "/rpc/admin/grants.list",
        Some(&owner_caller),
        serde_json::json!({}),
    )
    .await;
    assert_eq!(listed.status(), StatusCode::OK);
    let body = json(listed).await;
    let grants = body["grants"].as_array().expect("an array");
    assert_eq!(grants.len(), 1);
    assert_eq!(grants[0]["email"], "beta@example.com");

    let events = test_app.database().audit().events();
    assert!(
        events
            .iter()
            .any(|event| event.kind == "admin.grant_created")
    );
}

#[tokio::test]
async fn req_058_the_plugin_review_approve_then_a_second_decision_conflicts() {
    let test_app = TestApp::builder().build();
    let owner_id = test_app
        .seed_user("owner@example.com", "correct horse battery")
        .await
        .expect("the owner exists");
    let space_id = test_app
        .seed_space(&owner_id, "Admin space")
        .await
        .expect("the space exists");
    let _ = test_app
        .seed_bot(&space_id, &owner_id, "Analyst")
        .await
        .expect("the bot exists");
    let _ = boma_app::usecases::submit_plugin(
        test_app.app(),
        &boma_domain::PluginSourceRef {
            git_url: "github.com/acme/connector".to_owned(),
            ref_: None,
            subdir: None,
        },
        Some("Acme connector".to_owned()),
        None,
        None,
    )
    .await
    .expect("the submission is filed");
    let owner_cookie = sign_in_as(&test_app, "owner@example.com").await;
    let owner_caller = Caller {
        cookie: owner_cookie,
        space_id: space_id.as_str().to_owned(),
    };

    let queued = post(
        state(&test_app),
        "/rpc/admin/plugins.list",
        Some(&owner_caller),
        serde_json::json!({ "status": "pending" }),
    )
    .await;
    assert_eq!(queued.status(), StatusCode::OK);
    let body = json(queued).await;
    let submissions = body["submissions"].as_array().expect("an array").clone();
    assert_eq!(submissions.len(), 1);
    let id = submissions[0]["id"].as_str().expect("an id").to_owned();

    let decided = post(
        state(&test_app),
        "/rpc/admin/plugins.decide",
        Some(&owner_caller),
        serde_json::json!({ "submissionId": id, "decision": "approved" }),
    )
    .await;
    assert_eq!(decided.status(), StatusCode::OK);
    let body = json(decided).await;
    assert_eq!(body["status"], "approved");

    let second = post(
        state(&test_app),
        "/rpc/admin/plugins.decide",
        Some(&owner_caller),
        serde_json::json!({ "submissionId": id, "decision": "rejected" }),
    )
    .await;
    assert_eq!(second.status(), StatusCode::CONFLICT);
}

// ---------------------------------------------------------------------------
// M9 — export download round trip (REQ-044)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn req_044_the_download_route_serves_a_zip_to_the_session_that_owns_it() {
    let world = world().await;
    let response = post(
        state(&world.app),
        "/rpc/export/start",
        Some(&world.caller),
        serde_json::json!({}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    let export_id = body["exportId"].as_str().expect("an export id").to_owned();

    let request = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/me/exports/{export_id}"))
        .header(header::COOKIE, &world.caller.cookie)
        .header("x-space-id", &world.caller.space_id)
        .body(Body::empty())
        .expect("the request builds");
    let response = router(state(&world.app))
        .oneshot(request)
        .await
        .expect("the router answers");
    assert_eq!(response.status(), StatusCode::OK);
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    assert_eq!(content_type, "application/zip");
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("the body is in memory")
        .to_bytes();
    assert_eq!(&bytes[..4], b"PK\x03\x04", "the body is a zip archive");
    let text = String::from_utf8_lossy(&bytes);
    assert!(
        text.contains("manifest.json"),
        "the manifest is in the archive"
    );

    // A stranger without the session is refused.
    let request = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/me/exports/{export_id}"))
        .body(Body::empty())
        .expect("the request builds");
    let response = router(state(&world.app))
        .oneshot(request)
        .await
        .expect("the router answers");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

// ---------------------------------------------------------------------------
// M10 — password recovery over the HTTP surface (REQ-040)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn req_040_the_recovery_link_round_trips_over_http_and_returns_a_session() {
    let test_app = TestApp::builder()
        .tokens(vec![
            // The scripted sequence: one token sign-in will not mint, because forgot-password
            // must not depend on order. The recovery flow mints exactly this token.
            "recovery-rpc-token".to_owned(),
        ])
        .build();
    let _user_id = test_app
        .seed_user("person@example.com", "correct horse battery")
        .await
        .expect("the person exists");

    let request = Request::builder()
        .method("POST")
        .uri("/api/auth/forgot-password")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"email":"person@example.com"}"#))
        .expect("the request builds");
    let response = router(state(&test_app))
        .oneshot(request)
        .await
        .expect("the router answers");
    assert_eq!(response.status(), StatusCode::OK);

    let request = Request::builder()
        .method("POST")
        .uri("/api/auth/forgot-password")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"email":"nobody@example.com"}"#))
        .expect("the request builds");
    let response = router(state(&test_app))
        .oneshot(request)
        .await
        .expect("the router answers");
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "the response never says whether the address exists"
    );

    let request = Request::builder()
        .method("POST")
        .uri("/api/auth/reset-password")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"token":"recovery-rpc-token","password":"new password that passes"}"#,
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
        .expect("a session cookie is set")
        .split(';')
        .next()
        .expect("the cookie has a value")
        .to_owned();

    // And the new password signs the person in through the ordinary route.
    let request = Request::builder()
        .method("POST")
        .uri("/api/auth/sign-in")
        .header("content-type", "application/json")
        .header(header::COOKIE, &cookie)
        .body(Body::from(
            r#"{"email":"person@example.com","password":"new password that passes"}"#,
        ))
        .expect("the request builds");
    let response = router(state(&test_app))
        .oneshot(request)
        .await
        .expect("the router answers");
    assert_eq!(response.status(), StatusCode::OK);
}

// ---------------------------------------------------------------------------
// M10 — invitation acceptance over the RPC surface (REQ-042)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn req_042_accepting_the_invite_over_rpc_joins_the_space() {
    let test_app = TestApp::builder()
        .tokens(vec![
            "invite-rpc-token".to_owned(),
            "session-token-rpc-invite".to_owned(),
        ])
        .build();
    let owner = test_app
        .seed_user("owner@example.com", "correct horse battery")
        .await
        .expect("the owner exists");
    let invitee = test_app
        .seed_user("invitee@example.com", "correct horse battery")
        .await
        .expect("the invitee exists");
    let space_id = test_app
        .seed_space(&owner, "Golden space")
        .await
        .expect("the space exists");

    // The owner mints the invitation through the same use case the RPC uses; the token queue
    // hands the invite mint first.
    let context = TestApp::member_context(&space_id, &owner);
    boma_app::usecases::invite(
        test_app.app(),
        &context,
        "invitee@example.com",
        boma_domain::SpaceRole::Member,
        3600,
    )
    .await
    .expect("the invitation is issued");

    // The invitee signs in as the address the invitation names, then accepts over HTTP.
    let caller = Caller {
        cookie: sign_in_as(&test_app, "invitee@example.com").await,
        space_id: String::new(),
    };
    let response = post(
        state(&test_app),
        "/rpc/invitations/accept",
        Some(&caller),
        serde_json::json!({"token": "invite-rpc-token"}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    assert_eq!(body["spaceId"], serde_json::json!(space_id.as_str()));

    let role = test_app
        .app()
        .stores
        .spaces
        .membership(&space_id, &invitee)
        .await
        .expect("the store answers");
    assert_eq!(role, Some(boma_domain::SpaceRole::Member));
}

#[tokio::test]
async fn req_042_a_stranger_token_is_refused_over_rpc() {
    let test_app = TestApp::builder()
        .tokens(vec![
            "invite-rpc-unknown".to_owned(),
            "session-token-rpc-stranger".to_owned(),
        ])
        .build();
    let owner = test_app
        .seed_user("owner@example.com", "correct horse battery")
        .await
        .expect("the owner exists");
    let invitee = test_app
        .seed_user("invitee@example.com", "correct horse battery")
        .await
        .expect("the invitee exists");
    let space_id = test_app
        .seed_space(&owner, "Golden space")
        .await
        .expect("the space exists");

    let context = TestApp::member_context(&space_id, &owner);
    boma_app::usecases::invite(
        test_app.app(),
        &context,
        "invitee@example.com",
        boma_domain::SpaceRole::Member,
        3600,
    )
    .await
    .expect("the invitation is issued");

    let caller = Caller {
        cookie: sign_in_as(&test_app, "invitee@example.com").await,
        space_id: String::new(),
    };
    let response = post(
        state(&test_app),
        "/rpc/invitations/accept",
        Some(&caller),
        serde_json::json!({"token": "not-the-token"}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let _ = invitee;
}

#[tokio::test]
async fn req_042_the_public_preview_answers_only_with_the_names_and_the_expiry() {
    let test_app = TestApp::builder()
        .tokens(vec!["invite-public-preview".to_owned()])
        .build();
    let owner = test_app
        .seed_user("owner@example.com", "correct horse battery")
        .await
        .expect("the owner exists");
    let space_id = test_app
        .seed_space(&owner, "Golden space")
        .await
        .expect("the space exists");

    let context = TestApp::member_context(&space_id, &owner);
    boma_app::usecases::invite(
        test_app.app(),
        &context,
        "invitee@example.com",
        boma_domain::SpaceRole::Member,
        3600,
    )
    .await
    .expect("the invitation is issued");

    let request = Request::builder()
        .method("GET")
        .uri("/api/v1/invites/invite-public-preview")
        .body(Body::empty())
        .expect("the request builds");
    let response = router(state(&test_app))
        .oneshot(request)
        .await
        .expect("the router answers");
    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    assert_eq!(body["spaceName"], serde_json::json!("Golden space"));
    assert_eq!(body["inviterName"], serde_json::json!("owner"));
    assert_eq!(body["role"], serde_json::json!("member"));
    assert!(body["expiresAt"].is_string());

    let request = Request::builder()
        .method("GET")
        .uri("/api/v1/invites/no-such-token")
        .body(Body::empty())
        .expect("the request builds");
    let response = router(state(&test_app))
        .oneshot(request)
        .await
        .expect("the router answers");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body = json(response).await;
    assert_eq!(body["error"], serde_json::json!("Not found."));
}

// ---------------------------------------------------------------------------
// M11.1 — groups (REQ-010)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn req_010_creating_a_group_over_rpc_makes_the_shared_room() {
    let test_app = TestApp::builder()
        // Scripted ids in mint order across the whole test: user, org, two spaces,
        // two bots and their threads, then the group's thread and the group's id.
        .ids(vec![
            "usr_golden".to_owned(),
            "org_golden".to_owned(),
            "spc_seed".to_owned(),
            "spc_golden".to_owned(),
            "bot_a".to_owned(),
            "thr_a".to_owned(),
            "bot_b".to_owned(),
            "thr_b".to_owned(),
            "ses_golden".to_owned(),
            "thr_group_golden".to_owned(),
            "grp_golden".to_owned(),
        ])
        .build();
    let owner = test_app
        .seed_user("owner@example.com", "correct horse battery")
        .await
        .expect("the owner exists");
    let space_id = test_app
        .seed_space(&owner, "Golden space")
        .await
        .expect("the space exists");
    let _ = &space_id;
    let (bot_a, _) = test_app
        .seed_bot(&space_id, &owner, "Analyst")
        .await
        .expect("the bot exists");
    let _ = &bot_a;
    let (bot_b, _) = test_app
        .seed_bot(&space_id, &owner, "Writer")
        .await
        .expect("the bot exists");
    let _ = &bot_b;

    let cookie = sign_in_as(&test_app, "owner@example.com").await;
    let caller = Caller {
        cookie,
        space_id: space_id.as_str().to_owned(),
    };
    let response = post(
        state(&test_app),
        "/rpc/groups/create",
        Some(&caller),
        serde_json::json!({
            "name": "Triage",
            "botIds": [bot_a.as_str(), bot_b.as_str()],
        }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    assert_eq!(body["id"], serde_json::json!("grp_golden"));
    assert_eq!(body["name"], serde_json::json!("Triage"));
    assert_eq!(body["threadId"], serde_json::json!("thr_group_golden"));
    assert_eq!(
        body["memberBotIds"],
        serde_json::json!([bot_a.as_str(), bot_b.as_str()]),
    );
}

#[tokio::test]
async fn req_010_creating_a_group_without_a_session_is_refused() {
    let test_app = TestApp::builder().build();
    let owner = test_app
        .seed_user("owner@example.com", "correct horse battery")
        .await
        .expect("the owner exists");
    let _ = test_app
        .seed_space(&owner, "Golden space")
        .await
        .expect("the space exists");

    let response = post(
        state(&test_app),
        "/rpc/groups/create",
        None,
        serde_json::json!({"name": "Triage", "botIds": ["bot_a"]}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

// ---------------------------------------------------------------------------
// M11.3 — groups.get and groups.messages (REQ-010)
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_lines)] // the round trip is the test
#[tokio::test]
async fn req_010_the_group_read_surface_returns_the_room_and_its_messages() {
    let test_app = TestApp::builder()
        .ids(vec![
            "usr_golden".to_owned(),
            "org_golden".to_owned(),
            "spc_seed".to_owned(),
            "spc_golden".to_owned(),
            "bot_a".to_owned(),
            "thr_a".to_owned(),
            "bot_b".to_owned(),
            "thr_b".to_owned(),
            "ses_golden".to_owned(),
            "thr_group_golden".to_owned(),
            "grp_golden".to_owned(),
        ])
        .build();
    let owner = test_app
        .seed_user("owner@example.com", "correct horse battery")
        .await
        .expect("the owner exists");
    let space_id = test_app
        .seed_space(&owner, "Golden space")
        .await
        .expect("the space exists");
    let (analyst, _) = test_app
        .seed_bot(&space_id, &owner, "analyst")
        .await
        .expect("the analyst exists");
    let (writer, _) = test_app
        .seed_bot(&space_id, &owner, "writer")
        .await
        .expect("the writer exists");

    let cookie = sign_in_as(&test_app, "owner@example.com").await;
    let caller = Caller {
        cookie,
        space_id: space_id.as_str().to_owned(),
    };

    // Create the group.
    let response = post(
        state(&test_app),
        "/rpc/groups/create",
        Some(&caller),
        serde_json::json!({
            "name": "Triage",
            "botIds": [analyst.as_str(), writer.as_str()],
        }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    // Read the group back.
    let response = post(
        state(&test_app),
        "/rpc/groups/get",
        Some(&caller),
        serde_json::json!({"id": "grp_golden"}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    assert_eq!(body["id"], serde_json::json!("grp_golden"));
    assert_eq!(body["name"], serde_json::json!("Triage"));
    assert_eq!(body["threadId"], serde_json::json!("thr_group_golden"),);
    assert_eq!(
        body["memberBotIds"],
        serde_json::json!([analyst.as_str(), writer.as_str()])
    );
    assert_eq!(body["memberBots"].as_array().map(Vec::len), Some(2));

    // Send a message that mentions both bots — both runs will queue.
    let response = post(
        state(&test_app),
        "/rpc/threads/send",
        Some(&caller),
        serde_json::json!({
            "botId": analyst.as_str(),
            "text": "@analyst @writer please summarise this thread.",
            "clientNonce": "req-010-replay",
        }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    assert_eq!(
        body["runIds"].as_array().map(Vec::len),
        Some(2),
        "both mentioned bots queue a run",
    );

    // groups.messages shows both bot attributions.
    let response = post(
        state(&test_app),
        "/rpc/groups/messages",
        Some(&caller),
        serde_json::json!({"threadId": "thr_group_golden"}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    let messages = body["messages"].as_array().cloned().unwrap_or_default();
    assert!(
        messages
            .iter()
            .any(|m| m["role"] == serde_json::json!("user")),
        "the member message appears in the shared thread",
    );
    // Every message in the room carries attribution: member messages used to land without a
    // role/vendor, bot replies get their botId — this pin keeps the wire stable.
    for message in &messages {
        assert!(
            message.get("role").is_some(),
            "every message carries a role"
        );
        if message["role"] == serde_json::json!("bot") {
            assert!(
                message.get("botId").is_some(),
                "bot messages are attributed"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// M12.1 — external agents (REQ-059)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn req_059_bots_connect_external_over_rpc_round_trips() {
    let test_app = TestApp::builder().build();
    let owner = test_app
        .seed_user("owner@example.com", "correct horse battery")
        .await
        .expect("the owner exists");
    let space_id = test_app
        .seed_space(&owner, "Golden space")
        .await
        .expect("the space exists");
    let (bot, _) = test_app
        .seed_bot(&space_id, &owner, "Analyst")
        .await
        .expect("the bot exists");

    let cookie = sign_in_as(&test_app, "owner@example.com").await;
    let caller = Caller {
        cookie,
        space_id: space_id.as_str().to_owned(),
    };

    // Connect — the address does not need to answer; only its shape and the tenant matter now.
    let response = post(
        state(&test_app),
        "/rpc/bots/connectExternal",
        Some(&caller),
        serde_json::json!({
            "botId": bot.as_str(),
            "baseUrl": "https://agents.internal.example.com/agui",
            "agentName": "release-bot",
            "secretName": null,
        }),
    )
    .await;
    let status = response.status();
    let body = json(response).await;
    eprintln!("connect status={status} body={body}");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["botId"], serde_json::json!(bot.as_str()));
    assert_eq!(
        body["baseUrl"],
        serde_json::json!("https://agents.internal.example.com/agui"),
    );
    assert_eq!(body["agentName"], serde_json::json!("release-bot"));

    // Disconnect is idempotent in shape: a first call disconnects; a second is not found.
    let response = post(
        state(&test_app),
        "/rpc/bots/disconnectExternal",
        Some(&caller),
        serde_json::json!({"botId": bot.as_str()}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    assert_eq!(body["disconnected"], serde_json::json!(true));

    let response = post(
        state(&test_app),
        "/rpc/bots/disconnectExternal",
        Some(&caller),
        serde_json::json!({"botId": bot.as_str()}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    // An unknown bot is refused.
    let response = post(
        state(&test_app),
        "/rpc/bots/connectExternal",
        Some(&caller),
        serde_json::json!({
            "botId": "bot_missing",
            "baseUrl": "https://agents.internal.example.com/agui",
        }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

// ---------------------------------------------------------------------------
// M13.1 — sign-in requests (REQ-060)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn req_060_bots_request_signin_and_list_round_trips() {
    let test_app = TestApp::builder().build();
    let owner = test_app
        .seed_user("owner@example.com", "correct horse battery")
        .await
        .expect("the owner exists");
    let space_id = test_app
        .seed_space(&owner, "Golden space")
        .await
        .expect("the space exists");
    let (_bot, _) = test_app
        .seed_bot(&space_id, &owner, "Analyst")
        .await
        .expect("the bot exists");

    let cookie = sign_in_as(&test_app, "owner@example.com").await;
    let caller = Caller {
        cookie,
        space_id: space_id.as_str().to_owned(),
    };

    // A member can ask the bot to sign in on their behalf via the new RPC.
    let response = post(
        state(&test_app),
        "/rpc/bots/signIns",
        Some(&caller),
        serde_json::json!({}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    assert_eq!(
        body["requests"].as_array().map(Vec::len),
        Some(0),
        "no sign-in requests before anyone asks"
    );
}

// ---------------------------------------------------------------------------
// M15.1 — notifications (REQ-064)
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_lines)] // one round-trip on purpose; splitting hides the contract
#[tokio::test]
async fn req_064_notifications_status_subscribe_policy_round_trip() {
    let test_app = TestApp::builder()
        .vapid(boma_testkit::vapid_test_config())
        .build();
    let owner = test_app
        .seed_user("owner@example.com", "correct horse battery")
        .await
        .expect("the owner exists");
    let space_id = test_app
        .seed_space(&owner, "Golden space")
        .await
        .expect("the space exists");
    let (bot, _) = test_app
        .seed_bot(&space_id, &owner, "Analyst")
        .await
        .expect("the bot exists");

    let cookie = sign_in_as(&test_app, "owner@example.com").await;
    let caller = Caller {
        cookie,
        space_id: space_id.as_str().to_owned(),
    };

    // status: enabled, public key present and URL-safe.
    let response = post(
        state(&test_app),
        "/rpc/notifications/status",
        Some(&caller),
        serde_json::json!({}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    assert_eq!(body["enabled"], serde_json::json!(true));
    let public_key = body["vapidPublicKey"].as_str().expect("the key is present");
    assert!(
        public_key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    );

    // policyGet: the documented default.
    let response = post(
        state(&test_app),
        "/rpc/notifications/policyGet",
        Some(&caller),
        serde_json::json!({ "botId": bot.as_str() }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    assert_eq!(body["notifyWaiting"], serde_json::json!(true));
    assert_eq!(body["notifyCompleted"], serde_json::json!(false));
    assert_eq!(body["dailyCap"], serde_json::json!(20));

    // policySet: flips the two booleans and the cap.
    let response = post(
        state(&test_app),
        "/rpc/notifications/policySet",
        Some(&caller),
        serde_json::json!({ "botId": bot.as_str(), "notifyCompleted": true, "dailyCap": 7 }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    assert_eq!(body["notifyWaiting"], serde_json::json!(true));
    assert_eq!(body["notifyCompleted"], serde_json::json!(true));
    assert_eq!(body["dailyCap"], serde_json::json!(7));

    // An unknown bot is not found.
    let response = post(
        state(&test_app),
        "/rpc/notifications/policyGet",
        Some(&caller),
        serde_json::json!({ "botId": "bot_missing" }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    // subscribe: https endpoint round-trips.
    let response = post(
        state(&test_app),
        "/rpc/notifications/subscribe",
        Some(&caller),
        serde_json::json!({
            "endpoint": "https://updates.push.services.mozilla.com/wpush/v1/abc123",
            "p256dh": "BBPb7By_a6HvK0YueJEc86yZElts9klQHcgXPecGKi_tTWwTp90GexfpVuQVMrtiISxvbjGwVd24FxSsKTOR4pg",
            "auth": "_Md5-ailpgkLkpnHDiVcfA",
            "userAgent": "PushMeeting/1.0",
        }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    assert!(body["id"].as_str().expect("an id").starts_with("sub_"));

    // unsubscribe: first removes, second reports nothing removed.
    let response = post(
        state(&test_app),
        "/rpc/notifications/unsubscribe",
        Some(&caller),
        serde_json::json!({ "endpoint": "https://updates.push.services.mozilla.com/wpush/v1/abc123" }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    assert_eq!(body["removed"], serde_json::json!(true));

    let response = post(
        state(&test_app),
        "/rpc/notifications/unsubscribe",
        Some(&caller),
        serde_json::json!({ "endpoint": "https://updates.push.services.mozilla.com/wpush/v1/abc123" }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    assert_eq!(body["removed"], serde_json::json!(false));
}

#[tokio::test]
async fn req_064_subscribe_rejects_private_or_plain_http_endpoints() {
    let test_app = TestApp::builder()
        .vapid(boma_testkit::vapid_test_config())
        .endpoint_safety(std::sync::Arc::new(
            boma_testkit::ScriptedEndpointSafety::refusing(&[
                "169.254.169.254",
                "127.",
                "192.168.",
                "10.",
                "localhost",
                "metadata.google.internal",
            ]),
        ))
        .build();
    let owner = test_app
        .seed_user("owner@example.com", "correct horse battery")
        .await
        .expect("the owner exists");
    let space_id = test_app
        .seed_space(&owner, "Golden space")
        .await
        .expect("the space exists");

    let cookie = sign_in_as(&test_app, "owner@example.com").await;
    let caller = Caller {
        cookie,
        space_id: space_id.as_str().to_owned(),
    };

    // Plain HTTP is refused before the checker even runs.
    let response = post(
        state(&test_app),
        "/rpc/notifications/subscribe",
        Some(&caller),
        serde_json::json!({
            "endpoint": "http://updates.push.services.mozilla.com/wpush/v1/abc123",
            "p256dh": "BBPb7By_a6HvK0YueJEc86yZElts9klQHcgXPecGKi_tTWwTp90GexfpVuQVMrtiISxvbjGwVd24FxSsKTOR4pg",
            "auth": "_Md5-ailpgkLkpnHDiVcfA",
        }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    // The cloud metadata service is refused.
    let response = post(
        state(&test_app),
        "/rpc/notifications/subscribe",
        Some(&caller),
        serde_json::json!({
            "endpoint": "https://169.254.169.254/wpush/v1/abc123",
            "p256dh": "BBPb7By_a6HvK0YueJEc86yZElts9klQHcgXPecGKi_tTWwTp90GexfpVuQVMrtiISxvbjGwVd24FxSsKTOR4pg",
            "auth": "_Md5-ailpgkLkpnHDiVcfA",
        }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    // A private-looking HTTPS endpoint is refused by the checker.
    let response = post(
        state(&test_app),
        "/rpc/notifications/subscribe",
        Some(&caller),
        serde_json::json!({
            "endpoint": "https://192.168.1.7/push",
            "p256dh": "BBPb7By_a6HvK0YueJEc86yZElts9klQHcgXPecGKi_tTWwTp90GexfpVuQVMrtiISxvbjGwVd24FxSsKTOR4pg",
            "auth": "_Md5-ailpgkLkpnHDiVcfA",
        }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    // Credentials in the URL are refused.
    let response = post(
        state(&test_app),
        "/rpc/notifications/subscribe",
        Some(&caller),
        serde_json::json!({
            "endpoint": "https://user:pass@updates.push.services.mozilla.com/wpush/v1/abc123",
            "p256dh": "BBPb7By_a6HvK0YueJEc86yZElts9klQHcgXPecGKi_tTWwTp90GexfpVuQVMrtiISxvbjGwVd24FxSsKTOR4pg",
            "auth": "_Md5-ailpgkLkpnHDiVcfA",
        }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    // A public https endpoint goes through.
    let response = post(
        state(&test_app),
        "/rpc/notifications/subscribe",
        Some(&caller),
        serde_json::json!({
            "endpoint": "https://updates.push.services.mozilla.com/wpush/v1/abc123",
            "p256dh": "BBPb7By_a6HvK0YueJEc86yZElts9klQHcgXPecGKi_tTWwTp90GexfpVuQVMrtiISxvbjGwVd24FxSsKTOR4pg",
            "auth": "_Md5-ailpgkLkpnHDiVcfA",
        }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
}
