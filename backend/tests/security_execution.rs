use async_trait::async_trait;
use axum::{
    body::{to_bytes, Body},
    extract::State,
    http::{header::CONTENT_TYPE, Method, Request, StatusCode},
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use parking_lot::Mutex;
use serde_json::{json, Value};
use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};
use tokio::{net::TcpListener, task::JoinHandle};
use tower::ServiceExt;
use yilian_backend::{
    agent::verifier::{VerificationResult, Verifier},
    api::build_router,
    config::types::{SandboxConfig, SandboxProfile},
    db::{Database, SecurityAuditEvent, SecurityAuditQuery},
    safety::{
        execution_gateway::SecurityExecutionOutcome, AuditEventInput, AuditEventType,
        AuditRecorder, BuiltInRole, ControlSession, PendingApproval, SecurityExecutionGateway,
        SecurityExecutionRequest, CONTROL_SESSION_HEADER,
    },
    server::AppServer,
    tools::{RiskLevel, Tool, ToolRegistry, ToolResult},
};

struct TestWorkspace {
    root: PathBuf,
    db_path: PathBuf,
}

impl TestWorkspace {
    fn new(label: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "yilian-security-execution-{label}-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        Self {
            db_path: root.join("security.db"),
            root,
        }
    }
}

impl Drop for TestWorkspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

struct CountingTool {
    name: &'static str,
    executions: Arc<AtomicUsize>,
    arguments: Arc<Mutex<Vec<Value>>>,
    result: ToolResult,
}

#[async_trait]
impl Tool for CountingTool {
    fn name(&self) -> &str {
        self.name
    }

    fn description(&self) -> &str {
        "trusted execution regression tool"
    }

    fn parameters(&self) -> Value {
        json!({"type": "object"})
    }

    async fn execute(&self, args: Value) -> ToolResult {
        self.executions.fetch_add(1, Ordering::SeqCst);
        self.arguments.lock().push(args);
        self.result.clone()
    }
}

struct CountingVerifier {
    invocations: Arc<AtomicUsize>,
    result: VerificationResult,
}

#[async_trait]
impl Verifier for CountingVerifier {
    async fn verify(
        &self,
        _tool_name: &str,
        _args: &Value,
        _tool_result: &ToolResult,
    ) -> VerificationResult {
        self.invocations.fetch_add(1, Ordering::SeqCst);
        self.result.clone()
    }
}

#[derive(Clone)]
struct MockLlmState {
    responses: Arc<tokio::sync::Mutex<VecDeque<String>>>,
    requests: Arc<tokio::sync::Mutex<Vec<Value>>>,
}

struct MockLlm {
    base_url: String,
    requests: Arc<tokio::sync::Mutex<Vec<Value>>>,
    task: JoinHandle<()>,
}

impl Drop for MockLlm {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn mock_llm_handler(
    State(state): State<MockLlmState>,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    state.requests.lock().await.push(body);
    let response = state
        .responses
        .lock()
        .await
        .pop_front()
        .expect("mock LLM response queue exhausted");
    ([(CONTENT_TYPE, "text/event-stream")], response)
}

async fn start_mock_llm(responses: Vec<String>) -> MockLlm {
    let state = MockLlmState {
        responses: Arc::new(tokio::sync::Mutex::new(responses.into())),
        requests: Arc::new(tokio::sync::Mutex::new(Vec::new())),
    };
    let requests = Arc::clone(&state.requests);
    let app = Router::new()
        .route("/chat/completions", post(mock_llm_handler))
        .with_state(state);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    MockLlm {
        base_url: format!("http://{address}"),
        requests,
        task,
    }
}

fn registry_with_tool(
    name: &'static str,
    result: ToolResult,
) -> (Arc<ToolRegistry>, Arc<AtomicUsize>, Arc<Mutex<Vec<Value>>>) {
    let executions = Arc::new(AtomicUsize::new(0));
    let arguments = Arc::new(Mutex::new(Vec::new()));
    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(CountingTool {
        name,
        executions: Arc::clone(&executions),
        arguments: Arc::clone(&arguments),
        result,
    }));
    (Arc::new(registry), executions, arguments)
}

fn sandbox_config(
    profile: SandboxProfile,
    writable_paths: &[&str],
    denied_write_paths: &[&str],
) -> SandboxConfig {
    SandboxConfig {
        profile,
        writable_paths: writable_paths
            .iter()
            .map(|path| (*path).to_string())
            .collect(),
        denied_write_paths: denied_write_paths
            .iter()
            .map(|path| (*path).to_string())
            .collect(),
    }
}

