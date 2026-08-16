// ============================================================
// Real I/O integration tests — modern Streamable HTTP via an in-process
// loopback axum mock server (no external network).
// ============================================================

use axum::response::IntoResponse;
use axum::{routing::post, Json, Router};
use serde_json::{json, Value};

use super::transport::McpTransport;
use super::*;

async fn start_mock_http() -> String {
    let app = Router::new().route("/mcp", post(handler));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    addr
}

async fn handler(Json(body): Json<Value>) -> axum::response::Response {
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
