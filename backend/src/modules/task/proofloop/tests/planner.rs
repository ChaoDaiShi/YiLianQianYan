use crate::modules::task::proofloop::contracts::CapabilitySnapshot;

fn planner_test_snapshot() -> CapabilitySnapshot {
    use crate::modules::task::proofloop::contracts::{CapabilityKindSpec, CapabilitySpec};

    CapabilitySnapshot {
        snapshot_id: "snapshot-test".into(),
        captured_at_ms: 0,
        capabilities: vec![CapabilitySpec {
            capability_id: "mcp.reader".into(),
            executor_ref: "capability://mcp.reader".into(),
            kind: CapabilityKindSpec::McpTool,
            name: "读取文件".into(),
            description: "读取指定路径的文件".into(),
            input_schema: Some(serde_json::json!({
                "type": "object",
                "properties": {"path": {"type": "string"}}
            })),
            output_schema: Some(serde_json::json!({
                "type": "object",
                "properties": {"content": {"type": "string"}}
            })),
            output_path_prefix: vec!["structured".into()],
            output_provenance: "declared".into(),
            risk: "low".into(),
            readiness: "ready".into(),
            unavailable_reason: None,
        }],
    }
}

#[test]
fn planner_prompt_limit_counts_full_unicode_system_and_user_messages() {
    use crate::modules::task::proofloop::planner::{
        build_planning_messages, planning_prompt_char_count, validate_planning_prompt_size,
        MAX_PLANNING_PROMPT_CHARS,
    };

    let snapshot = planner_test_snapshot();
    let empty_goal = build_planning_messages("", &snapshot);
    let goal_chars = MAX_PLANNING_PROMPT_CHARS - planning_prompt_char_count(&empty_goal);
    let exact_limit_goal = "界".repeat(goal_chars);
    let exact_limit = build_planning_messages(&exact_limit_goal, &snapshot);
    assert_eq!(
        planning_prompt_char_count(&exact_limit),
        MAX_PLANNING_PROMPT_CHARS
    );
    assert!(validate_planning_prompt_size(&exact_limit).is_ok());

    let over_limit_goal = format!("{exact_limit_goal}界");
    let over_limit = build_planning_messages(&over_limit_goal, &snapshot);
    assert_eq!(
        planning_prompt_char_count(&over_limit),
        MAX_PLANNING_PROMPT_CHARS + 1
    );
    let error = validate_planning_prompt_size(&over_limit).unwrap_err();
    assert_eq!(error.code, "proofloop_planning_input_too_large");
    assert!(error.message.contains("缩减"));
    assert!(error.message.contains("选择"));
}

#[tokio::test]
async fn oversized_planner_input_is_rejected_before_model_invocation() {
    use std::sync::Arc;

    use crate::modules::task::proofloop::planner::{
        propose_candidate_with_usage, MAX_PLANNING_PROMPT_CHARS,
    };
    use crate::{
        config::types::ModelConfig,
        integrations::{
            llm::client::LlmClient,
            secret::{InMemorySecretStore, SecretResolver},
        },
    };
    use tokio_util::sync::CancellationToken;

    let mut model_config = ModelConfig::default();
    model_config.api_key_env.clear();
    let resolver = Arc::new(SecretResolver::new(Arc::new(InMemorySecretStore::new())));
    let client = LlmClient::new(&model_config, resolver);
    let user_goal = "目标".repeat(MAX_PLANNING_PROMPT_CHARS + 1);

    let error = propose_candidate_with_usage(
        "workspace-test",
        &user_goal,
        &planner_test_snapshot(),
        Some(&client),
        &CancellationToken::new(),
    )
    .await
    .unwrap_err();

    assert_eq!(error.code, "proofloop_planning_input_too_large");
}