fn audit_events(recorder: &AuditRecorder, tool_call_id: &str) -> Vec<SecurityAuditEvent> {
    recorder
        .query(&SecurityAuditQuery {
            correlation_id: Some(tool_call_id.to_string()),
            ..Default::default()
        })
        .unwrap()
}

fn event_count(events: &[SecurityAuditEvent], event_type: &str) -> usize {
    events
        .iter()
        .filter(|event| event.event_type == event_type)
        .count()
}

fn assert_event_counts(
    events: &[SecurityAuditEvent],
    expected_once: &[&str],
    expected_absent: &[&str],
) {
    for event_type in expected_once {
        assert_eq!(
            event_count(events, event_type),
            1,
            "expected exactly one {event_type}, got events: {:?}",
            events
                .iter()
                .map(|event| event.event_type.as_str())
                .collect::<Vec<_>>()
        );
    }
    for event_type in expected_absent {
        assert_eq!(
            event_count(events, event_type),
            0,
            "expected no {event_type}"
        );
    }
}

fn sse_tool_call(tool_call_id: &str, tool_name: &str, arguments: Value) -> String {
    let arguments = serde_json::to_string(&arguments).unwrap();
    let chunk = json!({
        "id": "trusted-execution-test",
        "choices": [{
            "index": 0,
            "delta": {
                "tool_calls": [{
                    "index": 0,
                    "id": tool_call_id,
                    "type": "function",
                    "function": {
                        "name": tool_name,
                        "arguments": arguments
                    }
                }]
            },
            "finish_reason": "tool_calls"
        }]
    });
    format!("data: {chunk}\n\ndata: [DONE]\n\n")
}

fn sse_text(text: &str) -> String {
    let chunk = json!({
        "id": "trusted-execution-test",
        "choices": [{
            "index": 0,
            "delta": {"content": text},
            "finish_reason": "stop"
        }]
    });
    format!("data: {chunk}\n\ndata: [DONE]\n\n")
}

fn test_server(
    workspace: &TestWorkspace,
    mock_llm: &MockLlm,
    registry: Arc<ToolRegistry>,
    sandbox: SandboxConfig,
) -> (Arc<AppServer>, String) {
    let token = "t".repeat(64);
    let mut server = AppServer::new_with_control_session(
        &workspace.db_path,
        workspace.root.to_string_lossy().as_ref(),
        ControlSession::new(token.clone()).unwrap(),
    )
    .unwrap();
    server.tool_registry = registry;
    {
        let mut config = server.config.write();
        config.model.base_url = mock_llm.base_url.clone();
        config.model.api_key = "test-key".to_string();
        config.model.api_key_env.clear();
        config.model.invoke_timeout_ms = 5_000;
        config.sandbox = sandbox;
    }
    (Arc::new(server), token)
}

async fn protected_request(
    server: Arc<AppServer>,
    token: &str,
    method: Method,
    uri: &str,
    body: Value,
) -> (StatusCode, String) {
    let response = build_router(server)
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .header(CONTROL_SESSION_HEADER, token)
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = to_bytes(response.into_body(), 4 * 1024 * 1024)
        .await
        .unwrap();
    (status, String::from_utf8(body.to_vec()).unwrap())
}

fn create_pending(
    server: &AppServer,
    tool_call_id: &str,
    tool_name: &str,
    arguments: Value,
    risk_level: RiskLevel,
) -> PendingApproval {
    let conversation = server.db.create_conversation("approval test").unwrap();
    server.approval_store.create(
        conversation.id,
        tool_call_id.to_string(),
        tool_name.to_string(),
        arguments,
        risk_level,
        "trusted execution approval".to_string(),
    )
}

#[tokio::test]
async fn trusted_execution_harness_uses_isolated_workspace_and_audit_store() {
    let workspace = TestWorkspace::new("harness");
    assert!(workspace.root.is_absolute());

    let database = Database::new(&workspace.db_path).unwrap();
    let recorder = AuditRecorder::new(database.clone_connection());
    recorder
        .record(AuditEventInput {
            event_type: AuditEventType::PolicyDecided,
            correlation_id: "harness-call".to_string(),
            request_id: "harness-call".to_string(),
            subject_id: "local-user".to_string(),
            role_key: "owner".to_string(),
            decision_status: Some("allow".to_string()),
            details: json!({"phase": "harness"}),
            ..Default::default()
        })
        .unwrap();

    let events = audit_events(&recorder, "harness-call");
    assert_event_counts(&events, &["policy_decided"], &["execution_started"]);
}

