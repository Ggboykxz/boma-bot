//! The stdio MCP transport, against a real spawned server (REQ-033).
//!
//! The deployment's allow-list is the whole story here: a program it names runs, a program it
//! does not name is refused before a process exists. The fixture is a five-line Python server
//! written to a temp file — an actual child process speaking actual JSON-RPC over real pipes,
//! no double.

// An integration test file is compiled as its own crate, so clippy's `allow-*-in-tests` (which
// covers `#[test]` functions and `#[cfg(test)]` modules only) does not reach the helpers below.
// AGENTS.md allows unwrap, expect and panic in test code; this is where that permission is taken,
// once and in writing.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use boma_app::ports::{McpServerRecord, McpTransport};
use boma_infra::StdioMcpTransport;

const FIXTURE: &str = r#"
import json, sys
for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    msg = json.loads(line)
    method = msg.get("method")
    if method == "initialize":
        print(json.dumps({"jsonrpc": "2.0", "id": msg.get("id"), "result": {
            "protocolVersion": "2024-11-05",
            "serverInfo": {"name": "fixture", "version": "0.0.0"},
            "capabilities": {"tools": {}},
        }}))
    elif method == "tools/list":
        print(json.dumps({"jsonrpc": "2.0", "id": msg.get("id"), "result": {
            "tools": [{"name": "ping", "description": "Answers pong.", "inputSchema": {"type": "object", "properties": {}}}],
        }}))
    elif method == "tools/call":
        arguments = msg.get("params", {}).get("arguments", {})
        print(json.dumps({"jsonrpc": "2.0", "id": msg.get("id"), "result": {
            "content": [{"type": "text", "text": "pong:" + json.dumps(arguments)}],
        }}))
    sys.stdout.flush()
"#;

fn record(command: &str) -> McpServerRecord {
    McpServerRecord {
        id: "mcp_stdio_test".to_owned(),
        name: "fixture-server".to_owned(),
        transport: "stdio".to_owned(),
        url: None,
        command: Some(command.to_owned()),
        auth_state: "none".to_owned(),
        allow_private: false,
        authorization_url: None,
        token_url: None,
        client_id: None,
        tool_count: None,
        created_at: "2026-01-01T00:00:00Z".to_owned(),
    }
}

#[tokio::test]
async fn req_033_an_allow_listed_stdio_server_is_spawned_and_answers() {
    let path = std::env::temp_dir().join(format!("boma-mcp-fixture-{}.py", std::process::id()));
    std::fs::write(&path, FIXTURE).expect("the fixture is written");

    let transport = StdioMcpTransport::new(vec!["python3".to_owned()]);
    let server = record(&format!("python3 {}", path.display()));

    let tools = transport
        .list_tools(&server, None)
        .await
        .expect("the server answers");
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].name, "ping");
    assert_eq!(tools[0].description, "Answers pong.");

    let result = transport
        .call_tool(&server, None, "ping", &serde_json::json!({ "quiet": true }))
        .await
        .expect("the call answers");
    let text = result["content"][0]["text"]
        .as_str()
        .expect("a text answer");
    assert!(text.contains("quiet"), "{text}");

    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn req_033_a_program_not_on_the_allow_list_is_refused_before_spawn() {
    let transport = StdioMcpTransport::new(Vec::new());
    let server = record("python3 /tmp/this-would-not-exist-but-it-does-not-matter.py");
    let outcome = transport.list_tools(&server, None).await;
    let error = outcome.expect_err("an empty allow-list refuses every program");
    assert!(matches!(error, boma_app::AppError::StdioDisabled));
}

#[tokio::test]
async fn req_033_a_command_with_shell_syntax_is_refused_before_spawn() {
    let transport = StdioMcpTransport::new(vec!["sh".to_owned()]);
    let server = record("sh -c 'id'");
    let outcome = transport.list_tools(&server, None).await;
    let error = outcome.expect_err("shell syntax is refused by split_command");
    assert!(matches!(error, boma_app::AppError::Rule(_)));
}
