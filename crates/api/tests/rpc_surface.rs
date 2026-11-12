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