#[tokio::test]
async fn agent_allow_runs_tool_once_and_records_complete_audit_chain() {
    let workspace = TestWorkspace::new("agent-allow");
    let tool_call_id = "agent-allow-call";
    let mock_llm = start_mock_llm(vec![
        sse_tool_call(tool_call_id, "read_file", json!({"path": "README.md"})),
        sse_text("检查完成"),
    ])
    .await;
    let (registry, executions, _arguments) =
        registry_with_tool("read_file", ToolResult::success("README contents"));
    let (server, token) = test_server(
        &workspace,
        &mock_llm,
        registry,
        sandbox_config(SandboxProfile::WorkspaceWrite, &[], &[]),
    );

    let (status, _body) = protected_request(
        Arc::clone(&server),
        &token,
        Method::POST,
        "/api/chat",
        json!({"message": "检查 README"}),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(executions.load(Ordering::SeqCst), 1);
    assert_eq!(mock_llm.requests.lock().await.len(), 2);
    let events = audit_events(&server.audit_recorder, tool_call_id);
    assert_event_counts(
        &events,
        &[
            "policy_decided",
            "execution_started",
            "execution_finished",
            "verification_finished",
        ],
        &["approval_requested"],
    );
    assert!(events.iter().any(|event| {
        event.event_type == "policy_decided" && event.decision_status.as_deref() == Some("allow")
    }));
}

#[tokio::test]
async fn agent_high_risk_tool_requires_approval_without_execution() {
    let workspace = TestWorkspace::new("agent-approval");
    let tool_call_id = "agent-approval-call";
    let mock_llm = start_mock_llm(vec![sse_tool_call(
        tool_call_id,
        "bash",
        json!({"command": "echo held"}),
    )])
    .await;
    let (registry, executions, _arguments) =
        registry_with_tool("bash", ToolResult::success("must not run"));
    let (server, token) = test_server(
        &workspace,
        &mock_llm,
        registry,
        sandbox_config(SandboxProfile::Open, &[], &[]),
    );

    let (status, _body) = protected_request(
        Arc::clone(&server),
        &token,
        Method::POST,
        "/api/chat",
        json!({"message": "运行高风险工具"}),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(executions.load(Ordering::SeqCst), 0);
    assert_eq!(mock_llm.requests.lock().await.len(), 1);
    let pending = server.approval_store.list_pending();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].tool_call_id, tool_call_id);
    let events = audit_events(&server.audit_recorder, tool_call_id);
    assert_event_counts(
        &events,
        &["policy_decided", "approval_requested"],
        &[
            "execution_started",
            "execution_finished",
            "verification_finished",
        ],
    );
    assert!(events.iter().any(|event| {
        event.event_type == "policy_decided"
            && event.decision_status.as_deref() == Some("require_approval")
    }));
}

