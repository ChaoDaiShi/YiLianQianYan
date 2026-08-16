// ============================================================
// Real I/O integration tests — modern Streamable HTTP via an in-process
// loopback axum mock server (no external network).
// ============================================================

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use axum::extract::State;
use axum::response::IntoResponse;
use axum::{routing::post, Json, Router};
use serde_json::{json, Value};

use super::transport::McpTransport;
use super::*;

#[derive(Clone, Default)]
struct MockState {
    read_count: Arc<AtomicUsize>,
}

async fn start_mock_http() -> String {
    let (addr, _) = start_mock_http_counted().await;
    addr
}

async fn start_mock_http_counted() -> (String, MockState) {
    let state = MockState::default();
    let app = Router::new()
        .route("/mcp", post(handler))
        .with_state(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (addr, state)
}

async fn handler(
    State(state): State<MockState>,
    Json(body): Json<Value>,
) -> axum::response::Response {
    let id = body.get("id").cloned().unwrap_or(json!(0));
    let method = body
        .get("method")
        .and_then(|m| m.as_str())
        .unwrap_or_default();
    let result = match method {
        "server/discover" => json!({
            "supportedVersions": ["2026-07-28", "2025-11-25"],
            "serverInfo": { "name": "mock", "version": "1.0.0" },
            "capabilities": { "tools": true, "resources": true, "prompts": true }
        }),
        "tools/list" => json!({
            "tools": [
                { "name": "echo", "description": "echo a message", "inputSchema": { "type": "object" } },
                { "name": "add", "description": "add two numbers", "inputSchema": { "type": "object" } }
            ]
        }),
        "tools/call" => json!({
            "content": [ { "type": "text", "text": "hello from mock" } ],
            "isError": false,
            "resultType": "complete"
        }),
        "resources/list" => json!({
            "resources": [
                { "uri": "file:///notes", "name": "notes", "mimeType": "text/plain" }
            ]
        }),
        "resources/read" => {
            state.read_count.fetch_add(1, Ordering::SeqCst);
            json!({
                "contents": [ { "uri": "file:///notes", "mimeType": "text/plain", "text": "hello resource" } ],
                "_meta": { "ttlMs": 60000 }
            })
        }
        "prompts/list" => json!({
            "prompts": [
                { "name": "greet", "description": "a greeting", "arguments": [{ "name": "name", "required": true }] }
            ]
        }),
        "prompts/get" => json!({
            "description": "greeting",
            "messages": [ { "role": "user", "content": { "type": "text", "text": "hi there" } } ]
        }),
        _ => json!({ "error": { "code": -32601, "message": "method not found" } }),
    };
    let is_error = result.get("error").is_some();
    let response = if is_error {
        json!({ "jsonrpc": "2.0", "id": id, "error": result["error"] })
    } else {
        json!({ "jsonrpc": "2.0", "id": id, "result": result })
    };
    axum::response::Json(response).into_response()
}

fn http_config(addr: &str) -> McpTransportConfig {
    McpTransportConfig::StreamableHttp {
        url: format!("{addr}/mcp"),
        headers_from_env: Default::default(),
    }
}

#[tokio::test]
async fn http_transport_tools_list_and_call_real_wire() {
    let addr = start_mock_http().await;
    let manager = McpRuntimeManager::new();
    manager.register_server(
        "http1".to_string(),
        "Mock HTTP".to_string(),
        http_config(&addr),
    );

    manager.refresh_server("http1").await.unwrap();
    let tools = manager.list_tools("http1").await.unwrap();
    assert_eq!(tools.len(), 2);
    assert_eq!(tools[0].name, "add"); // deterministic sort

    let cancel = tokio_util::sync::CancellationToken::new();
    let result = manager
        .call_tool("http1", "echo", json!({ "message": "hi" }), &cancel)
        .await
        .unwrap();
    assert!(!result.is_error);
    assert!(result.text.contains("hello from mock"));
}

#[tokio::test]
async fn http_transport_json_response_direct() {
    let addr = start_mock_http().await;
    let transport = HttpTransport::new(format!("{addr}/mcp"), Default::default()).unwrap();
    let mut params = json!({});
    crate::mcp_runtime::protocol::attach_request_metadata(&mut params, "0.8.0");
    let req = JsonRpcRequest::new(1, "tools/list", Some(params));
    let cancel = tokio_util::sync::CancellationToken::new();
    match transport.send(&req, &cancel).await.unwrap() {
        JsonRpcMessage::Success(s) => {
            assert!(s.result.get("tools").is_some());
        }
        other => panic!("expected success, got {other:?}"),
    }
}

#[tokio::test]
async fn http_resources_and_prompts_real_wire() {
    let addr = start_mock_http().await;
    let manager = McpRuntimeManager::new();
    manager.register_server(
        "http2".to_string(),
        "Mock HTTP 2".to_string(),
        http_config(&addr),
    );
    manager.refresh_server("http2").await.unwrap();

    // Version propagation: HTTP is always modern 2026.
    let runtime = manager.get_server("http2").unwrap();
    assert_eq!(runtime.protocol_version, McpProtocolVersion::V2026_07_28);

    let resources = manager.list_resources("http2").await.unwrap();
    assert_eq!(resources.len(), 1);
    assert_eq!(resources[0].name, "notes");

    let cancel = tokio_util::sync::CancellationToken::new();
    match manager
        .read_resource("http2", "file:///notes", &cancel)
        .await
        .unwrap()
    {
        McpOperationOutcome::Complete(contents) => {
            assert_eq!(contents.len(), 1);
            match &contents[0] {
                McpResourceContent::Text { text, .. } => assert!(text.contains("hello resource")),
                _ => panic!("expected text"),
            }
        }
        McpOperationOutcome::InputRequired(_) => panic!("unexpected input required"),
    }

    let prompts = manager.list_prompts("http2").await.unwrap();
    assert_eq!(prompts.len(), 1);
    assert_eq!(prompts[0].name, "greet");

    let cancel = tokio_util::sync::CancellationToken::new();
    match manager
        .get_prompt("http2", "greet", json!({}), &cancel)
        .await
        .unwrap()
    {
        McpOperationOutcome::Complete(result) => assert_eq!(result.messages.len(), 1),
        McpOperationOutcome::InputRequired(_) => panic!("unexpected input required"),
    }
}

#[tokio::test]
async fn resource_read_cache_avoids_second_wire() {
    let (addr, state) = start_mock_http_counted().await;
    let manager = McpRuntimeManager::new();
    manager.register_server("http3".to_string(), "Cache".to_string(), http_config(&addr));
    manager.refresh_server("http3").await.unwrap();

    let cancel = tokio_util::sync::CancellationToken::new();
    let _ = manager
        .read_resource("http3", "file:///notes", &cancel)
        .await
        .unwrap();
    assert_eq!(state.read_count.load(Ordering::SeqCst), 1);

    // Second read is served from cache — no new wire call.
    let _ = manager
        .read_resource("http3", "file:///notes", &cancel)
        .await
        .unwrap();
    assert_eq!(state.read_count.load(Ordering::SeqCst), 1);

    // Manual refresh bypasses cache.
    manager.refresh_server("http3").await.unwrap();
    let _ = manager
        .read_resource("http3", "file:///notes", &cancel)
        .await
        .unwrap();
    assert_eq!(state.read_count.load(Ordering::SeqCst), 2);
}
