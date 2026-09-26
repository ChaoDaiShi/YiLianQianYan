// ============================================================
// Legacy stdio client — `tools/call` protocol, `ToolResult` bridge and
// argument-validation tests.
// ============================================================

use super::super::*;
use super::*;
use crate::db::McpServer;
use std::time::Duration;

// ── tools/call protocol tests ──

async fn run_call_session(
    mock: tokio::task::JoinHandle<()>,
    client: DuplexStream,
    remote_name: &str,
    args: serde_json::Value,
) -> Result<McpCallResult, McpError> {
    let mut session = McpSession::new(client);
    session.initialize().await.unwrap();
    session.send_initialized().await.unwrap();
    let result = session.call_tool(remote_name, args).await;
    drop(session);
    mock.await.unwrap();
    result
}

#[tokio::test]
async fn tools_call_request_uses_remote_name_and_arguments() {
    let (client, server) = duplex_pair();
    let mock = tokio::spawn(async move {
        let mut reader = BufReader::new(server);
        let _ = read_server_line(&mut reader).await; // initialize
        write_server_line(&mut reader, &initialize_response(MCP_PROTOCOL_VERSION)).await;
        let _ = read_server_line(&mut reader).await; // initialized
        let call = read_server_line(&mut reader).await;
        assert_eq!(call["method"], "tools/call");
        assert_eq!(call["params"]["name"], "read-file");
        assert_eq!(call["params"]["arguments"], json!({"path": "/tmp/a"}));
        let id = call["id"].as_i64().unwrap();
        write_server_line(
            &mut reader,
            &json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": { "content": [{ "type": "text", "text": "hello" }], "isError": false }
            }),
        )
        .await;
    });

    let result = run_call_session(mock, client, "read-file", json!({"path": "/tmp/a"}))
        .await
        .unwrap();
    assert!(!result.is_error);
    assert_eq!(result.content.len(), 1);
}

#[tokio::test]
async fn tools_call_success_returns_tool_result_ok() {
    let (client, server) = duplex_pair();
    let mock = tokio::spawn(async move {
        let mut reader = BufReader::new(server);
        let _ = read_server_line(&mut reader).await;
        write_server_line(&mut reader, &initialize_response(MCP_PROTOCOL_VERSION)).await;
        let _ = read_server_line(&mut reader).await;
        let call = read_server_line(&mut reader).await;
        let id = call["id"].as_i64().unwrap();
        write_server_line(
            &mut reader,
            &json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": { "content": [{ "type": "text", "text": "hello" }], "isError": false }
            }),
        )
        .await;
    });

    let result = run_call_session(mock, client, "echo", json!({}))
        .await
        .unwrap();
    let tr = result.into_tool_result();
    assert!(tr.ok);
    assert_eq!(tr.content, "hello");
}

#[tokio::test]
async fn tools_call_tool_error_is_not_rpc_error() {
    let (client, server) = duplex_pair();
    let mock = tokio::spawn(async move {
        let mut reader = BufReader::new(server);
        let _ = read_server_line(&mut reader).await;
        write_server_line(&mut reader, &initialize_response(MCP_PROTOCOL_VERSION)).await;
        let _ = read_server_line(&mut reader).await;
        let call = read_server_line(&mut reader).await;
        let id = call["id"].as_i64().unwrap();
        write_server_line(
            &mut reader,
            &json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "content": [{ "type": "text", "text": "invalid input" }],
                    "isError": true
                }
            }),
        )
        .await;
    });

    let result = run_call_session(mock, client, "echo", json!({}))
        .await
        .unwrap();
    assert!(result.is_error);
    let tr = result.into_tool_result();
    assert!(!tr.ok);
}

#[tokio::test]
async fn tools_call_jsonrpc_error_is_rpc_error() {
    let (client, server) = duplex_pair();
    let mock = tokio::spawn(async move {
        let mut reader = BufReader::new(server);
        let _ = read_server_line(&mut reader).await;
        write_server_line(&mut reader, &initialize_response(MCP_PROTOCOL_VERSION)).await;
        let _ = read_server_line(&mut reader).await;
        let call = read_server_line(&mut reader).await;
        let id = call["id"].as_i64().unwrap();
        write_server_line(
            &mut reader,
            &json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": { "code": -32602, "message": "Unknown tool" }
            }),
        )
        .await;
    });

    let err = run_call_session(mock, client, "nope", json!({}))
        .await
        .unwrap_err();
    assert!(matches!(err, McpError::Rpc { code: -32602, .. }));
}

