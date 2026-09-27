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
    let (server, calls, id, fixture) = fixture().await;
    let execution = start(&server, &id).await;
    assert_eq!(
        execution.status,
        crate::modules::task::NodeExecutionStatus::WaitingApproval
    );
    assert!(execution.approval_ref.is_some());
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    fixture.abort();
}
