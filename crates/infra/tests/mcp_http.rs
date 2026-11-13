//! The HTTP JSON-RPC transport for MCP servers, against a loopback wire server (REQ-033).
//!
//! Nothing here touches the network beyond 127.0.0.1: the server is a fixture, the allow-list
//! flags are what a private deployment of a LAN server would set, and the refusal paths are
//! asserted just as firmly as the happy path.

// An integration test file is compiled as its own crate, so clippy's `allow-*-in-tests` (which
// covers `#[test]` functions and `#[cfg(test)]` modules only) does not reach the helpers below.
// AGENTS.md allows unwrap, expect and panic in test code; this is where that permission is taken,
// once and in writing.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use boma_app::ports::{McpServerRecord, McpTransport};
use boma_infra::HttpMcpTransport;
use secrecy::SecretString;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{body_partial_json, header, method, path},
};

fn record(url: &str, allow_private: bool) -> McpServerRecord {
    McpServerRecord {
        id: "mcp_test".to_owned(),
        name: "example".to_owned(),
        transport: "http".to_owned(),
        url: Some(url.to_owned()),
        command: None,
        auth_state: "none".to_owned(),
        allow_private,
        authorization_url: None,
        token_url: None,
        client_id: None,
        tool_count: None,
        created_at: "2026-01-01T00:00:00Z".to_owned(),
    }
}

fn mcp_result(value: &serde_json::Value) -> serde_json::Value {
    serde_json::json!({ "jsonrpc": "2.0", "id": 1, "result": value })
}

#[tokio::test]
async fn req_033_the_http_transport_initializes_discovers_and_calls_with_the_bearer() {
    let server = MockServer::start().await;
    let url = format!("{}/mcp", server.uri());
    Mock::given(method("POST"))
        .and(path("/mcp"))
        .and(header("authorization", "Bearer token-1"))
        .and(body_partial_json(
            serde_json::json!({ "method": "initialize" }),
        ))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(mcp_result(&serde_json::json!({
                "protocolVersion": "2024-11-05",
                "serverInfo": { "name": "fixture", "version": "0.0.0" },
            }))),
        )
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/mcp"))
        .and(header("authorization", "Bearer token-1"))
        .and(body_partial_json(serde_json::json!({ "method": "tools/list" })))
        .respond_with(ResponseTemplate::new(200).set_body_json(mcp_result(&serde_json::json!({
            "tools": [
                { "name": "list-issues", "description": "Lists the open issues.", "inputSchema": { "type": "object" } },
                { "name": "../escape", "description": "Must be skipped.", "inputSchema": { "type": "object" } }
            ],
        }))))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/mcp"))
        .and(header("authorization", "Bearer token-1"))
        .and(body_partial_json(
            serde_json::json!({ "method": "tools/call", "params": { "name": "list-issues" } }),
        ))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(mcp_result(&serde_json::json!({
                "content": [{ "type": "text", "text": "A" }],
            }))),
        )
        .mount(&server)
        .await;

    let transport = HttpMcpTransport::new().expect("the transport builds");
    let server_record = record(&url, true);
    let bearer = SecretString::from("token-1");

    let tools = transport
        .list_tools(&server_record, Some(&bearer))
        .await
        .expect("the probe answers");
    assert_eq!(tools.len(), 1, "the unsafe tool name is skipped: {tools:?}");
    assert_eq!(tools[0].name, "list-issues");

    let called = transport
        .call_tool(
            &server_record,
            Some(&bearer),
            "list-issues",
            &serde_json::json!({ "state": "open" }),
        )
        .await
        .expect("the call answers");
    assert_eq!(called["content"][0]["text"], serde_json::json!("A"));

    server.verify().await;
}

#[tokio::test]
async fn req_033_the_http_transport_refuses_a_private_endpoint_without_the_allow_list() {
    // A private HTTPS endpoint without the allow-list is refused before any byte leaves.
    let url = "https://127.0.0.1/mcp".to_owned();
    let transport = HttpMcpTransport::new().expect("the transport builds");
    let refused = transport.list_tools(&record(&url, false), None).await;
    let error = refused.expect_err("a private endpoint without the allow-list is refused");
    assert!(
        format!("{error}").contains("refused") || format!("{error}").contains("address"),
        "{error}"
    );
}

#[tokio::test]
async fn req_033_the_http_transport_surfaces_a_server_error_as_a_tool_failure() {
    let server = MockServer::start().await;
    let url = format!("{}/mcp", server.uri());
    Mock::given(method("POST"))
        .and(path("/mcp"))
        .and(body_partial_json(
            serde_json::json!({ "method": "initialize" }),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "error": { "code": -32601, "message": "method not found" },
        })))
        .mount(&server)
        .await;

    let transport = HttpMcpTransport::new().expect("the transport builds");
    let outcome = transport.list_tools(&record(&url, true), None).await;
    let error = outcome.expect_err("the server's error is surfaced");
    assert!(format!("{error}").contains("method not found"), "{error}");
}
