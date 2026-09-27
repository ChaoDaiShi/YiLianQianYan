//! Real loopback MCP, production resolver and Task Harness (no fake executor).
use super::super::*;
use super::*;
use crate::integrations::mcp::McpTransportConfig;
use std::sync::atomic::{AtomicUsize, Ordering};

async fn fixture() -> (
    Arc<AppServer>,
    Arc<AtomicUsize>,
    String,
    tokio::task::JoinHandle<()>,
) {
    let server = server();
    let calls = Arc::new(AtomicUsize::new(0));
    server.config.write().sandbox.profile = crate::config::types::SandboxProfile::Open;
    let app=axum::Router::new().route("/mcp",axum::routing::post({let calls=calls.clone();move |Json(body):Json<Value>|{let calls=calls.clone();async move{
        let result=match body["method"].as_str().unwrap_or("") {
            "server/discover"=>json!({"supportedVersions":["2026-07-28","2025-11-25"],"serverInfo":{"name":"LOCAL MCP FIXTURE","version":"1"},"capabilities":{"tools":true}}),
            "tools/list"=>json!({"tools":[{"name":"echo","description":"LOCAL MCP FIXTURE echo","inputSchema":{"type":"object","properties":{"text":{"type":"string"}}}}]}),
            "tools/call"=>{calls.fetch_add(1,Ordering::SeqCst);json!({"content":[{"type":"text","text":body["params"]["arguments"].to_string()}],"isError":false,"resultType":"complete"})},
            _=>json!({}),
        };Json(json!({"jsonrpc":"2.0","id":body["id"],"result":result}))
    }}}));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/mcp", listener.local_addr().unwrap());
    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let row = crate::db::McpServer {
        id: "u3-local-fixture".into(),
        name: "LOCAL MCP FIXTURE".into(),
        transport: "streamable_http".into(),
        command: None,
        args: None,
        url: Some(url.clone()),
        env: None,
        env_secret_refs: Default::default(),
        enabled: true,
        created_at: 1,
        updated_at: 1,
    };
    server.db.create_mcp_server(&row).unwrap();
    server.mcp_runtime_manager.register_server(
        row.id.clone(),
        row.name,
        McpTransportConfig::StreamableHttp {
            url,
            headers_from_env: Default::default(),
        },
    );
    server
        .mcp_runtime_manager
        .refresh_server(&row.id)
        .await
        .unwrap();
    server.invalidate_capability_registry();
    let registry = server.build_capability_registry().await;
    let capability = registry
        .list()
        .into_iter()
        .find(|d| d.kind == crate::modules::capability::CapabilityKind::McpTool)
        .unwrap();
    (server, calls, capability.id.to_string(), handle)
}
async fn start(server: &Arc<AppServer>, capability: &str) -> crate::modules::task::NodeExecution {
    let graph = TaskGraphId::new("u3-graph").unwrap();
    let node = TaskNodeId::new("u3-node").unwrap();
    server.task_world.create_graph(graph.clone(),vec![TaskNode::new(node.clone(),TaskNodeKind::Work,"MCP echo",json!({"executor_ref":format!("capability://{capability}"),"capability_input":{"text":"frozen arguments"}})).unwrap()],vec![],1).unwrap();
    let resolver = executor_resolver(server, &graph, &node).await.unwrap();
    let execution = server
        .task_world
        .start_execution_with_resolver(&graph, &node, 1, resolver, 2)
        .unwrap();
    dispatch_execution(server.clone(), graph, execution.id)
        .await
        .unwrap()
}
#[tokio::test]
async fn mcp_canvas_harness_requires_approval_before_invocation() {
    let (server, calls, id, fixture_handle) = fixture().await;
    let execution = start(&server, &id).await;
    assert_eq!(
        execution.status,
        crate::modules::task::NodeExecutionStatus::WaitingApproval
    );
    assert!(execution.approval_ref.is_some());
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    fixture_handle.abort();
}
async fn decide(
    server: Arc<AppServer>,
    approval_id: &str,
    approve: bool,
) -> Result<String, (StatusCode, String)> {
    use crate::api::approvals::{approve_handler, reject_handler, ApprovalDecisionRequest};
    let body = Json(ApprovalDecisionRequest {
        conversation_id: None,
        attestation_id: None,
    });
    let response = if approve {
        approve_handler(State(server), Path(approval_id.into()), body).await
    } else {
        reject_handler(State(server), Path(approval_id.into()), body).await
    }?;
    Ok(String::from_utf8(
        to_bytes(response.into_response().into_body(), 128 * 1024)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap())
}
#[tokio::test]
async fn mcp_canvas_approval_executes_once_and_persists_validated_output() {
    let (server, calls, id, fixture_handle) = fixture().await;
    let execution = start(&server, &id).await;
    let approval = execution.approval_ref.as_ref().unwrap();
    let response = decide(server.clone(), approval, true).await.unwrap();
    assert!(response.contains("approval_resolved"));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let (_, done) = server.task_world.find_execution(&execution.id).unwrap();
    assert_eq!(
        done.status,
        crate::modules::task::NodeExecutionStatus::Succeeded
    );
    assert!(done.validation.unwrap().verified());
    assert!(done.output.unwrap()["content"]
        .as_str()
        .unwrap()
        .contains("frozen arguments"));
    assert!(decide(server.clone(), approval, true).await.is_err());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    fixture_handle.abort();
}
#[tokio::test]
async fn mcp_canvas_rejection_and_cancel_prevent_remote_calls() {
    for cancel in [false, true] {
        let (server, calls, id, fixture_handle) = fixture().await;
        let execution = start(&server, &id).await;
        let approval = execution.approval_ref.as_ref().unwrap();
        if cancel {
            server
                .task_world
                .cancel_execution(&execution.graph_id, &execution.id, 1, 3)
                .unwrap();
            assert!(decide(server.clone(), approval, true).await.is_err());
        } else {
            decide(server.clone(), approval, false).await.unwrap();
            let (_, done) = server.task_world.find_execution(&execution.id).unwrap();
            assert_eq!(done.failure_code.as_deref(), Some("approval_rejected"));
        }
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        fixture_handle.abort();
    }
}

#[tokio::test]
async fn mcp_canvas_concurrent_approval_and_current_policy() {
    let (server, calls, id, fixture_handle) = fixture().await;
    let registry = server.build_agent_tool_registry().await;
    let binding = super::super::super::application::capability_binding::resolve_binding(
        &server,
        &json!({"executor_ref":format!("capability://{id}"),"capability_input":{}}),
    )
    .await
    .unwrap();
    let tool = registry.get(&binding.tool_registry_name).unwrap();
    assert_eq!(tool.risk_level(), crate::tools::trait_def::RiskLevel::High);
    let descriptor = tool.security_descriptor(&json!({})).unwrap();
    let serialized = serde_json::to_value(&descriptor).unwrap().to_string();
    assert!(serialized.contains("mcp.invoke"));
    assert!(descriptor.resources.iter().any(|r|matches!(r,crate::safety::ResourceDescriptor::Mcp{server_id,tool_name} if server_id=="u3-local-fixture" && tool_name=="echo")));
    let execution = start(&server, &id).await;
    let approval = execution.approval_ref.as_ref().unwrap();
    let (a, b) = tokio::join!(
        decide(server.clone(), approval, true),
        decide(server.clone(), approval, true)
    );
    assert_ne!(a.is_ok(), b.is_ok());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let events = server
        .audit_recorder
        .query(&crate::db::SecurityAuditQuery {
            correlation_id: Some(execution.id.to_string()),
            ..Default::default()
        })
        .unwrap();
    let audit = serde_json::to_string(&events).unwrap();
    for event in [
        "policy_decided",
        "approval_resolved",
        "execution_started",
        "verification_finished",
    ] {
        assert!(audit.contains(event), "{event} missing");
    }
    fixture_handle.abort();

    let (server, calls, id, fixture_handle) = fixture().await;
    let execution = start(&server, &id).await;
    // MCP is RBAC-only in the existing grant evaluator. Revoke that permission
    // after approval creation; execute_approved must use the current binding.
    server.db.conn().execute("UPDATE security_role_bindings SET role_key='restricted' WHERE subject_id='local-user' AND revoked_at IS NULL",[]).unwrap();
    decide(
        server.clone(),
        execution.approval_ref.as_ref().unwrap(),
        true,
    )
    .await
    .unwrap();
    let (_, denied) = server.task_world.find_execution(&execution.id).unwrap();
    assert_eq!(denied.failure_code.as_deref(), Some("security_denied"));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    fixture_handle.abort();
}