#[tokio::test]
async fn agent_sandbox_deny_returns_failure_context_without_execution() {
    let workspace = TestWorkspace::new("agent-deny");
    let tool_call_id = "agent-deny-call";
    let mock_llm = start_mock_llm(vec![
        sse_tool_call(
            tool_call_id,
            "write_file",
            json!({"path": "blocked.txt", "content": "blocked"}),
        ),
        sse_text("已改用安全方案"),
    ])
    .await;
    let (registry, executions, _arguments) =
        registry_with_tool("write_file", ToolResult::success("must not run"));
    let (server, token) = test_server(
        &workspace,
        &mock_llm,
        registry,
        sandbox_config(SandboxProfile::ReadOnly, &[], &[]),
    );

    let (status, _body) = protected_request(
        Arc::clone(&server),
        &token,
        Method::POST,
        "/api/chat",
        json!({"message": "写入文件"}),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(executions.load(Ordering::SeqCst), 0);
    let requests = mock_llm.requests.lock().await;
    assert_eq!(requests.len(), 2);
    assert!(requests[1]["messages"]
        .as_array()
        .unwrap()
        .iter()
        .any(|message| {
            message["role"] == "tool"
                && message["content"]
                    .as_str()
                    .unwrap_or_default()
                    .contains("denied")
        }));
    drop(requests);

    let events = audit_events(&server.audit_recorder, tool_call_id);
    assert_event_counts(
        &events,
        &["policy_decided"],
        &[
            "approval_requested",
            "execution_started",
            "execution_finished",
            "verification_finished",
        ],
    );
    assert!(events.iter().any(|event| {
        event.event_type == "policy_decided" && event.decision_status.as_deref() == Some("deny")
    }));
}

#[tokio::test]
async fn gateway_allow_executes_tool_and_verifier_exactly_once() {
    let workspace = TestWorkspace::new("gateway-allow");
    let database = Database::new(&workspace.db_path).unwrap();
    let recorder = Arc::new(AuditRecorder::new(database.clone_connection()));
    let (registry, executions, _arguments) =
        registry_with_tool("read_file", ToolResult::success("README contents"));
    let verifier_invocations = Arc::new(AtomicUsize::new(0));
    let verifier = Arc::new(CountingVerifier {
        invocations: Arc::clone(&verifier_invocations),
        result: VerificationResult::success("verified", None),
    });
    let gateway = SecurityExecutionGateway::with_sandbox_registry_verifier_and_audit(
        sandbox_config(SandboxProfile::Open, &[], &[]),
        &workspace.root,
        registry,
        verifier,
        Arc::clone(&recorder),
    );
    let request = SecurityExecutionRequest {
        conversation_id: "gateway-conversation".to_string(),
        tool_call_id: "gateway-allow-call".to_string(),
        tool_name: "read_file".to_string(),
        arguments: json!({"path": "README.md"}),
    };

    let outcome = gateway
        .execute(&request, BuiltInRole::Owner, RiskLevel::Low)
        .await
        .unwrap();

    assert!(matches!(
        outcome,
        SecurityExecutionOutcome::Executed {
            verification: VerificationResult { success: true, .. },
            ..
        }
    ));
    assert_eq!(executions.load(Ordering::SeqCst), 1);
    assert_eq!(verifier_invocations.load(Ordering::SeqCst), 1);
    let events = audit_events(&recorder, &request.tool_call_id);
    assert_event_counts(
        &events,
        &[
            "policy_decided",
            "execution_started",
            "execution_finished",
            "verification_finished",
        ],
        &["approval_requested"],
    );
}

#[tokio::test]
async fn gateway_enforces_workspace_custom_and_denied_write_paths() {
    struct Case {
        label: &'static str,
        profile: SandboxProfile,
        writable_paths: &'static [&'static str],
        denied_write_paths: &'static [&'static str],
        path: &'static str,
        allowed: bool,
    }

    let cases = [
        Case {
            label: "workspace-inside",
            profile: SandboxProfile::WorkspaceWrite,
            writable_paths: &[],
            denied_write_paths: &[],
            path: "inside.txt",
            allowed: true,
        },
        Case {
            label: "workspace-escape",
            profile: SandboxProfile::WorkspaceWrite,
            writable_paths: &[],
            denied_write_paths: &[],
            path: "../../outside.txt",
            allowed: false,
        },
        Case {
            label: "custom-inside",
            profile: SandboxProfile::Custom,
            writable_paths: &["allowed"],
            denied_write_paths: &[],
            path: "allowed/file.txt",
            allowed: true,
        },
        Case {
            label: "custom-outside",
            profile: SandboxProfile::Custom,
            writable_paths: &["allowed"],
            denied_write_paths: &[],
            path: "other/file.txt",
            allowed: false,
        },
        Case {
            label: "deny-overrides-allow",
            profile: SandboxProfile::Custom,
            writable_paths: &["allowed"],
            denied_write_paths: &["allowed/blocked"],
            path: "allowed/blocked/file.txt",
            allowed: false,
        },
    ];

    let workspace = TestWorkspace::new("gateway-sandbox");
    let database = Database::new(&workspace.db_path).unwrap();
    let recorder = Arc::new(AuditRecorder::new(database.clone_connection()));
    let (registry, executions, _arguments) =
        registry_with_tool("write_file", ToolResult::success("written"));
    let verifier_invocations = Arc::new(AtomicUsize::new(0));

    for case in cases {
        let verifier = Arc::new(CountingVerifier {
            invocations: Arc::clone(&verifier_invocations),
            result: VerificationResult::success("verified", None),
        });
        let gateway = SecurityExecutionGateway::with_sandbox_registry_verifier_and_audit(
            sandbox_config(case.profile, case.writable_paths, case.denied_write_paths),
            &workspace.root,
            Arc::clone(&registry),
            verifier,
            Arc::clone(&recorder),
        );
        let request = SecurityExecutionRequest {
            conversation_id: "gateway-sandbox-conversation".to_string(),
            tool_call_id: format!("gateway-sandbox-{}", case.label),
            tool_name: "write_file".to_string(),
            arguments: json!({"path": case.path, "content": "test"}),
        };
        let executions_before = executions.load(Ordering::SeqCst);
        let verifications_before = verifier_invocations.load(Ordering::SeqCst);

        let outcome = gateway
            .execute(&request, BuiltInRole::Owner, RiskLevel::Low)
            .await
            .unwrap();

        let events = audit_events(&recorder, &request.tool_call_id);
        if case.allowed {
            assert!(
                matches!(outcome, SecurityExecutionOutcome::Executed { .. }),
                "{} should execute",
                case.label
            );
            assert_eq!(executions.load(Ordering::SeqCst), executions_before + 1);
            assert_eq!(
                verifier_invocations.load(Ordering::SeqCst),
                verifications_before + 1
            );
            assert_event_counts(
                &events,
                &[
                    "policy_decided",
                    "execution_started",
                    "execution_finished",
                    "verification_finished",
                ],
                &[],
            );
        } else {
            assert!(
                matches!(outcome, SecurityExecutionOutcome::Denied { .. }),
                "{} should be denied",
                case.label
            );
            assert_eq!(executions.load(Ordering::SeqCst), executions_before);
            assert_eq!(
                verifier_invocations.load(Ordering::SeqCst),
                verifications_before
            );
            assert_event_counts(
                &events,
                &["policy_decided"],
                &["execution_started", "verification_finished"],
            );
            assert!(events.iter().any(|event| {
                event.event_type == "policy_decided"
                    && event.decision_status.as_deref() == Some("deny")
            }));
        }
    }
}

