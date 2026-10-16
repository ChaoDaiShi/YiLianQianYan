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
