use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use tower::ServiceExt;
use yilian_backend::{
    api,
    db::McpServer,
    safety::{
        ControlSession, PermissionId, ResourceScope, SideEffectKind, CONTROL_SESSION_HEADER,
        POLICY_VERSION,
    },
    tools::RiskLevel,
    AppServer,
};

struct TempRuntime {
    root: PathBuf,
    workspace: PathBuf,
    database: PathBuf,
}

impl TempRuntime {
    fn new() -> Self {
        let root =
            std::env::temp_dir().join(format!("yilian-runtime-smoke-{}", uuid::Uuid::new_v4()));
        let workspace = root.join("workspace");
        let agent_dir = workspace.join(".agents/agents/researcher");
        std::fs::create_dir_all(&agent_dir).unwrap();
        std::fs::write(
            agent_dir.join("AGENT.md"),
            "---\nname: researcher\ndescription: Local smoke researcher\ntools: [read_file, grep]\n---\n\nUse only the allowed local read tools.",
        )
        .unwrap();

        Self {
            database: root.join("runtime-smoke.db"),
            root,
            workspace,
        }
    }

    fn workspace_str(&self) -> &str {
        self.workspace.to_str().unwrap()
    }
}

impl Drop for TempRuntime {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn disabled_stdio_server() -> McpServer {
    McpServer {
        id: uuid::Uuid::new_v4().to_string(),
        name: "disabled-local-smoke".to_string(),
        transport: "stdio".to_string(),
        command: Some("never-started".to_string()),
        args: Some(Vec::new()),
        url: None,
        env: None,
        enabled: false,
        created_at: 1,
        updated_at: 1,
    }
}

#[tokio::test]
async fn v03_runtime_initializes_and_exposes_core_local_paths() {
    let temp = TempRuntime::new();
    let token = "r".repeat(64);
    let control_session = ControlSession::new(token.clone()).unwrap();
    let server = Arc::new(
        AppServer::new_with_control_session(
            Path::new(&temp.database),
            temp.workspace_str(),
            control_session,
        )
        .unwrap(),
    );

    for tool_name in ["read_file", "write_file", "grep", "bash"] {
        assert!(
            server.tool_registry.get(tool_name).is_some(),
            "missing builtin tool: {tool_name}",
        );
    }
    assert_eq!(POLICY_VERSION, "security-rbac-v3");

    assert!(server.db.list_mcp_servers().unwrap().is_empty());
    let mcp = disabled_stdio_server();
    server.db.create_mcp_server(&mcp).unwrap();
    let persisted_mcp = server.db.list_mcp_servers().unwrap();
    assert_eq!(persisted_mcp.len(), 1);
    assert_eq!(persisted_mcp[0].transport, "stdio");
    assert!(!persisted_mcp[0].enabled);

    let discovered = server
        .subagents
        .iter()
        .find(|subagent| subagent.name == "researcher")
        .expect("valid AGENT.md should be discovered");
    assert_eq!(discovered.allowed_tools, ["read_file", "grep"]);
    assert!(!discovered.instructions.is_empty());
    assert!(
        serde_json::to_value(discovered)
            .unwrap()
            .get("instructions")
            .is_none(),
        "private subagent instructions must not be serialized",
    );

    let runtime_registry = server.build_agent_tool_registry().await;
    let subagent_tool = runtime_registry
        .get("subagent_researcher")
        .expect("parent runtime should expose the discovered subagent");
    let descriptor = subagent_tool
        .security_descriptor(&serde_json::json!({"task": "summarize local files"}))
        .unwrap();
    assert_eq!(descriptor.default_risk, RiskLevel::High);
    assert_eq!(
        descriptor.requested_permissions,
        vec![PermissionId::AgentDelegate.in_scope(ResourceScope::Subagent)],
    );
    assert_eq!(
        descriptor.side_effects,
        vec![SideEffectKind::AgentDelegation]
    );

    let app = api::build_router(Arc::clone(&server));
    let memory_response = app
        .oneshot(
            Request::builder()
                .uri("/api/memories")
                .header(CONTROL_SESSION_HEADER, token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(memory_response.status(), StatusCode::OK);
}
