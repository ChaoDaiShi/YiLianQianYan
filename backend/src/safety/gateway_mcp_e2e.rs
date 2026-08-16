// ============================================================
// SecurityExecutionGateway → Managed MCP real E2E.
//
// Proves the full production execution chain against a real loopback
// Streamable-HTTP MCP mock:
//
//   SecurityExecutionGateway
//     → ToolRegistry
//       → McpToolAdapter (with_manager)
//         → McpRuntimeManager
//           → HttpTransport
//             → local axum MCP mock (AtomicUsize counters)
//
// The remote `tools/call` counter is the source of truth. The three
// invariants Phase 4 must prove:
//   - RequiresApproval (before approval) → remote call count == 0
//   - Approved execution                    → remote call count == 1
//   - Denied subject                        → remote call count == 0
//
// These tests never call `adapter.execute()` or `manager.call_tool()`
// directly — everything flows through `SecurityExecutionGateway::execute`
// / `execute_approved`, the real authorization boundary.
// ============================================================

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use axum::extract::State;
use axum::response::IntoResponse;
use axum::{routing::post, Json, Router};
use serde_json::{json, Value};

use crate::config::types::{SandboxConfig, SandboxProfile};
use crate::db::{Database, McpServer};
use crate::mcp_runtime::{McpRuntimeManager, McpRuntimeStatus, McpTransportConfig};
use crate::safety::execution_gateway::{
    SecurityExecutionGateway, SecurityExecutionOutcome, SecurityExecutionRequest,
};
use crate::safety::SecuritySubject;
use crate::tools::trait_def::{RiskLevel, Tool};
use crate::tools::{McpToolAdapter, ToolRegistry};

#[derive(Clone, Default)]
struct MockState {
    call_count: Arc<AtomicUsize>,
}

/// A minimal Streamable-HTTP MCP mock. Every `tools/call` bumps `call_count`.
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
            "serverInfo": { "name": "gateway-e2e-mock", "version": "1.0.0" },
            "capabilities": { "tools": true }
        }),
        "tools/list" => json!({
            "tools": [
                {
                    "name": "ping",
                    "description": "ping the mock",
                    "inputSchema": {
                        "type": "object",
                        "properties": { "message": { "type": "string" } }
                    }
                }
            ]
        }),
        "tools/call" => {
            state.call_count.fetch_add(1, Ordering::SeqCst);
            json!({
                "content": [ { "type": "text", "text": "pong from mock" } ],
                "isError": false,
                "resultType": "complete"
            })
        }
        _ => json!({ "error": { "code": -32601, "message": "method not found" } }),
    };
    let is_error = result.get("error").is_some();
    let response = if is_error {
        json!({ "jsonrpc": "2.0", "id": id, "error": result["error"] })
    } else {
        json!({ "jsonrpc": "2.0", "id": id, "result": result })
    };
    Json(response).into_response()
}

async fn start_mock() -> (String, MockState) {
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

fn temp_db() -> (Arc<Database>, std::path::PathBuf) {
    let path = std::env::temp_dir().join(format!(
        "yilian-gateway-mcp-e2e-{}.db",
        uuid::Uuid::new_v4()
    ));
    let db = Database::new(&path).unwrap();
    (Arc::new(db), path)
}

fn request(tool_name: &str) -> SecurityExecutionRequest {
    SecurityExecutionRequest {
        conversation_id: "conv-e2e".to_string(),
        tool_call_id: "call-e2e".to_string(),
        tool_name: tool_name.to_string(),
        arguments: json!({ "message": "hi" }),
        subject: SecuritySubject::local_user(),
    }
}

/// Build the full production chain (mock → manager → adapter → registry →
/// gateway) and return the gateway, the remote call counter, and the exposed
/// tool name that must be used in `SecurityExecutionRequest`.
async fn gateway_stack(db: Arc<Database>) -> (SecurityExecutionGateway, Arc<AtomicUsize>, String) {
    let (addr, state) = start_mock().await;

    // 1. Managed runtime with a real Streamable-HTTP server.
    let manager = Arc::new(McpRuntimeManager::new());
    let server_id = "gateway-mcp-e2e".to_string();
    manager.register_server(
        server_id.clone(),
        "Gateway E2E Mock".to_string(),
        McpTransportConfig::StreamableHttp {
            url: format!("{addr}/mcp"),
            headers_from_env: Default::default(),
        },
    );
    manager.refresh_server(&server_id).await.unwrap();

    // 2. The runtime catalog is Ready and discovered a real tool.
    let runtime = manager.get_server(&server_id).unwrap();
    assert_eq!(runtime.status, McpRuntimeStatus::Ready);
    assert!(!runtime.tools.is_empty());

    // 3. DB metadata only for adapter identity (naming / binding tag) —
    //    discovery and execution come from the runtime manager snapshot.
    let db_server = McpServer {
        id: server_id.clone(),
        name: "Gateway E2E Mock".to_string(),
        transport: "streamable_http".to_string(),
        command: None,
        args: None,
        url: Some(format!("{addr}/mcp")),
        env: None,
        env_secret_refs: Default::default(),
        enabled: true,
        created_at: 0,
        updated_at: 0,
    };
    let legacy_tool = crate::mcp::McpTool {
        name: runtime.tools[0].name.clone(),
        description: runtime.tools[0].description.clone(),
        input_schema: runtime.tools[0].input_schema.clone(),
    };

    // 4. Managed adapter → ToolRegistry (never a fake Tool).
    let adapter = McpToolAdapter::new(&db_server, &legacy_tool)
        .unwrap()
        .with_manager(Arc::clone(&manager));
    let exposed_name = adapter.name().to_string();
    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(adapter));

    // 5. Gateway with the registry + DB-backed role resolution.
    let gateway = SecurityExecutionGateway::with_sandbox_and_registry(
        SandboxConfig {
            profile: SandboxProfile::Open,
            writable_paths: vec![],
            denied_write_paths: vec![],
        },
        "workspace",
        Arc::new(registry),
    )
    .with_db(db);

    (gateway, state.call_count, exposed_name)
}

