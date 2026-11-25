use super::*;
use crate::tools::trait_def::{RiskLevel, Tool};
use async_trait::async_trait;

struct NoopTool {
    name: &'static str,
}

#[async_trait]
impl Tool for NoopTool {
    fn name(&self) -> &str {
        self.name
    }

    fn description(&self) -> &str {
        "test tool"
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({"type": "object"})
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Low
    }

    async fn execute(&self, _args: serde_json::Value) -> ToolResult {
        ToolResult::success("ok")
    }
}

fn source_registry() -> ToolRegistry {
    let mut reg = ToolRegistry::new();
    reg.register(Arc::new(NoopTool { name: "read_file" }));
    reg.register(Arc::new(NoopTool { name: "grep" }));
    reg.register(Arc::new(NoopTool { name: "bash" }));
    reg.register(Arc::new(NoopTool {
        name: "subagent_other",
    }));
    reg
}

#[tokio::test]
async fn integration_child_write_uses_live_scoped_rule_and_records_delegator() {
    use crate::config::types::{PermissionMode, SandboxProfile};
    use crate::db::access_rules::{AccessMode, AccessRule};
    use crate::db::SecurityAuditQuery;
    use crate::integrations::secret::InMemorySecretStore;
    use crate::safety::execution_gateway::SecurityExecutionOutcome;
    use crate::safety::{
        grant::{GrantEffect, GrantResource, GrantSource, SecurityGrant},
        ControlSession, PermissionId, SecurityExecutionRequest, SecuritySubject,
        CONTROL_SESSION_HEADER,
    };
    use axum::{
        body::{to_bytes, Body},
        http::{header::CONTENT_TYPE, Request, StatusCode},
        routing::post,
        Json, Router,
    };
    use serde_json::{json, Value};
    use tokio::sync::Notify;
    use tower::ServiceExt;

    let root = std::env::temp_dir().join(format!("yilian-child-identity-{}", uuid::Uuid::new_v4()));
    let workspace = root.join("workspace");
    let definition = workspace.join(".agents/agents/researcher");
    std::fs::create_dir_all(&definition).unwrap();
    std::fs::write(
        definition.join("AGENT.md"),
        "---\nname: researcher\ndescription: Fixture researcher\ntools: [write_file]\n---\n\nWrite the fixture output.",
    )
    .unwrap();
    let destination = workspace.join("allowed");
    std::fs::create_dir_all(&destination).unwrap();
    let output = destination.join("report.txt");
    let outside_scope = workspace.join("ungranted.txt");
    let pending = Arc::new(Notify::new());
    let resume_provider = Arc::new(Notify::new());
    let provider = Router::new().route(
        "/chat/completions",
        post({
            let output = output.clone();
            let outside_scope = outside_scope.clone();
            let pending = Arc::clone(&pending);
            let resume_provider = Arc::clone(&resume_provider);
            move |Json(body): Json<Value>| {
                let output = output.clone();
                let outside_scope = outside_scope.clone();
                let pending = Arc::clone(&pending);
                let resume_provider = Arc::clone(&resume_provider);
                async move {
                    let messages = body["messages"].as_array().unwrap();
                    let is_first = !messages.iter().any(|message| message["role"] == "tool");
                    let (delta, finish) = if is_first {
                        let task = messages
                            .iter()
                            .find(|message| message["role"] == "user")
                            .and_then(|message| message["content"].as_str());
                        if task == Some("Wait before writing") {
                            pending.notify_one();
                            resume_provider.notified().await;
                        }
                        let path = if task == Some("Write outside child scope") {
                            outside_scope
                        } else {
                            output
                        };
                        let args = json!({"path":path,"content":"scoped result"}).to_string();
                        (
                            json!({"tool_calls":[{"index":0,"id":"child-write","type":"function",
                            "function":{"name":"write_file","arguments":args}}]}),
                            "tool_calls",
                        )
                    } else {
                        (json!({"content":"Fixture finished"}), "stop")
                    };
                    let chunk = json!({
                        "id":"fixture-child-model",
                        "choices":[{"index":0,"delta":delta,"finish_reason":finish}],
                        "usage":{"prompt_tokens":20,"completion_tokens":10,"total_tokens":30}
                    });
                    (
                        [(CONTENT_TYPE, "text/event-stream")],
                        format!("data: {chunk}\n\ndata: [DONE]\n\n"),
                    )
                }
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let provider = tokio::spawn(async move { axum::serve(listener, provider).await.unwrap() });
    let workspace_path = workspace.to_str().unwrap();
    let control = ControlSession::generate();
    let server = Arc::new(
        crate::app::state::AppServer::new_with_control_session_and_store(
            &root.join("test.db"),
            workspace_path,
            control.clone(),
            Arc::new(InMemorySecretStore::new()),
        )
        .unwrap(),
    );
    {
        let mut config = server.config.write();
        config.model.provider = "lmstudio".into();
        config.model.name = "delegation-fixture".into();
        config.model.base_url = format!("http://{addr}");
        config.model.api_key.clear();
        config.model.api_key_env.clear();
        config.model.api_key_ref = None;
        config.permissions.mode = PermissionMode::FullAccess;
        config.sandbox.profile = SandboxProfile::WorkspaceWrite;
    }
    let parent_rule = AccessRule {
        id: "parent-only".into(),
        scope: "agent".into(),
        subject_id: Some("principal-parent".into()),
        permission_id: PermissionId::FilesystemWrite,
        resource: None,
        mode: AccessMode::Full,
    };
    let child_rule = AccessRule {
        id: "child-resource".into(),
        scope: "resource".into(),
        subject_id: Some("subagent:researcher".into()),
        permission_id: PermissionId::FilesystemWrite,
        resource: Some(GrantResource::Filesystem {
            root: destination.to_str().unwrap().into(),
            recursive: true,
        }),
        mode: AccessMode::Full,
    };
    let rules_api = crate::api::build_router(Arc::clone(&server));
    let save_rules = |rules: Vec<AccessRule>| {
        let api = rules_api.clone();
        let control = control.clone();
        async move {
            let result = api
                .oneshot(
                    Request::put("/api/security/access-rules")
                        .header(CONTROL_SESSION_HEADER, control.token())
                        .header(CONTENT_TYPE, "application/json")
                        .body(Body::from(json!({"rules": rules}).to_string()))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(result.status(), StatusCode::OK);
            let response: Value =
                serde_json::from_slice(&to_bytes(result.into_body(), 8192).await.unwrap()).unwrap();
            assert!(response["subagents"]
                .as_array()
                .unwrap()
                .iter()
                .any(|subject| subject["subject_id"] == "subagent:researcher"));
        }
    };
    save_rules(vec![parent_rule.clone()]).await;
    let registry = server.build_agent_tool_registry().await;
    let gateway = crate::safety::production_gateway::build(
        &server,
        workspace_path,
        registry,
        Some("principal-parent".into()),
    );
    let request = |tool_call_id: &str, task: &str| SecurityExecutionRequest {
        conversation_id: "parent-conversation".into(),
        tool_call_id: tool_call_id.into(),
        tool_name: "subagent_researcher".into(),
        arguments: json!({"task":task, "subject_id":"local-user"}),
        subject: SecuritySubject::local_user(),
    };

    let unauthorized = gateway
        .execute(&request("parent-no-grant", "Write report"), RiskLevel::Low)
        .await
        .unwrap();
    assert!(matches!(
        unauthorized,
        SecurityExecutionOutcome::Executed { .. }
    ));
    assert!(
        !output.exists(),
        "child inherited the parent's write permission"
    );

    save_rules(vec![parent_rule.clone(), child_rule.clone()]).await;
    let authorized = gateway
        .execute(
            &request("parent-with-grant", "Write report"),
            RiskLevel::Low,
        )
        .await
        .unwrap();
    assert!(
        matches!(authorized, SecurityExecutionOutcome::Executed { tool_result, .. } if tool_result.ok)
    );
    assert_eq!(std::fs::read_to_string(&output).unwrap(), "scoped result");
    std::fs::remove_file(&output).unwrap();

    let out_of_scope = gateway
        .execute(
            &request("parent-outside-scope", "Write outside child scope"),
            RiskLevel::Low,
        )
        .await
        .unwrap();
    assert!(matches!(
        out_of_scope,
        SecurityExecutionOutcome::Executed { .. }
    ));
    assert!(
        !outside_scope.exists(),
        "the grant reached outside its directory"
    );

    let revoke_during_child = async {
        tokio::time::timeout(std::time::Duration::from_secs(5), pending.notified())
            .await
            .unwrap();
        save_rules(vec![parent_rule.clone()]).await;
        resume_provider.notify_one();
    };
    let revoked_request = request("parent-during-revocation", "Wait before writing");
    let (revoked, ()) = tokio::join!(
        gateway.execute(&revoked_request, RiskLevel::Low),
        revoke_during_child
    );
    assert!(matches!(
        revoked.unwrap(),
        SecurityExecutionOutcome::Executed { .. }
    ));
    assert!(
        !output.exists(),
        "child reused a permission revoked while its model was running"
    );

    save_rules(vec![parent_rule, child_rule]).await;
    let revoke_parent = async {
        tokio::time::timeout(std::time::Duration::from_secs(5), pending.notified())
            .await
            .unwrap();
        server
            .db
            .create_grant(&SecurityGrant {
                id: "revoked-parent-delegation".into(),
                subject_id: "local-user".into(),
                effect: GrantEffect::Deny,
                permission: PermissionId::AgentDelegate,
                resource: GrantResource::Any,
                source: GrantSource::User,
                created_at: chrono::Utc::now().timestamp_millis(),
                expires_at: None,
            })
            .unwrap();
        resume_provider.notify_one();
    };
    let request = request("parent-delegation-revoked", "Wait before writing");
    let (denied, ()) = tokio::join!(gateway.execute(&request, RiskLevel::Low), revoke_parent);
    assert!(matches!(
        denied.unwrap(),
        SecurityExecutionOutcome::Executed { .. }
    ));
    assert!(!output.exists(), "child reused a revoked parent delegation");

    let audit = server
        .db
        .list_security_audit_events(&SecurityAuditQuery::default())
        .unwrap();
    let child_decisions: Vec<_> = audit
        .iter()
        .filter(|event| {
            event.tool_call_id.as_deref() == Some("child-write")
                && event.event_type == "policy_decided"
        })
        .collect();
    assert_eq!(child_decisions.len(), 5);
    assert!(child_decisions
        .iter()
        .all(|event| event.subject_id == "subagent:researcher"));
    assert!(child_decisions
        .iter()
        .any(|event| event.decision_status.as_deref() == Some("allow")
            && event.details["context"]["delegation"]["parent_tool_call_id"]
                == "parent-with-grant"));
    assert!(child_decisions
        .iter()
        .any(|event| event.decision_status.as_deref() == Some("deny")
            && event.details["context"]["delegation"]["parent_tool_call_id"]
                == "parent-delegation-revoked"
            && event.details["context"]["reason"] == "delegation permission was revoked"));
    drop(gateway);
    drop(rules_api);
    drop(server);
    provider.abort();
    std::fs::remove_dir_all(root).unwrap();
}

fn spec(name: &str) -> SubagentExecutionSpec {
    SubagentExecutionSpec {
        name: name.to_string(),
        instructions: "instructions".to_string(),
        allowed_tools: vec!["read_file".to_string(), "grep".to_string()],
        model: Some("deepseek-v4-flash".to_string()),
        workdir: None,
    }
}

fn handoff_request(tools: Vec<&str>) -> crate::agent::delegation::DelegationRequest {
    crate::agent::delegation::DelegationRequest {
        goal_id: None,
        goal_version: None,
        role: crate::agent::delegation::DelegationRole::Research,
        allowed_tools: tools.into_iter().map(str::to_string).collect(),
        input_evidence_ids: Vec::new(),
        required_artifact_names: Vec::new(),
        timeout_ms: None,
    }
}

#[test]
fn structured_tools_are_intersected_with_the_trusted_definition() {
    let source = source_registry();
    let spec = spec("researcher");
    let request = handoff_request(vec!["grep"]);
    assert_eq!(
        effective_child_tools(&spec, Some(&request), &source).unwrap(),
        vec!["grep"]
    );

    let request = handoff_request(vec!["bash"]);
    assert!(effective_child_tools(&spec, Some(&request), &source).is_err());
}

#[test]
fn goal_and_evidence_references_require_active_execution_context() {
    let scope = crate::agent::delegation::ExecutionScope {
        owner: None,
        cancel: tokio_util::sync::CancellationToken::new(),
        budget: None,
        handoff_id: None,
        goal_id: None,
        goal_version: None,
        evidence_ids: Vec::new(),
        deadline: None,
    };
    let request = handoff_request(vec!["grep"]);
    assert!(validate_handoff_scope(&request, Some(&scope)).is_ok());

    let mut request = handoff_request(vec!["grep"]);
    request.goal_id = Some("invented-goal".into());
    request.goal_version = Some(1);
    assert!(validate_handoff_scope(&request, Some(&scope)).is_err());

    let mut request = handoff_request(vec!["grep"]);
    request.input_evidence_ids.push("ev_missing".into());
    assert!(validate_handoff_scope(&request, Some(&scope)).is_err());
}

#[test]
fn required_artifact_path_must_be_a_real_file_inside_the_workspace() {
    let base = std::env::temp_dir().join(format!("delegation-path-{}", uuid::Uuid::new_v4()));
    let workspace = base.join("workspace");
    std::fs::create_dir_all(&workspace).unwrap();
    let artifact = workspace.join("report.md");
    std::fs::write(&artifact, "verified output").unwrap();
    let outside = base.join("outside.md");
    std::fs::write(&outside, "outside output").unwrap();

    assert_eq!(
        resolve_handoff_artifact_path("report.md", workspace.to_str().unwrap()).unwrap(),
        artifact.canonicalize().unwrap().to_string_lossy()
    );
    assert!(
        resolve_handoff_artifact_path(outside.to_str().unwrap(), workspace.to_str().unwrap())
            .is_err()
    );
    assert!(resolve_handoff_artifact_path("missing.md", workspace.to_str().unwrap()).is_err());

    std::fs::remove_dir_all(base).unwrap();
}

#[test]
fn child_done_claim_does_not_satisfy_a_required_artifact() {
    let base = std::env::temp_dir().join(format!("delegation-result-{}", uuid::Uuid::new_v4()));
    let workspace = base.join("workspace");
    std::fs::create_dir_all(&workspace).unwrap();
    let db = Database::new(&base.join("test.db")).unwrap();
    let mut request = handoff_request(Vec::new());
    request.required_artifact_names.push("review.md".into());

    let result = build_delegation_result(
        "done",
        Some(&request),
        &db,
        workspace.to_str().unwrap(),
        "handoff-test",
        None,
        Some((1, Some(10))),
        std::time::Duration::from_millis(1),
    );

    assert!(result.unwrap_err().contains("required_artifact_missing"));
    drop(db);
    std::fs::remove_dir_all(base).unwrap();
}

#[test]
fn child_registry_is_strict_whitelist() {
    let source = source_registry();
    let child = build_child_tool_registry(&source, &spec("researcher").allowed_tools);
    assert!(child.get("read_file").is_some());
    assert!(child.get("grep").is_some());
    assert!(child.get("bash").is_none());
    assert_eq!(child.list_tools().len(), 2);
}

#[test]
fn child_registry_has_no_implicit_builtins() {
    let source = source_registry();
    let child = build_child_tool_registry(&source, &["read_file".to_string()]);
    assert!(child.get("read_file").is_some());
    assert!(child.get("write_file").is_none());
    assert!(child.get("bash").is_none());
    assert!(child.get("http_request").is_none());
}

#[test]
fn child_registry_skips_missing_allowed_tools() {
    let source = source_registry();
    let child = build_child_tool_registry(
        &source,
        &["read_file".to_string(), "not_exists".to_string()],
    );
    assert!(child.get("read_file").is_some());
    assert!(child.get("not_exists").is_none());
}

#[test]
fn child_registry_filters_subagent_tools() {
    let source = source_registry();
    let mut s = spec("researcher");
    s.allowed_tools = vec!["subagent_other".to_string(), "read_file".to_string()];
    let child = build_child_tool_registry(&source, &s.allowed_tools);
    assert!(child.get("subagent_other").is_none());
    assert!(child.get("read_file").is_some());
}

#[test]
fn child_config_overrides_only_model_name() {
    let mut parent = AppConfig::default();
    parent.model.name = "parent-model".to_string();
    parent.model.base_url = "https://example.com/v1".to_string();
    parent.model.api_key = "secret".to_string();
    parent.sandbox.profile = crate::config::types::SandboxProfile::ReadOnly;

    let child = build_child_config(&parent, Some("child-model"));
    assert_eq!(child.model.name, "child-model");
    assert_eq!(child.model.base_url, "https://example.com/v1");
    assert_eq!(child.model.api_key, "secret");
    assert_eq!(
        child.sandbox.profile,
        crate::config::types::SandboxProfile::ReadOnly
    );
}

#[test]
fn child_config_ignores_empty_model_override() {
    let mut parent = AppConfig::default();
    parent.model.name = "parent-model".to_string();
    let child = build_child_config(&parent, Some("   "));
    assert_eq!(child.model.name, "parent-model");
    let child_none = build_child_config(&parent, None);
    assert_eq!(child_none.model.name, "parent-model");
}

#[test]
fn workdir_override_fails_closed() {
    let mut s = spec("researcher");
    s.workdir = Some("./other".to_string());
    let err = validate_runtime_spec(&s).unwrap_err();
    assert!(err.contains("workdir"));
}

#[test]
fn workdir_none_passes_validation() {
    let s = spec("researcher");
    assert!(validate_runtime_spec(&s).is_ok());
}

#[test]
fn bounded_success_truncates_utf8_safe() {
    let long = "长".repeat(MAX_SUBAGENT_RESULT_CHARS + 5000);
    let result = bounded_subagent_success(&long);
    assert!(result.ok);
    assert!(result.content.chars().count() <= MAX_SUBAGENT_RESULT_CHARS + 3);
    assert!(result.content.is_char_boundary(result.content.len()));
}

#[test]
fn bounded_success_keeps_short_output() {
    let result = bounded_subagent_success("short");
    assert!(result.ok);
    assert_eq!(result.content, "short");
}
