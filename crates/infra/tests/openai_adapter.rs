//! REQ-015: the bring-your-own-key adapter, replayed against captured wire answers.
//!
//! No test makes a live network call. Each test runs a loopback wire server, plays exactly the
//! bytes an OpenAI-compatible provider would answer, and asserts what the port produces and
//! what the adapter sent. `finish_reason` and `tool_calls` map onto the same `ProviderTurn`
//! the scripted provider produces.

#![allow(clippy::expect_used)]

use boma_app::ports::ModelProvider;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{header, method, path},
};

fn config(endpoint: &str) -> boma_infra::OpenAiConfig {
    boma_infra::OpenAiConfig {
        endpoint: endpoint.to_owned(),
        model: "anthropic/claude-sonnet".to_owned(),
        api_key: secrecy::SecretString::from("sk-test-key".to_owned()),
    }
}

fn request(prompt: &str) -> boma_app::ports::ProviderRequest {
    let bot = boma_domain::Bot {
        id: boma_domain::BotId::new("bot_1").expect("an id is accepted"),
        space_id: boma_domain::SpaceId::new("spc_1").expect("an id is accepted"),
        user_id: boma_domain::UserId::new("usr_1").expect("an id is accepted"),
        name: "Assistant".to_owned(),
        description: None,
        color: None,
        thread_id: boma_domain::ThreadId::new("thr_1").expect("an id is accepted"),
        archived_at: None,
        created_at: "2026-01-01T00:00:00Z".to_owned(),
        enabled_tools: Vec::new(),
        model_provider: None,
        model_id: None,
        model_pinned: false,
    };
    let thread = boma_domain::Thread {
        id: boma_domain::ThreadId::new("thr_1").expect("an id is accepted"),
        space_id: boma_domain::SpaceId::new("spc_1").expect("an id is accepted"),
        bot_id: bot.id.clone(),
        user_id: boma_domain::UserId::new("usr_1").expect("an id is accepted"),
        next_message_seq: 1,
        next_event_seq: 1,
        created_at: "2026-01-01T00:00:00Z".to_owned(),
    };
    boma_app::ports::ProviderRequest {
        bot,
        thread,
        prompt: prompt.to_owned(),
        history: Vec::new(),
        tools: boma_app::usecases::builtin_tools(),
        memory: Vec::new(),
        skills: Vec::new(),
    }
}

fn adapter_for(endpoint: &str) -> boma_infra::OpenAiCompatibleAdapter {
    boma_infra::OpenAiCompatibleAdapter::new(config(endpoint)).expect("the adapter builds")
}

#[tokio::test]
async fn req_015_the_openai_compatible_adapter_maps_tool_calls_to_a_tool_call_turn() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(header("authorization", "Bearer sk-test-key"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "choices": [{
                    "finish_reason": "tool_calls",
                    "message": {
                        "tool_calls": [{
                            "id": "call_42",
                            "type": "function",
                            "function": {
                                "name": "memory.read",
                                "arguments": "{\"scope\":\"bot\",\"botId\":\"bot_1\",\"path\":\"notes.md\"}"
                            }
                        }]
                    }
                }]
            })),
        )
        .expect(1)
        .mount(&server)
        .await;

    let adapter = adapter_for(&format!("{}/v1", server.uri()));
    let turn = adapter
        .complete(request("Summarise my week."))
        .await
        .expect("the adapter answers");
    assert_eq!(
        turn,
        boma_app::ports::ProviderTurn::ToolCalls(vec![boma_app::ports::ToolCallRequest {
            id: "call_42".to_owned(),
            name: "memory.read".to_owned(),
            arguments: serde_json::json!({
                "scope": "bot",
                "botId": "bot_1",
                "path": "notes.md",
            }),
        }])
    );
}

#[tokio::test]
async fn req_015_the_openai_compatible_adapter_maps_a_text_answer_to_ordered_chunks() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "choices": [{
                "finish_reason": "stop",
                "message": { "content": "Considering Tuesday." }
            }]
        })))
        .expect(1)
        .mount(&server)
        .await;

    let adapter = adapter_for(&format!("{}/v1", server.uri()));
    let turn = adapter
        .complete(request("When?"))
        .await
        .expect("the adapter answers");
    let boma_app::ports::ProviderTurn::Chunks(chunks) = turn else {
        panic!("the answer maps to chunks");
    };
    assert_eq!(chunks.concat(), "Considering Tuesday.");
    assert_eq!(chunks, vec!["Considering ", "Tuesday."]);
}