#[tokio::test]
async fn gateway_managed_mcp_requires_approval_without_remote_call() {
    let (db, db_path) = temp_db();
    let (gateway, call_count, exposed_name) = gateway_stack(Arc::clone(&db)).await;
    let req = request(&exposed_name);

    // Owner MCP invoke is High-risk → RequiresApproval, and the remote mock
    // must not have been hit.
    let outcome = gateway.execute(&req, RiskLevel::High).await.unwrap();
    assert!(
        matches!(outcome, SecurityExecutionOutcome::RequiresApproval { .. }),
        "expected RequiresApproval, got {outcome:?}"
    );
    assert_eq!(call_count.load(Ordering::SeqCst), 0);

    drop(gateway);
    drop(db);
    let _ = std::fs::remove_file(&db_path);
}

#[tokio::test]
async fn gateway_managed_mcp_approved_executes_exactly_once() {
    let (db, db_path) = temp_db();
    let (gateway, call_count, exposed_name) = gateway_stack(Arc::clone(&db)).await;
    let req = request(&exposed_name);

    // Pre-approval: no remote call.
    let outcome = gateway.execute(&req, RiskLevel::High).await.unwrap();
    assert!(
        matches!(outcome, SecurityExecutionOutcome::RequiresApproval { .. }),
        "expected RequiresApproval, got {outcome:?}"
    );
    assert_eq!(call_count.load(Ordering::SeqCst), 0);

    // Real approval-resume path: re-evaluates the subject's current role from
    // the DB, then dispatches through the full chain exactly once.
    let outcome = gateway
        .execute_approved(&req, RiskLevel::High)
        .await
        .unwrap();
    match outcome {
        SecurityExecutionOutcome::Executed { tool_result, .. } => {
            assert!(tool_result.ok);
            assert!(tool_result.content.contains("pong from mock"));
        }
        other => panic!("expected Executed, got {other:?}"),
    }
    assert_eq!(call_count.load(Ordering::SeqCst), 1);

    drop(gateway);
    drop(db);
    let _ = std::fs::remove_file(&db_path);
}

#[tokio::test]
async fn gateway_managed_mcp_denied_executes_zero_remote_calls() {
    let (db, db_path) = temp_db();
    // Restricted role → McpInvoke is Deny.
    db.conn()
        .execute(
            "UPDATE security_role_bindings SET role_key = 'restricted'
             WHERE subject_id = 'local-user' AND revoked_at IS NULL",
            [],
        )
        .unwrap();

    let (gateway, call_count, exposed_name) = gateway_stack(Arc::clone(&db)).await;
    let req = request(&exposed_name);

    let outcome = gateway.execute(&req, RiskLevel::High).await.unwrap();
    assert!(
        matches!(outcome, SecurityExecutionOutcome::Denied { .. }),
        "expected Denied, got {outcome:?}"
    );
    assert_eq!(call_count.load(Ordering::SeqCst), 0);

    drop(gateway);
    drop(db);
    let _ = std::fs::remove_file(&db_path);
}