#[tokio::test]
async fn approval_approve_executes_original_tool_once_and_cannot_be_replayed() {
    let workspace = TestWorkspace::new("approval-approve");
    let mock_llm = start_mock_llm(vec![sse_text("审批后的任务已继续")]).await;
    let (registry, executions, arguments) =
        registry_with_tool("bash", ToolResult::success("executed"));
    let (server, token) = test_server(
        &workspace,
        &mock_llm,
        registry,
        sandbox_config(SandboxProfile::Open, &[], &[]),
    );
    let original_arguments = json!({"command": "echo exact-original"});
    let pending = create_pending(
        &server,
        "approval-approve-call",
        "bash",
        original_arguments.clone(),
        RiskLevel::High,
    );
    let uri = format!("/api/approvals/{}/approve", pending.approval_id);

    let (first_status, _first_body) = protected_request(
        Arc::clone(&server),
        &token,
        Method::POST,
        &uri,
        json!({"conversation_id": pending.conversation_id}),
    )
    .await;

    assert_eq!(first_status, StatusCode::OK);
    assert_eq!(executions.load(Ordering::SeqCst), 1);
    assert_eq!(arguments.lock().as_slice(), &[original_arguments]);
    assert_eq!(mock_llm.requests.lock().await.len(), 1);
    let first_events = audit_events(&server.audit_recorder, &pending.tool_call_id);
    assert_event_counts(
        &first_events,
        &[
            "approval_resolved",
            "policy_decided",
            "execution_started",
            "execution_finished",
            "verification_finished",
        ],
        &[],
    );
    assert!(first_events.iter().any(|event| {
        event.event_type == "approval_resolved"
            && event.decision_status.as_deref() == Some("approved")
    }));

    let (second_status, _second_body) = protected_request(
        Arc::clone(&server),
        &token,
        Method::POST,
        &uri,
        json!({"conversation_id": pending.conversation_id}),
    )
    .await;

    assert_eq!(second_status, StatusCode::CONFLICT);
    assert_eq!(executions.load(Ordering::SeqCst), 1);
    let second_events = audit_events(&server.audit_recorder, &pending.tool_call_id);
    assert_eq!(event_count(&second_events, "approval_resolved"), 1);
    assert_eq!(event_count(&second_events, "execution_started"), 1);
}