#[tokio::test]
async fn tools_call_handles_notification_before_response() {
    let (client, server) = duplex_pair();
    let mock = tokio::spawn(async move {
        let mut reader = BufReader::new(server);
        let _ = read_server_line(&mut reader).await;
        write_server_line(&mut reader, &initialize_response(MCP_PROTOCOL_VERSION)).await;
        let _ = read_server_line(&mut reader).await;
        let call = read_server_line(&mut reader).await;
        let id = call["id"].as_i64().unwrap();
        // Notification first, then the actual response.
        write_server_line(
            &mut reader,
            &json!({ "jsonrpc": "2.0", "method": "notifications/message", "params": {} }),
        )
        .await;
        write_server_line(
            &mut reader,
            &json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": { "content": [{ "type": "text", "text": "ok" }], "isError": false }
            }),
        )
        .await;
    });

    let result = run_call_session(mock, client, "echo", json!({}))
        .await
        .unwrap();
    assert_eq!(result.content[0]["text"], "ok");
}

#[tokio::test]
async fn tools_call_multiple_text_blocks_join_in_order() {
    let (client, server) = duplex_pair();
    let mock = tokio::spawn(async move {
        let mut reader = BufReader::new(server);
        let _ = read_server_line(&mut reader).await;
        write_server_line(&mut reader, &initialize_response(MCP_PROTOCOL_VERSION)).await;
        let _ = read_server_line(&mut reader).await;
        let call = read_server_line(&mut reader).await;
        let id = call["id"].as_i64().unwrap();
        write_server_line(
            &mut reader,
            &json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "content": [
                        { "type": "text", "text": "A" },
                        { "type": "text", "text": "B" }
                    ],
                    "isError": false
                }
            }),
        )
        .await;
    });

    let result = run_call_session(mock, client, "echo", json!({}))
        .await
        .unwrap();
    let tr = result.into_tool_result();
    assert_eq!(tr.content, "A\nB");
}

// ── ToolResult bridge tests (no protocol needed) ──

fn raw_call_result(value: serde_json::Value) -> McpCallResult {
    serde_json::from_value(value).unwrap()
}

#[test]
fn image_payload_is_not_leaked() {
    let result = raw_call_result(json!({
        "content": [
            { "type": "text", "text": "visible" },
            { "type": "image", "data": "VERY_SECRET_BASE64_PAYLOAD", "mimeType": "image/png" }
        ],
        "isError": false
    }));
    let tr = result.into_tool_result();
    assert!(tr.content.contains("visible"));
    assert!(tr.content.contains("[MCP image content omitted]"));
    assert!(!tr.content.contains("VERY_SECRET_BASE64_PAYLOAD"));
}

#[test]
fn audio_payload_is_not_leaked() {
    let result = raw_call_result(json!({
        "content": [
            { "type": "audio", "data": "VERY_SECRET_BASE64_PAYLOAD", "mimeType": "audio/wav" }
        ],
        "isError": false
    }));
    let tr = result.into_tool_result();
    assert!(tr.content.contains("[MCP audio content omitted]"));
    assert!(!tr.content.contains("VERY_SECRET_BASE64_PAYLOAD"));
}

#[test]
fn structured_content_is_included() {
    let result = raw_call_result(json!({
        "content": [],
        "structuredContent": { "answer": 42 }
    }));
    let tr = result.into_tool_result();
    assert!(tr.content.contains("[MCP structured content]"));
    assert!(tr.content.contains("\"answer\":42"));
}

#[test]
fn empty_success_result_gets_default_text() {
    let result = raw_call_result(json!({ "content": [], "isError": false }));
    let tr = result.into_tool_result();
    assert_eq!(tr.content, "MCP tool returned no content");
    assert!(tr.ok);
}

#[test]
fn empty_error_result_gets_default_text() {
    let result = raw_call_result(json!({ "content": [], "isError": true }));
    let tr = result.into_tool_result();
    assert_eq!(tr.content, "MCP tool failed without error content");
    assert!(!tr.ok);
}