#[tokio::test]
async fn req_015_the_model_sees_the_schema_of_the_builtin_tools() {
    use std::sync::{Arc, Mutex};

    struct Capture(Arc<Mutex<Option<serde_json::Value>>>);
    impl wiremock::Respond for Capture {
        fn respond(&self, request: &wiremock::Request) -> ResponseTemplate {
            *self.0.lock().expect("the capture lock") = serde_json::from_slice(&request.body).ok();
            ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "choices": [{
                    "finish_reason": "stop",
                    "message": { "content": "ok" }
                }]
            }))
        }
    }

    let server = MockServer::start().await;
    let captured = Arc::new(Mutex::new(None));
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(Capture(Arc::clone(&captured)))
        .expect(1)
        .mount(&server)
        .await;

    let adapter = adapter_for(&format!("{}/v1", server.uri()));
    adapter
        .complete(request("Hi."))
        .await
        .expect("the adapter answers");

    let body = captured
        .lock()
        .expect("the capture lock")
        .take()
        .expect("the body was captured");
    let tools = body
        .get("tools")
        .and_then(serde_json::Value::as_array)
        .expect("the tools are offered");
    let memory_write = tools
        .iter()
        .find(|tool| tool["function"]["name"].as_str() == Some("memory.write"))
        .expect("memory.write is offered");
    // The model sees the dispatcher's accepted arguments, not a guess.
    assert_eq!(
        memory_write["function"]["parameters"]["required"],
        serde_json::json!(["path", "text"])
    );
}

#[tokio::test]
async fn req_015_the_tools_ride_the_request_as_openai_function_schemas() {
    use std::sync::{Arc, Mutex};

    struct Capture(Arc<Mutex<Option<serde_json::Value>>>);
    impl wiremock::Respond for Capture {
        fn respond(&self, request: &wiremock::Request) -> ResponseTemplate {
            *self.0.lock().expect("the capture lock") = serde_json::from_slice(&request.body).ok();
            ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "choices": [{
                    "finish_reason": "stop",
                    "message": { "content": "ok" }
                }]
            }))
        }
    }

    let server = MockServer::start().await;
    let captured = Arc::new(Mutex::new(None));
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(Capture(Arc::clone(&captured)))
        .expect(1)
        .mount(&server)
        .await;

    let adapter = adapter_for(&format!("{}/v1", server.uri()));
    adapter
        .complete(request("Delegate this."))
        .await
        .expect("the adapter answers");

    let body = captured
        .lock()
        .expect("the capture lock")
        .take()
        .expect("the body was captured");
    assert_eq!(body["model"], serde_json::json!("anthropic/claude-sonnet"));
    assert_eq!(body["tool_choice"], serde_json::json!("auto"));
    let names: Vec<&str> = body["tools"]
        .as_array()
        .expect("the tools are offered")
        .iter()
        .filter_map(|tool| tool["function"]["name"].as_str())
        .collect();
    assert_eq!(
        names,
        boma_app::usecases::builtin_tools()
            .iter()
            .map(|tool| tool.key.as_str())
            .collect::<Vec<_>>(),
    );
}

#[tokio::test]
async fn req_015_a_refused_request_maps_to_unavailable_which_is_retryable() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(429).set_body_json(serde_json::json!({
            "error": { "message": "slow down" }
        })))
        .expect(1)
        .mount(&server)
        .await;

    let adapter = adapter_for(&format!("{}/v1", server.uri()));
    let error = adapter
        .complete(request("Hi."))
        .await
        .expect_err("a 429 is a provider refusal");
    assert!(matches!(error, boma_app::AppError::Unavailable));
    assert!(boma_app::AppError::Unavailable.is_retryable());
}

#[tokio::test]
async fn req_015_malformed_tool_call_arguments_fail_the_turn_not_the_process() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "choices": [{
                "finish_reason": "tool_calls",
                "message": {
                    "tool_calls": [{
                        "id": "call_1",
                        "type": "function",
                        "function": {
                            "name": "memory.read",
                            "arguments": "this-is-not-json"
                        }
                    }]
                }
            }]
        })))
        .expect(1)
        .mount(&server)
        .await;

    let adapter = adapter_for(&format!("{}/v1", server.uri()));
    let error = adapter
        .complete(request("Hi."))
        .await
        .expect_err("malformed arguments must surface as an error");
    assert!(matches!(error, boma_app::AppError::Unavailable));
}