#[tokio::test]
async fn approval_reject_records_resolution_without_execution() {
    let workspace = TestWorkspace::new("approval-reject");
    let mock_llm = start_mock_llm(vec![sse_text("已根据拒绝结果调整")]).await;
    let (registry, executions, _arguments) =
        registry_with_tool("bash", ToolResult::success("must not run"));
    let (server, token) = test_server(
        &workspace,
        &mock_llm,
        registry,
        sandbox_config(SandboxProfile::Open, &[], &[]),
    );
    let pending = create_pending(
        &server,
        "approval-reject-call",
        "bash",
        json!({"command": "echo rejected"}),
        RiskLevel::High,
    );
    let uri = format!("/api/approvals/{}/reject", pending.approval_id);

    let (status, _body) = protected_request(
        Arc::clone(&server),
        &token,
        Method::POST,
        &uri,
        json!({"conversation_id": pending.conversation_id}),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(executions.load(Ordering::SeqCst), 0);
    assert_eq!(mock_llm.requests.lock().await.len(), 1);
    let events = audit_events(&server.audit_recorder, &pending.tool_call_id);
    assert_event_counts(&events, &["approval_resolved"], &["execution_started"]);
    assert!(events.iter().any(|event| {
        event.event_type == "approval_resolved"
            && event.decision_status.as_deref() == Some("rejected")
    }));
}

#[tokio::test]
async fn approval_cancel_records_resolution_without_execution() {
    let workspace = TestWorkspace::new("approval-cancel");
    let mock_llm = start_mock_llm(Vec::new()).await;
    let (registry, executions, _arguments) =
        registry_with_tool("bash", ToolResult::success("must not run"));
    let (server, token) = test_server(
        &workspace,
        &mock_llm,
        registry,
        sandbox_config(SandboxProfile::Open, &[], &[]),
    );
    let pending = create_pending(
        &server,
        "approval-cancel-call",
        "bash",
        json!({"command": "echo cancelled"}),
        RiskLevel::High,
    );
    let uri = format!("/api/approvals/{}/cancel", pending.approval_id);

    let (status, body) = protected_request(
        Arc::clone(&server),
        &token,
        Method::POST,
        &uri,
        json!({"conversation_id": pending.conversation_id}),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let body: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(body["ok"], true);
    assert_eq!(body["status"], "cancelled");
    assert_eq!(executions.load(Ordering::SeqCst), 0);
    assert_eq!(mock_llm.requests.lock().await.len(), 0);
    let events = audit_events(&server.audit_recorder, &pending.tool_call_id);
    assert_event_counts(&events, &["approval_resolved"], &["execution_started"]);
    assert!(events.iter().any(|event| {
        event.event_type == "approval_resolved"
            && event.decision_status.as_deref() == Some("cancelled")
    }));
}

#[tokio::test]
async fn agent_utf8_message_survives_logging_preview_and_persistence() {
    let workspace = TestWorkspace::new("agent-utf8");
    let mock_llm = start_mock_llm(vec![sse_text("中文内容检查完成😀")]).await;
    let (server, token) = test_server(
        &workspace,
        &mock_llm,
        Arc::new(ToolRegistry::new()),
        sandbox_config(SandboxProfile::WorkspaceWrite, &[], &[]),
    );
    let message = "请检查这个中文项目文件😀";

    let (status, body) = protected_request(
        Arc::clone(&server),
        &token,
        Method::POST,
        "/api/chat",
        json!({"message": message}),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert!(!body.contains('\u{FFFD}'));
    let requests = mock_llm.requests.lock().await;
    assert_eq!(requests.len(), 1);
    let matching_messages = requests[0]["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|entry| entry["role"] == "user" && entry["content"] == message)
        .count();
    assert_eq!(matching_messages, 1);
    drop(requests);

    let conversations = server.db.list_conversations().unwrap();
    assert_eq!(conversations.len(), 1);
    assert_eq!(conversations[0].title, message);
    assert!(!conversations[0].title.contains('\u{FFFD}'));
    let conversation = server.db.get_conversation(&conversations[0].id).unwrap();
    assert_eq!(
        conversation
            .messages
            .iter()
            .filter(|entry| entry.role == "user" && entry.content == message)
            .count(),
        1
    );
    assert!(conversation
        .messages
        .iter()
        .all(|entry| !entry.content.contains('\u{FFFD}')));
    let logs = server.log_buffer.recent(100);
    assert!(logs.iter().any(|entry| entry.message.contains(message)));
    assert!(logs.iter().all(|entry| !entry.message.contains('\u{FFFD}')));
}