#[test]
fn result_is_truncated_utf8_safe() {
    let long = "长".repeat(MAX_MCP_TOOL_RESULT_CHARS + 5000);
    let result = raw_call_result(json!({
        "content": [{ "type": "text", "text": long }],
        "isError": false
    }));
    let tr = result.into_tool_result();
    assert!(tr.content.chars().count() <= MAX_MCP_TOOL_RESULT_CHARS + 3);
    assert!(tr.content.is_char_boundary(tr.content.len()));
}

// ── argument / name validation ──

#[tokio::test]
async fn call_tool_rejects_non_object_arguments() {
    let (client, server) = duplex_pair();
    let mock = tokio::spawn(async move {
        // Server must never receive a tools/call for bad arguments.
        let mut reader = BufReader::new(server);
        let _ = read_server_line(&mut reader).await;
        write_server_line(&mut reader, &initialize_response(MCP_PROTOCOL_VERSION)).await;
        let notif = read_server_line(&mut reader).await;
        assert_eq!(notif["method"], "notifications/initialized");
        // Then the client returns InvalidToolCall and drops; EOF expected.
        tokio::time::sleep(Duration::from_millis(100)).await;
    });

    let mut session = McpSession::new(client);
    session.initialize().await.unwrap();
    session.send_initialized().await.unwrap();
    let err = session
        .call_tool("search", json!(["bad"]))
        .await
        .unwrap_err();
    assert!(matches!(err, McpError::InvalidToolCall(_)));
    drop(session);
    mock.abort();
}

#[tokio::test]
async fn call_tool_rejects_empty_remote_name() {
    let (client, server) = duplex_pair();
    let mock = tokio::spawn(async move {
        let mut reader = BufReader::new(server);
        let _ = read_server_line(&mut reader).await;
        write_server_line(&mut reader, &initialize_response(MCP_PROTOCOL_VERSION)).await;
        let _ = read_server_line(&mut reader).await;
        tokio::time::sleep(Duration::from_millis(100)).await;
    });

    let mut session = McpSession::new(client);
    session.initialize().await.unwrap();
    session.send_initialized().await.unwrap();
    let err = session.call_tool("", json!({})).await.unwrap_err();
    assert!(matches!(err, McpError::InvalidToolCall(_)));
    drop(session);
    mock.abort();
}

#[tokio::test]
async fn call_tool_missing_content_is_invalid_message() {
    let (client, server) = duplex_pair();
    let mock = tokio::spawn(async move {
        let mut reader = BufReader::new(server);
        let _ = read_server_line(&mut reader).await;
        write_server_line(&mut reader, &initialize_response(MCP_PROTOCOL_VERSION)).await;
        let _ = read_server_line(&mut reader).await;
        let call = read_server_line(&mut reader).await;
        let id = call["id"].as_i64().unwrap();
        write_server_line(
            &mut reader,
            &json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": { "isError": false }
            }),
        )
        .await;
    });

    let err = run_call_session(mock, client, "echo", json!({}))
        .await
        .unwrap_err();
    assert!(matches!(err, McpError::InvalidMessage(_)));
}

#[tokio::test]
async fn call_tool_non_object_structured_content_is_invalid() {
    let (client, server) = duplex_pair();
    let mock = tokio::spawn(async move {
        let mut reader = BufReader::new(server);
        let _ = read_server_line(&mut reader).await;
        write_server_line(&mut reader, &initialize_response(MCP_PROTOCOL_VERSION)).await;
        let _ = read_server_line(&mut reader).await;
        let call = read_server_line(&mut reader).await;
        let id = call["id"].as_i64().unwrap();
        write_server_line(
            &mut reader,
            &json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": { "content": [], "structuredContent": [] }
            }),
        )
        .await;
    });

    let err = run_call_session(mock, client, "echo", json!({}))
        .await
        .unwrap_err();
    assert!(matches!(err, McpError::InvalidMessage(_)));
}

#[test]
fn call_stdio_tool_rejects_non_object_arguments() {
    let server = McpServer {
        id: "id".to_string(),
        name: "n".to_string(),
        transport: "stdio".to_string(),
        command: Some("echo".to_string()),
        args: None,
        url: None,
        env: None,
        env_secret_refs: Default::default(),
        enabled: true,
        created_at: 0,
        updated_at: 0,
    };
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let err = rt
        .block_on(call_stdio_tool(&server, "search", json!(["bad"])))
        .unwrap_err();
    assert!(matches!(err, McpError::InvalidToolCall(_)));
}

